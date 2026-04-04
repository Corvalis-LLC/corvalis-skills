use std::collections::BTreeMap;

use serde::Serialize;

/// Top-level output for the `analyze` command.
///
/// ```json
/// {
///   "version": "0.1.0",
///   "project": { ... },
///   "files": [{ "path": "src/main.ts", "language": "typescript", ... }],
///   "graph": { ... },
///   "hotspots": [{ ... }],
///   "summary": { ... }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub version: String,
    pub project: ProjectOverview,
    pub files: Vec<FileAnalysis>,
    pub graph: DependencyGraph,
    pub hotspots: Vec<Hotspot>,
    pub warnings: Vec<Warning>,
    pub summary: Summary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectOverview {
    pub name: String,
    pub root: String,
    pub languages: BTreeMap<String, LanguageStats>,
    pub entry_points: Vec<String>,
    pub config_files: Vec<String>,
    pub directory_tree: Vec<DirectoryEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageStats {
    pub file_count: usize,
    pub lines_of_code: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectoryEntry {
    pub path: String,
    pub file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileAnalysis {
    pub path: String,
    pub language: String,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<Import>,
    pub exports: Vec<Export>,
    pub metrics: FileMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub end_line: usize,
    pub exported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    ArrowFunction,
    Method,
    Class,
    Interface,
    TypeAlias,
    Enum,
    Variable,
    Component,
    Rune,
}

#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub source: String,
    pub specifiers: Vec<String>,
    pub kind: ImportKind,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Named,
    Default,
    Namespace,
    SideEffect,
    TypeOnly,
    Dynamic,
}

#[derive(Debug, Clone, Serialize)]
pub struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Named,
    Default,
    ReExport,
    StarReExport,
    TypeOnly,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct FileMetrics {
    pub total_lines: usize,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub blank_lines: usize,
    pub cyclomatic_complexity: u32,
    pub max_nesting_depth: u32,
    pub functions: Vec<FunctionMetrics>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionMetrics {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub lines_of_code: usize,
    pub cyclomatic_complexity: u32,
    pub max_nesting_depth: u32,
    pub parameter_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyGraph {
    pub adjacency: BTreeMap<String, Vec<Dependency>>,
    pub entry_points: Vec<String>,
    pub leaf_nodes: Vec<String>,
    pub cycles: Vec<Vec<String>>,
    pub stats: GraphStats,
}

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub target: String,
    pub specifiers: Vec<String>,
    pub resolved: bool,
    pub external: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct GraphStats {
    pub total_files: usize,
    pub total_edges: usize,
    pub avg_dependencies: f64,
    pub max_dependencies: usize,
    pub avg_dependents: f64,
    pub max_dependents: usize,
    pub most_imported: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    pub path: String,
    pub function: String,
    pub metric: String,
    pub value: u32,
    pub threshold: u32,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct Summary {
    pub total_files: usize,
    pub total_symbols: usize,
    pub total_lines_of_code: usize,
    pub avg_complexity: f64,
}

/// Warning emitted during analysis (non-fatal).
#[derive(Debug, Clone, Serialize)]
pub struct Warning {
    pub path: String,
    pub message: String,
}

/// Intermediate representation for a discovered source file.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub language: DetectedLanguage,
    pub declaration_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectedLanguage {
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Svelte,
}

impl DetectedLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Jsx => "jsx",
            Self::Svelte => "svelte",
        }
    }
}

impl AnalysisResult {
    pub fn empty() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            project: ProjectOverview {
                name: String::new(),
                root: String::new(),
                languages: BTreeMap::new(),
                entry_points: Vec::new(),
                config_files: Vec::new(),
                directory_tree: Vec::new(),
            },
            files: Vec::new(),
            graph: DependencyGraph {
                adjacency: BTreeMap::new(),
                entry_points: Vec::new(),
                leaf_nodes: Vec::new(),
                cycles: Vec::new(),
                stats: GraphStats::default(),
            },
            hotspots: Vec::new(),
            warnings: Vec::new(),
            summary: Summary::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_kind_serializes_as_snake_case() {
        let kind = SymbolKind::ArrowFunction;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""arrow_function""#);
    }

    #[test]
    fn import_kind_serializes_as_snake_case() {
        let kind = ImportKind::SideEffect;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""side_effect""#);
    }

    #[test]
    fn export_kind_serializes_as_snake_case() {
        let kind = ExportKind::StarReExport;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""star_re_export""#);
    }

    #[test]
    fn detected_language_as_str() {
        assert_eq!(DetectedLanguage::TypeScript.as_str(), "typescript");
        assert_eq!(DetectedLanguage::Tsx.as_str(), "tsx");
        assert_eq!(DetectedLanguage::Svelte.as_str(), "svelte");
    }

    #[test]
    fn empty_analysis_result_serializes_to_valid_json() {
        let result = AnalysisResult::empty();
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""version""#));
        assert!(json.contains(r#""files":[]"#));
        assert!(json.contains(r#""warnings":[]"#));

        // Verify it round-trips as valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_object());
    }

    #[test]
    fn optional_signature_omitted_when_none() {
        let sym = Symbol {
            name: "foo".into(),
            kind: SymbolKind::Function,
            line: 1,
            end_line: 5,
            exported: false,
            signature: None,
        };
        let json = serde_json::to_string(&sym).unwrap();
        assert!(!json.contains("signature"));
    }

    #[test]
    fn optional_signature_included_when_some() {
        let sym = Symbol {
            name: "bar".into(),
            kind: SymbolKind::Function,
            line: 1,
            end_line: 10,
            exported: true,
            signature: Some("(x: number) => string".into()),
        };
        let json = serde_json::to_string(&sym).unwrap();
        assert!(json.contains(r#""signature":"(x: number) => string""#));
    }

    #[test]
    fn btreemap_produces_sorted_keys() {
        let mut languages: BTreeMap<String, LanguageStats> = BTreeMap::new();
        languages.insert(
            "typescript".into(),
            LanguageStats {
                file_count: 10,
                lines_of_code: 500,
            },
        );
        languages.insert(
            "javascript".into(),
            LanguageStats {
                file_count: 3,
                lines_of_code: 100,
            },
        );
        let json = serde_json::to_string(&languages).unwrap();
        let js_pos = json.find("javascript").unwrap();
        let ts_pos = json.find("typescript").unwrap();
        assert!(js_pos < ts_pos, "BTreeMap should produce sorted keys");
    }
}
