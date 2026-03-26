// === m1nd-core/src/git_history.rs ===
// @m1nd:temponizer:IGNITION — wraps git log + feeds CoChangeMatrix
// @m1nd:emca:pattern — EXECUTE(parser) → MEASURE(parse_log_output test) → done
// @m1nd:primitives — temporal::CoChangeMatrix::populate_from_commit_groups
//
// RB-01 — 4D Git Graph: depth-configurable git history parser that enriches
// the CoChangeMatrix independently of a full ingest cycle.
//
// The walker already collects commit groups during ingest, but only with
// a single-pass `git log --format=%at --name-only`.  This module adds:
//
//   1. Configurable depth (7d, 30d, 90d, all).
//   2. Per-commit metadata (hash, timestamp, author).
//   3. Standalone invocation from MCP without re-ingest.
//   4. Ghost-edge generation: co-change pairs that have NO static edge
//      in the graph become ghost edges with temporal provenance.

use crate::error::{M1ndError, M1ndResult};
use crate::graph::Graph;
use crate::temporal::CoChangeMatrix;
use crate::types::{FiniteF32, NodeId};
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// How far back to look in git history.
#[derive(Clone, Copy, Debug)]
pub enum GitDepth {
    Days(u32),
    All,
}

impl GitDepth {
    /// Parse a string like "7d", "30d", "90d", "all" into a depth.
    pub fn parse(s: &str) -> M1ndResult<Self> {
        let s = s.trim().to_lowercase();
        if s == "all" {
            return Ok(Self::All);
        }
        if let Some(n) = s.strip_suffix('d') {
            let days: u32 = n.parse().map_err(|_| {
                M1ndError::InvalidParams {
                    tool: "ghost_edges".into(),
                    detail: format!("bad depth: {s} — expected 7d, 30d, 90d, all"),
                }
            })?;
            return Ok(Self::Days(days));
        }
        Err(M1ndError::InvalidParams {
            tool: "ghost_edges".into(),
            detail: format!("bad depth: {s} — expected 7d, 30d, 90d, all"),
        })
    }

    /// Convert to a `--since` argument for `git log`, or `None` for `All`.
    fn as_since_arg(&self) -> Option<String> {
        match self {
            Self::Days(d) => Some(format!("{d} days ago")),
            Self::All => None,
        }
    }
}

/// A single parsed commit from git history.
#[derive(Clone, Debug)]
pub struct GitCommit {
    pub hash: String,
    pub timestamp: f64,
    pub author: String,
    pub files: Vec<String>,
}

/// Result of a git history parse.
#[derive(Clone, Debug)]
pub struct GitHistoryResult {
    pub commits_parsed: usize,
    pub co_change_pairs_injected: usize,
    pub ghost_edges_found: usize,
    pub commits: Vec<GitCommit>,
}

/// A ghost edge discovered from temporal co-change that has no static edge.
#[derive(Clone, Debug)]
pub struct TemporalGhostEdge {
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub source_ext: String,
    pub target_ext: String,
    pub co_change_count: u32,
    pub strength: FiniteF32,
}

// ---------------------------------------------------------------------------
// Core parser
// ---------------------------------------------------------------------------

/// Parse git log output from a repository root with configurable depth.
///
/// Returns a list of commits with their associated files.
pub fn parse_git_history(repo_root: &Path, depth: GitDepth) -> M1ndResult<Vec<GitCommit>> {
    let mut args = vec![
        "log".to_string(),
        "--format=%H|%at|%an".to_string(),
        "--name-only".to_string(),
        "--diff-filter=ACDMR".to_string(),
    ];

    if let Some(since) = depth.as_since_arg() {
        args.push(format!("--since={since}"));
    }

    let output = Command::new("git")
        .args(&args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| {
            M1ndError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("git log failed: {e}"),
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("git log failed: {stderr}"),
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_git_log_output(&stdout)
}

/// Parse the raw output of `git log --format=%H|%at|%an --name-only`.
fn parse_git_log_output(raw: &str) -> M1ndResult<Vec<GitCommit>> {
    let mut commits = Vec::new();
    let mut current: Option<GitCommit> = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            // Empty line — flush current commit files (separator between commits)
            continue;
        }

        // Try to parse as a commit header: HASH|TIMESTAMP|AUTHOR
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() == 3 && parts[0].len() >= 7 && parts[0].chars().all(|c| c.is_ascii_hexdigit())
        {
            // Flush previous commit
            if let Some(c) = current.take() {
                if !c.files.is_empty() {
                    commits.push(c);
                }
            }
            let timestamp: f64 = parts[1].parse().unwrap_or(0.0);
            current = Some(GitCommit {
                hash: parts[0].to_string(),
                timestamp,
                author: parts[2].to_string(),
                files: Vec::new(),
            });
        } else if let Some(ref mut c) = current {
            // File path line
            c.files.push(line.to_string());
        }
    }

    // Flush final commit
    if let Some(c) = current {
        if !c.files.is_empty() {
            commits.push(c);
        }
    }

    Ok(commits)
}

// ---------------------------------------------------------------------------
// Co-change injection
// ---------------------------------------------------------------------------

/// Inject git history into a CoChangeMatrix and detect ghost edges.
///
/// Ghost edges are co-change pairs where the two files have NO static edge
/// in the graph — they are structurally invisible but temporally coupled.
pub fn inject_git_history(
    graph: &Graph,
    co_change: &mut CoChangeMatrix,
    commits: &[GitCommit],
) -> M1ndResult<GitHistoryResult> {
    let mut pairs_injected: usize = 0;
    let mut ghost_edges: Vec<TemporalGhostEdge> = Vec::new();

    // Build commit groups from parsed commits (files that changed together)
    let commit_groups: Vec<Vec<String>> = commits
        .iter()
        .filter(|c| c.files.len() >= 2) // Only multi-file commits create co-change
        .map(|c| c.files.clone())
        .collect();

    // Populate the co-change matrix
    co_change.populate_from_commit_groups(graph, &commit_groups)?;

    // Count unique pairs and detect ghost edges
    for commit in commits.iter().filter(|c| c.files.len() >= 2) {
        // Resolve files to node IDs
        let resolved: Vec<(NodeId, String)> = commit
            .files
            .iter()
            .filter_map(|path| {
                let file_id = if path.starts_with("file::") {
                    path.clone()
                } else {
                    format!("file::{path}")
                };
                graph.resolve_id(&file_id).map(|nid| (nid, path.clone()))
            })
            .collect();

        for i in 0..resolved.len() {
            for j in (i + 1)..resolved.len() {
                pairs_injected += 1;

                let (nid_a, ref path_a) = resolved[i];
                let (nid_b, ref path_b) = resolved[j];

                // Check if a static edge exists between these two nodes
                if !has_static_edge(graph, nid_a, nid_b) {
                    ghost_edges.push(TemporalGhostEdge {
                        source_id: nid_a,
                        target_id: nid_b,
                        source_ext: path_a.to_string(),
                        target_ext: path_b.to_string(),
                        co_change_count: 1, // Will be aggregated below
                        strength: FiniteF32::new(0.5), // Default; refined after aggregation
                    });
                }
            }
        }
    }

    // Aggregate ghost edges: merge duplicates, count co-occurrences
    let ghost_edges = aggregate_ghost_edges(ghost_edges);

    Ok(GitHistoryResult {
        commits_parsed: commits.len(),
        co_change_pairs_injected: pairs_injected,
        ghost_edges_found: ghost_edges.len(),
        commits: commits.to_vec(),
    })
}

/// Check whether a static (non-ghost) edge exists between two nodes.
fn has_static_edge(graph: &Graph, a: NodeId, b: NodeId) -> bool {
    if !graph.finalized {
        return false;
    }
    // Check forward adjacency from a → b
    for idx in graph.csr.out_range(a) {
        if graph.csr.targets[idx] == b {
            return true;
        }
    }
    // Check reverse adjacency: edges from b → a
    for idx in graph.csr.in_range(a) {
        if graph.csr.rev_sources[idx] == b {
            return true;
        }
    }
    false
}

/// Aggregate duplicate ghost edges, counting co-change frequency.
fn aggregate_ghost_edges(raw: Vec<TemporalGhostEdge>) -> Vec<TemporalGhostEdge> {
    use std::collections::HashMap;

    let mut counts: HashMap<(u32, u32), (TemporalGhostEdge, u32)> = HashMap::new();
    for edge in raw {
        let key = (edge.source_id.0, edge.target_id.0);
        // Normalize key so (a,b) == (b,a)
        let key = if key.0 <= key.1 { key } else { (key.1, key.0) };
        let entry = counts.entry(key).or_insert_with(|| (edge.clone(), 0));
        entry.1 += 1;
    }

    let max_count = counts.values().map(|(_, c)| *c).max().unwrap_or(1).max(1);

    counts
        .into_values()
        .map(|(mut edge, count)| {
            edge.co_change_count = count;
            // Strength proportional to co-change frequency (normalized 0.1-1.0)
            let norm = (count as f32 / max_count as f32).clamp(0.1, 1.0);
            edge.strength = FiniteF32::new(norm);
            edge
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_parse_days() {
        assert!(matches!(GitDepth::parse("7d").unwrap(), GitDepth::Days(7)));
        assert!(matches!(GitDepth::parse("30d").unwrap(), GitDepth::Days(30)));
        assert!(matches!(GitDepth::parse("90d").unwrap(), GitDepth::Days(90)));
    }

    #[test]
    fn depth_parse_all() {
        assert!(matches!(GitDepth::parse("all").unwrap(), GitDepth::All));
        assert!(matches!(GitDepth::parse("ALL").unwrap(), GitDepth::All));
    }

    #[test]
    fn depth_parse_invalid() {
        assert!(GitDepth::parse("xyz").is_err());
        assert!(GitDepth::parse("").is_err());
    }

    #[test]
    fn parse_log_output_basic() {
        let log = r#"abc1234deadbeef1234567890abcdef1234567890|1710000000|Max Klein
m1nd-core/src/graph.rs
m1nd-core/src/temporal.rs

def5678badcafe1234567890abcdef1234567891|1709999000|Max Klein
m1nd-core/src/flow.rs
"#;
        let commits = parse_git_log_output(log).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].files.len(), 2);
        assert_eq!(commits[0].author, "Max Klein");
        assert!((commits[0].timestamp - 1_710_000_000.0).abs() < 1.0);
        assert_eq!(commits[1].files.len(), 1);
    }

    #[test]
    fn parse_log_output_empty() {
        let commits = parse_git_log_output("").unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn aggregate_ghost_edges_merges_duplicates() {
        let edges = vec![
            TemporalGhostEdge {
                source_id: NodeId::new(0),
                target_id: NodeId::new(1),
                source_ext: "a.rs".into(),
                target_ext: "b.rs".into(),
                co_change_count: 1,
                strength: FiniteF32::new(0.5),
            },
            TemporalGhostEdge {
                source_id: NodeId::new(0),
                target_id: NodeId::new(1),
                source_ext: "a.rs".into(),
                target_ext: "b.rs".into(),
                co_change_count: 1,
                strength: FiniteF32::new(0.5),
            },
            TemporalGhostEdge {
                source_id: NodeId::new(1),
                target_id: NodeId::new(0), // reversed dup
                source_ext: "b.rs".into(),
                target_ext: "a.rs".into(),
                co_change_count: 1,
                strength: FiniteF32::new(0.5),
            },
        ];
        let aggregated = aggregate_ghost_edges(edges);
        assert_eq!(aggregated.len(), 1, "Should merge all into one edge");
        assert_eq!(aggregated[0].co_change_count, 3);
        assert!((aggregated[0].strength.get() - 1.0).abs() < 0.01); // max normalized
    }

    #[test]
    fn depth_since_arg() {
        assert_eq!(
            GitDepth::Days(30).as_since_arg(),
            Some("30 days ago".to_string())
        );
        assert_eq!(GitDepth::All.as_since_arg(), None);
    }
}
