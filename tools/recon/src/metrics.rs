use crate::complexity;
use crate::config;
use crate::output::{FileMetrics, Hotspot};
use crate::parse::ParsedFile;

/// Configurable thresholds for hotspot detection.
#[derive(Debug, Clone)]
pub struct HotspotThresholds {
    pub complexity: u32,
    pub nesting: u32,
    pub loc: u32,
    pub params: u32,
}

impl Default for HotspotThresholds {
    fn default() -> Self {
        Self {
            complexity: config::DEFAULT_COMPLEXITY_THRESHOLD,
            nesting: config::DEFAULT_NESTING_THRESHOLD,
            loc: config::DEFAULT_LOC_THRESHOLD,
            params: config::DEFAULT_PARAMS_THRESHOLD,
        }
    }
}

/// Identify functions that exceed hotspot thresholds.
pub fn detect_hotspots(
    path: &str,
    metrics: &FileMetrics,
    thresholds: &HotspotThresholds,
) -> Vec<Hotspot> {
    let mut hotspots = Vec::new();

    for func in &metrics.functions {
        if func.cyclomatic_complexity > thresholds.complexity {
            hotspots.push(Hotspot {
                path: path.to_string(),
                function: func.name.clone(),
                metric: "cyclomatic_complexity".to_string(),
                value: func.cyclomatic_complexity,
                threshold: thresholds.complexity,
            });
        }
        if func.max_nesting_depth > thresholds.nesting {
            hotspots.push(Hotspot {
                path: path.to_string(),
                function: func.name.clone(),
                metric: "max_nesting_depth".to_string(),
                value: func.max_nesting_depth,
                threshold: thresholds.nesting,
            });
        }
        if func.lines_of_code as u32 > thresholds.loc {
            hotspots.push(Hotspot {
                path: path.to_string(),
                function: func.name.clone(),
                metric: "lines_of_code".to_string(),
                value: func.lines_of_code as u32,
                threshold: thresholds.loc,
            });
        }
        if func.parameter_count > thresholds.params {
            hotspots.push(Hotspot {
                path: path.to_string(),
                function: func.name.clone(),
                metric: "parameter_count".to_string(),
                value: func.parameter_count,
                threshold: thresholds.params,
            });
        }
    }

    hotspots
}

/// Compute full file-level metrics from a parsed file.
///
/// Aggregates per-function metrics into file-level totals.
/// Declaration-only files (`.d.ts`) skip complexity metrics.
pub fn analyze_file(parsed: &ParsedFile) -> FileMetrics {
    let loc = complexity::count_lines(&parsed.source, &parsed.tree);

    if parsed.source_file.declaration_only {
        return FileMetrics {
            total_lines: loc.total_lines,
            code_lines: loc.code_lines,
            comment_lines: loc.comment_lines,
            blank_lines: loc.blank_lines,
            cyclomatic_complexity: 0,
            max_nesting_depth: 0,
            functions: Vec::new(),
        };
    }

    let functions = if parsed.script_blocks.is_empty() {
        complexity::extract_function_metrics(&parsed.tree, &parsed.source)
    } else {
        // Svelte: extract from re-parsed script blocks (TS grammar).
        let mut all_fns = Vec::new();
        for block in &parsed.script_blocks {
            let mut block_fns = complexity::extract_function_metrics(&block.tree, &block.source);
            // Adjust line numbers to be relative to the original Svelte file.
            for f in &mut block_fns {
                f.line = f.line.saturating_add(block.start_line);
                f.end_line = f.end_line.saturating_add(block.start_line);
            }
            all_fns.extend(block_fns);
        }
        all_fns
    };

    let file_complexity = functions
        .iter()
        .map(|f| f.cyclomatic_complexity)
        .sum::<u32>()
        .max(1_u32.min(functions.len() as u32)); // At least 1 if there are functions

    let max_nesting = functions
        .iter()
        .map(|f| f.max_nesting_depth)
        .max()
        .unwrap_or(0);

    FileMetrics {
        total_lines: loc.total_lines,
        code_lines: loc.code_lines,
        comment_lines: loc.comment_lines,
        blank_lines: loc.blank_lines,
        cyclomatic_complexity: file_complexity,
        max_nesting_depth: max_nesting,
        functions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language;
    use crate::output::{FunctionMetrics, SourceFile};
    use crate::parse::parse_files_sequential;
    use std::path::Path;

    fn make_function(
        name: &str,
        complexity: u32,
        nesting: u32,
        loc: usize,
        params: u32,
    ) -> FunctionMetrics {
        FunctionMetrics {
            name: name.to_string(),
            line: 1,
            end_line: loc,
            lines_of_code: loc,
            cyclomatic_complexity: complexity,
            max_nesting_depth: nesting,
            parameter_count: params,
        }
    }

    #[test]
    fn detect_hotspots_flags_exceeding_complexity() {
        let thresholds = HotspotThresholds {
            complexity: 10,
            nesting: 3,
            loc: 30,
            params: 4,
        };
        let metrics = FileMetrics {
            functions: vec![
                make_function("simple", 2, 1, 10, 1),
                make_function("complex", 15, 2, 20, 3),
            ],
            ..Default::default()
        };

        let hotspots = detect_hotspots("src/main.ts", &metrics, &thresholds);
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].function, "complex");
        assert_eq!(hotspots[0].metric, "cyclomatic_complexity");
        assert_eq!(hotspots[0].value, 15);
        assert_eq!(hotspots[0].threshold, 10);
    }

    #[test]
    fn detect_hotspots_flags_multiple_metrics_for_same_function() {
        let thresholds = HotspotThresholds {
            complexity: 10,
            nesting: 3,
            loc: 30,
            params: 4,
        };
        let metrics = FileMetrics {
            functions: vec![make_function("monster", 20, 5, 50, 8)],
            ..Default::default()
        };

        let hotspots = detect_hotspots("src/monster.ts", &metrics, &thresholds);
        assert_eq!(hotspots.len(), 4); // all four metrics exceeded
        let metric_names: Vec<&str> = hotspots.iter().map(|h| h.metric.as_str()).collect();
        assert!(metric_names.contains(&"cyclomatic_complexity"));
        assert!(metric_names.contains(&"max_nesting_depth"));
        assert!(metric_names.contains(&"lines_of_code"));
        assert!(metric_names.contains(&"parameter_count"));
    }

    #[test]
    fn detect_hotspots_returns_empty_for_clean_code() {
        let thresholds = HotspotThresholds::default();
        let metrics = FileMetrics {
            functions: vec![make_function("clean", 5, 2, 15, 2)],
            ..Default::default()
        };

        let hotspots = detect_hotspots("src/clean.ts", &metrics, &thresholds);
        assert!(hotspots.is_empty());
    }

    #[test]
    fn detect_hotspots_at_exact_threshold_not_flagged() {
        let thresholds = HotspotThresholds {
            complexity: 10,
            nesting: 3,
            loc: 30,
            params: 4,
        };
        // Values exactly AT threshold — should NOT be flagged (> not >=).
        let metrics = FileMetrics {
            functions: vec![make_function("borderline", 10, 3, 30, 4)],
            ..Default::default()
        };

        let hotspots = detect_hotspots("src/edge.ts", &metrics, &thresholds);
        assert!(hotspots.is_empty());
    }

    fn write_fixture(dir: &Path, name: &str, content: &str) -> SourceFile {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        SourceFile {
            path: name.to_string(),
            language: language::detect_language(Path::new(name)).unwrap(),
            declaration_only: language::is_declaration_file(Path::new(name)),
        }
    }

    #[test]
    fn analyze_file_produces_correct_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "example.ts",
            r#"// A module
function greet(name: string) {
    return `Hello ${name}`;
}

function check(x: number) {
    if (x > 0) {
        return "positive";
    }
    return "non-positive";
}
"#,
        );
        let result = parse_files_sequential(&[sf], dir.path());
        let parsed = &result.files[0];
        let metrics = analyze_file(parsed);

        assert_eq!(metrics.total_lines, 11);
        assert!(metrics.code_lines > 0);
        assert!(metrics.comment_lines > 0);
        assert_eq!(metrics.functions.len(), 2);
        assert_eq!(metrics.functions[0].name, "greet");
        assert_eq!(metrics.functions[1].name, "check");
        assert_eq!(metrics.functions[1].cyclomatic_complexity, 2);
    }

    #[test]
    fn analyze_declaration_file_skips_complexity() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "types.d.ts",
            "declare function foo(x: number): string;\ndeclare function bar(): void;\n",
        );
        let result = parse_files_sequential(&[sf], dir.path());
        let parsed = &result.files[0];
        let metrics = analyze_file(parsed);

        assert_eq!(metrics.cyclomatic_complexity, 0);
        assert!(metrics.functions.is_empty());
        assert!(metrics.total_lines > 0);
    }

    #[test]
    fn analyze_svelte_file_extracts_script_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "Counter.svelte",
            r#"<script>
  let count = 0;

  function increment() {
    if (count < 100) {
      count += 1;
    }
  }

  function reset() {
    count = 0;
  }
</script>

<button on:click={increment}>{count}</button>
<button on:click={reset}>Reset</button>
"#,
        );
        let result = parse_files_sequential(&[sf], dir.path());
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        let parsed = &result.files[0];
        let metrics = analyze_file(parsed);

        // Should find functions from the script block.
        assert!(
            metrics.functions.len() >= 2,
            "expected at least 2 functions, got {}",
            metrics.functions.len()
        );
        assert!(metrics.code_lines > 0);
    }

    #[test]
    fn analyze_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "empty.ts", "");
        let result = parse_files_sequential(&[sf], dir.path());
        let parsed = &result.files[0];
        let metrics = analyze_file(parsed);

        assert_eq!(metrics.total_lines, 0);
        assert!(metrics.functions.is_empty());
    }

    #[test]
    fn default_thresholds_match_config() {
        let t = HotspotThresholds::default();
        assert_eq!(t.complexity, config::DEFAULT_COMPLEXITY_THRESHOLD);
        assert_eq!(t.nesting, config::DEFAULT_NESTING_THRESHOLD);
        assert_eq!(t.loc, config::DEFAULT_LOC_THRESHOLD);
        assert_eq!(t.params, config::DEFAULT_PARAMS_THRESHOLD);
    }
}
