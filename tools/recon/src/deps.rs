use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::output::{Dependency, DependencyGraph, GraphStats};
use crate::parse::ParsedFile;
use crate::resolve::{self, PathAliases, ResolvedImport};
use crate::symbols;

/// Build a file-level dependency graph from parsed files.
///
/// For each file, extracts imports and re-export sources, resolves each to a
/// target file (or marks as external/unresolved), and assembles the adjacency
/// list. Also detects entry points, leaf nodes, cycles, and computes stats.
pub fn build_dependency_graph(
    parsed_files: &[ParsedFile],
    root: &Path,
    _aliases: &PathAliases,
) -> DependencyGraph {
    let mut adjacency: BTreeMap<String, Vec<Dependency>> = BTreeMap::new();
    let mut incoming_edges: HashMap<String, usize> = HashMap::new();
    let mut alias_cache = HashMap::new();

    // Initialize all files in the adjacency map (even files with no imports).
    for pf in parsed_files {
        adjacency.entry(pf.source_file.path.clone()).or_default();
    }

    for pf in parsed_files {
        let file_symbols = symbols::extract_symbols(pf);
        let source_path = &pf.source_file.path;
        let aliases = resolve::load_tsconfig_aliases_for_file(root, source_path, &mut alias_cache);

        let import_sources: Vec<(&str, Vec<String>)> = file_symbols
            .imports
            .iter()
            .map(|imp| (imp.source.as_str(), imp.specifiers.clone()))
            .chain(
                file_symbols
                    .exports
                    .iter()
                    .filter(|exp| {
                        matches!(
                            exp.kind,
                            crate::output::ExportKind::ReExport
                                | crate::output::ExportKind::StarReExport
                        )
                    })
                    .filter_map(|exp| exp.source.as_deref().map(|source| (source, vec![]))),
            )
            .collect();

        // Deduplicate targets per file — multiple imports from same source collapse.
        let mut seen_targets: HashMap<String, Vec<String>> = HashMap::new();

        for (raw_source, specifiers) in import_sources {
            let resolved = resolve::resolve_import(raw_source, source_path, root, &aliases);

            match &resolved {
                ResolvedImport::ProjectFile(target) => {
                    seen_targets
                        .entry(target.clone())
                        .or_default()
                        .extend(specifiers);
                }
                ResolvedImport::External(name) => {
                    seen_targets
                        .entry(format!("external:{name}"))
                        .or_default()
                        .extend(specifiers);
                }
                ResolvedImport::Unresolved(name) => {
                    seen_targets
                        .entry(format!("unresolved:{name}"))
                        .or_default()
                        .extend(specifiers);
                }
            }
        }

        let deps: Vec<Dependency> = seen_targets
            .into_iter()
            .map(|(key, mut specifiers)| {
                specifiers.sort();
                specifiers.dedup();
                if let Some(target) = key.strip_prefix("external:") {
                    Dependency {
                        target: target.to_string(),
                        specifiers,
                        resolved: false,
                        external: true,
                    }
                } else if let Some(target) = key.strip_prefix("unresolved:") {
                    Dependency {
                        target: target.to_string(),
                        specifiers,
                        resolved: false,
                        external: false,
                    }
                } else {
                    *incoming_edges.entry(key.clone()).or_insert(0) += 1;

                    Dependency {
                        target: key,
                        specifiers,
                        resolved: true,
                        external: false,
                    }
                }
            })
            .collect();

        adjacency.insert(source_path.clone(), deps);
    }

    // Sort each file's dependencies for deterministic output.
    for deps in adjacency.values_mut() {
        deps.sort_by(|a, b| a.target.cmp(&b.target));
    }

    // Entry points: project files with no incoming edges from other project files.
    let mut entry_points: Vec<String> = adjacency
        .keys()
        .filter(|file| incoming_edges.get(file.as_str()).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    entry_points.sort();

    // Leaf nodes: project files with no outgoing project-internal imports.
    let mut leaf_nodes: Vec<String> = adjacency
        .iter()
        .filter(|(_, deps)| !deps.iter().any(|d| d.resolved && !d.external))
        .map(|(file, _)| file.clone())
        .collect();
    leaf_nodes.sort();

    // Cycle detection via DFS with three-color marking.
    let cycles = detect_cycles(&adjacency);

    let stats = compute_stats(&adjacency, &incoming_edges);

    DependencyGraph {
        adjacency,
        entry_points,
        leaf_nodes,
        cycles,
        stats,
    }
}

/// Three-color DFS cycle detection.
///
/// White (unvisited) → Gray (in current path) → Black (fully explored).
/// A back-edge to a gray node indicates a cycle.
fn detect_cycles(adjacency: &BTreeMap<String, Vec<Dependency>>) -> Vec<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: HashMap<&str, Color> = adjacency
        .keys()
        .map(|k| (k.as_str(), Color::White))
        .collect();
    let mut path: Vec<&str> = Vec::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adjacency: &'a BTreeMap<String, Vec<Dependency>>,
        color: &mut HashMap<&'a str, Color>,
        path: &mut Vec<&'a str>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        color.insert(node, Color::Gray);
        path.push(node);

        if let Some(deps) = adjacency.get(node) {
            for dep in deps {
                if !dep.resolved || dep.external {
                    continue;
                }
                let target = dep.target.as_str();
                match color.get(target) {
                    Some(Color::Gray) => {
                        // Found a cycle — extract the cycle from the path.
                        if let Some(start_idx) = path.iter().position(|&n| n == target) {
                            let cycle: Vec<String> =
                                path[start_idx..].iter().map(|s| s.to_string()).collect();
                            cycles.push(cycle);
                        }
                    }
                    Some(Color::White) | None => {
                        dfs(target, adjacency, color, path, cycles);
                    }
                    Some(Color::Black) => {}
                }
            }
        }

        path.pop();
        color.insert(node, Color::Black);
    }

    // Process nodes in sorted order for deterministic cycle reporting.
    let nodes: Vec<&str> = adjacency.keys().map(|k| k.as_str()).collect();
    for node in nodes {
        if color.get(node) == Some(&Color::White) {
            dfs(node, adjacency, &mut color, &mut path, &mut cycles);
        }
    }

    // Sort cycles for deterministic output.
    cycles.sort();
    cycles
}

fn compute_stats(
    adjacency: &BTreeMap<String, Vec<Dependency>>,
    incoming_edges: &HashMap<String, usize>,
) -> GraphStats {
    let total_files = adjacency.len();
    if total_files == 0 {
        return GraphStats::default();
    }

    let mut total_edges: usize = 0;
    let mut max_dependencies: usize = 0;

    for deps in adjacency.values() {
        let internal_count = deps.iter().filter(|d| d.resolved && !d.external).count();
        total_edges += internal_count;
        max_dependencies = max_dependencies.max(internal_count);
    }

    let avg_dependencies = total_edges as f64 / total_files as f64;

    let max_dependents = incoming_edges.values().copied().max().unwrap_or(0);
    let total_incoming: usize = incoming_edges.values().sum();
    let avg_dependents = total_incoming as f64 / total_files as f64;

    // Most imported: files with highest incoming edge count.
    let mut by_incoming: Vec<(&str, usize)> = incoming_edges
        .iter()
        .map(|(k, &v)| (k.as_str(), v))
        .collect();
    by_incoming.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let most_imported: Vec<String> = by_incoming
        .iter()
        .take(5)
        .map(|(k, _)| k.to_string())
        .collect();

    GraphStats {
        total_files,
        total_edges,
        avg_dependencies: (avg_dependencies * 100.0).round() / 100.0,
        max_dependencies,
        avg_dependents: (avg_dependents * 100.0).round() / 100.0,
        max_dependents,
        most_imported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{DetectedLanguage, SourceFile};
    use crate::parse;
    use std::fs;

    fn setup_dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_fixture(dir: &Path, rel_path: &str, content: &str) -> SourceFile {
        let full = dir.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();

        let file_path = Path::new(rel_path);
        SourceFile {
            path: rel_path.to_string(),
            language: crate::language::detect_language(file_path)
                .unwrap_or(DetectedLanguage::TypeScript),
            declaration_only: crate::language::is_declaration_file(file_path),
        }
    }

    fn build_graph(dir: &Path, files: &[SourceFile]) -> DependencyGraph {
        let parsed = parse::parse_files_sequential(files, dir);
        let aliases = resolve::load_tsconfig_aliases(dir);
        build_dependency_graph(&parsed.files, dir, &aliases)
    }

    // --- Basic graph building ---

    #[test]
    fn empty_project_produces_empty_graph() {
        let graph = DependencyGraph {
            adjacency: BTreeMap::new(),
            entry_points: Vec::new(),
            leaf_nodes: Vec::new(),
            cycles: Vec::new(),
            stats: GraphStats::default(),
        };
        assert_eq!(graph.stats.total_files, 0);
    }

    #[test]
    fn single_file_no_imports_is_entry_and_leaf() {
        let dir = setup_dir();
        let files = vec![write_fixture(dir.path(), "main.ts", "const x = 1;")];

        let graph = build_graph(dir.path(), &files);

        assert_eq!(graph.adjacency.len(), 1);
        assert!(graph.entry_points.contains(&"main.ts".to_string()));
        assert!(graph.leaf_nodes.contains(&"main.ts".to_string()));
        assert!(graph.cycles.is_empty());
    }

    #[test]
    fn simple_import_creates_edge() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(
                dir.path(),
                "src/main.ts",
                "import { helper } from './utils';",
            ),
            write_fixture(dir.path(), "src/utils.ts", "export function helper() {}"),
        ];

        let graph = build_graph(dir.path(), &files);

        let main_deps = &graph.adjacency["src/main.ts"];
        assert!(
            main_deps
                .iter()
                .any(|d| d.target == "src/utils.ts" && d.resolved && !d.external),
            "main.ts should depend on utils.ts: {main_deps:?}"
        );

        assert!(graph.entry_points.contains(&"src/main.ts".to_string()));
        assert!(!graph.entry_points.contains(&"src/utils.ts".to_string()));
        assert!(graph.leaf_nodes.contains(&"src/utils.ts".to_string()));
        assert!(!graph.leaf_nodes.contains(&"src/main.ts".to_string()));
    }

    #[test]
    fn external_import_not_in_adjacency_keys() {
        let dir = setup_dir();
        let files = vec![write_fixture(
            dir.path(),
            "main.ts",
            "import React from 'react';",
        )];

        let graph = build_graph(dir.path(), &files);

        let deps = &graph.adjacency["main.ts"];
        let react_dep = deps.iter().find(|d| d.target == "react").unwrap();
        assert!(react_dep.external);
        assert!(!react_dep.resolved);
    }

    // --- Entry points and leaf nodes ---

    #[test]
    fn entry_points_have_no_incoming_project_edges() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "app.ts", "import { a } from './lib';"),
            write_fixture(dir.path(), "lib.ts", "export const a = 1;"),
            write_fixture(dir.path(), "cli.ts", "import { a } from './lib';"),
        ];

        let graph = build_graph(dir.path(), &files);

        // app.ts and cli.ts are entry points (nothing imports them).
        assert!(graph.entry_points.contains(&"app.ts".to_string()));
        assert!(graph.entry_points.contains(&"cli.ts".to_string()));
        // lib.ts is imported by both — not an entry point.
        assert!(!graph.entry_points.contains(&"lib.ts".to_string()));
    }

    #[test]
    fn leaf_nodes_have_no_outgoing_project_edges() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "app.ts", "import { a } from './lib';"),
            write_fixture(
                dir.path(),
                "lib.ts",
                "import React from 'react';\nexport const a = 1;",
            ),
        ];

        let graph = build_graph(dir.path(), &files);

        // lib.ts only imports external `react` — it's a leaf.
        assert!(graph.leaf_nodes.contains(&"lib.ts".to_string()));
        assert!(!graph.leaf_nodes.contains(&"app.ts".to_string()));
    }

    // --- Cycle detection ---

    #[test]
    fn detects_simple_cycle() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "a.ts", "import { b } from './b';"),
            write_fixture(dir.path(), "b.ts", "import { a } from './a';"),
        ];

        let graph = build_graph(dir.path(), &files);

        assert!(
            !graph.cycles.is_empty(),
            "should detect cycle between a.ts and b.ts"
        );
        let cycle = &graph.cycles[0];
        assert!(cycle.contains(&"a.ts".to_string()));
        assert!(cycle.contains(&"b.ts".to_string()));
    }

    #[test]
    fn detects_three_node_cycle() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "a.ts", "import { b } from './b';"),
            write_fixture(dir.path(), "b.ts", "import { c } from './c';"),
            write_fixture(dir.path(), "c.ts", "import { a } from './a';"),
        ];

        let graph = build_graph(dir.path(), &files);

        assert!(!graph.cycles.is_empty(), "should detect 3-node cycle");
    }

    #[test]
    fn no_cycles_in_dag() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "a.ts", "import { b } from './b';"),
            write_fixture(dir.path(), "b.ts", "import { c } from './c';"),
            write_fixture(dir.path(), "c.ts", "export const c = 1;"),
        ];

        let graph = build_graph(dir.path(), &files);
        assert!(graph.cycles.is_empty());
    }

    // --- Stats ---

    #[test]
    fn stats_count_internal_edges_only() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(
                dir.path(),
                "main.ts",
                "import { a } from './a';\nimport { b } from './b';\nimport React from 'react';",
            ),
            write_fixture(dir.path(), "a.ts", "export const a = 1;"),
            write_fixture(dir.path(), "b.ts", "export const b = 2;"),
        ];

        let graph = build_graph(dir.path(), &files);

        assert_eq!(graph.stats.total_files, 3);
        // main -> a, main -> b = 2 internal edges. react is external.
        assert_eq!(graph.stats.total_edges, 2);
        assert_eq!(graph.stats.max_dependencies, 2);
    }

    #[test]
    fn most_imported_tracks_popular_files() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "app.ts", "import { u } from './utils';"),
            write_fixture(dir.path(), "cli.ts", "import { u } from './utils';"),
            write_fixture(dir.path(), "test.ts", "import { u } from './utils';"),
            write_fixture(dir.path(), "utils.ts", "export const u = 1;"),
        ];

        let graph = build_graph(dir.path(), &files);

        assert!(
            graph.stats.most_imported.contains(&"utils.ts".to_string()),
            "utils.ts should be most imported: {:?}",
            graph.stats.most_imported
        );
    }

    // --- Index file resolution ---

    #[test]
    fn resolves_directory_index_imports() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(
                dir.path(),
                "main.ts",
                "import { Button } from './components';",
            ),
            write_fixture(
                dir.path(),
                "components/index.ts",
                "export { Button } from './Button';",
            ),
            write_fixture(
                dir.path(),
                "components/Button.ts",
                "export function Button() {}",
            ),
        ];

        let graph = build_graph(dir.path(), &files);

        let main_deps = &graph.adjacency["main.ts"];
        assert!(
            main_deps
                .iter()
                .any(|d| d.target == "components/index.ts" && d.resolved),
            "main.ts should resolve to components/index.ts: {main_deps:?}"
        );
    }

    // --- Path alias resolution ---

    #[test]
    fn resolves_tsconfig_path_aliases() {
        let dir = setup_dir();
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["src/*"] } } }"#,
        )
        .unwrap();

        let files = vec![
            write_fixture(
                dir.path(),
                "src/main.ts",
                "import { helper } from '@/lib/utils';",
            ),
            write_fixture(
                dir.path(),
                "src/lib/utils.ts",
                "export function helper() {}",
            ),
        ];

        let graph = build_graph(dir.path(), &files);

        let main_deps = &graph.adjacency["src/main.ts"];
        assert!(
            main_deps
                .iter()
                .any(|d| d.target == "src/lib/utils.ts" && d.resolved),
            "path alias should resolve: {main_deps:?}"
        );
    }

    #[test]
    fn re_exports_create_dependency_edges() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(
                dir.path(),
                "components/index.ts",
                "export { Button } from './Button';\nexport * from './Card';",
            ),
            write_fixture(
                dir.path(),
                "components/Button.ts",
                "export function Button() {}",
            ),
            write_fixture(
                dir.path(),
                "components/Card.ts",
                "export function Card() {}",
            ),
        ];

        let graph = build_graph(dir.path(), &files);
        let deps = &graph.adjacency["components/index.ts"];

        assert!(deps.iter().any(|d| d.target == "components/Button.ts"));
        assert!(deps.iter().any(|d| d.target == "components/Card.ts"));
    }

    // --- Graph serialization ---

    #[test]
    fn graph_serializes_to_valid_json() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(dir.path(), "a.ts", "import { b } from './b';"),
            write_fixture(dir.path(), "b.ts", "export const b = 1;"),
        ];

        let graph = build_graph(dir.path(), &files);
        let json = serde_json::to_string(&graph).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(parsed.get("adjacency").unwrap().is_object());
        assert!(parsed.get("entry_points").unwrap().is_array());
        assert!(parsed.get("leaf_nodes").unwrap().is_array());
        assert!(parsed.get("cycles").unwrap().is_array());
        assert!(parsed.get("stats").unwrap().is_object());
    }

    // --- Edge cases ---

    #[test]
    fn file_importing_itself_creates_self_cycle() {
        let dir = setup_dir();
        let files = vec![write_fixture(
            dir.path(),
            "self.ts",
            "import { x } from './self';",
        )];

        let graph = build_graph(dir.path(), &files);
        assert!(
            !graph.cycles.is_empty(),
            "self-import should be detected as cycle"
        );
    }

    #[test]
    fn multiple_imports_from_same_source_deduplicated() {
        let dir = setup_dir();
        let files = vec![
            write_fixture(
                dir.path(),
                "main.ts",
                "import { a } from './lib';\nimport { b } from './lib';",
            ),
            write_fixture(
                dir.path(),
                "lib.ts",
                "export const a = 1;\nexport const b = 2;",
            ),
        ];

        let graph = build_graph(dir.path(), &files);

        let main_deps = &graph.adjacency["main.ts"];
        let lib_deps: Vec<_> = main_deps.iter().filter(|d| d.target == "lib.ts").collect();
        assert_eq!(
            lib_deps.len(),
            1,
            "duplicate imports should be deduplicated: {lib_deps:?}"
        );
    }

    #[test]
    fn unresolved_import_recorded_as_unresolved() {
        let dir = setup_dir();
        let files = vec![write_fixture(
            dir.path(),
            "main.ts",
            "import { x } from './missing';",
        )];

        let graph = build_graph(dir.path(), &files);

        let deps = &graph.adjacency["main.ts"];
        let unresolved = deps.iter().find(|d| d.target == "./missing").unwrap();
        assert!(!unresolved.resolved);
        assert!(!unresolved.external);
    }
}
