use std::path::Path;

use crate::output::DetectedLanguage;

/// Detect the source language from a file's extension.
///
/// Returns `None` for unsupported extensions.
/// Files ending in `.d.ts` are detected as TypeScript (flagged
/// as declaration-only by the caller).
pub fn detect_language(path: &Path) -> Option<DetectedLanguage> {
    let name = path.file_name()?.to_str()?;

    // Check `.d.ts` before general `.ts` — the two-part extension
    // would otherwise match as plain TypeScript.
    if name.ends_with(".d.ts") {
        return Some(DetectedLanguage::TypeScript);
    }

    match path.extension()?.to_str()? {
        "ts" => Some(DetectedLanguage::TypeScript),
        "tsx" => Some(DetectedLanguage::Tsx),
        "js" | "mjs" | "cjs" => Some(DetectedLanguage::JavaScript),
        "jsx" => Some(DetectedLanguage::Jsx),
        "svelte" => Some(DetectedLanguage::Svelte),
        _ => None,
    }
}

/// Check whether a file path represents a declaration-only file (`.d.ts`).
pub fn is_declaration_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".d.ts"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_typescript() {
        assert_eq!(
            detect_language(Path::new("src/main.ts")),
            Some(DetectedLanguage::TypeScript)
        );
    }

    #[test]
    fn detect_tsx() {
        assert_eq!(
            detect_language(Path::new("App.tsx")),
            Some(DetectedLanguage::Tsx)
        );
    }

    #[test]
    fn detect_javascript_variants() {
        for ext in ["js", "mjs", "cjs"] {
            let path = PathBuf::from(format!("file.{ext}"));
            assert_eq!(
                detect_language(&path),
                Some(DetectedLanguage::JavaScript),
                "failed for .{ext}"
            );
        }
    }

    #[test]
    fn detect_jsx() {
        assert_eq!(
            detect_language(Path::new("Component.jsx")),
            Some(DetectedLanguage::Jsx)
        );
    }

    #[test]
    fn detect_svelte() {
        assert_eq!(
            detect_language(Path::new("Button.svelte")),
            Some(DetectedLanguage::Svelte)
        );
    }

    #[test]
    fn detect_declaration_file_as_typescript() {
        assert_eq!(
            detect_language(Path::new("types/globals.d.ts")),
            Some(DetectedLanguage::TypeScript)
        );
    }

    #[test]
    fn is_declaration_true_for_d_ts() {
        assert!(is_declaration_file(Path::new("globals.d.ts")));
        assert!(is_declaration_file(Path::new("src/types/env.d.ts")));
    }

    #[test]
    fn is_declaration_false_for_regular_ts() {
        assert!(!is_declaration_file(Path::new("main.ts")));
        assert!(!is_declaration_file(Path::new("App.tsx")));
    }

    #[test]
    fn unsupported_extension_returns_none() {
        assert_eq!(detect_language(Path::new("readme.md")), None);
        assert_eq!(detect_language(Path::new("style.css")), None);
        assert_eq!(detect_language(Path::new("Cargo.toml")), None);
    }

    #[test]
    fn no_extension_returns_none() {
        assert_eq!(detect_language(Path::new("Makefile")), None);
    }

    #[test]
    fn hidden_file_with_supported_extension() {
        assert_eq!(
            detect_language(Path::new(".eslintrc.js")),
            Some(DetectedLanguage::JavaScript)
        );
    }
}
