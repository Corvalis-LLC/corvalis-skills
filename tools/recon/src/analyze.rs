use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{BUDGET_HEADROOM, CHARS_PER_TOKEN};
use crate::deps;
use crate::metrics::{self, HotspotThresholds};
use crate::output::{AnalysisResult, FileAnalysis, Hotspot, Summary, Symbol};
use crate::overview;
use crate::parse;
use crate::ranking;
use crate::resolve;
use crate::symbols;
use crate::walk::{self, WalkOptions};

/// Full analysis result plus pretty-printed rendering for `--format pretty`.
pub struct AnalyzeOutput {
    pub result: AnalysisResult,
    pub pretty: String,
}

/// Run the full project analysis pipeline used by the `analyze` subcommand.
pub fn analyze_project(
    root: &Path,
    walk_options: &WalkOptions,
    budget_tokens: Option<usize>,
) -> Result<AnalyzeOutput> {
    let walk_result =
        walk::discover_files(root, walk_options).context("discovering source files")?;
    let parse_result = parse::parse_files(&walk_result.files, root);
    let aliases = resolve::load_tsconfig_aliases(root);
    let graph = deps::build_dependency_graph(&parse_result.files, root, &aliases);

    let thresholds = HotspotThresholds::default();
    let mut files = Vec::new();
    let mut hotspots = Vec::new();

    for parsed in &parse_result.files {
        let file_symbols = symbols::extract_symbols(parsed);
        let file_metrics = metrics::analyze_file(parsed);
        hotspots.extend(metrics::detect_hotspots(
            &parsed.source_file.path,
            &file_metrics,
            &thresholds,
        ));

        files.push(FileAnalysis {
            path: parsed.source_file.path.clone(),
            language: parsed.source_file.language.as_str().to_string(),
            symbols: file_symbols.symbols,
            imports: file_symbols.imports,
            exports: file_symbols.exports,
            metrics: file_metrics,
        });
    }

    let ranked = ranking::score_files(&files, &graph, &hotspots);
    let mut ranked_files: Vec<FileAnalysis> = ranked
        .iter()
        .filter_map(|score| files.iter().find(|file| file.path == score.path).cloned())
        .collect();

    let project = overview::build_overview(root, &walk_result.files, &files, &graph.entry_points);
    let warnings = walk_result
        .warnings
        .into_iter()
        .chain(parse_result.warnings)
        .collect::<Vec<_>>();

    let summary = build_summary(&files);

    let mut result = AnalysisResult {
        version: env!("CARGO_PKG_VERSION").to_string(),
        project,
        files: std::mem::take(&mut ranked_files),
        graph,
        hotspots,
        warnings,
        summary,
    };

    if let Some(budget) = budget_tokens {
        apply_budget(&mut result, budget);
    }

    let pretty = format_pretty(&result);

    Ok(AnalyzeOutput { result, pretty })
}

fn build_summary(files: &[FileAnalysis]) -> Summary {
    let total_files = files.len();
    let total_symbols = files.iter().map(|file| file.symbols.len()).sum();
    let total_lines_of_code = files.iter().map(|file| file.metrics.code_lines).sum();
    let avg_complexity = if total_files == 0 {
        0.0
    } else {
        let total_complexity: u32 = files
            .iter()
            .map(|file| file.metrics.cyclomatic_complexity)
            .sum();
        ((total_complexity as f64 / total_files as f64) * 100.0).round() / 100.0
    };

    Summary {
        total_files,
        total_symbols,
        total_lines_of_code,
        avg_complexity,
    }
}

fn apply_budget(result: &mut AnalysisResult, budget_tokens: usize) {
    let target_chars =
        ((budget_tokens as f64) * (CHARS_PER_TOKEN as f64) * BUDGET_HEADROOM).floor() as usize;

    if target_chars == 0 {
        result.files.clear();
        return;
    }

    let fixed_cost = fixed_cost_chars(result);
    if fixed_cost >= target_chars {
        result.files.clear();
        return;
    }

    let full_files = std::mem::take(&mut result.files);
    let mut kept = Vec::new();
    let mut used_chars = fixed_cost;

    for file in full_files {
        let full_cost = serialized_len(&file);
        if used_chars + full_cost <= target_chars {
            used_chars += full_cost;
            kept.push(file);
            continue;
        }

        let summary_file = summarize_file(&file);
        let summary_cost = serialized_len(&summary_file);
        if used_chars + summary_cost <= target_chars {
            used_chars += summary_cost;
            kept.push(summary_file);
        } else {
            break;
        }
    }

    result.files = kept;
}

fn fixed_cost_chars(result: &AnalysisResult) -> usize {
    let fixed = serde_json::json!({
        "version": result.version,
        "project": result.project,
        "files": [],
        "graph": result.graph,
        "hotspots": result.hotspots,
        "warnings": result.warnings,
        "summary": result.summary,
    });
    fixed.to_string().len()
}

fn serialized_len<T: serde::Serialize>(value: &T) -> usize {
    serde_json::to_string(value)
        .map(|json| json.len())
        .unwrap_or_default()
}

fn summarize_file(file: &FileAnalysis) -> FileAnalysis {
    FileAnalysis {
        path: file.path.clone(),
        language: file.language.clone(),
        symbols: file.symbols.iter().map(summarize_symbol).collect(),
        imports: Vec::new(),
        exports: file.exports.clone(),
        metrics: crate::output::FileMetrics {
            total_lines: file.metrics.total_lines,
            code_lines: file.metrics.code_lines,
            comment_lines: file.metrics.comment_lines,
            blank_lines: file.metrics.blank_lines,
            cyclomatic_complexity: file.metrics.cyclomatic_complexity,
            max_nesting_depth: file.metrics.max_nesting_depth,
            functions: Vec::new(),
        },
    }
}

fn summarize_symbol(symbol: &Symbol) -> Symbol {
    Symbol {
        name: symbol.name.clone(),
        kind: symbol.kind.clone(),
        line: symbol.line,
        end_line: symbol.line,
        exported: symbol.exported,
        signature: None,
    }
}

fn format_pretty(result: &AnalysisResult) -> String {
    let mut lines = Vec::new();
    let hotspot_counts = hotspot_counts(&result.hotspots);

    for file in &result.files {
        let hotspot_count = hotspot_counts.get(file.path.as_str()).copied().unwrap_or(0);
        let mut header = format!(
            "{} (complexity: {}, {} symbols",
            file.path,
            file.metrics.cyclomatic_complexity,
            file.symbols.len()
        );
        if hotspot_count > 0 {
            header.push_str(&format!(", {} hotspots", hotspot_count));
        }
        header.push_str("):");
        lines.push(header);

        for symbol in &file.symbols {
            let descriptor = symbol
                .signature
                .as_deref()
                .map(|sig| format!("{} {}", symbol.name, sig))
                .unwrap_or_else(|| symbol.name.clone());
            lines.push(format!(
                "  {}: {:?} {}",
                symbol.line, symbol.kind, descriptor
            ));
        }

        if file.symbols.is_empty() {
            lines.push("  (no symbols)".to_string());
        }
    }

    if lines.is_empty() {
        lines.push("(no analyzed files)".to_string());
    }

    lines.join("\n")
}

fn hotspot_counts(hotspots: &[Hotspot]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for hotspot in hotspots {
        *counts.entry(hotspot.path.as_str()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{
        Dependency, DependencyGraph, Export, ExportKind, FileMetrics, FunctionMetrics, GraphStats,
        Import, ImportKind, ProjectOverview, SymbolKind, Warning,
    };

    fn make_file(
        path: &str,
        symbol_count: usize,
        export_count: usize,
        complexity: u32,
    ) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            language: "typescript".to_string(),
            symbols: (0..symbol_count)
                .map(|idx| Symbol {
                    name: format!("sym_{idx}"),
                    kind: SymbolKind::Function,
                    line: idx + 1,
                    end_line: idx + 2,
                    exported: idx < export_count,
                    signature: Some("(x: number)".to_string()),
                })
                .collect(),
            imports: vec![Import {
                source: "./dep".to_string(),
                specifiers: vec!["dep".to_string()],
                kind: ImportKind::Named,
                line: 1,
            }],
            exports: (0..export_count)
                .map(|idx| Export {
                    name: format!("exp_{idx}"),
                    kind: ExportKind::Named,
                    line: idx + 1,
                    source: None,
                })
                .collect(),
            metrics: FileMetrics {
                total_lines: 50,
                code_lines: 40,
                comment_lines: 5,
                blank_lines: 5,
                cyclomatic_complexity: complexity,
                max_nesting_depth: 4,
                functions: vec![FunctionMetrics {
                    name: "sym_0".to_string(),
                    line: 1,
                    end_line: 10,
                    lines_of_code: 10,
                    cyclomatic_complexity: complexity,
                    max_nesting_depth: 4,
                    parameter_count: 2,
                }],
            },
        }
    }

    fn make_result(files: Vec<FileAnalysis>) -> AnalysisResult {
        AnalysisResult {
            version: "0.1.0".to_string(),
            project: ProjectOverview {
                name: "fixture".to_string(),
                root: ".".to_string(),
                languages: BTreeMap::new(),
                entry_points: vec!["src/main.ts".to_string()],
                config_files: vec!["package.json".to_string()],
                directory_tree: Vec::new(),
            },
            files,
            graph: DependencyGraph {
                adjacency: BTreeMap::from([(
                    "src/main.ts".to_string(),
                    vec![Dependency {
                        target: "src/lib.ts".to_string(),
                        specifiers: vec!["lib".to_string()],
                        resolved: true,
                        external: false,
                    }],
                )]),
                entry_points: vec!["src/main.ts".to_string()],
                leaf_nodes: vec!["src/lib.ts".to_string()],
                cycles: Vec::new(),
                stats: GraphStats::default(),
            },
            hotspots: vec![Hotspot {
                path: "src/main.ts".to_string(),
                function: "sym_0".to_string(),
                metric: "cyclomatic_complexity".to_string(),
                value: 12,
                threshold: 10,
            }],
            warnings: vec![Warning {
                path: "src/generated.ts".to_string(),
                message: "skipped".to_string(),
            }],
            summary: Summary {
                total_files: 2,
                total_symbols: 8,
                total_lines_of_code: 80,
                avg_complexity: 6.0,
            },
        }
    }

    #[test]
    fn summary_counts_files_symbols_and_loc() {
        let files = vec![make_file("a.ts", 3, 2, 5), make_file("b.ts", 1, 0, 7)];
        let summary = build_summary(&files);

        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.total_symbols, 4);
        assert_eq!(summary.total_lines_of_code, 80);
        assert_eq!(summary.avg_complexity, 6.0);
    }

    #[test]
    fn budget_keeps_fixed_sections_even_when_no_files_fit() {
        let mut result = make_result(vec![make_file("src/main.ts", 12, 6, 12)]);
        apply_budget(&mut result, 1);

        assert!(result.files.is_empty());
        assert_eq!(result.project.name, "fixture");
        assert_eq!(result.hotspots.len(), 1);
        assert_eq!(result.graph.entry_points, vec!["src/main.ts".to_string()]);
    }

    #[test]
    fn budget_prefers_higher_ranked_files() {
        let high = make_file("src/main.ts", 12, 6, 12);
        let low = make_file("src/leaf.ts", 1, 0, 1);
        let mut result = make_result(vec![high.clone(), low.clone()]);

        // Pre-sort like the real pipeline would.
        result.files = vec![high, low];
        apply_budget(&mut result, 4000);

        assert!(!result.files.is_empty());
        assert_eq!(result.files[0].path, "src/main.ts");
    }

    #[test]
    fn summarized_files_drop_heavy_details() {
        let file = make_file("src/main.ts", 4, 2, 12);
        let summarized = summarize_file(&file);

        assert!(summarized.imports.is_empty());
        assert!(summarized.metrics.functions.is_empty());
        assert!(
            summarized
                .symbols
                .iter()
                .all(|symbol| symbol.signature.is_none())
        );
    }

    #[test]
    fn pretty_format_includes_symbol_lines() {
        let result = make_result(vec![make_file("src/main.ts", 2, 1, 12)]);
        let pretty = format_pretty(&result);

        assert!(pretty.contains("src/main.ts (complexity: 12, 2 symbols"));
        assert!(pretty.contains("sym_0"));
    }

    #[test]
    fn analyze_end_to_end_builds_project_result() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"fixture-project"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.ts"),
            "import { helper } from './lib';\nexport function main() { return helper(); }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/lib.ts"),
            "export function helper() { return 1; }\n",
        )
        .unwrap();

        let output = analyze_project(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
            None,
        )
        .unwrap();

        assert_eq!(output.result.project.name, "fixture-project");
        assert_eq!(output.result.summary.total_files, 2);
        assert_eq!(output.result.graph.stats.total_files, 2);
        assert_eq!(output.result.files[0].path, "src/main.ts");
        assert!(output.pretty.contains("src/main.ts"));
    }
}
