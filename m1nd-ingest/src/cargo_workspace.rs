use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId};
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::NodeProvenanceInput;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ownership::{graph_has_edge, OwnedEdgeClaimV1, OwnershipDeltaV1};

#[derive(Clone, Debug, Default)]
pub struct CargoWorkspaceStats {
    pub nodes_added: u64,
    pub edges_added: u64,
    pub workspace_members_expected: u64,
    pub workspace_members_accounted: u64,
    pub dependency_inputs_expected: u64,
    pub dependency_inputs_accounted: u64,
    pub package_file_links_expected: u64,
    pub package_file_links_accounted: u64,
    pub ownership: OwnershipDeltaV1,
}

pub fn enrich_rust_workspace(
    graph: &mut m1nd_core::graph::Graph,
    root: &Path,
) -> M1ndResult<CargoWorkspaceStats> {
    let metadata = match load_metadata(root) {
        Ok(Some(metadata)) => metadata,
        // No Cargo.toml — not a cargo project, nothing to enrich.
        Ok(None) => return Ok(CargoWorkspaceStats::default()),
        // Best-effort: `cargo metadata` can fail for reasons orthogonal to the
        // source graph — a workspace MEMBER ingested in isolation (its Cargo.toml
        // inherits `x.workspace = true`, but the snapshot carries no workspace
        // root), or a host without cargo on PATH. Workspace/dependency enrichment
        // is ADDITIVE graph metadata, never the core of ingesting source files, so
        // a metadata failure degrades to an un-enriched graph instead of failing
        // the whole read-only ingest.
        Err(error) => {
            eprintln!("[m1nd ingest] cargo workspace enrichment skipped: {error}");
            return Ok(CargoWorkspaceStats::default());
        }
    };

    let workspace_root = PathBuf::from(metadata.workspace_root.as_str());
    let workspace_manifest = workspace_root.join("Cargo.toml");
    let workspace_manifest_rel = relative_to_root(root, &workspace_manifest)?;
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
        &workspace_manifest_rel,
        &mut stats,
    )?;
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
    let mut ordered_workspace_members = metadata.workspace_members.clone();
    ordered_workspace_members.sort_by_key(|member| member.to_string());
    let package_index: HashMap<PackageId, &Package> = metadata
        .packages
        .iter()
        .map(|package| (package.id.clone(), package))
        .collect();

    let mut package_nodes: HashMap<PackageId, m1nd_core::types::NodeId> = HashMap::new();
    let mut package_manifests: HashMap<PackageId, String> = HashMap::new();
    stats.workspace_members_expected = ordered_workspace_members.len() as u64;
    for package_id in &ordered_workspace_members {
        let package = package_index.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo metadata workspace member {package_id} has no package record"
            ))
        })?;

        let manifest_path = PathBuf::from(package.manifest_path.as_str());
        let manifest_rel = relative_to_root(root, &manifest_path)?;
        let crate_id = format!("cargo::crate::{}::{}", manifest_rel, package.name);
        let crate_node = ensure_module_node(
            graph,
            &crate_id,
            &package.name,
            &["rust", "rust:crate", "cargo"],
            &manifest_rel,
            &mut stats,
        )?;
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
            &workspace_manifest_rel,
            &mut stats,
        )?;
        package_manifests.insert(package_id.clone(), manifest_rel);
        package_nodes.insert(package_id.clone(), crate_node);
        stats.workspace_members_accounted += 1;
    }

    for package_id in &ordered_workspace_members {
        let package = package_index.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo metadata workspace member {package_id} disappeared before file binding"
            ))
        })?;
        let &crate_node = package_nodes.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo workspace member {package_id} has no accounted graph node"
            ))
        })?;
        let manifest_rel = package_manifests.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo workspace member {package_id} has no accounted manifest identity"
            ))
        })?;
        attach_package_files(graph, root, package, crate_node, manifest_rel, &mut stats)?;
    }

    let mut workspace_by_name: HashMap<&str, PackageId> = HashMap::new();
    for package in metadata
        .packages
        .iter()
        .filter(|package| workspace_members.contains(&package.id))
    {
        if workspace_by_name
            .insert(package.name.as_str(), package.id.clone())
            .is_some()
        {
            return Err(M1ndError::IngestError(format!(
                "cargo workspace package name is not bijective: {:?}",
                package.name
            )));
        }
    }

    for package_id in &ordered_workspace_members {
        let package = package_index.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo metadata workspace member {package_id} disappeared before dependency binding"
            ))
        })?;
        let &source_node = package_nodes.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo workspace dependency source {package_id} has no graph node"
            ))
        })?;
        let source_manifest = package_manifests.get(package_id).ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo workspace dependency source {package_id} has no manifest identity"
            ))
        })?;

        for dependency in &package.dependencies {
            if dependency.kind != DependencyKind::Normal
                && dependency.kind != DependencyKind::Unknown
            {
                continue;
            }
            stats.dependency_inputs_expected += 1;

            if let Some(target_package_id) = workspace_by_name.get(dependency.name.as_str()) {
                let &target_node = package_nodes.get(target_package_id).ok_or_else(|| {
                    M1ndError::IngestError(format!(
                        "cargo dependency {:?} resolved to workspace member {target_package_id} without a graph node",
                        dependency.name
                    ))
                })?;
                add_edge_once(
                    graph,
                    source_node,
                    target_node,
                    "depends_on",
                    FiniteF32::new(0.75),
                    EdgeDirection::Forward,
                    FiniteF32::new(0.7),
                    source_manifest,
                    &mut stats,
                )?;
                stats.dependency_inputs_accounted += 1;
                continue;
            }

            let external_id = format!("cargo::dep::{}", dependency.name);
            let dep_node = ensure_module_node(
                graph,
                &external_id,
                &dependency.name,
                &["rust", "rust:dependency", "cargo", "external"],
                source_manifest,
                &mut stats,
            )?;
            add_edge_once(
                graph,
                source_node,
                dep_node,
                "depends_on",
                FiniteF32::new(0.55),
                EdgeDirection::Forward,
                FiniteF32::new(0.55),
                source_manifest,
                &mut stats,
            )?;
            stats.dependency_inputs_accounted += 1;
        }
    }

    if stats.workspace_members_expected != stats.workspace_members_accounted
        || stats.dependency_inputs_expected != stats.dependency_inputs_accounted
        || stats.package_file_links_expected != stats.package_file_links_accounted
    {
        return Err(M1ndError::IngestError(format!(
            "cargo enrichment accounting mismatch: members={}/{}, dependencies={}/{}, file_links={}/{}",
            stats.workspace_members_accounted,
            stats.workspace_members_expected,
            stats.dependency_inputs_accounted,
            stats.dependency_inputs_expected,
            stats.package_file_links_accounted,
            stats.package_file_links_expected
        )));
    }

    Ok(stats)
}

fn load_metadata(root: &Path) -> M1ndResult<Option<Metadata>> {
    if !root.join("Cargo.toml").is_file() {
        return Ok(None);
    }
    let mut cmd = MetadataCommand::new();
    cmd.current_dir(root);
    cmd.no_deps();
    cmd.exec()
        .map(Some)
        .map_err(|error| M1ndError::IngestError(format!("cargo metadata failed: {error}")))
}

fn ensure_module_node(
    graph: &mut m1nd_core::graph::Graph,
    external_id: &str,
    label: &str,
    tags: &[&str],
    owner_source_key: &str,
    stats: &mut CargoWorkspaceStats,
) -> M1ndResult<m1nd_core::types::NodeId> {
    if !crate::is_valid_external_id(external_id)
        || !crate::is_valid_relative_file_path(owner_source_key)
    {
        return Err(M1ndError::IngestError(format!(
            "cargo enrichment produced invalid identity: node={external_id:?}, owner={owner_source_key:?}"
        )));
    }
    if let Some(node) = graph.resolve_id(external_id) {
        stats.ownership.claim_node(owner_source_key, external_id);
        return Ok(node);
    }

    let node = graph.add_node(external_id, label, NodeType::Module, tags, 0.0, 0.2)?;
    stats.nodes_added += 1;
    stats.ownership.claim_node(owner_source_key, external_id);
    Ok(node)
}

fn attach_package_files(
    graph: &mut m1nd_core::graph::Graph,
    root: &Path,
    package: &Package,
    crate_node: m1nd_core::types::NodeId,
    owner_source_key: &str,
    stats: &mut CargoWorkspaceStats,
) -> M1ndResult<()> {
    let package_dir = PathBuf::from(package.manifest_path.as_str())
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo package manifest has no parent: {}",
                package.manifest_path
            ))
        })?;
    let package_rel = relative_to_root(root, &package_dir)?;
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
        let external_id = external_id_for_node(graph, i)?;
        let file_rel = external_id.strip_prefix("file::").ok_or_else(|| {
            M1ndError::IngestError(format!(
                "cargo enrichment encountered File slot {i} with non-file identity {external_id:?}"
            ))
        })?;
        if !crate::is_valid_relative_file_path(file_rel) {
            return Err(M1ndError::IngestError(format!(
                "cargo enrichment encountered non-bijective file identity {external_id:?}"
            )));
        }

        let belongs = if package_rel.is_empty() {
            true
        } else {
            file_rel == package_rel || file_rel.starts_with(&format!("{package_rel}/"))
        };
        if belongs {
            owned_files.push(m1nd_core::types::NodeId::new(i as u32));
            stats.package_file_links_expected += 1;
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
            owner_source_key,
            stats,
        )?;
        stats.package_file_links_accounted += 1;
    }
    Ok(())
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
    owner_source_key: &str,
    stats: &mut CargoWorkspaceStats,
) -> M1ndResult<()> {
    let source_external_id = external_id_for_node(graph, source.as_usize())?;
    let target_external_id = external_id_for_node(graph, target.as_usize())?;
    let already_exists = graph_has_edge(graph, source, target, relation, direction, false);
    let represented = if already_exists {
        true
    } else {
        match graph.add_edge(
            source,
            target,
            relation,
            weight,
            direction,
            false,
            causal_strength,
        ) {
            Ok(_) => {
                stats.edges_added += 1;
                true
            }
            Err(error) => {
                if graph_has_edge(graph, source, target, relation, direction, false) {
                    true
                } else {
                    return Err(error);
                }
            }
        }
    };

    if !represented {
        return Err(M1ndError::IngestError(format!(
            "cargo enrichment edge was not represented: {source_external_id:?} -> {target_external_id:?} ({relation})"
        )));
    }
    stats.ownership.claim_edge(OwnedEdgeClaimV1 {
        source_key: owner_source_key.to_string(),
        source: source_external_id,
        target: target_external_id,
        relation: relation.to_string(),
        direction: if direction == EdgeDirection::Bidirectional {
            1
        } else {
            0
        },
        inhibitory: false,
    });
    Ok(())
}

fn external_id_for_node(graph: &m1nd_core::graph::Graph, index: usize) -> M1ndResult<String> {
    if index >= graph.num_nodes() as usize {
        return Err(M1ndError::IngestError(format!(
            "cargo enrichment referenced out-of-range node slot {index}"
        )));
    }
    let mut found = None;
    for (interned, &node) in &graph.id_to_node {
        if node.as_usize() == index {
            if found.is_some() {
                return Err(M1ndError::IngestError(format!(
                    "cargo enrichment encountered multiply-identified node slot {index}"
                )));
            }
            found = Some(graph.strings.resolve(*interned).to_string());
        }
    }
    found.ok_or_else(|| {
        M1ndError::IngestError(format!(
            "cargo enrichment encountered orphan node slot {index}"
        ))
    })
}

fn relative_to_root(root: &Path, path: &Path) -> M1ndResult<String> {
    let root = root.canonicalize().map_err(|error| {
        M1ndError::IngestError(format!(
            "cargo root canonicalization failed for {}: {error}",
            root.display()
        ))
    })?;
    let candidate = path.canonicalize().map_err(|error| {
        M1ndError::IngestError(format!(
            "cargo path canonicalization failed for {}: {error}",
            path.display()
        ))
    })?;
    let rel = candidate.strip_prefix(&root).map_err(|_| {
        M1ndError::IngestError(format!(
            "cargo metadata path {} escapes governed root {}",
            candidate.display(),
            root.display()
        ))
    })?;
    let rel_str = rel.to_str().ok_or_else(|| {
        M1ndError::IngestError(format!(
            "cargo relative path is not valid UTF-8: {}",
            rel.display()
        ))
    })?;
    if rel_str.is_empty() {
        return Ok(".".to_string());
    }
    #[cfg(windows)]
    let relative = rel_str.replace('\\', "/");
    #[cfg(not(windows))]
    let relative = rel_str.to_string();
    if !crate::is_valid_relative_file_path(&relative) {
        return Err(M1ndError::IngestError(format!(
            "cargo relative path is not bijective: {relative:?}"
        )));
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::{external_id_for_node, relative_to_root};
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;
    use std::path::{Path, PathBuf};

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "m1nd-cargo-workspace-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&path).expect("tempdir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn relative_cargo_paths_preserve_exact_identity() {
        let temp = TempTree::new("exact-path");
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        let normal = temp.path().join("src/lib.rs");
        std::fs::write(&normal, "pub fn normal() {}\n").unwrap();
        assert_eq!(
            relative_to_root(temp.path(), &normal).unwrap(),
            "src/lib.rs"
        );

        #[cfg(unix)]
        {
            let marginal = temp.path().join(" src.rs");
            std::fs::write(&marginal, "pub fn marginal() {}\n").unwrap();
            assert!(relative_to_root(temp.path(), &marginal).is_err());

            let backslash = temp.path().join("src\\lib.rs");
            std::fs::write(&backslash, "pub fn backslash() {}\n").unwrap();
            assert!(relative_to_root(temp.path(), &backslash).is_err());
        }
    }

    #[test]
    fn cargo_node_identity_lookup_rejects_orphan_and_multiple_slots() {
        let mut graph = Graph::new();
        let node = graph
            .add_node("node::one", "one", NodeType::Module, &[], 0.0, 0.0)
            .unwrap();
        assert_eq!(
            external_id_for_node(&graph, node.as_usize()).unwrap(),
            "node::one"
        );

        let alias = graph.strings.get_or_intern("node::alias");
        graph.id_to_node.insert(alias, node);
        assert!(external_id_for_node(&graph, node.as_usize()).is_err());

        graph.id_to_node.clear();
        assert!(external_id_for_node(&graph, node.as_usize()).is_err());
    }
}
