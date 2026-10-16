//! Exercises GraphDiff application, not dirty-file discovery or public refresh.
//!
//! There is no Ingestor::ingest_incremental API in this source revision. Full
//! Ingestor scans supply resolved extraction rows to GraphDiff::compute; only
//! GraphDiff::apply mutates the retained graph. The clean graph is an oracle,
//! never a replacement for the graph under test. Unsupported barrel/export
//! semantics may be absent on both sides and are reported rather than invented.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, NodeId, NodeType};
use m1nd_ingest::diff::GraphDiff;
use m1nd_ingest::extract::{ExtractedEdge, ExtractedNode};
use m1nd_ingest::{IngestConfig, Ingestor};

struct TempRepo(PathBuf);

impl TempRepo {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("m1nd-diff-parity-{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug)]
enum Leaf {
    Missing,
    Exported,
    Renamed,
    Private,
    WithoutCall,
}

fn stable_sources() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    for ext in ["ts", "js"] {
        files.insert(
            format!("src/{ext}/index.{ext}"),
            "export * from './nested';\n".into(),
        );
        files.insert(
            format!("src/{ext}/nested/index.{ext}"),
            format!("export {{ {ext}page }} from './page';\n"),
        );
        files.insert(
            format!("src/{ext}/use.{ext}"),
            format!(
                "import {{ {ext}page }} from './index';\n\
                 import {{ {ext}helper }} from './support';\n\
                 export function {ext}use() {{\n  return {ext}page() + {ext}helper();\n}}\n"
            ),
        );
        files.insert(
            format!("src/{ext}/support.{ext}"),
            format!(
                "export function {ext}helper() {{\n  return 1;\n}}\n\
                 export function {ext}control() {{\n  return {ext}helper();\n}}\n"
            ),
        );
    }
    files
}

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn set_leaf(root: &Path, state: Leaf) {
    for ext in ["ts", "js"] {
        for name in ["page", "renamed"] {
            let path = root.join(format!("src/{ext}/nested/{name}.{ext}"));
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
        if matches!(state, Leaf::Missing) {
            continue;
        }
        let name = if matches!(state, Leaf::Renamed) {
            "renamed"
        } else {
            "page"
        };
        let export = if matches!(state, Leaf::Private) {
            ""
        } else {
            "export "
        };
        let body = if matches!(state, Leaf::WithoutCall) {
            "1".to_string()
        } else {
            format!("{ext}helper()")
        };
        write(
            root,
            &format!("src/{ext}/nested/{name}.{ext}"),
            &format!(
                "import {{ {ext}helper }} from '../support';\n\
                 {export}function {ext}page() {{\n  return {body};\n}}\n"
            ),
        );
    }
}

fn ingest(root: &Path) -> Graph {
    Ingestor::new(IngestConfig {
        root: root.to_path_buf(),
        ..IngestConfig::default()
    })
    .ingest()
    .expect("real code ingest")
    .0
}

fn external_ids(graph: &Graph) -> Vec<String> {
    let mut ids = vec![String::new(); graph.num_nodes() as usize];
    for (external, node) in &graph.id_to_node {
        assert!(
            ids[node.as_usize()].is_empty(),
            "one external identity per node"
        );
        ids[node.as_usize()] = graph.strings.resolve(*external).to_string();
    }
    assert!(
        ids.iter().all(|id| !id.is_empty()),
        "all nodes have external identities"
    );
    ids
}

fn relevant(relation: &str) -> bool {
    matches!(relation, "contains" | "imports" | "calls" | "reexports")
}

// GraphDiff accepts extraction rows. Preserve the real ingestor's resolved IDs
// and relations instead of implementing a second resolver inside the test.
fn extraction_rows(graph: &Graph) -> (Vec<ExtractedNode>, Vec<ExtractedEdge>) {
    let ids = external_ids(graph);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        nodes.push(ExtractedNode {
            id: id.clone(),
            label: graph.strings.resolve(graph.nodes.label[index]).to_string(),
            node_type: graph.nodes.node_type[index],
            tags: graph
                .node_tags(NodeId::new(index as u32))
                .into_iter()
                .map(str::to_owned)
                .collect(),
            line: graph.nodes.provenance[index].line_start,
            end_line: graph.nodes.provenance[index].line_end,
        });
        for edge in graph.csr.out_range(NodeId::new(index as u32)) {
            let relation = graph.strings.resolve(graph.csr.relations[edge]);
            if !relevant(relation) {
                continue;
            }
            assert_eq!(graph.csr.directions[edge], EdgeDirection::Forward);
            assert!(!graph.csr.inhibitory[edge]);
            edges.push(ExtractedEdge {
                source: id.clone(),
                target: ids[graph.csr.targets[edge].as_usize()].clone(),
                relation: relation.to_string(),
                weight: graph
                    .csr
                    .read_weight(m1nd_core::types::EdgeIdx::new(edge as u32))
                    .get(),
            });
        }
    }
    (nodes, edges)
}

type Edge = (String, String, String);
type Nodes = BTreeMap<String, (String, NodeType, BTreeSet<String>)>;

#[derive(Debug, PartialEq, Eq)]
struct ActiveGraph {
    nodes: Nodes,
    edges: BTreeSet<Edge>,
}

fn active_graph(graph: &Graph) -> ActiveGraph {
    assert!(graph.finalized, "query only a finalized graph");
    let ids = external_ids(graph);
    let mut nodes = BTreeMap::new();
    for (index, id) in ids.iter().enumerate() {
        let tags: BTreeSet<String> = graph
            .node_tags(NodeId::new(index as u32))
            .into_iter()
            .map(str::to_owned)
            .collect();
        if !tags.contains("__m1nd_deleted") {
            nodes.insert(
                id.clone(),
                (
                    graph.strings.resolve(graph.nodes.label[index]).to_string(),
                    graph.nodes.node_type[index],
                    tags,
                ),
            );
        }
    }
    let mut edges = BTreeSet::new();
    for (index, source) in ids.iter().enumerate() {
        for edge in graph.csr.out_range(NodeId::new(index as u32)) {
            let target = &ids[graph.csr.targets[edge].as_usize()];
            let relation = graph.strings.resolve(graph.csr.relations[edge]);
            if relevant(relation) && nodes.contains_key(source) && nodes.contains_key(target) {
                edges.insert((source.clone(), relation.to_string(), target.clone()));
            }
        }
    }
    ActiveGraph { nodes, edges }
}

fn edge(source: &str, relation: &str, target: &str) -> Edge {
    (source.into(), relation.into(), target.into())
}

#[test]
fn graph_diff_matches_clean_ingest_through_leaf_lifecycle() {
    let repo = TempRepo::new();
    let stable = stable_sources();
    for (path, content) in &stable {
        write(&repo.0, path, content);
    }
    let mut retained = ingest(&repo.0);
    let (mut old_nodes, mut old_edges) = extraction_rows(&retained);
    let identities: Vec<_> = stable
        .keys()
        .map(|path| {
            let external = format!("file::{path}");
            let node = retained
                .resolve_id(&external)
                .expect("stable file identity");
            (external, node)
        })
        .collect();
    let mut failures = Vec::new();
    let states = [
        ("add", Leaf::Exported),
        ("remove_call_control", Leaf::WithoutCall),
        ("restore_call_control", Leaf::Exported),
        ("remove", Leaf::Missing),
        ("restore", Leaf::Exported),
        ("rename", Leaf::Renamed),
        ("restore_name", Leaf::Exported),
        ("lose_export", Leaf::Private),
        ("gain_export", Leaf::Exported),
    ];
    for (name, state) in states {
        set_leaf(&repo.0, state);
        for (path, content) in &stable {
            assert_eq!(fs::read_to_string(repo.0.join(path)).unwrap(), *content);
        }
        let clean = ingest(&repo.0);
        let expected = active_graph(&clean);
        let (new_nodes, new_edges) = extraction_rows(&clean);
        let diff = GraphDiff::compute(&old_nodes, &old_edges, &new_nodes, &new_edges);
        match name {
            "add" | "restore" | "restore_name" => assert!(diff.nodes_added > 0),
            "remove" => assert!(diff.nodes_removed > 0),
            "rename" => assert!(diff.nodes_added > 0 && diff.nodes_removed > 0),
            "remove_call_control" => assert!(diff.edges_removed > 0),
            "restore_call_control" => assert!(diff.edges_added > 0),
            _ => {} // An unsupported export distinction may yield an empty diff.
        }
        diff.apply(&mut retained)
            .expect("real GraphDiff application");
        retained
            .finalize()
            .expect("finalize retained graph after diff");
        let actual = active_graph(&retained);
        for (external, node) in &identities {
            assert_eq!(
                retained.resolve_id(external),
                Some(*node),
                "{name}: stable identity"
            );
            assert!(
                actual.nodes.contains_key(external),
                "{name}: stable node stays active"
            );
        }
        for ext in ["ts", "js"] {
            let importer = format!("file::src/{ext}/use.{ext}");
            let helper = format!("file::src/{ext}/support.{ext}::fn::{ext}helper");
            let caller = format!("{importer}::fn::{ext}use");
            let control = edge(&caller, "calls", &helper);
            assert!(
                expected.edges.contains(&control),
                "{name}: clean positive call control"
            );
            assert!(
                actual.edges.contains(&control),
                "{name}: retained positive call control"
            );
            let barrel = format!("file::src/{ext}/index.{ext}");
            let nested = format!("file::src/{ext}/nested/index.{ext}");
            let import_control = edge(&importer, "imports", &barrel);
            assert!(
                expected.edges.contains(&import_control),
                "{name}: clean relative import control"
            );
            assert!(
                actual.edges.contains(&import_control),
                "{name}: retained relative import control"
            );
            let original_leaf = format!("file::src/{ext}/nested/page.{ext}::fn::{ext}page");
            let renamed_leaf = format!("file::src/{ext}/nested/renamed.{ext}::fn::{ext}page");
            match state {
                Leaf::Missing => {
                    assert!(!expected.nodes.contains_key(&original_leaf));
                    assert!(!expected.nodes.contains_key(&renamed_leaf));
                }
                _ => {
                    let (leaf, absent) = if matches!(state, Leaf::Renamed) {
                        (&renamed_leaf, &original_leaf)
                    } else {
                        (&original_leaf, &renamed_leaf)
                    };
                    assert!(
                        expected.nodes.contains_key(leaf),
                        "{name}: clean leaf extraction"
                    );
                    assert!(
                        !expected.nodes.contains_key(absent),
                        "{name}: clean filename identity"
                    );
                    let leaf_call = edge(leaf, "calls", &helper);
                    assert_eq!(
                        expected.edges.contains(&leaf_call),
                        !matches!(state, Leaf::WithoutCall),
                        "{name}: clean leaf-call removal has live endpoints"
                    );
                }
            }
            let barrel_link = edge(&barrel, "reexports", &nested);
            eprintln!("{name}/{ext}: explicit barrel reexport clean={} diff={}; export distinctions are not assumed",
                expected.edges.contains(&barrel_link), actual.edges.contains(&barrel_link));
        }
        if actual != expected {
            let actual_ids: BTreeSet<_> = actual.nodes.keys().cloned().collect();
            let expected_ids: BTreeSet<_> = expected.nodes.keys().cloned().collect();
            failures.push(format!("{name}: missing nodes={:?}; extra nodes={:?}; node metadata equal={}; missing edges={:?}; extra edges={:?}",
                expected_ids.difference(&actual_ids).collect::<Vec<_>>(),
                actual_ids.difference(&expected_ids).collect::<Vec<_>>(), actual.nodes == expected.nodes,
                expected.edges.difference(&actual.edges).collect::<Vec<_>>(),
                actual.edges.difference(&expected.edges).collect::<Vec<_>>()));
        }
        // Advance the prior clean extraction, never transplant the clean graph.
        old_nodes = new_nodes;
        old_edges = new_edges;
        eprintln!(
            "{name}: diff actions={} added={} removed={}",
            diff.actions.len(),
            diff.nodes_added,
            diff.nodes_removed
        );
    }
    assert!(
        failures.is_empty(),
        "GraphDiff/clean topology mismatches:\n{}",
        failures.join("\n")
    );
}
