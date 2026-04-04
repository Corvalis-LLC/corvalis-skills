use anyhow::{Context, Result};
use clap::Parser;

use corvalis_recon::analyze;
use corvalis_recon::cli::{Cli, Command, OutputFormat};
use corvalis_recon::deps;
use corvalis_recon::metrics::{self, HotspotThresholds};
use corvalis_recon::output::Hotspot;
use corvalis_recon::parse;
use corvalis_recon::resolve;
use corvalis_recon::walk::{self, WalkOptions};

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Analyze { budget: None }) {
        Command::Analyze { budget } => {
            let walk_options = WalkOptions {
                include: cli.include,
                exclude: cli.exclude,
            };
            let output = analyze::analyze_project(&cli.root, &walk_options, budget)?;

            match cli.format {
                OutputFormat::Json => print_json(&output.result, &cli.format)?,
                OutputFormat::Pretty => println!("{}", output.pretty),
            }
        }
        Command::Symbols { files: file_args } => {
            let walk_options = WalkOptions {
                include: cli.include,
                exclude: cli.exclude,
            };

            let source_files = if file_args.is_empty() {
                let walk_result = walk::discover_files(&cli.root, &walk_options)
                    .context("discovering source files")?;
                for w in &walk_result.warnings {
                    eprintln!("warning: {}", w.message);
                }
                walk_result.files
            } else {
                // Filter specific files through language detection.
                file_args
                    .iter()
                    .filter_map(|p| {
                        let lang = corvalis_recon::language::detect_language(p)?;
                        let path_str = p.to_string_lossy().replace('\\', "/");
                        Some(corvalis_recon::output::SourceFile {
                            path: path_str,
                            language: lang,
                            declaration_only: corvalis_recon::language::is_declaration_file(p),
                        })
                    })
                    .collect()
            };

            let parse_result = parse::parse_files(&source_files, &cli.root);
            for w in &parse_result.warnings {
                eprintln!("warning: {}: {}", w.path, w.message);
            }

            let mut output: Vec<serde_json::Value> = Vec::new();
            for parsed in &parse_result.files {
                let syms = corvalis_recon::symbols::extract_symbols(parsed);
                output.push(serde_json::json!({
                    "path": parsed.source_file.path,
                    "language": parsed.source_file.language.as_str(),
                    "symbols": syms.symbols,
                    "imports": syms.imports,
                    "exports": syms.exports,
                }));
            }

            print_json(&output, &cli.format)?;
        }
        Command::Deps => {
            let walk_options = WalkOptions {
                include: cli.include,
                exclude: cli.exclude,
            };
            let walk_result = walk::discover_files(&cli.root, &walk_options)
                .context("discovering source files")?;

            for warning in &walk_result.warnings {
                eprintln!("warning: {}", warning.message);
            }

            let parse_result = parse::parse_files(&walk_result.files, &cli.root);

            for warning in &parse_result.warnings {
                eprintln!("warning: {}: {}", warning.path, warning.message);
            }

            let aliases = resolve::load_tsconfig_aliases(&cli.root);
            let graph = deps::build_dependency_graph(&parse_result.files, &cli.root, &aliases);

            print_json(&graph, &cli.format)?;
        }
        Command::Complexity {
            threshold,
            complexity_threshold,
            nesting_threshold,
            loc_threshold,
            params_threshold,
        } => {
            let walk_options = WalkOptions {
                include: cli.include,
                exclude: cli.exclude,
            };
            let walk_result = walk::discover_files(&cli.root, &walk_options)
                .context("discovering source files")?;

            for warning in &walk_result.warnings {
                eprintln!("warning: {}", warning.message);
            }

            let parse_result = parse::parse_files(&walk_result.files, &cli.root);

            for warning in &parse_result.warnings {
                eprintln!("warning: {}: {}", warning.path, warning.message);
            }

            let thresholds = HotspotThresholds {
                complexity: complexity_threshold
                    .unwrap_or(corvalis_recon::config::DEFAULT_COMPLEXITY_THRESHOLD),
                nesting: nesting_threshold
                    .unwrap_or(corvalis_recon::config::DEFAULT_NESTING_THRESHOLD),
                loc: loc_threshold.unwrap_or(corvalis_recon::config::DEFAULT_LOC_THRESHOLD),
                params: params_threshold
                    .unwrap_or(corvalis_recon::config::DEFAULT_PARAMS_THRESHOLD),
            };

            let mut file_metrics: Vec<serde_json::Value> = Vec::new();
            let mut all_hotspots: Vec<Hotspot> = Vec::new();

            for parsed in &parse_result.files {
                let file_met = metrics::analyze_file(parsed);
                let hotspots =
                    metrics::detect_hotspots(&parsed.source_file.path, &file_met, &thresholds);

                if threshold.is_some_and(|min| file_met.cyclomatic_complexity < min) {
                    continue;
                }

                all_hotspots.extend(hotspots);
                file_metrics.push(serde_json::json!({
                    "path": parsed.source_file.path,
                    "language": parsed.source_file.language.as_str(),
                    "metrics": file_met,
                    "hotspots": all_hotspots.iter()
                        .filter(|h| h.path == parsed.source_file.path)
                        .collect::<Vec<_>>(),
                }));
            }

            let output = serde_json::json!({
                "files": file_metrics,
                "hotspots": all_hotspots,
            });

            print_json(&output, &cli.format)?;
        }
    }

    Ok(())
}

fn print_json(value: &impl serde::Serialize, format: &OutputFormat) -> Result<()> {
    let output = match format {
        OutputFormat::Json => serde_json::to_string(value)?,
        OutputFormat::Pretty => serde_json::to_string_pretty(value)?,
    };
    println!("{output}");
    Ok(())
}
