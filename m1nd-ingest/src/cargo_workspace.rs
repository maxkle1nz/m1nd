use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId};
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::NodeProvenanceInput;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct CargoWorkspaceStats {
    pub nodes_added: u64,
    pub edges_added: u64,
}

pub fn enrich_rust_workspace(
    graph: &mut m1nd_core::graph::Graph,
    root: &Path,
) -> M1ndResult<CargoWorkspaceStats> {
    let Some(metadata) = load_metadata(root) else {
        return Ok(CargoWorkspaceStats::default());
    };

    let workspace_root = PathBuf::from(metadata.workspace_root.as_str());
    let workspace_manifest = workspace_root.join("Cargo.toml");
    let workspace_manifest_rel =
        relative_to_root(root, &workspace_manifest).unwrap_or_else(|| "Cargo.toml".to_string());
    let workspace_id = format!("cargo::workspace::{}", workspace_manifest_rel);
    let workspace_label = workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace");

    let mut stats = CargoWorkspaceStats::default();
    let workspace_node = ensure_module_node(
        graph,
        &workspace_id,
        workspace_label,
        &["rust", "rust:workspace", "cargo"],
        &mut stats,
    );
    graph.merge_node_provenance(
        workspace_node,
        NodeProvenanceInput {
            source_path: Some(&workspace_manifest_rel),
            line_start: None,
            line_end: None,
            excerpt: None,
            namespace: Some("rust:cargo"),
            canonical: true,
        },
    );

    let workspace_members: HashSet<PackageId> =
        metadata.workspace_members.iter().cloned().collect();
    let package_index: HashMap<PackageId, &Package> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();

    let mut package_nodes: HashMap<PackageId, m1nd_core::types::NodeId> = HashMap::new();
    for package_id in &workspace_members {
        let Some(package) = package_index.get(package_id) else {
            continue;
        };

        let manifest_path = PathBuf::from(package.manifest_path.as_str());
        let manifest_rel = relative_to_root(root, &manifest_path)
            .unwrap_or_else(|| package.manifest_path.as_str().to_string());
        let crate_id = format!("cargo::crate::{}::{}", manifest_rel, package.name);
        let crate_node = ensure_module_node(
            graph,
            &crate_id,
            &package.name,
            &["rust", "rust:crate", "cargo"],
            &mut stats,
        );
        graph.merge_node_provenance(
            crate_node,
            NodeProvenanceInput {
                source_path: Some(&manifest_rel),
                line_start: None,
                line_end: None,
                excerpt: None,
                namespace: Some("rust:cargo"),
                canonical: true,
            },
        );
        add_edge_once(
            graph,
            workspace_node,
            crate_node,
            "contains",
            FiniteF32::new(1.0),
            EdgeDirection::Bidirectional,
            FiniteF32::new(0.85),
            &mut stats,
        );
        package_nodes.insert(package_id.clone(), crate_node);
    }

    for package_id in &workspace_members {
        let Some(package) = package_index.get(package_id) else {
            continue;
        };
        let Some(&crate_node) = package_nodes.get(package_id) else {
            continue;
        };

        attach_package_files(graph, root, package, crate_node, &mut stats);
    }

    let workspace_by_name: HashMap<&str, PackageId> = metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
        .map(|package| (package.name.as_str(), package.id.clone()))
        .collect();

    for package_id in &workspace_members {
        let Some(package) = package_index.get(package_id) else {
            continue;
        };
        let Some(&source_node) = package_nodes.get(package_id) else {
            continue;
        };

        for dependency in &package.dependencies {
            if dependency.kind != DependencyKind::Normal
                && dependency.kind != DependencyKind::Unknown
            {
                continue;
            }

            if let Some(target_package_id) = workspace_by_name.get(dependency.name.as_str()) {
                if let Some(&target_node) = package_nodes.get(target_package_id) {
                    add_edge_once(
                        graph,
                        source_node,
                        target_node,
                        "depends_on",
                        FiniteF32::new(0.75),
                        EdgeDirection::Forward,
                        FiniteF32::new(0.7),
                        &mut stats,
                    );
                    continue;
                }
            }

            let external_id = format!("cargo::dep::{}", dependency.name);
            let dep_node = ensure_module_node(
                graph,
                &external_id,
                &dependency.name,
                &["rust", "rust:dependency", "cargo", "external"],
                &mut stats,
            );
            add_edge_once(
                graph,
                source_node,
                dep_node,
                "depends_on",
                FiniteF32::new(0.55),
                EdgeDirection::Forward,
                FiniteF32::new(0.55),
                &mut stats,
            );
        }
    }

    Ok(stats)
}

fn load_metadata(root: &Path) -> Option<Metadata> {
    let mut cmd = MetadataCommand::new();
    cmd.current_dir(root);
    cmd.no_deps();
    cmd.exec().ok()
}

fn ensure_module_node(
    graph: &mut m1nd_core::graph::Graph,
    external_id: &str,
    label: &str,
    tags: &[&str],
    stats: &mut CargoWorkspaceStats,
) -> m1nd_core::types::NodeId {
    if let Some(node) = graph.resolve_id(external_id) {
        return node;
    }

    let node = graph
        .add_node(external_id, label, NodeType::Module, tags, 0.0, 0.2)
        .expect("cargo workspace node creation should be valid");
    stats.nodes_added += 1;
    node
}

fn attach_package_files(
    graph: &mut m1nd_core::graph::Graph,
    root: &Path,
    package: &Package,
    crate_node: m1nd_core::types::NodeId,
    stats: &mut CargoWorkspaceStats,
) {
    let Some(package_dir) = PathBuf::from(package.manifest_path.as_str())
        .parent()
        .map(Path::to_path_buf)
    else {
        return;
    };
    let Some(package_rel) = relative_to_root(root, &package_dir) else {
        return;
    };
    let package_rel = if package_rel == "." {
        String::new()
    } else {
        package_rel
    };

    let mut owned_files = Vec::new();
    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }
        let external_id = external_id_for_node(graph, i);
        let Some(external_id) = external_id else {
            continue;
        };
        let Some(file_rel) = external_id.strip_prefix("file::") else {
            continue;
        };

        let belongs = if package_rel.is_empty() {
            true
        } else {
            file_rel == package_rel || file_rel.starts_with(&format!("{package_rel}/"))
        };
        if belongs {
            owned_files.push(m1nd_core::types::NodeId::new(i as u32));
        }
    }

    for file_node in owned_files {
        add_edge_once(
            graph,
            crate_node,
            file_node,
            "contains",
            FiniteF32::new(0.95),
            EdgeDirection::Bidirectional,
            FiniteF32::new(0.8),
            stats,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn add_edge_once(
    graph: &mut m1nd_core::graph::Graph,
    source: m1nd_core::types::NodeId,
    target: m1nd_core::types::NodeId,
    relation: &str,
    weight: FiniteF32,
    direction: EdgeDirection,
    causal_strength: FiniteF32,
    stats: &mut CargoWorkspaceStats,
) {
    let rel = graph.strings.lookup(relation);
    let already_exists = graph.csr.pending_edges.iter().any(|edge| {
        edge.source == source
            && edge.target == target
            && Some(edge.relation) == rel
            && edge.direction == direction
    });
    if already_exists {
        return;
    }

    if graph
        .add_edge(
            source,
            target,
            relation,
            weight,
            direction,
            false,
            causal_strength,
        )
        .is_ok()
    {
        stats.edges_added += 1;
    }
}

fn external_id_for_node(graph: &m1nd_core::graph::Graph, index: usize) -> Option<String> {
    for (interned, &node) in &graph.id_to_node {
        if node.as_usize() == index {
            return Some(graph.strings.resolve(*interned).to_string());
        }
    }
    None
}

fn relative_to_root(root: &Path, path: &Path) -> Option<String> {
    let root = root.canonicalize().ok()?;
    let candidate = path.canonicalize().ok()?;
    let rel = candidate.strip_prefix(root).ok()?;
    let rel_str = rel.to_string_lossy();
    if rel_str.is_empty() {
        Some(".".to_string())
    } else {
        Some(rel_str.to_string())
    }
}
