use std::collections::BTreeMap;

use crate::output::{DependencyGraph, FileAnalysis, Hotspot};

/// Importance score for a single file, used to prioritize output in budget-constrained mode.
#[derive(Debug, Clone)]
pub struct FileScore {
    pub path: String,
    pub score: f64,
    pub is_entry_point: bool,
}

/// Score files by structural importance to the project.
///
/// Scoring factors:
/// - Export count: files that expose more symbols are more central
/// - Incoming dependency edges: hub files score higher
/// - Hotspot presence: files with complexity hotspots are worth surfacing
/// - Entry point status: entry points always rank high
pub fn score_files(
    files: &[FileAnalysis],
    graph: &DependencyGraph,
    hotspots: &[Hotspot],
) -> Vec<FileScore> {
    // Count incoming edges per file from the adjacency list.
    let mut incoming_count: BTreeMap<&str, usize> = BTreeMap::new();
    for deps in graph.adjacency.values() {
        for dep in deps {
            if dep.resolved && !dep.external {
                *incoming_count.entry(&dep.target).or_insert(0) += 1;
            }
        }
    }

    let mut scores: Vec<FileScore> = files
        .iter()
        .map(|file| {
            let export_count = file.exports.len() as f64;
            let incoming = *incoming_count.get(file.path.as_str()).unwrap_or(&0) as f64;
            let hotspot_count = hotspots.iter().filter(|h| h.path == file.path).count() as f64;
            let is_entry_point = graph.entry_points.contains(&file.path);

            let score = export_count * EXPORT_WEIGHT
                + incoming * INCOMING_EDGE_WEIGHT
                + hotspot_count * HOTSPOT_WEIGHT
                + if is_entry_point {
                    ENTRY_POINT_WEIGHT
                } else {
                    0.0
                };

            FileScore {
                path: file.path.clone(),
                score,
                is_entry_point,
            }
        })
        .collect();

    // Sort by score descending, then path for determinism.
    scores.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    scores
}

// Scoring weights — named constants per auto-hardcoding discipline.
const EXPORT_WEIGHT: f64 = 1.0;
const INCOMING_EDGE_WEIGHT: f64 = 2.0;
const HOTSPOT_WEIGHT: f64 = 3.0;
const ENTRY_POINT_WEIGHT: f64 = 5.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{
        Dependency, DependencyGraph, Export, ExportKind, FileAnalysis, FileMetrics, GraphStats,
        Hotspot,
    };

    fn make_file(path: &str, export_count: usize) -> FileAnalysis {
        FileAnalysis {
            path: path.to_string(),
            language: "typescript".to_string(),
            symbols: Vec::new(),
            imports: Vec::new(),
            exports: (0..export_count)
                .map(|i| Export {
                    name: format!("export_{i}"),
                    kind: ExportKind::Named,
                    line: i + 1,
                    source: None,
                })
                .collect(),
            metrics: FileMetrics::default(),
        }
    }

    fn make_graph(
        adjacency: BTreeMap<String, Vec<Dependency>>,
        entry_points: Vec<String>,
    ) -> DependencyGraph {
        DependencyGraph {
            adjacency,
            entry_points,
            leaf_nodes: Vec::new(),
            cycles: Vec::new(),
            stats: GraphStats::default(),
        }
    }

    #[test]
    fn entry_points_rank_higher_than_non_entry_points() {
        let files = vec![make_file("lib.ts", 2), make_file("main.ts", 0)];
        let graph = make_graph(BTreeMap::new(), vec!["main.ts".to_string()]);

        let scores = score_files(&files, &graph, &[]);

        let main_score = scores.iter().find(|s| s.path == "main.ts").unwrap();
        let lib_score = scores.iter().find(|s| s.path == "lib.ts").unwrap();
        assert!(
            main_score.score > lib_score.score,
            "entry point (score={}) should rank above non-entry with 2 exports (score={})",
            main_score.score,
            lib_score.score
        );
        assert!(main_score.is_entry_point);
        assert!(!lib_score.is_entry_point);
    }

    #[test]
    fn files_with_more_incoming_edges_rank_higher() {
        let files = vec![make_file("utils.ts", 1), make_file("helper.ts", 1)];

        let mut adjacency = BTreeMap::new();
        // Two files import utils.ts, one imports helper.ts.
        adjacency.insert(
            "a.ts".to_string(),
            vec![Dependency {
                target: "utils.ts".to_string(),
                specifiers: vec![],
                resolved: true,
                external: false,
            }],
        );
        adjacency.insert(
            "b.ts".to_string(),
            vec![
                Dependency {
                    target: "utils.ts".to_string(),
                    specifiers: vec![],
                    resolved: true,
                    external: false,
                },
                Dependency {
                    target: "helper.ts".to_string(),
                    specifiers: vec![],
                    resolved: true,
                    external: false,
                },
            ],
        );
        let graph = make_graph(adjacency, Vec::new());

        let scores = score_files(&files, &graph, &[]);
        let utils_score = scores.iter().find(|s| s.path == "utils.ts").unwrap();
        let helper_score = scores.iter().find(|s| s.path == "helper.ts").unwrap();
        assert!(utils_score.score > helper_score.score);
    }

    #[test]
    fn hotspot_files_rank_higher() {
        let files = vec![make_file("clean.ts", 1), make_file("complex.ts", 1)];
        let graph = make_graph(BTreeMap::new(), Vec::new());
        let hotspots = vec![Hotspot {
            path: "complex.ts".to_string(),
            function: "doStuff".to_string(),
            metric: "cyclomatic_complexity".to_string(),
            value: 15,
            threshold: 10,
        }];

        let scores = score_files(&files, &graph, &hotspots);
        let complex_score = scores.iter().find(|s| s.path == "complex.ts").unwrap();
        let clean_score = scores.iter().find(|s| s.path == "clean.ts").unwrap();
        assert!(complex_score.score > clean_score.score);
    }

    #[test]
    fn scores_sorted_descending() {
        let files = vec![
            make_file("low.ts", 0),
            make_file("mid.ts", 3),
            make_file("high.ts", 10),
        ];
        let graph = make_graph(BTreeMap::new(), Vec::new());

        let scores = score_files(&files, &graph, &[]);
        assert_eq!(scores[0].path, "high.ts");
        assert_eq!(scores[1].path, "mid.ts");
        assert_eq!(scores[2].path, "low.ts");
    }

    #[test]
    fn empty_files_produces_empty_scores() {
        let graph = make_graph(BTreeMap::new(), Vec::new());
        let scores = score_files(&[], &graph, &[]);
        assert!(scores.is_empty());
    }

    #[test]
    fn deterministic_ordering_for_equal_scores() {
        let files = vec![
            make_file("b.ts", 1),
            make_file("a.ts", 1),
            make_file("c.ts", 1),
        ];
        let graph = make_graph(BTreeMap::new(), Vec::new());

        let scores = score_files(&files, &graph, &[]);
        // Equal scores → sorted by path ascending.
        assert_eq!(scores[0].path, "a.ts");
        assert_eq!(scores[1].path, "b.ts");
        assert_eq!(scores[2].path, "c.ts");
    }
}
