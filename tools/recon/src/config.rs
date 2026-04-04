/// Directories always skipped during file discovery (in addition to .gitignore).
pub const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    ".git",
    "target",
    "__pycache__",
    ".next",
    "build",
];

/// Supported file extensions for analysis.
pub const SUPPORTED_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte"];

/// Maximum file size in bytes before skipping (1 MB).
pub const MAX_FILE_SIZE_BYTES: u64 = 1_048_576;

// Hotspot detection thresholds — functions exceeding these are flagged.

/// Cyclomatic complexity above this value is a hotspot.
pub const DEFAULT_COMPLEXITY_THRESHOLD: u32 = 10;

/// Max nesting depth above this value is a hotspot.
pub const DEFAULT_NESTING_THRESHOLD: u32 = 3;

/// Lines of code above this value is a hotspot.
pub const DEFAULT_LOC_THRESHOLD: u32 = 30;

/// Parameter count above this value is a hotspot.
pub const DEFAULT_PARAMS_THRESHOLD: u32 = 4;

/// Token estimation: approximate chars per token.
pub const CHARS_PER_TOKEN: usize = 4;

/// Headroom factor for token budget estimation (15%).
pub const BUDGET_HEADROOM: f64 = 0.85;

/// Declaration-only file extension.
pub const DECLARATION_EXTENSION: &str = "d.ts";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_dirs_includes_node_modules() {
        assert!(DEFAULT_SKIP_DIRS.contains(&"node_modules"));
    }

    #[test]
    fn supported_extensions_cover_all_target_languages() {
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte"] {
            assert!(
                SUPPORTED_EXTENSIONS.contains(&ext),
                "missing extension: {ext}"
            );
        }
    }

    #[test]
    fn max_file_size_is_one_megabyte() {
        assert_eq!(MAX_FILE_SIZE_BYTES, 1024 * 1024);
    }

    #[test]
    fn budget_headroom_leaves_room() {
        let headroom = BUDGET_HEADROOM;
        assert!(headroom > 0.0 && headroom < 1.0);
    }
}
