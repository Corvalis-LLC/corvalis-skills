use std::collections::BTreeMap;
use std::path::Path;

use crate::output::{DirectoryEntry, FileAnalysis, LanguageStats, ProjectOverview, SourceFile};

/// Known config files to detect in the project root.
const CONFIG_FILE_NAMES: &[&str] = &[
    "tsconfig.json",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    ".env",
    ".env.local",
    "Dockerfile",
    "docker-compose.yml",
    "docker-compose.yaml",
    ".eslintrc.js",
    ".eslintrc.json",
    "eslint.config.js",
    "eslint.config.mjs",
    "vite.config.ts",
    "vite.config.js",
    "svelte.config.js",
    "next.config.js",
    "next.config.mjs",
    "tailwind.config.js",
    "tailwind.config.ts",
    ".prettierrc",
    ".prettierrc.json",
    "vitest.config.ts",
    "jest.config.ts",
    "jest.config.js",
];

/// Build project overview metadata from discovered files and analysis results.
pub fn build_overview(
    root: &Path,
    source_files: &[SourceFile],
    file_analyses: &[FileAnalysis],
    entry_points: &[String],
) -> ProjectOverview {
    let name = detect_project_name(root);
    let root_str = normalize_path(root);
    let languages = compute_language_stats(source_files, file_analyses);
    let config_files = detect_config_files(root);
    let directory_tree = build_directory_tree(source_files);

    ProjectOverview {
        name,
        root: root_str,
        languages,
        entry_points: entry_points.to_vec(),
        config_files,
        directory_tree,
    }
}

/// Detect project name from package.json `name` field, falling back to directory name.
fn detect_project_name(root: &Path) -> String {
    let package_json = root.join("package.json");
    if let Ok(contents) = std::fs::read_to_string(&package_json)
        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&contents)
        && let Some(name) = parsed.get("name").and_then(|v| v.as_str())
        && !name.is_empty()
    {
        return name.to_string();
    }

    root.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Compute language breakdown: file count and LOC per language.
fn compute_language_stats(
    source_files: &[SourceFile],
    file_analyses: &[FileAnalysis],
) -> BTreeMap<String, LanguageStats> {
    let mut stats: BTreeMap<String, LanguageStats> = BTreeMap::new();

    // Build a lookup from path to metrics LOC.
    let loc_by_path: BTreeMap<&str, usize> = file_analyses
        .iter()
        .map(|fa| (fa.path.as_str(), fa.metrics.code_lines))
        .collect();

    for sf in source_files {
        let lang = sf.language.as_str().to_string();
        let entry = stats.entry(lang).or_insert(LanguageStats {
            file_count: 0,
            lines_of_code: 0,
        });
        entry.file_count += 1;
        entry.lines_of_code += loc_by_path.get(sf.path.as_str()).copied().unwrap_or(0);
    }

    stats
}

/// Detect known config files present in the project root.
fn detect_config_files(root: &Path) -> Vec<String> {
    let mut found: Vec<String> = CONFIG_FILE_NAMES
        .iter()
        .filter(|name| root.join(name).exists())
        .map(|name| name.to_string())
        .collect();
    found.sort();
    found
}

/// Build a summary of top-level directories with file counts.
fn build_directory_tree(source_files: &[SourceFile]) -> Vec<DirectoryEntry> {
    let mut dir_counts: BTreeMap<String, usize> = BTreeMap::new();

    for sf in source_files {
        let top_dir = top_level_directory(&sf.path);
        *dir_counts.entry(top_dir).or_insert(0) += 1;
    }

    let mut entries: Vec<DirectoryEntry> = dir_counts
        .into_iter()
        .map(|(path, file_count)| DirectoryEntry { path, file_count })
        .collect();
    entries.sort_by(|a, b| b.file_count.cmp(&a.file_count).then(a.path.cmp(&b.path)));
    entries
}

/// Extract the top-level directory from a relative path, or "." for root-level files.
fn top_level_directory(path: &str) -> String {
    match path.find('/') {
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

/// Normalize a path to forward slashes.
fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{DetectedLanguage, FileMetrics};
    use std::fs;

    fn make_source_file(path: &str, lang: DetectedLanguage) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language: lang,
            declaration_only: false,
        }
    }

    fn make_file_analysis(path: &str, language: &str, code_lines: usize) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            language: language.to_string(),
            symbols: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            metrics: FileMetrics {
                code_lines,
                ..Default::default()
            },
        }
    }

    #[test]
    fn detect_project_name_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "my-cool-project"}"#,
        )
        .unwrap();

        let name = detect_project_name(dir.path());
        assert_eq!(name, "my-cool-project");
    }

    #[test]
    fn detect_project_name_falls_back_to_dir_name() {
        let dir = tempfile::tempdir().unwrap();
        let name = detect_project_name(dir.path());
        // Should be the temp dir name, not empty.
        assert!(!name.is_empty());
        assert_ne!(name, "unknown");
    }

    #[test]
    fn detect_project_name_with_empty_package_name() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name": ""}"#).unwrap();

        let name = detect_project_name(dir.path());
        // Empty name should fall back to directory name.
        assert!(!name.is_empty());
        assert_ne!(name, "");
    }

    #[test]
    fn detect_project_name_with_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "not valid json").unwrap();

        let name = detect_project_name(dir.path());
        // Should fall back to directory name without panicking.
        assert!(!name.is_empty());
    }

    #[test]
    fn compute_language_stats_groups_by_language() {
        let source_files = vec![
            make_source_file("src/a.ts", DetectedLanguage::TypeScript),
            make_source_file("src/b.ts", DetectedLanguage::TypeScript),
            make_source_file("src/c.js", DetectedLanguage::JavaScript),
        ];
        let analyses = vec![
            make_file_analysis("src/a.ts", "typescript", 100),
            make_file_analysis("src/b.ts", "typescript", 50),
            make_file_analysis("src/c.js", "javascript", 30),
        ];

        let stats = compute_language_stats(&source_files, &analyses);

        assert_eq!(stats["typescript"].file_count, 2);
        assert_eq!(stats["typescript"].lines_of_code, 150);
        assert_eq!(stats["javascript"].file_count, 1);
        assert_eq!(stats["javascript"].lines_of_code, 30);
    }

    #[test]
    fn compute_language_stats_empty_input() {
        let stats = compute_language_stats(&[], &[]);
        assert!(stats.is_empty());
    }

    #[test]
    fn detect_config_files_finds_existing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();

        let configs = detect_config_files(dir.path());
        assert!(configs.contains(&"tsconfig.json".to_string()));
        assert!(configs.contains(&"package.json".to_string()));
    }

    #[test]
    fn detect_config_files_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let configs = detect_config_files(dir.path());
        assert!(configs.is_empty());
    }

    #[test]
    fn build_directory_tree_groups_by_top_level() {
        let source_files = vec![
            make_source_file("src/a.ts", DetectedLanguage::TypeScript),
            make_source_file("src/b.ts", DetectedLanguage::TypeScript),
            make_source_file("src/sub/c.ts", DetectedLanguage::TypeScript),
            make_source_file("lib/d.ts", DetectedLanguage::TypeScript),
            make_source_file("root.ts", DetectedLanguage::TypeScript),
        ];

        let tree = build_directory_tree(&source_files);

        let src = tree.iter().find(|d| d.path == "src").unwrap();
        assert_eq!(src.file_count, 3); // a.ts, b.ts, sub/c.ts all under "src"
        let lib = tree.iter().find(|d| d.path == "lib").unwrap();
        assert_eq!(lib.file_count, 1);
        let root = tree.iter().find(|d| d.path == ".").unwrap();
        assert_eq!(root.file_count, 1);
    }

    #[test]
    fn build_directory_tree_sorted_by_count_descending() {
        let source_files = vec![
            make_source_file("big/a.ts", DetectedLanguage::TypeScript),
            make_source_file("big/b.ts", DetectedLanguage::TypeScript),
            make_source_file("big/c.ts", DetectedLanguage::TypeScript),
            make_source_file("small/a.ts", DetectedLanguage::TypeScript),
        ];

        let tree = build_directory_tree(&source_files);
        assert_eq!(tree[0].path, "big");
        assert_eq!(tree[1].path, "small");
    }

    #[test]
    fn top_level_directory_extracts_first_segment() {
        assert_eq!(top_level_directory("src/foo/bar.ts"), "src");
        assert_eq!(top_level_directory("lib/utils.ts"), "lib");
        assert_eq!(top_level_directory("root.ts"), ".");
    }

    #[test]
    fn build_overview_assembles_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "test-project"}"#,
        )
        .unwrap();
        fs::write(dir.path().join("tsconfig.json"), "{}").unwrap();

        let source_files = vec![make_source_file(
            "src/main.ts",
            DetectedLanguage::TypeScript,
        )];
        let analyses = vec![make_file_analysis("src/main.ts", "typescript", 42)];
        let entry_points = vec!["src/main.ts".to_string()];

        let overview = build_overview(dir.path(), &source_files, &analyses, &entry_points);

        assert_eq!(overview.name, "test-project");
        assert!(!overview.root.is_empty());
        assert_eq!(overview.languages["typescript"].file_count, 1);
        assert_eq!(overview.languages["typescript"].lines_of_code, 42);
        assert_eq!(overview.entry_points, vec!["src/main.ts"]);
        assert!(overview.config_files.contains(&"package.json".to_string()));
        assert!(overview.config_files.contains(&"tsconfig.json".to_string()));
        assert!(!overview.directory_tree.is_empty());
    }
}
