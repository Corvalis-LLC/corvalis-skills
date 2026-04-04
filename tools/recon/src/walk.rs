use std::path::Path;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;

use crate::config::{DEFAULT_SKIP_DIRS, MAX_FILE_SIZE_BYTES};
use crate::language::{detect_language, is_declaration_file};
use crate::output::{SourceFile, Warning};

/// Options controlling which files are discovered.
pub struct WalkOptions {
    pub include: Option<String>,
    pub exclude: Option<String>,
}

/// Result of walking the project directory.
#[derive(Debug)]
pub struct WalkResult {
    pub files: Vec<SourceFile>,
    pub warnings: Vec<Warning>,
}

/// Discover all supported source files under `root`, respecting .gitignore
/// and the default skip list.
///
/// Files larger than `MAX_FILE_SIZE_BYTES` are skipped with a warning.
/// Non-UTF8 filenames are skipped silently.
pub fn discover_files(root: &Path, options: &WalkOptions) -> Result<WalkResult> {
    let mut files = Vec::new();
    let mut warnings: Vec<Warning> = Vec::new();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false) // don't skip hidden files (e.g. .eslintrc.js)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);

    // Hard-skip directories from DEFAULT_SKIP_DIRS and apply user overrides.
    let mut overrides = OverrideBuilder::new(root);
    for dir in DEFAULT_SKIP_DIRS {
        overrides
            .add(&format!("!{dir}/"))
            .with_context(|| format!("invalid built-in skip override for directory '{dir}'"))?;
    }
    if let Some(ref include) = options.include {
        overrides
            .add(include)
            .with_context(|| format!("invalid include glob: {include}"))?;
    }
    if let Some(ref exclude) = options.exclude {
        overrides
            .add(&format!("!{exclude}"))
            .with_context(|| format!("invalid exclude glob: {exclude}"))?;
    }
    let built = overrides
        .build()
        .context("failed to build override globs")?;
    builder.overrides(built);

    for entry in builder.build().flatten() {
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        // Skip non-UTF8 filenames — we can't represent them in JSON output.
        if path.file_name().and_then(|n| n.to_str()).is_none() {
            continue;
        }

        // Filter by supported extension.
        let Some(language) = detect_language(path) else {
            continue;
        };

        // Check for binary content (null bytes in first 512 bytes).
        if is_likely_binary(path) {
            warnings.push(Warning {
                path: normalize_path(path, root),
                message: "skipped: file appears to be binary".into(),
            });
            continue;
        }

        // Check file size.
        if let Ok(metadata) = path.metadata()
            && metadata.len() > MAX_FILE_SIZE_BYTES
        {
            warnings.push(Warning {
                path: normalize_path(path, root),
                message: format!(
                    "skipped: file exceeds {} byte limit ({} bytes)",
                    MAX_FILE_SIZE_BYTES,
                    metadata.len()
                ),
            });
            continue;
        }

        let declaration_only = is_declaration_file(path);

        files.push(SourceFile {
            path: normalize_path(path, root),
            language,
            declaration_only,
        });
    }

    // Sort for deterministic output regardless of filesystem ordering.
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(WalkResult { files, warnings })
}

/// Normalize a path to forward-slash relative-to-root format.
fn normalize_path(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.to_string_lossy().replace('\\', "/")
}

/// Check if a file is likely binary by reading the first 512 bytes and
/// looking for null bytes.
fn is_likely_binary(path: &Path) -> bool {
    use std::fs::File;
    use std::io::Read;

    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 512];
    let Ok(n) = file.read(&mut buf) else {
        return false;
    };
    buf[..n].contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_fixture_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Supported files
        fs::write(root.join("main.ts"), "const x = 1;").unwrap();
        fs::write(root.join("app.tsx"), "export default function App() {}").unwrap();
        fs::write(root.join("utils.js"), "module.exports = {};").unwrap();
        fs::write(root.join("helper.mjs"), "export const h = 1;").unwrap();
        fs::write(root.join("legacy.cjs"), "module.exports = {};").unwrap();
        fs::write(root.join("Component.jsx"), "function Comp() {}").unwrap();
        fs::write(root.join("Button.svelte"), "<script>let x = 1;</script>").unwrap();
        fs::write(root.join("types.d.ts"), "declare module 'foo';").unwrap();

        // Unsupported files
        fs::write(root.join("readme.md"), "# Hello").unwrap();
        fs::write(root.join("style.css"), "body {}").unwrap();

        dir
    }

    #[test]
    fn discovers_all_supported_extensions() {
        let dir = setup_fixture_dir();
        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"main.ts"), "missing main.ts: {paths:?}");
        assert!(paths.contains(&"app.tsx"), "missing app.tsx: {paths:?}");
        assert!(paths.contains(&"utils.js"), "missing utils.js: {paths:?}");
        assert!(
            paths.contains(&"helper.mjs"),
            "missing helper.mjs: {paths:?}"
        );
        assert!(
            paths.contains(&"legacy.cjs"),
            "missing legacy.cjs: {paths:?}"
        );
        assert!(
            paths.contains(&"Component.jsx"),
            "missing Component.jsx: {paths:?}"
        );
        assert!(
            paths.contains(&"Button.svelte"),
            "missing Button.svelte: {paths:?}"
        );
        assert!(
            paths.contains(&"types.d.ts"),
            "missing types.d.ts: {paths:?}"
        );
    }

    #[test]
    fn excludes_unsupported_extensions() {
        let dir = setup_fixture_dir();
        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert!(
            !paths.contains(&"readme.md"),
            "should not include readme.md"
        );
        assert!(
            !paths.contains(&"style.css"),
            "should not include style.css"
        );
    }

    #[test]
    fn tags_declaration_files() {
        let dir = setup_fixture_dir();
        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        let dts = result
            .files
            .iter()
            .find(|f| f.path == "types.d.ts")
            .unwrap();
        assert!(dts.declaration_only);

        let ts = result.files.iter().find(|f| f.path == "main.ts").unwrap();
        assert!(!ts.declaration_only);
    }

    #[test]
    fn skips_oversized_files_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let big_file = dir.path().join("huge.ts");
        let content = "x".repeat((MAX_FILE_SIZE_BYTES + 1) as usize);
        fs::write(&big_file, content).unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert!(result.files.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("exceeds"));
    }

    #[test]
    fn skips_binary_files_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let bin_file = dir.path().join("bundle.js");
        let mut content = b"var x = 1;\0\0\0binary garbage".to_vec();
        content.extend_from_slice(b"\0\0");
        fs::write(&bin_file, content).unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert!(result.files.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].message.contains("binary"));
    }

    #[test]
    fn empty_directory_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert!(result.files.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn output_paths_use_forward_slashes() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("index.ts"), "export {};").unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "src/index.ts");
        assert!(!result.files[0].path.contains('\\'));
    }

    #[test]
    fn files_are_sorted_by_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("z.ts"), "").unwrap();
        fs::write(dir.path().join("a.ts"), "").unwrap();
        fs::write(dir.path().join("m.ts"), "").unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        let paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.ts", "m.ts", "z.ts"]);
    }

    #[test]
    fn empty_files_are_included() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("empty.ts"), "").unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "empty.ts");
    }

    #[test]
    fn subdirectory_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("src").join("components").join("ui");
        fs::create_dir_all(&deep).unwrap();
        fs::write(deep.join("Button.tsx"), "export {};").unwrap();

        let result = discover_files(
            dir.path(),
            &WalkOptions {
                include: None,
                exclude: None,
            },
        )
        .unwrap();

        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "src/components/ui/Button.tsx");
    }

    #[test]
    fn invalid_include_glob_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = discover_files(
            dir.path(),
            &WalkOptions {
                include: Some("[".into()),
                exclude: None,
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid include glob"));
    }
}
