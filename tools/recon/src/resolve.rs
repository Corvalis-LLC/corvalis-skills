use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Resolved result for an import specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedImport {
    /// Resolved to a project-internal file (forward-slash relative path).
    ProjectFile(String),
    /// External package (bare specifier like `react`, `lodash/get`).
    External(String),
    /// Could not resolve to a file (relative import with no matching file).
    Unresolved(String),
}

/// Path alias mappings from tsconfig.json `compilerOptions.paths`.
#[derive(Debug, Default, Clone)]
pub struct PathAliases {
    pub base_url: Option<PathBuf>,
    pub config_dir: PathBuf,
    pub mappings: Vec<(String, Vec<String>)>,
}

/// Resolve an import source string to a project file, external package, or unresolved.
///
/// Resolution strategy:
/// - Relative imports (`./`, `../`): resolve against the importing file's directory
/// - Bare specifiers: marked as external
/// - Path aliases: applied if provided, then resolved as relative
///
/// Resolution order for relative imports:
/// 1. Exact file match
/// 2. Add extensions (.ts, .tsx, .js, .jsx, .mjs)
/// 3. Directory index files (index.ts, index.tsx, index.js, index.jsx, index.mjs)
pub fn resolve_import(
    import_source: &str,
    importing_file: &str,
    root: &Path,
    aliases: &PathAliases,
) -> ResolvedImport {
    if import_source.starts_with("./") || import_source.starts_with("../") {
        resolve_relative(import_source, importing_file, root)
    } else if let Some(resolved) = try_resolve_alias(import_source, root, aliases) {
        resolved
    } else {
        ResolvedImport::External(import_source.to_string())
    }
}

/// Try to apply path alias mappings from tsconfig.
fn try_resolve_alias(
    import_source: &str,
    root: &Path,
    aliases: &PathAliases,
) -> Option<ResolvedImport> {
    for (pattern, replacements) in &aliases.mappings {
        let matched = if let Some(prefix) = pattern.strip_suffix('*') {
            import_source
                .strip_prefix(prefix)
                .map(|rest| (prefix, rest))
        } else if pattern == import_source {
            Some((pattern.as_str(), ""))
        } else {
            None
        };

        if let Some((_prefix, rest)) = matched {
            for replacement in replacements {
                let resolved_path = if let Some(rep_prefix) = replacement.strip_suffix('*') {
                    format!("{rep_prefix}{rest}")
                } else {
                    replacement.clone()
                };

                // Resolve relative to the tsconfig directory plus baseUrl.
                let base = if let Some(base_url) = aliases.base_url.as_deref() {
                    aliases.config_dir.join(base_url)
                } else if aliases.config_dir.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    aliases.config_dir.clone()
                };
                let relative_base = base.strip_prefix(root).unwrap_or(&base);
                let relative_source = format!(
                    "./{}",
                    relative_base
                        .join(&resolved_path)
                        .to_string_lossy()
                        .replace('\\', "/")
                );

                let result = resolve_relative(&relative_source, "", root);
                if matches!(result, ResolvedImport::ProjectFile(_)) {
                    return Some(result);
                }
            }
        }
    }
    None
}

fn resolve_relative(import_source: &str, importing_file: &str, root: &Path) -> ResolvedImport {
    let importing_dir = Path::new(importing_file).parent().unwrap_or(Path::new(""));

    let raw_target = importing_dir.join(import_source);

    // Normalize the path (resolve `.` and `..` components).
    let normalized = normalize_path(&raw_target);
    let normalized_str = normalized.to_string_lossy().replace('\\', "/");

    // 1. Exact file match.
    let abs = root.join(&*normalized_str);
    if abs.is_file() {
        return ResolvedImport::ProjectFile(normalized_str);
    }

    // 2. Try adding extensions.
    static RESOLVE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];
    for ext in RESOLVE_EXTENSIONS {
        let with_ext = format!("{normalized_str}.{ext}");
        if root.join(&with_ext).is_file() {
            return ResolvedImport::ProjectFile(with_ext);
        }
    }

    // 3. Try directory index files.
    for ext in RESOLVE_EXTENSIONS {
        let index = format!("{normalized_str}/index.{ext}");
        if root.join(&index).is_file() {
            return ResolvedImport::ProjectFile(index);
        }
    }

    ResolvedImport::Unresolved(import_source.to_string())
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !components.is_empty() {
                    components.pop();
                }
            }
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Load path alias configuration from the nearest tsconfig.json.
///
/// Walks up from `start_dir` looking for `tsconfig.json`. Uses the `json5` crate
/// to handle comments and trailing commas (standard in real tsconfig files).
///
/// Returns `PathAliases::default()` if no tsconfig found or parsing fails.
pub fn load_tsconfig_aliases(root: &Path) -> PathAliases {
    let mut cache = HashMap::new();
    load_tsconfig_aliases_for_file(root, "", &mut cache)
}

pub fn load_tsconfig_aliases_for_file(
    root: &Path,
    importing_file: &str,
    cache: &mut HashMap<PathBuf, PathAliases>,
) -> PathAliases {
    let search_dir = if importing_file.is_empty() {
        root.to_path_buf()
    } else {
        root.join(importing_file)
            .parent()
            .unwrap_or(root)
            .to_path_buf()
    };

    load_tsconfig_from_dir(&search_dir, cache)
}

fn load_tsconfig_from_dir(
    search_dir: &Path,
    cache: &mut HashMap<PathBuf, PathAliases>,
) -> PathAliases {
    if let Some(cached) = cache.get(search_dir) {
        return cached.clone();
    }

    let tsconfig_path = search_dir.join("tsconfig.json");
    let mut aliases = if tsconfig_path.is_file() {
        match std::fs::read_to_string(&tsconfig_path) {
            Ok(content) => parse_tsconfig_aliases(&content).unwrap_or_default(),
            Err(_) => PathAliases::default(),
        }
    } else {
        // Walk up to parent, but stop at filesystem root.
        match search_dir.parent() {
            Some(parent) if parent != search_dir => load_tsconfig_from_dir(parent, cache),
            _ => PathAliases::default(),
        }
    };

    if aliases.config_dir.as_os_str().is_empty() {
        aliases.config_dir = search_dir.to_path_buf();
    }

    cache.insert(search_dir.to_path_buf(), aliases.clone());
    aliases
}

/// Parse path aliases from tsconfig JSON content.
fn parse_tsconfig_aliases(content: &str) -> Option<PathAliases> {
    let value: serde_json::Value = json5::from_str(content).ok()?;
    let compiler_options = value.get("compilerOptions")?;

    let base_url = compiler_options
        .get("baseUrl")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let paths_obj = compiler_options.get("paths")?.as_object()?;
    let mut mappings = Vec::new();
    for (pattern, targets) in paths_obj {
        let targets: Vec<String> = targets
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        if !targets.is_empty() {
            mappings.push((pattern.clone(), targets));
        }
    }

    Some(PathAliases {
        base_url,
        config_dir: PathBuf::new(),
        mappings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_file(root: &Path, rel_path: &str) {
        let full = root.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, "// placeholder").unwrap();
    }

    // --- Relative import resolution ---

    #[test]
    fn resolve_relative_with_extension_already_present() {
        let dir = setup_dir();
        write_file(dir.path(), "src/utils.ts");

        let result = resolve_import(
            "./utils.ts",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/utils.ts".into()));
    }

    #[test]
    fn resolve_relative_adds_ts_extension() {
        let dir = setup_dir();
        write_file(dir.path(), "src/utils.ts");

        let result = resolve_import(
            "./utils",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/utils.ts".into()));
    }

    #[test]
    fn resolve_relative_adds_tsx_extension() {
        let dir = setup_dir();
        write_file(dir.path(), "src/Button.tsx");

        let result = resolve_import(
            "./Button",
            "src/App.tsx",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/Button.tsx".into()));
    }

    #[test]
    fn resolve_relative_index_file() {
        let dir = setup_dir();
        write_file(dir.path(), "src/components/index.ts");

        let result = resolve_import(
            "./components",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(
            result,
            ResolvedImport::ProjectFile("src/components/index.ts".into())
        );
    }

    #[test]
    fn resolve_parent_directory_import() {
        let dir = setup_dir();
        write_file(dir.path(), "src/utils.ts");

        let result = resolve_import(
            "../utils",
            "src/components/Button.tsx",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/utils.ts".into()));
    }

    #[test]
    fn resolve_unresolved_relative() {
        let dir = setup_dir();

        let result = resolve_import(
            "./nonexistent",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::Unresolved("./nonexistent".into()));
    }

    // --- Bare specifiers (external packages) ---

    #[test]
    fn bare_specifier_marked_as_external() {
        let dir = setup_dir();

        let result = resolve_import("react", "src/main.ts", dir.path(), &PathAliases::default());
        assert_eq!(result, ResolvedImport::External("react".into()));
    }

    #[test]
    fn scoped_package_marked_as_external() {
        let dir = setup_dir();

        let result = resolve_import(
            "@tanstack/react-query",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(
            result,
            ResolvedImport::External("@tanstack/react-query".into())
        );
    }

    #[test]
    fn deep_package_path_marked_as_external() {
        let dir = setup_dir();

        let result = resolve_import(
            "lodash/get",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::External("lodash/get".into()));
    }

    // --- tsconfig path aliases ---

    #[test]
    fn resolve_path_alias_with_wildcard() {
        let dir = setup_dir();
        write_file(dir.path(), "src/utils/helpers.ts");
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["src/*"] } } }"#,
        )
        .unwrap();

        let aliases = load_tsconfig_aliases(dir.path());
        let result = resolve_import("@/utils/helpers", "src/main.ts", dir.path(), &aliases);
        assert_eq!(
            result,
            ResolvedImport::ProjectFile("src/utils/helpers.ts".into())
        );
    }

    #[test]
    fn resolve_path_alias_without_wildcard() {
        let dir = setup_dir();
        write_file(dir.path(), "src/config/index.ts");
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "config": ["src/config/index.ts"] } } }"#,
        ).unwrap();

        let aliases = load_tsconfig_aliases(dir.path());
        let result = resolve_import("config", "src/main.ts", dir.path(), &aliases);
        assert_eq!(
            result,
            ResolvedImport::ProjectFile("src/config/index.ts".into())
        );
    }

    #[test]
    fn resolve_path_alias_with_base_url() {
        let dir = setup_dir();
        write_file(dir.path(), "src/lib/utils.ts");
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": "src", "paths": { "lib/*": ["lib/*"] } } }"#,
        )
        .unwrap();

        let aliases = load_tsconfig_aliases(dir.path());
        let result = resolve_import("lib/utils", "src/main.ts", dir.path(), &aliases);
        assert_eq!(
            result,
            ResolvedImport::ProjectFile("src/lib/utils.ts".into())
        );
    }

    #[test]
    fn missing_tsconfig_returns_default_aliases() {
        let dir = setup_dir();
        let aliases = load_tsconfig_aliases(dir.path());
        assert!(aliases.mappings.is_empty());
        assert!(aliases.base_url.is_none());
    }

    #[test]
    fn tsconfig_with_comments_parses_via_json5() {
        let dir = setup_dir();
        write_file(dir.path(), "src/utils.ts");
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{
                // This is a comment
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": {
                        "@/*": ["src/*"], // trailing comma
                    },
                },
            }"#,
        )
        .unwrap();

        let aliases = load_tsconfig_aliases(dir.path());
        assert_eq!(aliases.mappings.len(), 1);
        let result = resolve_import("@/utils", "main.ts", dir.path(), &aliases);
        assert_eq!(result, ResolvedImport::ProjectFile("src/utils.ts".into()));
    }

    // --- Edge cases ---

    #[test]
    fn resolve_extension_priority_ts_before_tsx() {
        let dir = setup_dir();
        write_file(dir.path(), "src/Component.ts");
        write_file(dir.path(), "src/Component.tsx");

        let result = resolve_import(
            "./Component",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        // .ts should be tried before .tsx
        assert_eq!(
            result,
            ResolvedImport::ProjectFile("src/Component.ts".into())
        );
    }

    #[test]
    fn resolve_index_file_only_when_no_direct_file() {
        let dir = setup_dir();
        write_file(dir.path(), "src/lib.ts");
        write_file(dir.path(), "src/lib/index.ts");

        let result = resolve_import("./lib", "src/main.ts", dir.path(), &PathAliases::default());
        // Direct file match takes priority over directory/index
        assert_eq!(result, ResolvedImport::ProjectFile("src/lib.ts".into()));
    }

    #[test]
    fn normalize_path_resolves_dot_and_dotdot() {
        let result = normalize_path(Path::new("src/./components/../utils"));
        assert_eq!(result, PathBuf::from("src/utils"));
    }

    #[test]
    fn resolve_from_root_level_file() {
        let dir = setup_dir();
        write_file(dir.path(), "utils.ts");

        let result = resolve_import("./utils", "main.ts", dir.path(), &PathAliases::default());
        assert_eq!(result, ResolvedImport::ProjectFile("utils.ts".into()));
    }

    #[test]
    fn resolve_js_extension() {
        let dir = setup_dir();
        write_file(dir.path(), "src/helpers.js");

        let result = resolve_import(
            "./helpers",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/helpers.js".into()));
    }

    #[test]
    fn resolve_mjs_extension() {
        let dir = setup_dir();
        write_file(dir.path(), "src/config.mjs");

        let result = resolve_import(
            "./config",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/config.mjs".into()));
    }

    #[test]
    fn resolve_cjs_extension() {
        let dir = setup_dir();
        write_file(dir.path(), "src/config.cjs");

        let result = resolve_import(
            "./config",
            "src/main.ts",
            dir.path(),
            &PathAliases::default(),
        );
        assert_eq!(result, ResolvedImport::ProjectFile("src/config.cjs".into()));
    }

    #[test]
    fn nearest_tsconfig_wins_for_nested_package() {
        let dir = setup_dir();
        write_file(dir.path(), "packages/app/src/shared/util.ts");
        write_file(dir.path(), "src/root-only.ts");

        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@shared/*": ["src/*"] } } }"#,
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("packages/app")).unwrap();
        fs::write(
            dir.path().join("packages/app/tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@shared/*": ["src/shared/*"] } } }"#,
        )
        .unwrap();

        let mut cache = HashMap::new();
        let aliases =
            load_tsconfig_aliases_for_file(dir.path(), "packages/app/src/feature.ts", &mut cache);
        let result = resolve_import(
            "@shared/util",
            "packages/app/src/feature.ts",
            dir.path(),
            &aliases,
        );

        assert_eq!(
            result,
            ResolvedImport::ProjectFile("packages/app/src/shared/util.ts".into())
        );
    }
}
