use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rayon::prelude::*;
use tree_sitter::{Language, Parser, Tree};

use crate::output::{DetectedLanguage, SourceFile, Warning};
use crate::{language_svelte, language_tsx, language_typescript};

/// Owned result of parsing a single source file.
///
/// Trees are consumed during extraction and NOT retained — downstream
/// streams receive only the owned data they need.
#[derive(Debug)]
pub struct ParsedFile {
    pub source_file: SourceFile,
    pub source: String,
    pub tree: Tree,
    /// For Svelte files: every re-parsed script block and its source text.
    /// Downstream symbol extraction uses these for TS-level analysis.
    pub script_blocks: Vec<ScriptBlock>,
}

/// A `<script>` block extracted from a Svelte component.
#[derive(Debug)]
pub struct ScriptBlock {
    pub source: String,
    pub tree: Tree,
    /// Byte offset of the script block within the original file.
    /// Used to adjust line numbers back to the original file.
    pub start_line: usize,
}

/// Result of parsing all files in the project.
pub struct ParseResult {
    pub files: Vec<ParsedFile>,
    pub warnings: Vec<Warning>,
}

/// Get the tree-sitter `Language` for a `DetectedLanguage`.
///
/// JavaScript and JSX files use the TSX grammar because the vendored
/// JavaScript grammar is ABI version 15, which tree-sitter 0.24 does
/// not support. TSX is a strict superset that parses plain JS correctly.
fn ts_language(lang: DetectedLanguage) -> Language {
    match lang {
        DetectedLanguage::TypeScript => language_typescript(),
        DetectedLanguage::Tsx | DetectedLanguage::Jsx | DetectedLanguage::JavaScript => {
            language_tsx()
        }
        DetectedLanguage::Svelte => language_svelte(),
    }
}

thread_local! {
    static PARSERS: RefCell<HashMap<DetectedLanguage, Parser>> = RefCell::new(HashMap::new());
}

/// Parse a single source file with tree-sitter.
///
/// Returns `None` with a warning if the file cannot be read or parsed.
fn parse_single(source_file: &SourceFile) -> Result<ParsedFile, Warning> {
    let source = fs::read(Path::new(&source_file.path)).map_err(|e| Warning {
        path: source_file.path.clone(),
        message: format!("unable to read file: {e}"),
    })?;

    // Handle non-UTF8 content gracefully.
    let source = String::from_utf8_lossy(&source).into_owned();

    PARSERS.with(|parsers| {
        let mut parsers = parsers.borrow_mut();
        let parser = parsers.entry(source_file.language).or_insert_with(|| {
            let mut p = Parser::new();
            p.set_language(&ts_language(source_file.language))
                .expect("grammar version mismatch is a build bug");
            p
        });

        let tree = parser.parse(&source, None).ok_or_else(|| Warning {
            path: source_file.path.clone(),
            message: "tree-sitter returned no parse tree".into(),
        })?;

        let script_blocks = if source_file.language == DetectedLanguage::Svelte {
            extract_svelte_scripts(&source, &tree, &source_file.path)?
        } else {
            Vec::new()
        };

        Ok(ParsedFile {
            source_file: source_file.clone(),
            source,
            tree,
            script_blocks,
        })
    })
}

/// Extract all `<script>` blocks from a Svelte file and re-parse them with
/// the TypeScript grammar.
///
/// Returns an empty vector (not an error) if the component has no script block.
fn extract_svelte_scripts(
    source: &str,
    tree: &Tree,
    path: &str,
) -> Result<Vec<ScriptBlock>, Warning> {
    let root = tree.root_node();
    let mut blocks = Vec::new();

    // Walk top-level children looking for script_element nodes.
    let mut cursor = root.walk();
    for script_node in root
        .children(&mut cursor)
        .filter(|node| node.kind() == "script_element")
    {
        // Find the raw_text child inside the script element — that's the JS/TS content.
        let mut child_cursor = script_node.walk();
        let Some(raw_text) = script_node
            .children(&mut child_cursor)
            .find(|node| node.kind() == "raw_text")
        else {
            continue;
        };

        let script_source = raw_text
            .utf8_text(source.as_bytes())
            .map_err(|_| Warning {
                path: path.to_string(),
                message: "unable to extract script block text".into(),
            })?
            .to_string();

        let start_line = raw_text.start_position().row;

        // Re-parse with TypeScript grammar via a fresh parser (not from the pool,
        // since the pool parser for this thread may be set to Svelte).
        let mut ts_parser = Parser::new();
        ts_parser
            .set_language(&language_typescript())
            .expect("TypeScript grammar version mismatch is a build bug");

        let ts_tree = ts_parser
            .parse(&script_source, None)
            .ok_or_else(|| Warning {
                path: path.to_string(),
                message: "unable to parse Svelte script block as TypeScript".into(),
            })?;

        blocks.push(ScriptBlock {
            source: script_source,
            tree: ts_tree,
            start_line,
        });
    }

    Ok(blocks)
}

/// Parse all source files in parallel using rayon.
///
/// Each thread gets its own parser via `thread_local!`. Parse failures
/// produce warnings and are skipped — the pipeline never crashes on
/// unparseable files.
pub fn parse_files(source_files: &[SourceFile], root: &Path) -> ParseResult {
    let (parsed, warnings): (Vec<_>, Vec<_>) = source_files
        .par_iter()
        .map(|sf| {
            // Resolve path relative to root for reading.
            let absolute = SourceFile {
                path: root.join(&sf.path).to_string_lossy().into_owned(),
                language: sf.language,
                declaration_only: sf.declaration_only,
            };
            match parse_single(&absolute) {
                Ok(mut pf) => {
                    // Restore the relative path for downstream consumers.
                    pf.source_file.path = sf.path.clone();
                    Ok(pf)
                }
                Err(mut w) => {
                    // Normalize warning path back to relative.
                    w.path = sf.path.clone();
                    Err(w)
                }
            }
        })
        .partition_map(|r| match r {
            Ok(pf) => rayon::iter::Either::Left(pf),
            Err(w) => rayon::iter::Either::Right(w),
        });

    ParseResult {
        files: parsed,
        warnings,
    }
}

/// Parse all source files sequentially (for testing determinism).
pub fn parse_files_sequential(source_files: &[SourceFile], root: &Path) -> ParseResult {
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    for sf in source_files {
        let absolute = SourceFile {
            path: root.join(&sf.path).to_string_lossy().into_owned(),
            language: sf.language,
            declaration_only: sf.declaration_only,
        };
        match parse_single(&absolute) {
            Ok(mut pf) => {
                pf.source_file.path = sf.path.clone();
                files.push(pf);
            }
            Err(mut w) => {
                w.path = sf.path.clone();
                warnings.push(w);
            }
        }
    }

    ParseResult { files, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_fixture(dir: &Path, name: &str, content: &str) -> SourceFile {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();

        let file_path = Path::new(name);
        SourceFile {
            path: name.to_string(),
            language: crate::language::detect_language(file_path).unwrap(),
            declaration_only: crate::language::is_declaration_file(file_path),
        }
    }

    #[test]
    fn parse_typescript_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "main.ts", "const x: number = 42;\n");

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
        assert!(!result.files[0].tree.root_node().has_error());
    }

    #[test]
    fn parse_tsx_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "App.tsx",
            "export default function App() { return <div/>; }\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
        assert!(!result.files[0].tree.root_node().has_error());
    }

    #[test]
    fn parse_javascript_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "utils.js",
            "function add(a, b) { return a + b; }\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn parse_svelte_file_extracts_script() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "Counter.svelte",
            "<script>\n  let count = 0;\n  function increment() { count += 1; }\n</script>\n\n<button on:click={increment}>{count}</button>\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        assert_eq!(result.files.len(), 1);

        let pf = &result.files[0];
        assert!(
            !pf.script_blocks.is_empty(),
            "Svelte file should have script blocks"
        );

        let script = &pf.script_blocks[0];
        assert!(script.source.contains("let count = 0;"));
        assert!(!script.tree.root_node().has_error());
    }

    #[test]
    fn parse_svelte_with_module_and_instance_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "Dual.svelte",
            "<script context=\"module\">\n  export const prerender = true;\n</script>\n<script>\n  let count = 0;\n</script>\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(
            result.warnings.is_empty(),
            "warnings: {:?}",
            result.warnings
        );
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].script_blocks.len(), 2);
        assert!(
            result.files[0].script_blocks[0]
                .source
                .contains("prerender")
        );
        assert!(result.files[0].script_blocks[1].source.contains("count"));
    }

    #[test]
    fn parse_svelte_without_script_block() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "Static.svelte", "<h1>Hello world</h1>\n");

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].script_blocks.is_empty());
    }

    #[test]
    fn parse_malformed_file_produces_tree_with_error() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "bad.ts", "function { broken syntax here (\n");

        let result = parse_files_sequential(&[sf], dir.path());
        // tree-sitter is error-tolerant — it should still produce a tree
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].tree.root_node().has_error());
    }

    #[test]
    fn parse_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "empty.ts", "");

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn parse_file_with_bom() {
        let dir = tempfile::tempdir().unwrap();
        let content = "\u{FEFF}const x = 1;\n";
        let sf = write_fixture(dir.path(), "bom.ts", content);

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn parse_file_with_crlf_line_endings() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(dir.path(), "crlf.ts", "const x = 1;\r\nconst y = 2;\r\n");

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn missing_file_produces_warning() {
        let dir = tempfile::tempdir().unwrap();
        let sf = SourceFile {
            path: "nonexistent.ts".to_string(),
            language: DetectedLanguage::TypeScript,
            declaration_only: false,
        };

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.files.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("unable to read"));
    }

    #[test]
    fn parallel_and_sequential_produce_same_file_count() {
        let dir = tempfile::tempdir().unwrap();
        let files: Vec<SourceFile> = (0..10)
            .map(|i| {
                write_fixture(
                    dir.path(),
                    &format!("file{i}.ts"),
                    &format!("const x{i} = {i};\n"),
                )
            })
            .collect();

        let seq = parse_files_sequential(&files, dir.path());
        let par = parse_files(&files, dir.path());

        assert_eq!(seq.files.len(), par.files.len());
        assert_eq!(seq.warnings.len(), par.warnings.len());
    }

    #[test]
    fn parse_declaration_file() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "types.d.ts",
            "declare module 'foo' {\n  export function bar(): void;\n}\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].source_file.declaration_only);
    }

    #[test]
    fn parse_deeply_nested_code() {
        let dir = tempfile::tempdir().unwrap();
        let mut code = String::new();
        for i in 0..25 {
            code.push_str(&format!("{}if (true) {{\n", "  ".repeat(i)));
        }
        for i in (0..25).rev() {
            code.push_str(&format!("{}}}\n", "  ".repeat(i)));
        }
        let sf = write_fixture(dir.path(), "deep.ts", &code);

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn parse_file_with_only_comments() {
        let dir = tempfile::tempdir().unwrap();
        let sf = write_fixture(
            dir.path(),
            "comments.ts",
            "// just a comment\n/* block comment */\n",
        );

        let result = parse_files_sequential(&[sf], dir.path());
        assert!(result.warnings.is_empty());
        assert_eq!(result.files.len(), 1);
    }
}
