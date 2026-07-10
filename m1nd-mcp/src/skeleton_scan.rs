//! Human View v2 F0c-a — candidate skeleton scan engine.
//!
//! This module is intentionally pure: callers inject a graph-shaped view, a
//! Louvain assignment vector (when available), and the repo file list. The MCP
//! handler owns graph locking, git file discovery, OCC, and persistence.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use m1nd_core::graph::Graph;
use m1nd_core::topology::{CommunityDetector, CommunityResult};
use m1nd_core::types::{CommunityId, EdgeIdx, NodeId, NodeType};
use serde::Serialize;
use serde_json::json;

use crate::system_blocks::{
    CandidateMeta, Layout, MembershipEntry, MembershipRole, MembershipSource, NamedBy,
    ReceiptContract, SeedFile, SeedRatification, SeedRepo, SeedSkeleton, SeedSkeletonState, Socket,
    Sockets, SystemBlock, SystemBlockKind, SystemBlockState, UnmappedDefaultAction, UnmappedPolicy,
    SYSTEM_BLOCK_SEED_SCHEMA,
};

const GRAPH_COHESION_EDGE_FLOOR: usize = 5;
const DEFAULT_TINY_FILE_THRESHOLD: usize = 30;
/// The block anchor is a directory module: the smallest ancestor directory that
/// aggregates at least this many files. Directories below the floor bubble up to
/// their parent (never dropping a file) until they aggregate or reach the root.
/// This is what keeps a sparse graph from fragmenting into one block per file.
const DIRECTORY_MODULE_FLOOR: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateNamingMode {
    Auto,
    Heuristic,
}

impl CandidateNamingMode {
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        match value.unwrap_or("auto") {
            "auto" => Ok(Self::Auto),
            "heuristic" => Ok(Self::Heuristic),
            other => Err(format!(
                "naming must be \"auto\" or \"heuristic\", got \"{other}\""
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkeletonScanOptions {
    pub naming: CandidateNamingMode,
    pub tiny_file_threshold: usize,
    pub graph_cohesion_edge_floor: usize,
    /// Minimum files a directory module must aggregate before it becomes a block
    /// anchor (and the minimum community size that can split a module). Defaults
    /// to [`DIRECTORY_MODULE_FLOOR`].
    pub directory_module_floor: usize,
}

impl Default for SkeletonScanOptions {
    fn default() -> Self {
        Self {
            naming: CandidateNamingMode::Auto,
            tiny_file_threshold: DEFAULT_TINY_FILE_THRESHOLD,
            graph_cohesion_edge_floor: GRAPH_COHESION_EDGE_FLOOR,
            directory_module_floor: DIRECTORY_MODULE_FLOOR,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkeletonScanInput {
    pub repo_id: String,
    /// Seed-facing root. Keep this repo-relative (`.`) unless a caller has a
    /// public, non-absolute display root.
    pub repo_root: String,
    pub source_commit: String,
    pub repo_files: Vec<String>,
    pub nodes: Vec<SkeletonGraphNode>,
    /// Louvain community per NodeId index. Empty/short means no trusted community
    /// signal and triggers the declared fallbacks.
    pub assignments: Vec<Option<u32>>,
    pub edges: Vec<SkeletonGraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkeletonGraphNode {
    pub external_id: String,
    pub kind: NodeType,
}

#[derive(Debug, Clone)]
pub struct SkeletonGraphEdge {
    pub source: usize,
    pub target: usize,
    pub relation: Option<String>,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SkeletonScanReport {
    pub algorithm: String,
    pub repo_file_count: usize,
    pub graph_node_count: usize,
    pub graph_edge_count: usize,
    pub block_count: usize,
    pub claimed_file_count: usize,
    pub coverage_ratio: f64,
    pub graph_cohesion_edge_floor: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub multi_owner_seams: Vec<MultiOwnerSeam>,
    pub unmapped_total: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unmapped: Vec<String>,
    pub blocks: Vec<CandidateBlockReport>,
    pub naming: NamingReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MultiOwnerSeam {
    pub path: String,
    pub communities: Vec<u32>,
    pub block_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateBlockReport {
    pub block_id: String,
    pub name: String,
    pub member_count: usize,
    pub membership_count: usize,
    pub dominant_directory: String,
    pub candidate_meta: CandidateMeta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NamingReport {
    pub requested: String,
    pub applied: String,
    pub runner_available: bool,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonScanOutput {
    pub seed: SeedFile,
    pub report: SkeletonScanReport,
    /// F11-b: one naming packet per emitted block (member paths + dominant kinds +
    /// top symbols, NEVER file bodies) — what the handler sends to a live
    /// naming-runner. Internal to the scan→naming pipeline; not part of the verb's
    /// JSON output.
    pub naming_packets: Vec<crate::naming_runner::BlockNamingPacket>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum GroupKey {
    /// A directory module — the DIR-FIRST block anchor. `community` is `Some`
    /// when Louvain refined the module into per-community sub-blocks, or when the
    /// seam-merge promoted a set of sibling modules into a shared block; `None`
    /// is an unrefined whole module.
    Module {
        directory: String,
        community: Option<u32>,
    },
    /// No-edge fallback grouping: files bucketed by parent directory + extension.
    Directory {
        directory: String,
        extension: String,
    },
}

impl GroupKey {
    fn stable_label(&self) -> String {
        match self {
            GroupKey::Module {
                directory,
                community: Some(community),
            } => format!("mod-{directory}-c{community}"),
            GroupKey::Module {
                directory,
                community: None,
            } => format!("mod-{directory}"),
            GroupKey::Directory {
                directory,
                extension,
            } => format!("dir-{directory}-{extension}"),
        }
    }

    fn display_dir(&self) -> &str {
        match self {
            GroupKey::Module { directory, .. } | GroupKey::Directory { directory, .. } => directory,
        }
    }
}

#[derive(Debug, Clone)]
struct CandidateGroup {
    key: GroupKey,
    members: BTreeMap<String, MembershipRole>,
    graph_members: BTreeSet<String>,
    communities: BTreeSet<u32>,
}

impl CandidateGroup {
    fn new(key: GroupKey) -> Self {
        Self {
            key,
            members: BTreeMap::new(),
            graph_members: BTreeSet::new(),
            communities: BTreeSet::new(),
        }
    }

    fn add(
        &mut self,
        path: &str,
        role: MembershipRole,
        graph_backed: bool,
        community: Option<u32>,
    ) {
        self.members
            .entry(path.to_string())
            .and_modify(|existing| {
                if *existing != MembershipRole::Shared {
                    *existing = role;
                }
            })
            .or_insert(role);
        if graph_backed {
            self.graph_members.insert(path.to_string());
        }
        if let Some(c) = community {
            self.communities.insert(c);
        }
    }
}

#[derive(Debug, Clone)]
struct FileClaim {
    primary: Option<u32>,
    shared: Vec<u32>,
    graph_backed: bool,
}

impl FileClaim {
    fn unmapped() -> Self {
        Self {
            primary: None,
            shared: Vec::new(),
            graph_backed: false,
        }
    }
}

pub fn scan_skeleton(input: SkeletonScanInput, options: SkeletonScanOptions) -> SkeletonScanOutput {
    let repo_files = normalize_files(input.repo_files.clone());
    let has_trusted_edges = !input.edges.is_empty();
    let has_trusted_assignments = input.assignments.len() >= input.nodes.len()
        && input.assignments.iter().any(|c| c.is_some());
    let top_dirs: BTreeSet<String> = repo_files.iter().map(|p| top_dir(p)).collect();
    let single_src_code_repo = top_dirs.len() == 1
        && top_dirs.contains("src")
        && has_trusted_edges
        && has_trusted_assignments;
    let floor = options.directory_module_floor.max(1);

    let (algorithm, mut groups, seams) = if !has_trusted_edges || !has_trusted_assignments {
        (
            "directory_extension_no_edge".to_string(),
            directory_extension_groups(&repo_files),
            Vec::new(),
        )
    } else {
        // DIR-FIRST: every file first anchors to its directory module (the
        // smallest ancestor dir aggregating >= floor files), so a degenerate
        // Louvain can never fragment the map into one block per file.
        let (claims, raw_seams) = collapse_file_claims(&input, &repo_files);
        let file_module = directory_modules(&repo_files, floor);
        if repo_files.len() < options.tiny_file_threshold && !single_src_code_repo {
            // Tiny repo: directory modules only — Louvain is unreliable at this
            // scale, so no community refine and no seam-merge.
            (
                "directory_module_tiny".to_string(),
                dir_first_groups(&repo_files, &claims, &file_module, floor, false),
                Vec::new(),
            )
        } else {
            // Louvain REFINES each module (split only when >= 2 communities each
            // clear the floor); then the seam-merge stitches sibling modules that
            // share a dominant community.
            let mut groups = dir_first_groups(&repo_files, &claims, &file_module, floor, true);
            apply_costura(&mut groups, &claims);
            (
                "directory_module_louvain_refine".to_string(),
                groups,
                raw_seams,
            )
        }
    };

    groups.retain(|_, group| !group.members.is_empty());

    let block_id_by_key = assign_block_ids(&input.repo_id, groups.keys());
    let mut path_to_blocks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, group) in &groups {
        let block_id = block_id_by_key
            .get(key)
            .expect("block ids assigned for every group");
        for path in group.members.keys() {
            path_to_blocks
                .entry(path.clone())
                .or_default()
                .push(block_id.clone());
        }
    }
    for blocks in path_to_blocks.values_mut() {
        blocks.sort();
        blocks.dedup();
    }

    let mut seams = seams
        .into_iter()
        .filter_map(|mut seam| {
            // Resolve the seam directly to the blocks that actually own the file.
            // After DIR-FIRST refine a tie only stays a real seam when the file
            // still lands in >= 2 candidate blocks; otherwise it was absorbed.
            let ids = path_to_blocks.get(&seam.path).cloned().unwrap_or_default();
            if ids.len() < 2 {
                return None;
            }
            seam.block_ids = ids;
            Some(seam)
        })
        .collect::<Vec<_>>();
    seams.sort_by(|a, b| a.path.cmp(&b.path));

    let socket_pairs = aggregate_sockets(&input, &path_to_blocks);

    // F11-b: symbol names per member file — for the naming packets. One pass over
    // the nodes; a symbol is the external-id tail after `file::<path>::` (a pure
    // file node has no tail and contributes no symbol). No file bodies, ever.
    let mut symbols_by_path: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in &input.nodes {
        if let Some(rest) = node.external_id.strip_prefix("file::") {
            if let Some((path, tail)) = rest.split_once("::") {
                if !path.trim().is_empty() && !tail.trim().is_empty() {
                    symbols_by_path
                        .entry(path.replace('\\', "/"))
                        .or_default()
                        .insert(tail.to_string());
                }
            }
        }
    }

    let mut blocks = Vec::new();
    let mut block_reports = Vec::new();
    let mut naming_packets = Vec::new();
    let mut claimed_files = BTreeSet::new();
    for key in groups.keys().cloned().collect::<Vec<_>>() {
        let group = groups.get(&key).expect("group exists");
        let block_id = block_id_by_key.get(&key).expect("block id").clone();
        let dominant_directory = dominant_directory(group.members.keys());
        let (name, purpose) = heuristic_name_and_purpose(&key, group, &input.nodes);
        let membership = emit_membership(group, &repo_files);
        for path in group.members.keys() {
            claimed_files.insert(path.clone());
        }
        let (edge_sample_size, graph_cohesion) = graph_cohesion_for_block(
            group,
            &input,
            &path_to_blocks,
            &block_id,
            options.graph_cohesion_edge_floor,
        );
        let meta = CandidateMeta {
            named_by: NamedBy::Heuristic,
            needs_owner_naming: true,
            graph_cohesion,
            edge_sample_size,
            directory_support: directory_support(group.members.keys()),
            coverage_ratio: if group.members.is_empty() {
                0.0
            } else {
                group.graph_members.len() as f64 / group.members.len() as f64
            },
            shared_member_count: group
                .members
                .values()
                .filter(|role| **role == MembershipRole::Shared)
                .count(),
        };
        let sockets = sockets_for_block(&block_id, &socket_pairs);
        let block = SystemBlock {
            block_id: block_id.clone(),
            name: name.clone(),
            purpose,
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: MembershipSource::Proposed,
            membership,
            sockets,
            receipt_contract: empty_receipt_contract(),
            receipts: Vec::new(),
            layout: Layout {
                x: None,
                y: None,
                locked: false,
                algorithm_seed: Some(json!({
                    "source": "skeleton_candidate",
                    "group_key": key.stable_label(),
                })),
                version: 1,
            },
            unmapped_residue: Vec::new(),
            membership_fingerprint: None,
            resolved_members: Vec::new(),
            pre_archive_state: None,
            candidate_meta: Some(meta.clone()),
        };
        // F11-b: the block's naming packet — capped member paths (the honest total
        // rides beside them), dominant kinds, and top symbols under its files.
        let top_symbols: Vec<String> = group
            .members
            .keys()
            .filter_map(|path| symbols_by_path.get(path))
            .flatten()
            .cloned()
            .collect::<BTreeSet<String>>()
            .into_iter()
            .take(crate::naming_runner::PACKET_SYMBOLS_CAP)
            .collect();
        naming_packets.push(crate::naming_runner::BlockNamingPacket {
            instruction: crate::naming_runner::PACKET_INSTRUCTION.to_string(),
            block_id: block_id.clone(),
            member_count: group.members.len(),
            member_paths: group
                .members
                .keys()
                .take(crate::naming_runner::PACKET_MEMBER_PATHS_CAP)
                .cloned()
                .collect(),
            dominant_kinds: dominant_kinds_for_group(group, &input.nodes),
            top_symbols,
            dominant_directory: dominant_directory.clone(),
        });
        block_reports.push(CandidateBlockReport {
            block_id,
            name,
            member_count: group.members.len(),
            membership_count: block.membership.len(),
            dominant_directory,
            candidate_meta: meta,
        });
        blocks.push(block);
    }

    let unmapped: Vec<String> = repo_files
        .iter()
        .filter(|path| !claimed_files.contains(*path))
        .cloned()
        .collect();
    let coverage_ratio = if repo_files.is_empty() {
        1.0
    } else {
        claimed_files.len() as f64 / repo_files.len() as f64
    };

    let seed = SeedFile {
        schema: SYSTEM_BLOCK_SEED_SCHEMA.to_string(),
        repo: SeedRepo {
            repo_id: sanitize_repo_id(&input.repo_id),
            root: input.repo_root,
            source_commit: input.source_commit,
        },
        skeleton: SeedSkeleton {
            skeleton_id: format!("sk_{}_candidate", sanitize_slug(&input.repo_id)),
            version: 1,
            state: SeedSkeletonState::Candidate,
            ratification: SeedRatification {
                method: String::new(),
                ratifier: String::new(),
                ratified_at: String::new(),
                commit: String::new(),
            },
        },
        blocks,
        unmapped_policy: UnmappedPolicy {
            visible: true,
            default_action: UnmappedDefaultAction::LeaveUnmappedUntilRatified,
        },
    };

    let naming = NamingReport {
        requested: match options.naming {
            CandidateNamingMode::Auto => "auto".to_string(),
            CandidateNamingMode::Heuristic => "heuristic".to_string(),
        },
        applied: "heuristic".to_string(),
        runner_available: false,
        note: match options.naming {
            CandidateNamingMode::Auto => "naming-runner hook is backend-declared but not invoked in F0c-a; heuristic provisional names were used".to_string(),
            CandidateNamingMode::Heuristic => {
                "heuristic naming explicitly requested; every name requires owner review".to_string()
            }
        },
    };

    let report = SkeletonScanReport {
        algorithm,
        repo_file_count: repo_files.len(),
        graph_node_count: input.nodes.len(),
        graph_edge_count: input.edges.len(),
        block_count: seed.blocks.len(),
        claimed_file_count: claimed_files.len(),
        coverage_ratio,
        graph_cohesion_edge_floor: options.graph_cohesion_edge_floor,
        multi_owner_seams: seams,
        unmapped_total: unmapped.len(),
        unmapped,
        blocks: block_reports,
        naming,
    };

    SkeletonScanOutput {
        seed,
        report,
        naming_packets,
    }
}

pub fn scan_input_from_graph(
    graph: &Graph,
    repo_id: String,
    repo_root: String,
    source_commit: String,
    repo_files: Vec<String>,
) -> SkeletonScanInput {
    let n = graph.num_nodes() as usize;
    let node_to_ext = node_to_external_ids(graph);
    let nodes = (0..n)
        .map(|i| SkeletonGraphNode {
            external_id: node_to_ext.get(i).cloned().unwrap_or_default(),
            kind: graph.nodes.node_type[i],
        })
        .collect::<Vec<_>>();

    let assignments = if graph.finalized && graph.num_edges() > 0 && graph.num_nodes() > 0 {
        CommunityDetector::with_defaults()
            .detect(graph)
            .ok()
            .map(assignments_from_result)
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut edges = Vec::new();
    if graph.finalized {
        for src in 0..n {
            for edge_idx in graph.csr.out_range(NodeId::new(src as u32)) {
                let target = graph.csr.targets[edge_idx].as_usize();
                if target >= n {
                    continue;
                }
                let relation = graph
                    .strings
                    .try_resolve(graph.csr.relations[edge_idx])
                    .map(|s| s.to_string());
                edges.push(SkeletonGraphEdge {
                    source: src,
                    target,
                    relation,
                    weight: graph.csr.read_weight(EdgeIdx::new(edge_idx as u32)).get() as f64,
                });
            }
        }
    }

    SkeletonScanInput {
        repo_id,
        repo_root,
        source_commit,
        repo_files,
        nodes,
        assignments,
        edges,
    }
}

/// F11-c: just the graph's node view (external ids + kinds) — what the
/// `candidate_naming` route needs to build packets for STORE blocks without the
/// full scan input (no file list, no Louvain, no edges).
pub(crate) fn graph_nodes_for_naming(graph: &Graph) -> Vec<SkeletonGraphNode> {
    let n = graph.num_nodes() as usize;
    let node_to_ext = node_to_external_ids(graph);
    (0..n)
        .map(|i| SkeletonGraphNode {
            external_id: node_to_ext.get(i).cloned().unwrap_or_default(),
            kind: graph.nodes.node_type[i],
        })
        .collect()
}

/// F11-c: a naming packet for a STORE block (the `candidate_naming` route) — the
/// same packet shape the scan emits (§2a: member paths + dominant kinds + top
/// symbols, never file bodies), built from the block's stored membership. Member
/// paths prefer the reconcile cache (`resolved_members`, real files) and fall back
/// to the declared membership (which may carry globs — still honest naming
/// context). Graph nodes are matched against exact members or glob members.
pub(crate) fn naming_packet_for_store_block(
    block: &SystemBlock,
    nodes: &[SkeletonGraphNode],
) -> crate::naming_runner::BlockNamingPacket {
    let member_paths_full: Vec<String> = if block.resolved_members.is_empty() {
        block.membership.iter().map(|e| e.path.clone()).collect()
    } else {
        block.resolved_members.clone()
    };
    let exact: BTreeSet<&str> = member_paths_full
        .iter()
        .filter(|p| !crate::system_blocks::is_glob_pattern(p))
        .map(|p| p.as_str())
        .collect();
    let globs: Vec<glob::Pattern> = member_paths_full
        .iter()
        .filter(|p| crate::system_blocks::is_glob_pattern(p))
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    let mut kind_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut symbols: BTreeSet<String> = BTreeSet::new();
    for node in nodes {
        let Some(rest) = node.external_id.strip_prefix("file::") else {
            continue;
        };
        let (path, tail) = match rest.split_once("::") {
            Some((p, t)) => (p.replace('\\', "/"), Some(t)),
            None => (rest.replace('\\', "/"), None),
        };
        let claimed = exact.contains(path.as_str()) || globs.iter().any(|pat| pat.matches(&path));
        if !claimed {
            continue;
        }
        *kind_counts.entry(node_type_label(node.kind)).or_default() += 1;
        if let Some(t) = tail {
            if !t.trim().is_empty() {
                symbols.insert(t.to_string());
            }
        }
    }
    let mut kind_pairs: Vec<_> = kind_counts.into_iter().collect();
    kind_pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    crate::naming_runner::BlockNamingPacket {
        instruction: crate::naming_runner::PACKET_INSTRUCTION.to_string(),
        block_id: block.block_id.clone(),
        member_count: member_paths_full.len(),
        member_paths: member_paths_full
            .iter()
            .take(crate::naming_runner::PACKET_MEMBER_PATHS_CAP)
            .cloned()
            .collect(),
        dominant_kinds: kind_pairs
            .into_iter()
            .take(3)
            .map(|(kind, _)| kind.to_string())
            .collect(),
        top_symbols: symbols
            .into_iter()
            .take(crate::naming_runner::PACKET_SYMBOLS_CAP)
            .collect(),
        dominant_directory: dominant_directory(member_paths_full.iter()),
    }
}

fn assignments_from_result(result: CommunityResult) -> Vec<Option<u32>> {
    result
        .assignments
        .into_iter()
        .map(|CommunityId(id)| Some(id))
        .collect()
}

fn node_to_external_ids(graph: &Graph) -> Vec<String> {
    let n = graph.num_nodes() as usize;
    let mut node_to_ext = vec![String::new(); n];
    for (interned, node_id) in &graph.id_to_node {
        let idx = node_id.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }
    node_to_ext
}

fn normalize_files(files: Vec<String>) -> Vec<String> {
    let mut files: Vec<String> = files
        .into_iter()
        .map(|p| p.replace('\\', "/"))
        .filter(|p| !p.trim().is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

fn collapse_file_claims(
    input: &SkeletonScanInput,
    repo_files: &[String],
) -> (BTreeMap<String, FileClaim>, Vec<MultiOwnerSeam>) {
    let mut file_node_assignment: BTreeMap<String, u32> = BTreeMap::new();
    let mut symbol_votes: BTreeMap<String, BTreeMap<u32, f64>> = BTreeMap::new();

    for (idx, node) in input.nodes.iter().enumerate() {
        let Some(community) = input.assignments.get(idx).and_then(|c| *c) else {
            continue;
        };
        let Some((path, is_file_node)) = parent_file_from_external_id(&node.external_id) else {
            continue;
        };
        if is_file_node && node.kind == NodeType::File {
            file_node_assignment.insert(path, community);
        } else {
            *symbol_votes
                .entry(path)
                .or_default()
                .entry(community)
                .or_default() += node_vote_weight(node.kind);
        }
    }

    let mut claims = BTreeMap::new();
    let mut seams = Vec::new();
    for path in repo_files {
        if let Some(community) = file_node_assignment.get(path) {
            claims.insert(
                path.clone(),
                FileClaim {
                    primary: Some(*community),
                    shared: Vec::new(),
                    graph_backed: true,
                },
            );
            continue;
        }
        if let Some(votes) = symbol_votes.get(path) {
            let max = votes
                .values()
                .copied()
                .fold(f64::NEG_INFINITY, |acc, value| acc.max(value));
            let winners: Vec<u32> = votes
                .iter()
                .filter(|(_, weight)| (**weight - max).abs() <= f64::EPSILON)
                .map(|(community, _)| *community)
                .collect();
            if winners.len() == 1 {
                claims.insert(
                    path.clone(),
                    FileClaim {
                        primary: Some(winners[0]),
                        shared: Vec::new(),
                        graph_backed: true,
                    },
                );
            } else {
                seams.push(MultiOwnerSeam {
                    path: path.clone(),
                    communities: winners.clone(),
                    block_ids: Vec::new(),
                });
                claims.insert(
                    path.clone(),
                    FileClaim {
                        primary: None,
                        shared: winners,
                        graph_backed: true,
                    },
                );
            }
        } else {
            claims.insert(path.clone(), FileClaim::unmapped());
        }
    }
    (claims, seams)
}

/// DIR-FIRST anchor: map every file to its directory module — the smallest
/// ancestor directory that aggregates at least `floor` files. A thin sub-tree
/// bubbles its files up to the parent (never dropping one) until it aggregates
/// or reaches its top-level directory. Bubble-up is clamped at the top level: a
/// thin top-level directory keeps its own identity instead of dissolving into a
/// mixed Root, so a module never straddles unrelated top-level directories.
/// Files that live at the repo root (`"."`) form the legitimate Root module.
/// Deepest-first, so a thin child folds into its parent before the parent is
/// itself judged against the floor.
fn directory_modules(repo_files: &[String], floor: usize) -> BTreeMap<String, String> {
    let mut module: BTreeMap<String, String> = repo_files
        .iter()
        .map(|path| (path.clone(), parent_dir(path)))
        .collect();
    loop {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for dir in module.values() {
            *counts.entry(dir.clone()).or_default() += 1;
        }
        let target = counts
            .iter()
            .filter(|(dir, count)| {
                // Skip the root itself, and clamp at the top level: a top-level
                // directory (parent is root) never bubbles into the mixed Root.
                **count < floor && dir.as_str() != "." && parent_dir(dir) != "."
            })
            .max_by_key(|(dir, _)| dir_depth(dir))
            .map(|(dir, _)| dir.clone());
        let Some(target) = target else { break };
        let parent = parent_dir(&target);
        for dir in module.values_mut() {
            if *dir == target {
                *dir = parent.clone();
            }
        }
    }
    module
}

fn dir_depth(dir: &str) -> usize {
    if dir == "." {
        0
    } else {
        dir.matches('/').count() + 1
    }
}

/// Build candidate groups from directory modules. When `refine` is set, a module
/// holding >= 2 distinct communities that each clear `floor` is split into
/// per-community sub-blocks (this preserves the dense kernel/evidence case);
/// files whose community is small, absent, or off-module fold into the module's
/// dominant community. Cross-community ties keep the existing shared+seam rule.
fn dir_first_groups(
    repo_files: &[String],
    claims: &BTreeMap<String, FileClaim>,
    file_module: &BTreeMap<String, String>,
    floor: usize,
    refine: bool,
) -> BTreeMap<GroupKey, CandidateGroup> {
    let mut module_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in repo_files {
        let module = file_module
            .get(path)
            .cloned()
            .unwrap_or_else(|| parent_dir(path));
        module_files.entry(module).or_default().push(path.clone());
    }

    let mut groups: BTreeMap<GroupKey, CandidateGroup> = BTreeMap::new();
    for (module, files) in &module_files {
        let mut community_counts: BTreeMap<u32, usize> = BTreeMap::new();
        for path in files {
            if let Some(community) = claims.get(path).and_then(|claim| claim.primary) {
                *community_counts.entry(community).or_default() += 1;
            }
        }
        let big: BTreeSet<u32> = community_counts
            .iter()
            .filter(|(_, count)| **count >= floor)
            .map(|(community, _)| *community)
            .collect();
        let split = refine && big.len() >= 2;
        let dominant = community_counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(community, _)| *community);

        for path in files {
            let claim = claims
                .get(path)
                .cloned()
                .unwrap_or_else(FileClaim::unmapped);
            if !split {
                let key = GroupKey::Module {
                    directory: module.clone(),
                    community: None,
                };
                groups
                    .entry(key.clone())
                    .or_insert_with(|| CandidateGroup::new(key.clone()))
                    .add(
                        path,
                        MembershipRole::Primary,
                        claim.graph_backed,
                        claim.primary,
                    );
                continue;
            }

            let mut homes: Vec<u32> = Vec::new();
            if let Some(primary) = claim.primary {
                if big.contains(&primary) {
                    homes.push(primary);
                }
            } else {
                for community in &claim.shared {
                    if big.contains(community) && !homes.contains(community) {
                        homes.push(*community);
                    }
                }
            }
            if homes.is_empty() {
                // Small / absent / off-module community: fold into the dominant
                // (guaranteed to clear the floor whenever a module splits).
                if let Some(dom) = dominant {
                    let key = GroupKey::Module {
                        directory: module.clone(),
                        community: Some(dom),
                    };
                    groups
                        .entry(key.clone())
                        .or_insert_with(|| CandidateGroup::new(key.clone()))
                        .add(path, MembershipRole::Primary, claim.graph_backed, Some(dom));
                }
                continue;
            }
            let role = if homes.len() > 1 {
                MembershipRole::Shared
            } else {
                MembershipRole::Primary
            };
            for community in &homes {
                let key = GroupKey::Module {
                    directory: module.clone(),
                    community: Some(*community),
                };
                groups
                    .entry(key.clone())
                    .or_insert_with(|| CandidateGroup::new(key.clone()))
                    .add(path, role, claim.graph_backed, Some(*community));
            }
        }
    }
    groups
}

/// §2b seam-merge (costura): after refine, stitch sibling whole-modules (same
/// parent directory) that agree on a majority "dominant community" into one
/// block. Never merges across the repo root, so a block can never straddle
/// unrelated top-level directories.
fn apply_costura(
    groups: &mut BTreeMap<GroupKey, CandidateGroup>,
    claims: &BTreeMap<String, FileClaim>,
) {
    let mut buckets: BTreeMap<(String, u32), Vec<GroupKey>> = BTreeMap::new();
    for (key, group) in groups.iter() {
        let GroupKey::Module {
            directory,
            community: None,
        } = key
        else {
            continue;
        };
        let parent = parent_dir(directory);
        if parent == "." {
            continue;
        }
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        let mut total = 0usize;
        for path in group.members.keys() {
            if let Some(community) = claims.get(path).and_then(|claim| claim.primary) {
                *counts.entry(community).or_default() += 1;
                total += 1;
            }
        }
        let Some((dominant, best)) = counts
            .into_iter()
            .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        else {
            continue;
        };
        // Require a genuine majority of the module's graph-backed files, so a
        // scattered (degenerate-Louvain) module never triggers a merge.
        if best * 2 <= total {
            continue;
        }
        buckets
            .entry((parent, dominant))
            .or_default()
            .push(key.clone());
    }

    for ((parent, dominant), keys) in buckets {
        if keys.len() < 2 {
            continue;
        }
        let merged_key = GroupKey::Module {
            directory: parent,
            community: Some(dominant),
        };
        let mut merged = groups
            .remove(&merged_key)
            .unwrap_or_else(|| CandidateGroup::new(merged_key.clone()));
        for key in &keys {
            if let Some(group) = groups.remove(key) {
                for (path, role) in &group.members {
                    let graph_backed = group.graph_members.contains(path);
                    merged.add(
                        path,
                        *role,
                        graph_backed,
                        claims.get(path).and_then(|claim| claim.primary),
                    );
                }
            }
        }
        groups.insert(merged_key, merged);
    }
}

fn directory_extension_groups(repo_files: &[String]) -> BTreeMap<GroupKey, CandidateGroup> {
    let mut groups = BTreeMap::new();
    for path in repo_files {
        let key = GroupKey::Directory {
            directory: parent_dir(path),
            extension: extension(path),
        };
        groups
            .entry(key.clone())
            .or_insert_with(|| CandidateGroup::new(key.clone()))
            .add(path, MembershipRole::Primary, false, None);
    }
    groups
}

fn emit_membership(group: &CandidateGroup, repo_files: &[String]) -> Vec<MembershipEntry> {
    let mut by_parent: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in group.members.keys() {
        by_parent
            .entry(parent_dir(path))
            .or_default()
            .push(path.clone());
    }

    let mut emitted_paths = BTreeSet::new();
    let mut entries = Vec::new();
    for (dir, paths) in by_parent {
        let all_in_dir: Vec<&String> = repo_files.iter().filter(|p| parent_dir(p) == dir).collect();
        let can_glob = dir != "."
            && all_in_dir.len() > 1
            && (paths.len() as f64 / all_in_dir.len() as f64) >= 0.90
            && paths
                .iter()
                .all(|p| group.members.get(p) == Some(&MembershipRole::Primary));
        if can_glob {
            for p in &paths {
                emitted_paths.insert(p.clone());
            }
            entries.push(MembershipEntry {
                path: format!("{dir}/**"),
                role: MembershipRole::Primary,
                optional: false,
            });
        }
    }

    for (path, role) in &group.members {
        if emitted_paths.contains(path) {
            continue;
        }
        entries.push(MembershipEntry {
            path: path.clone(),
            role: *role,
            optional: false,
        });
    }
    entries.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| role_rank(a.role).cmp(&role_rank(b.role)))
    });
    entries
}

fn aggregate_sockets(
    input: &SkeletonScanInput,
    path_to_blocks: &BTreeMap<String, Vec<String>>,
) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for edge in &input.edges {
        let Some(src_node) = input.nodes.get(edge.source) else {
            continue;
        };
        let Some(tgt_node) = input.nodes.get(edge.target) else {
            continue;
        };
        let Some((src_path, _)) = parent_file_from_external_id(&src_node.external_id) else {
            continue;
        };
        let Some((tgt_path, _)) = parent_file_from_external_id(&tgt_node.external_id) else {
            continue;
        };
        let Some(src_blocks) = path_to_blocks.get(&src_path) else {
            continue;
        };
        let Some(tgt_blocks) = path_to_blocks.get(&tgt_path) else {
            continue;
        };
        for src_block in src_blocks {
            for tgt_block in tgt_blocks {
                if src_block == tgt_block {
                    continue;
                }
                out.entry((src_block.clone(), tgt_block.clone()))
                    .or_insert_with(|| {
                        edge.relation
                            .clone()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| "depends_on".to_string())
                    });
            }
        }
    }
    out
}

fn sockets_for_block(block_id: &str, socket_pairs: &BTreeMap<(String, String), String>) -> Sockets {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for ((src, tgt), relation) in socket_pairs {
        if src == block_id {
            outputs.push(Socket {
                to: Some(tgt.clone()),
                type_: Some(relation.clone()),
                alias: None,
                class_: None,
            });
        }
        if tgt == block_id {
            inputs.push(Socket {
                to: Some(src.clone()),
                type_: Some(relation.clone()),
                alias: None,
                class_: None,
            });
        }
    }
    Sockets {
        inputs,
        outputs,
        external: Vec::new(),
    }
}

fn graph_cohesion_for_block(
    group: &CandidateGroup,
    input: &SkeletonScanInput,
    path_to_blocks: &BTreeMap<String, Vec<String>>,
    block_id: &str,
    floor: usize,
) -> (usize, Option<f64>) {
    if input.edges.is_empty() {
        return (0, None);
    }
    let member_paths: HashSet<&str> = group.members.keys().map(|s| s.as_str()).collect();
    let mut sample = 0usize;
    let mut internal = 0usize;
    for edge in &input.edges {
        let Some(src_node) = input.nodes.get(edge.source) else {
            continue;
        };
        let Some(tgt_node) = input.nodes.get(edge.target) else {
            continue;
        };
        let Some((src_path, _)) = parent_file_from_external_id(&src_node.external_id) else {
            continue;
        };
        let Some((tgt_path, _)) = parent_file_from_external_id(&tgt_node.external_id) else {
            continue;
        };
        let source_in = member_paths.contains(src_path.as_str());
        let target_in = member_paths.contains(tgt_path.as_str());
        if !source_in && !target_in {
            continue;
        }
        sample += 1;
        let src_has_block = path_to_blocks
            .get(&src_path)
            .is_some_and(|blocks| blocks.iter().any(|b| b == block_id));
        let tgt_has_block = path_to_blocks
            .get(&tgt_path)
            .is_some_and(|blocks| blocks.iter().any(|b| b == block_id));
        if src_has_block && tgt_has_block {
            internal += 1;
        }
    }
    if sample < floor {
        (sample, None)
    } else {
        (sample, Some(internal as f64 / sample as f64))
    }
}

fn empty_receipt_contract() -> ReceiptContract {
    ReceiptContract {
        version: 1,
        required: Vec::new(),
        optional: Vec::new(),
        waived: Vec::new(),
        declared_by: None,
        declared_at: None,
    }
}

fn heuristic_name_and_purpose(
    key: &GroupKey,
    group: &CandidateGroup,
    nodes: &[SkeletonGraphNode],
) -> (String, String) {
    let dir = key.display_dir();
    let label = match key {
        GroupKey::Module { .. } => humanize_module(dir),
        GroupKey::Directory { .. } => humanize_path(dir),
    };
    let mut name = label.clone();
    if let GroupKey::Module {
        community: Some(community),
        ..
    } = key
    {
        if name == "Root" {
            name = format!("Root {}", ordinal_name(*community));
        }
    }
    let samples: Vec<String> = group.members.keys().take(3).cloned().collect();
    let dominant_kinds = dominant_kinds_for_group(group, nodes);
    let purpose = format!(
        "Provisional map for {} file{} around {}. Dominant graph kinds: {}. Sample: {}.",
        group.members.len(),
        if group.members.len() == 1 { "" } else { "s" },
        label.to_lowercase(),
        if dominant_kinds.is_empty() {
            "files".to_string()
        } else {
            dominant_kinds.join(", ")
        },
        if samples.is_empty() {
            "none".to_string()
        } else {
            samples.join(", ")
        }
    );
    (name, purpose)
}

fn dominant_kinds_for_group(group: &CandidateGroup, nodes: &[SkeletonGraphNode]) -> Vec<String> {
    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for node in nodes {
        if let Some((path, _)) = parent_file_from_external_id(&node.external_id) {
            if group.members.contains_key(&path) {
                *counts.entry(node_type_label(node.kind)).or_default() += 1;
            }
        }
    }
    let mut pairs: Vec<_> = counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    pairs
        .into_iter()
        .take(3)
        .map(|(kind, _)| kind.to_string())
        .collect()
}

fn node_type_label(kind: NodeType) -> &'static str {
    match kind {
        NodeType::File => "files",
        NodeType::Directory => "directories",
        NodeType::Function => "functions",
        NodeType::Class => "classes",
        NodeType::Struct => "structs",
        NodeType::Enum => "enums",
        NodeType::Type => "types",
        NodeType::Module => "modules",
        NodeType::Reference => "references",
        NodeType::Concept => "concepts",
        NodeType::Material => "materials",
        NodeType::Process => "processes",
        NodeType::Product => "products",
        NodeType::Supplier => "suppliers",
        NodeType::Regulatory => "regulatory nodes",
        NodeType::System => "systems",
        NodeType::Cost => "cost nodes",
        NodeType::Custom(_) => "custom nodes",
    }
}

fn assign_block_ids<'a>(
    repo_id: &str,
    keys: impl Iterator<Item = &'a GroupKey>,
) -> BTreeMap<GroupKey, String> {
    let repo = sanitize_slug(repo_id);
    let mut used = BTreeSet::new();
    let mut out = BTreeMap::new();
    for key in keys {
        let slug = sanitize_slug(key.display_dir());
        let hash = short_hash(&key.stable_label());
        let mut candidate = format!("sb_{repo}_{slug}_{hash}");
        if !used.insert(candidate.clone()) {
            let mut i = 2usize;
            loop {
                let alt = format!("{candidate}_{i}");
                if used.insert(alt.clone()) {
                    candidate = alt;
                    break;
                }
                i += 1;
            }
        }
        out.insert(key.clone(), candidate);
    }
    out
}

fn parent_file_from_external_id(external_id: &str) -> Option<(String, bool)> {
    let rest = external_id.strip_prefix("file::")?;
    if rest.trim().is_empty() {
        return None;
    }
    if let Some((path, _tail)) = rest.split_once("::") {
        if path.trim().is_empty() {
            None
        } else {
            Some((path.replace('\\', "/"), false))
        }
    } else {
        Some((rest.replace('\\', "/"), true))
    }
}

fn node_vote_weight(kind: NodeType) -> f64 {
    match kind {
        NodeType::Class | NodeType::Struct | NodeType::Enum | NodeType::Module => 2.0,
        NodeType::Function => 1.5,
        NodeType::Type => 1.25,
        NodeType::Reference | NodeType::Concept => 0.75,
        NodeType::File => 1.0,
        _ => 1.0,
    }
}

fn top_dir(path: &str) -> String {
    path.split('/').next().unwrap_or(".").trim().to_string()
}

fn parent_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| if dir.is_empty() { "." } else { dir })
        .unwrap_or(".")
        .to_string()
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_else(|| "no_ext".to_string())
}

pub(crate) fn dominant_directory<'a>(paths: impl Iterator<Item = &'a String>) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in paths {
        *counts.entry(top_dir(path)).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(dir, _)| dir)
        .unwrap_or_else(|| ".".to_string())
}

pub(crate) fn directory_support<'a>(paths: impl Iterator<Item = &'a String>) -> f64 {
    let mut total = 0usize;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for path in paths {
        total += 1;
        *counts.entry(top_dir(path)).or_default() += 1;
    }
    if total == 0 {
        return 0.0;
    }
    counts.values().copied().max().unwrap_or(0) as f64 / total as f64
}

fn role_rank(role: MembershipRole) -> u8 {
    match role {
        MembershipRole::Primary => 0,
        MembershipRole::Shared => 1,
        MembershipRole::Generated => 2,
        MembershipRole::Test => 3,
        MembershipRole::Docs => 4,
        MembershipRole::ExternalSocket => 5,
    }
}

fn humanize_path(path: &str) -> String {
    let cleaned = path
        .trim_matches('/')
        .trim_start_matches("./")
        .replace(['_', '-'], " ");
    if cleaned.is_empty() || cleaned == "." {
        return "Root".to_string();
    }
    cleaned
        .split('/')
        .next_back()
        .unwrap_or(cleaned.as_str())
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Human label for a directory module. Uses the leaf directory, but when that
/// leaf is a generic source folder it borrows the parent for context, so a repo
/// full of `src/` modules does not collapse into a wall of identical "Src".
pub(crate) fn humanize_module(dir: &str) -> String {
    let parts: Vec<&str> = dir
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() {
        return "Root".to_string();
    }
    let generic = matches!(
        parts[parts.len() - 1],
        "src" | "lib" | "source" | "app" | "pkg" | "dist" | "build"
    );
    let take = if generic && parts.len() >= 2 { 2 } else { 1 };
    parts[parts.len() - take..]
        .iter()
        .map(|part| humanize_path(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ordinal_name(id: u32) -> String {
    format!("Community {id}")
}

fn sanitize_repo_id(repo_id: &str) -> String {
    let sanitized = sanitize_slug(repo_id);
    if sanitized.is_empty() {
        "repo".to_string()
    } else {
        sanitized
    }
}

fn sanitize_slug(value: &str) -> String {
    let mut out = String::new();
    let mut last_underscore = false;
    for ch in value.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "repo".to_string()
    } else {
        out
    }
}

fn short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())[..8].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_blocks::{MembershipRole, SeedFile, SystemBlock};
    use glob::Pattern;

    fn options_force_community() -> SkeletonScanOptions {
        SkeletonScanOptions {
            tiny_file_threshold: 0,
            // Floor 1 so a 3-file `src` module still refines by community and the
            // shared-file tie surfaces as a seam across two blocks.
            directory_module_floor: 1,
            ..SkeletonScanOptions::default()
        }
    }

    fn node(external_id: &str, kind: NodeType) -> SkeletonGraphNode {
        SkeletonGraphNode {
            external_id: external_id.to_string(),
            kind,
        }
    }

    fn edge(source: usize, target: usize) -> SkeletonGraphEdge {
        SkeletonGraphEdge {
            source,
            target,
            relation: Some("depends_on".to_string()),
            weight: 1.0,
        }
    }

    fn base_input(files: Vec<&str>) -> SkeletonScanInput {
        SkeletonScanInput {
            repo_id: "repo-alpha".to_string(),
            repo_root: ".".to_string(),
            source_commit: "abc123".to_string(),
            repo_files: files.into_iter().map(str::to_string).collect(),
            nodes: Vec::new(),
            assignments: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn expand_membership(block: &SystemBlock, file_list: &[String]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for member in &block.membership {
            if member.path.contains('*') || member.path.contains('?') || member.path.contains('[') {
                let pat = Pattern::new(&member.path).expect("valid glob emitted by scan");
                for file in file_list {
                    if pat.matches(file) {
                        out.insert(file.clone());
                    }
                }
            } else {
                out.insert(member.path.clone());
            }
        }
        out
    }

    fn resolve_seed_members(seed: &SeedFile, file_list: &[String]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for block in &seed.blocks {
            out.extend(expand_membership(block, file_list));
        }
        out
    }

    #[test]
    fn collapse_ladder_tie_becomes_shared_in_both_blocks_and_reported_as_seam() {
        let mut input = base_input(vec!["src/a.rs", "src/b.rs", "src/shared.rs"]);
        input.nodes = vec![
            node("file::src/a.rs", NodeType::File),
            node("file::src/b.rs", NodeType::File),
            node("file::src/shared.rs::function::left", NodeType::Function),
            node("file::src/shared.rs::function::right", NodeType::Function),
        ];
        input.assignments = vec![Some(0), Some(1), Some(0), Some(1)];
        input.edges = vec![edge(0, 2), edge(1, 3), edge(2, 0), edge(3, 1), edge(2, 3)];

        let out = scan_skeleton(input, options_force_community());

        assert_eq!(out.report.multi_owner_seams.len(), 1);
        assert_eq!(out.report.multi_owner_seams[0].path, "src/shared.rs");
        let shared_blocks: Vec<_> = out
            .seed
            .blocks
            .iter()
            .filter(|block| {
                block.membership.iter().any(|member| {
                    member.path == "src/shared.rs" && member.role == MembershipRole::Shared
                })
            })
            .collect();
        assert_eq!(
            shared_blocks.len(),
            2,
            "shared file must appear in both candidate blocks"
        );
        assert!(out.report.multi_owner_seams[0].block_ids.len() >= 2);
    }

    #[test]
    fn docs_no_edge_repo_uses_directory_extension_and_absent_cohesion() {
        let input = base_input(vec!["docs/a.md", "docs/b.md", "README.md"]);
        let out = scan_skeleton(input, SkeletonScanOptions::default());

        assert_eq!(out.report.algorithm, "directory_extension_no_edge");
        assert!(
            out.seed.blocks.iter().all(|b| b
                .candidate_meta
                .as_ref()
                .unwrap()
                .graph_cohesion
                .is_none()),
            "no-edge docs fixture must not fake cohesion"
        );
    }

    #[test]
    fn single_src_repo_with_graph_uses_communities_not_one_top_dir() {
        // Dense single-`src` repo: two communities that each clear the default
        // floor (3) must refine the `src` module into two blocks rather than
        // collapse to one. Fixture grown from 3 -> 6 files because DIR-FIRST only
        // splits a module on communities that clear the floor (the old fixture's
        // 1-and-2 split would now — correctly — read as one human-scale block).
        let mut input = base_input(vec![
            "src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs", "src/f.rs",
        ]);
        input.nodes = vec![
            node("file::src/a.rs", NodeType::File),
            node("file::src/b.rs", NodeType::File),
            node("file::src/c.rs", NodeType::File),
            node("file::src/d.rs", NodeType::File),
            node("file::src/e.rs", NodeType::File),
            node("file::src/f.rs", NodeType::File),
        ];
        input.assignments = vec![Some(0), Some(0), Some(0), Some(1), Some(1), Some(1)];
        input.edges = vec![
            edge(0, 1),
            edge(1, 2),
            edge(2, 0),
            edge(3, 4),
            edge(4, 5),
            edge(5, 3),
        ];

        let out = scan_skeleton(input, SkeletonScanOptions::default());

        assert_eq!(out.report.algorithm, "directory_module_louvain_refine");
        assert!(
            out.seed.blocks.len() > 1,
            "single src repo must refine into communities, not one block"
        );
    }

    #[test]
    fn dogfood_seed_fixture_recovers_ratified_categories() {
        let seed: SeedFile =
            serde_json::from_str(include_str!("../../docs/system-blocks/m1nd.seed.v0.json"))
                .expect("seed fixture parses");
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root");
        let repo_files = crate::system_blocks::repo_file_list(repo_root).expect("repo file list");
        let seed_claimed = resolve_seed_members(&seed, &repo_files);

        let mut nodes = Vec::new();
        let mut assignments = Vec::new();
        let mut edges = Vec::new();
        let mut seen_files = BTreeSet::new();
        for (community, block) in seed.blocks.iter().enumerate() {
            for path in resolve_seed_members(
                &SeedFile {
                    schema: seed.schema.clone(),
                    repo: seed.repo.clone(),
                    skeleton: seed.skeleton.clone(),
                    blocks: vec![block.clone()],
                    unmapped_policy: seed.unmapped_policy.clone(),
                },
                &repo_files,
            ) {
                if !seen_files.insert(path.clone()) {
                    continue;
                }
                let idx = nodes.len();
                nodes.push(node(&format!("file::{path}"), NodeType::File));
                assignments.push(Some(community as u32));
                if idx > 0 {
                    edges.push(edge(idx - 1, idx));
                }
            }
        }
        // Add a few deliberate cross-category seams so sockets are exercised while
        // communities remain seed-derived.
        if nodes.len() > 3 {
            edges.push(edge(0, nodes.len() / 2));
            edges.push(edge(nodes.len() / 2, nodes.len() - 1));
        }

        let out = scan_skeleton(
            SkeletonScanInput {
                repo_id: "m1nd".to_string(),
                repo_root: ".".to_string(),
                source_commit: "seed-fixture".to_string(),
                repo_files: repo_files.clone(),
                nodes,
                assignments,
                edges,
            },
            SkeletonScanOptions {
                tiny_file_threshold: 0,
                ..SkeletonScanOptions::default()
            },
        );

        let candidate_claimed = resolve_seed_members(&out.seed, &repo_files);
        let recovered = seed_claimed
            .iter()
            .filter(|path| candidate_claimed.contains(*path))
            .count();
        let coverage = recovered as f64 / seed_claimed.len() as f64;
        assert!(
            coverage >= 0.80,
            "coverage {coverage:.3} below dogfood gate"
        );

        let block_containing = |needle: &str| {
            out.seed.blocks.iter().find_map(|b| {
                b.membership
                    .iter()
                    .any(|m| m.path.contains(needle))
                    .then_some(b.block_id.as_str())
            })
        };
        let core_graph = block_containing("m1nd-core/src/graph.rs").expect("core graph recovered");
        let core_evidence =
            block_containing("m1nd-core/src/trust.rs").expect("core evidence recovered");
        assert!(
            core_graph != core_evidence
                || out
                    .report
                    .multi_owner_seams
                    .iter()
                    .any(|s| s.path.contains("m1nd-core/src/graph.rs")
                        || s.path.contains("m1nd-core/src/trust.rs")),
            "core graph and evidence must be separate blocks or surfaced as a seam"
        );

        let ingest = block_containing("m1nd-ingest/").expect("ingest recovered");
        let served_owner =
            block_containing("m1nd-mcp/src/server.rs").expect("served owner recovered");
        let ui = block_containing("m1nd-ui/").expect("UI recovered");
        let distinct: BTreeSet<_> = [ingest, served_owner, ui].into_iter().collect();
        assert_eq!(
            distinct.len(),
            3,
            "ingest, served owner, and UI must recover as distinct blocks"
        );

        for block in &out.seed.blocks {
            let tops: BTreeSet<_> = block
                .membership
                .iter()
                .map(|m| {
                    let path = m.path.trim_end_matches("/**");
                    // A root-level file has no top directory; treat it as "." so
                    // the legitimate DIR-FIRST Root module is not misread as a
                    // block that crosses unrelated top dirs.
                    if path.contains('/') {
                        top_dir(path)
                    } else {
                        ".".to_string()
                    }
                })
                .filter(|top| top != ".")
                .collect();
            assert!(
                tops.len() <= 1,
                "candidate block {} crosses unrelated top dirs: {:?}",
                block.block_id,
                tops
            );
        }

        eprintln!(
            "dogfood category recovery: blocks={}, coverage={:.3}, seams={}",
            out.seed.blocks.len(),
            coverage,
            out.report.multi_owner_seams.len()
        );
    }

    #[test]
    fn sparse_repo_yields_a_human_scale_map() {
        // The proven live failure: a sparse repo whose Louvain degenerates into a
        // near-unique community per file (a Tauri-shaped ~100-file repo) used to
        // explode into ~89 one-file blocks. DIR-FIRST anchors every file to its
        // directory module first, so the degenerate signal cannot fragment it.
        let mut files: Vec<String> = Vec::new();
        let push_dir = |files: &mut Vec<String>, dir: &str, n: usize| {
            for i in 0..n {
                files.push(format!("{dir}/f{i}.rs"));
            }
        };
        push_dir(&mut files, "project-core/src", 14);
        push_dir(&mut files, "project-app/src", 20);
        push_dir(&mut files, "project-app/shell", 10);
        push_dir(&mut files, "project-docs", 15);
        push_dir(&mut files, "project-tasks", 18);
        for i in 0..6 {
            files.push(format!("root{i}.md"));
        }

        let mut input = base_input(files.iter().map(String::as_str).collect());
        input.nodes = files
            .iter()
            .map(|path| node(&format!("file::{path}"), NodeType::File))
            .collect();
        // Each file in its own community — the degenerate Louvain we must survive.
        input.assignments = (0..files.len() as u32).map(Some).collect();
        // A few edges so the graph path (not the no-edge fallback) is taken.
        input.edges = (1..files.len())
            .step_by(7)
            .map(|i| edge(i - 1, i))
            .collect();

        let out = scan_skeleton(
            input,
            SkeletonScanOptions {
                tiny_file_threshold: 0,
                ..SkeletonScanOptions::default()
            },
        );

        let block_count = out.seed.blocks.len();
        assert!(
            (4..=12).contains(&block_count),
            "expected a human-scale 4..=12 blocks, got {block_count}"
        );

        // >= 80% of blocks are human-scale (>= 3 members); a Root block may be
        // the only smaller one.
        let human_scale = out
            .seed
            .blocks
            .iter()
            .filter(|b| expand_membership(b, &files).len() >= 3)
            .count();
        assert!(
            human_scale as f64 / block_count as f64 >= 0.80,
            "only {human_scale}/{block_count} blocks are human-scale"
        );

        // No one-file block except a legitimate root file.
        for b in &out.seed.blocks {
            let members: Vec<String> = expand_membership(b, &files).into_iter().collect();
            if members.len() == 1 {
                assert!(
                    !members[0].contains('/'),
                    "one-file block {} is not a root file: {:?}",
                    b.block_id,
                    members
                );
            }
        }

        // Each cohesive directory recovers as exactly one block.
        for dir in ["project-core/src", "project-docs", "project-tasks"] {
            let owning: BTreeSet<_> = out
                .seed
                .blocks
                .iter()
                .filter(|b| {
                    expand_membership(b, &files)
                        .iter()
                        .any(|p| p.starts_with(dir))
                })
                .map(|b| b.block_id.clone())
                .collect();
            assert_eq!(owning.len(), 1, "{dir} must recover as exactly one block");
        }

        // Nothing dropped.
        assert_eq!(
            resolve_seed_members(&out.seed, &files).len(),
            files.len(),
            "every file must be claimed"
        );
        assert_eq!(out.report.unmapped_total, 0, "no file may be discarded");
    }

    #[test]
    fn costura_merges_sibling_modules_sharing_a_dominant_community() {
        // §2b seam-merge: two sibling directories whose files all share one
        // community must stitch into a single block instead of standing apart.
        let files = vec![
            "pkg/api/a.rs",
            "pkg/api/b.rs",
            "pkg/api/c.rs",
            "pkg/web/d.rs",
            "pkg/web/e.rs",
            "pkg/web/f.rs",
        ];
        let mut input = base_input(files.clone());
        input.nodes = files
            .iter()
            .map(|p| node(&format!("file::{p}"), NodeType::File))
            .collect();
        // Every file in community 7 -> each module has a 100% dominant community.
        input.assignments = vec![Some(7); files.len()];
        input.edges = vec![edge(0, 3), edge(3, 0)];

        let out = scan_skeleton(
            input,
            SkeletonScanOptions {
                tiny_file_threshold: 0,
                ..SkeletonScanOptions::default()
            },
        );

        assert_eq!(
            out.seed.blocks.len(),
            1,
            "sibling modules with one dominant community must merge into one block"
        );
        let file_strings: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let members = expand_membership(&out.seed.blocks[0], &file_strings);
        assert_eq!(members.len(), 6, "merged block must own all six files");
    }

    #[test]
    fn floor_bubble_folds_a_thin_directory_into_its_parent() {
        // A directory below the floor bubbles its lone file up into the parent
        // module rather than standing off as a one-file block.
        let files = vec![
            "mod-core/a.rs",
            "mod-core/b.rs",
            "mod-core/c.rs",
            "mod-core/util/lonely.rs",
        ];
        let mut input = base_input(files.clone());
        input.nodes = files
            .iter()
            .map(|p| node(&format!("file::{p}"), NodeType::File))
            .collect();
        input.assignments = vec![Some(3); files.len()];
        input.edges = vec![edge(0, 3), edge(3, 0)];

        let out = scan_skeleton(
            input,
            SkeletonScanOptions {
                tiny_file_threshold: 0,
                ..SkeletonScanOptions::default()
            },
        );

        assert_eq!(
            out.seed.blocks.len(),
            1,
            "thin child dir must fold into its parent, not split off"
        );
        let file_strings: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let members = expand_membership(&out.seed.blocks[0], &file_strings);
        assert!(
            members.contains("mod-core/util/lonely.rs"),
            "the bubbled-up file must ride along in the parent block"
        );
        assert_eq!(members.len(), 4);
    }
}
