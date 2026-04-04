use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "corvalis-recon",
    version,
    about = "Structured codebase analysis for AI planning"
)]
pub struct Cli {
    /// Project root directory
    #[arg(long, default_value = ".", global = true)]
    pub root: PathBuf,

    /// Output format
    #[arg(long, default_value = "json", global = true)]
    pub format: OutputFormat,

    /// Include only files matching this glob
    #[arg(long, global = true)]
    pub include: Option<String>,

    /// Exclude files matching this glob
    #[arg(long, global = true)]
    pub exclude: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Full project analysis (symbols, dependencies, complexity, overview)
    Analyze {
        /// Approximate output token budget (chars / 4)
        #[arg(long)]
        budget: Option<usize>,
    },

    /// Extract symbols from specific files or the whole project
    Symbols {
        /// Specific files to inspect relative to the project root
        files: Vec<PathBuf>,
    },

    /// Build file-level dependency graph
    Deps,

    /// Calculate complexity metrics
    Complexity {
        /// Minimum cyclomatic complexity to include in output
        #[arg(long)]
        threshold: Option<u32>,

        /// Cyclomatic complexity threshold for hotspot detection
        #[arg(long)]
        complexity_threshold: Option<u32>,

        /// Max nesting depth threshold for hotspot detection
        #[arg(long)]
        nesting_threshold: Option<u32>,

        /// Lines of code threshold for hotspot detection
        #[arg(long)]
        loc_threshold: Option<u32>,

        /// Parameter count threshold for hotspot detection
        #[arg(long)]
        params_threshold: Option<u32>,
    },
}

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Pretty,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_analyze_with_defaults() {
        let cli = Cli::parse_from(["corvalis-recon", "analyze"]);
        assert_eq!(cli.root, PathBuf::from("."));
        assert_eq!(cli.format, OutputFormat::Json);
        assert!(matches!(
            cli.command,
            Some(Command::Analyze { budget: None })
        ));
    }

    #[test]
    fn parse_without_subcommand_defaults_to_none() {
        let cli = Cli::parse_from(["corvalis-recon"]);
        assert_eq!(cli.root, PathBuf::from("."));
        assert_eq!(cli.format, OutputFormat::Json);
        assert!(cli.command.is_none());
    }

    #[test]
    fn parse_analyze_with_budget() {
        let cli = Cli::parse_from(["corvalis-recon", "analyze", "--budget", "8000"]);
        assert!(matches!(
            cli.command,
            Some(Command::Analyze { budget: Some(8000) })
        ));
    }

    #[test]
    fn parse_symbols_with_root() {
        let cli = Cli::parse_from(["corvalis-recon", "--root", "/tmp/project", "symbols"]);
        assert_eq!(cli.root, PathBuf::from("/tmp/project"));
        assert!(matches!(
            cli.command,
            Some(Command::Symbols { ref files }) if files.is_empty()
        ));
    }

    #[test]
    fn parse_symbols_with_specific_files() {
        let cli = Cli::parse_from(["corvalis-recon", "symbols", "src/main.ts", "src/lib.ts"]);
        match cli.command {
            Some(Command::Symbols { files }) => {
                assert_eq!(
                    files,
                    vec![PathBuf::from("src/main.ts"), PathBuf::from("src/lib.ts")]
                );
            }
            _ => panic!("expected Symbols command"),
        }
    }

    #[test]
    fn parse_deps() {
        let cli = Cli::parse_from(["corvalis-recon", "deps"]);
        assert!(matches!(cli.command, Some(Command::Deps)));
    }

    #[test]
    fn parse_complexity_with_thresholds() {
        let cli = Cli::parse_from([
            "corvalis-recon",
            "complexity",
            "--threshold",
            "5",
            "--complexity-threshold",
            "15",
        ]);
        match cli.command {
            Some(Command::Complexity {
                threshold,
                complexity_threshold,
                ..
            }) => {
                assert_eq!(threshold, Some(5));
                assert_eq!(complexity_threshold, Some(15));
            }
            _ => panic!("expected Complexity command"),
        }
    }

    #[test]
    fn parse_pretty_format() {
        let cli = Cli::parse_from(["corvalis-recon", "--format", "pretty", "analyze"]);
        assert_eq!(cli.format, OutputFormat::Pretty);
    }

    #[test]
    fn parse_include_exclude() {
        let cli = Cli::parse_from([
            "corvalis-recon",
            "--include",
            "src/**",
            "--exclude",
            "*.test.ts",
            "symbols",
        ]);
        assert_eq!(cli.include.as_deref(), Some("src/**"));
        assert_eq!(cli.exclude.as_deref(), Some("*.test.ts"));
    }
}
