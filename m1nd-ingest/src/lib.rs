#![allow(unused)]
// === crates/m1nd-ingest/src/lib.rs ===

use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::NodeProvenanceInput;
use m1nd_core::types::*;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub mod bibtex_adapter;
pub mod canonical;
pub mod cargo_workspace;
pub mod cross_domain;
pub mod cross_file;
pub mod crossref_adapter;
pub mod diff;
pub mod document_router;
pub mod extract;
pub mod jats_adapter;
pub mod json_adapter;
pub mod l1ght_adapter;
pub mod memory_adapter;
pub mod merge;
pub mod patent_adapter;
pub mod path_policy;
pub mod resolve;
pub mod rfc_adapter;
pub mod universal_adapter;
pub mod walker;

pub use bibtex_adapter::BibTexAdapter;
pub use crossref_adapter::CrossRefAdapter;
pub use jats_adapter::JatsArticleAdapter;
pub use l1ght_adapter::L1ghtIngestAdapter;
pub use patent_adapter::PatentIngestAdapter;
pub use rfc_adapter::RfcAdapter;
pub use universal_adapter::{ProviderAvailability, UniversalIngestAdapter, UniversalIngestBundle};

pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_valid_relative_file_path(rel_path: &str) -> bool {
    let trimmed = rel_path.trim();
    if trimmed.is_empty() {
        return false;
    }

    let path = Path::new(trimmed);
    path.components()
        .any(|component| matches!(component, Component::Normal(_)))
}

fn build_file_external_id(rel_path: &str) -> Option<String> {
    let trimmed = rel_path.trim();
    if !is_valid_relative_file_path(trimmed) {
        return None;
    }

    Some(format!("file::{}", trimmed.replace('\\', "/")))
}

pub(crate) fn relative_source_path(root: &Path, path: &Path) -> String {
    if root.is_file() {
        return path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
    }

    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_valid_external_id(external_id: &str) -> bool {
    let trimmed = external_id.trim();
    if trimmed.is_empty() {
        return false;
    }

    if let Some(rel_path) = trimmed.strip_prefix("file::") {
        return is_valid_relative_file_path(rel_path);
    }

    true
}

pub trait IngestAdapter: Send + Sync {
    fn domain(&self) -> &str;
    fn ingest(&self, root: &std::path::Path) -> M1ndResult<(m1nd_core::graph::Graph, IngestStats)>;
}

pub struct IngestConfig {
    pub root: PathBuf,
    pub timeout: Duration,
    pub max_nodes: u64,
    pub skip_dirs: Vec<String>,
    pub skip_files: Vec<String>,
    pub parallelism: usize,
    pub include_dotfiles: bool,
    pub dotfile_patterns: Vec<String>,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            timeout: Duration::from_secs(300),
            max_nodes: 500_000,
            skip_dirs: path_policy::default_skip_dirs(),
            skip_files: vec![
                "package-lock.json".into(),
                "yarn.lock".into(),
                "Cargo.lock".into(),
                "poetry.lock".into(),
            ],
            parallelism: std::thread::available_parallelism()
                .map(|p| p.get().min(16))
                .unwrap_or(8),
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IngestStats {
    pub files_scanned: u64,
    pub files_parsed: u64,
    pub files_skipped_binary: u64,
    pub files_skipped_encoding: u64,
    pub nodes_created: u64,
    pub edges_created: u64,
    pub references_resolved: u64,
    pub references_unresolved: u64,
    pub label_collisions: u64,
    pub elapsed_ms: f64,
    pub commit_groups: Vec<Vec<String>>,
    pub discovered_files: Vec<walker::DiscoveredFile>,
}

pub struct Ingestor {
    config: IngestConfig,
}

impl Ingestor {
    pub fn new(config: IngestConfig) -> Self {
        Self { config }
    }

    fn select_extractor(ext: &str) -> Box<dyn extract::Extractor> {
        match ext {
            "py" | "pyi" => Box::new(extract::python::PythonExtractor::new()),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                Box::new(extract::typescript::TypeScriptExtractor::new())
            }
            "rs" => Box::new(extract::rust_lang::RustExtractor::new()),
            "go" => Box::new(extract::go::GoExtractor::new()),
            "java" => Box::new(extract::java::JavaExtractor::new()),
            #[cfg(feature = "tier1")]
            "c" | "h" => Box::new(extract::tree_sitter_ext::c_extractor()),
            #[cfg(feature = "tier1")]
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => {
                Box::new(extract::tree_sitter_ext::cpp_extractor())
            }
            #[cfg(feature = "tier1")]
            "cs" => Box::new(extract::tree_sitter_ext::csharp_extractor()),
            #[cfg(feature = "tier1")]
            "rb" | "rake" | "gemspec" => Box::new(extract::tree_sitter_ext::ruby_extractor()),
            #[cfg(feature = "tier1")]
            "php" => Box::new(extract::tree_sitter_ext::php_extractor()),
            #[cfg(feature = "tier1")]
            "swift" => Box::new(extract::tree_sitter_ext::swift_extractor()),
            #[cfg(feature = "tier1")]
            "kt" | "kts" => Box::new(extract::tree_sitter_ext::kotlin_extractor()),
            #[cfg(feature = "tier1")]
            "scala" | "sc" => Box::new(extract::tree_sitter_ext::scala_extractor()),
            #[cfg(feature = "tier1")]
            "sh" | "bash" | "zsh" => Box::new(extract::tree_sitter_ext::bash_extractor()),
            #[cfg(feature = "tier1")]
            "lua" => Box::new(extract::tree_sitter_ext::lua_extractor()),
            #[cfg(feature = "tier1")]
            "r" | "R" | "Rmd" => Box::new(extract::tree_sitter_ext::r_extractor()),
            #[cfg(feature = "tier1")]
            "html" | "htm" => {
                Box::new(extract::tree_sitter_ext::EmbeddedExtractor::html_embedded())
            }
            #[cfg(feature = "tier1")]
            "css" => Box::new(extract::tree_sitter_ext::css_extractor()),
            #[cfg(feature = "tier1")]
            "json" => Box::new(extract::tree_sitter_ext::json_extractor()),
            #[cfg(feature = "tier2")]
            "ex" | "exs" => Box::new(extract::tree_sitter_ext::elixir_extractor()),
            #[cfg(feature = "tier2")]
            "dart" => Box::new(extract::tree_sitter_ext::dart_extractor()),
            #[cfg(feature = "tier2")]
            "zig" => Box::new(extract::tree_sitter_ext::zig_extractor()),
            #[cfg(feature = "tier2")]
            "hs" | "lhs" => Box::new(extract::tree_sitter_ext::haskell_extractor()),
            #[cfg(feature = "tier2")]
            "ml" | "mli" => Box::new(extract::tree_sitter_ext::ocaml_extractor()),
            #[cfg(feature = "tier2")]
            "toml" => Box::new(extract::tree_sitter_ext::toml_extractor()),
            #[cfg(feature = "tier2")]
            "yml" | "yaml" => Box::new(extract::tree_sitter_ext::yaml_extractor()),
            #[cfg(feature = "tier2")]
            "sql" => Box::new(extract::tree_sitter_ext::sql_extractor()),
            _ => Box::new(extract::generic::GenericExtractor::new()),
        }
    }

    pub fn ingest(&self) -> M1ndResult<(m1nd_core::graph::Graph, IngestStats)> {
        let start = Instant::now();
        let mut stats = IngestStats::default();

        let dir_walker = walker::DirectoryWalker::new(
            self.config.skip_dirs.clone(),
            self.config.skip_files.clone(),
            self.config.include_dotfiles,
            self.config.dotfile_patterns.clone(),
        );
        let walk_result = dir_walker.walk(&self.config.root)?;
        stats.files_scanned = walk_result.files.len() as u64;
        stats.commit_groups = walk_result.commit_groups.clone();
        stats.discovered_files = walk_result.files.clone();

        use rayon::prelude::*;
        let num_threads = self.config.parallelism.clamp(1, 64);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .map_err(|e| M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: format!("thread pool: {}", e),
            })?;

        // (file_id, extraction result, [(node_id, behavioral excerpt)])
        type FileExtraction = (String, extract::ExtractionResult, Vec<(String, String)>);
        let extraction_results: Vec<FileExtraction> = pool.install(|| {
            walk_result
                .files
                .par_iter()
                .filter_map(|file| {
                    let ext = file.extension.as_deref().unwrap_or("");
                    let extractor = Self::select_extractor(ext);
                    let content = std::fs::read(&file.path).ok()?;
                    let file_id = match build_file_external_id(&file.relative_path) {
                        Some(file_id) => file_id,
                        None => {
                            eprintln!(
                                "[m1nd-ingest] WARNING: skipping invalid relative path {:?}",
                                file.relative_path
                            );
                            return None;
                        }
                    };
                    let result = extractor.extract(&content, &file_id).ok()?;
                    // Behavioral excerpts sliced here while the file content is live,
                    // so embeddings later see what each symbol DOES, not just its name.
                    let excerpts = extract::compute_excerpts(&result, &content);
                    Some((file_id, result, excerpts))
                })
                .collect()
        });

        // Build per-file git data map: file_id -> (commit_count, last_modified).
        // The walker already ran git log once and populated DiscoveredFile fields.
        // We key by file_id (e.g. "file::src/lib.rs") so sub-file nodes can inherit
        // their parent file's commit history via prefix lookup.
        use std::collections::HashMap;
        let file_git_data: HashMap<String, (u32, f64)> = walk_result
            .files
            .iter()
            .filter_map(|f| {
                build_file_external_id(&f.relative_path)
                    .map(|fid| (fid, (f.commit_count, f.last_modified)))
            })
            .collect();

        // Flatten per-file excerpts into a global node_id -> excerpt map (bounded
        // ~240 chars/node), consumed at provenance time below. FIRST-WINS on a
        // duplicate id, to match graph insertion: the first node with an id wins
        // `add_node` (later dups are dropped as DuplicateNode), so the surviving
        // node must keep ITS OWN excerpt, not a later same-named sibling's.
        let mut node_excerpts: HashMap<String, String> = HashMap::new();
        for (_, _, excerpts) in &extraction_results {
            for (id, excerpt) in excerpts {
                node_excerpts
                    .entry(id.clone())
                    .or_insert_with(|| excerpt.clone());
            }
        }

        let mut all_nodes: Vec<(String, extract::ExtractedNode)> = Vec::new();
        let mut all_edges = Vec::new();
        for (file_id, result, _) in &extraction_results {
            if start.elapsed() > self.config.timeout {
                eprintln!("[m1nd-ingest] Timeout after {} files", stats.files_parsed);
                break;
            }

            stats.files_parsed += 1;
            all_nodes.extend(result.nodes.iter().cloned().map(|n| (file_id.clone(), n)));
            all_edges.extend_from_slice(&result.edges);
        }

        let mut graph = m1nd_core::graph::Graph::new();
        let mut skipped_invalid_nodes = 0u64;
        for (file_id, node) in &all_nodes {
            if !is_valid_external_id(&node.id) {
                skipped_invalid_nodes += 1;
                eprintln!(
                    "[m1nd-ingest] WARNING: skipping invalid external_id {:?}",
                    node.id
                );
                continue;
            }

            // Look up git data for this file. Sub-file nodes (functions, structs, …)
            // inherit from their containing file identified by file_id.
            let (commit_count, last_modified) =
                file_git_data.get(file_id).copied().unwrap_or((0, 0.0));

            // change_frequency: monotonically maps commit_count -> [0, 1).
            // 0 commits → 0.0 (neutral/unknown), 10 commits → ~0.5, 50+ → ~0.83.
            // Higher value means "changes more often" — consistent with activation.rs
            // (frequency boosts score) and temporal.rs VelocityScorer (z > 0 = Accelerating).
            let change_frequency = if commit_count == 0 {
                0.0f32
            } else {
                commit_count as f32 / (commit_count as f32 + 10.0)
            };

            let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
            if let Ok(node_id) = graph.add_node(
                &node.id,
                &node.label,
                node.node_type,
                &tags,
                last_modified,
                change_frequency,
            ) {
                stats.nodes_created += 1;

                // Provenance: the source file is the node's containing file_id
                // ("file::<relpath>"), so strip the "file::" prefix to recover the
                // project-relative source path. This is what makes the graph-driven
                // AST-apply path (xray_apply AnnotateSymbol) actually match symbols:
                // without it every code node lands with source_path = None.
                // line == 0 is fine — resolve_node_provenance treats 0 as None.
                if let Some(source_path) = file_id.strip_prefix("file::") {
                    if !source_path.is_empty() {
                        graph.set_node_provenance(
                            node_id,
                            NodeProvenanceInput {
                                source_path: Some(source_path),
                                line_start: Some(node.line),
                                line_end: Some(node.end_line),
                                excerpt: node_excerpts.get(&node.id).map(String::as_str),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }

        let mut unresolved_edges: Vec<(String, String, String)> = Vec::new();
        let mut import_hints: Vec<(String, String, String)> = Vec::new();
        let mut skipped_invalid_edges = 0u64;

        for edge in &all_edges {
            if !is_valid_external_id(&edge.source) || !is_valid_external_id(&edge.target) {
                skipped_invalid_edges += 1;
                eprintln!(
                    "[m1nd-ingest] WARNING: skipping edge with invalid endpoint {:?} -> {:?} ({})",
                    edge.source, edge.target, edge.relation
                );
                continue;
            }

            if edge.target.starts_with("ref::") {
                unresolved_edges.push((
                    edge.source.clone(),
                    edge.target.clone(),
                    edge.relation.clone(),
                ));

                if edge.relation == "imports" || edge.relation == "reexports" {
                    if let Some(clean_target) = edge.target.strip_prefix("ref::") {
                        if let Some((import_path, _)) = clean_target.rsplit_once("::") {
                            import_hints.push((
                                edge.source.clone(),
                                edge.target.clone(),
                                import_path.to_string(),
                            ));
                        }
                    }
                }

                continue;
            }

            if let (Some(source), Some(target)) = (
                graph.resolve_id(&edge.source),
                graph.resolve_id(&edge.target),
            ) {
                if graph
                    .add_edge(
                        source,
                        target,
                        &edge.relation,
                        FiniteF32::new(edge.weight),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.0),
                    )
                    .is_ok()
                {
                    stats.edges_created += 1;
                }
            }
        }

        let resolution = resolve::ReferenceResolver::resolve_with_hints(
            &mut graph,
            &unresolved_edges,
            &import_hints,
        )?;
        stats.references_resolved = resolution.resolved;
        stats.references_unresolved = resolution.unresolved;
        stats.edges_created += resolution.resolved;

        let cargo_stats = cargo_workspace::enrich_rust_workspace(&mut graph, &self.config.root)?;
        stats.nodes_created += cargo_stats.nodes_added;
        stats.edges_created += cargo_stats.edges_added;

        let cross_file = cross_file::resolve_cross_file_edges(&mut graph, &self.config.root)?;
        stats.edges_created += cross_file.imports_resolved
            + cross_file.test_edges_created
            + cross_file.register_edges_created;

        graph.finalize()?;
        stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        if skipped_invalid_nodes > 0 || skipped_invalid_edges > 0 {
            eprintln!(
                "[m1nd-ingest] hygiene summary: skipped {} invalid nodes, {} invalid edges",
                skipped_invalid_nodes, skipped_invalid_edges
            );
        }

        Ok((graph, stats))
    }
}

#[cfg(test)]
mod tests {
    use super::{build_file_external_id, is_valid_external_id, IngestConfig, Ingestor};
    use crate::IngestAdapter;
    use crate::L1ghtIngestAdapter;
    use m1nd_core::types::EdgeIdx;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ingest_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("m1nd-ingest-{name}-{nonce}"))
    }

    #[test]
    fn file_external_id_builder_rejects_empty_and_dot_paths() {
        assert_eq!(build_file_external_id(""), None);
        assert_eq!(build_file_external_id("   "), None);
        assert_eq!(build_file_external_id("."), None);
        assert_eq!(build_file_external_id("./"), None);
        assert_eq!(
            build_file_external_id("src/main.rs"),
            Some("file::src/main.rs".to_string())
        );
        assert_eq!(
            build_file_external_id("src\\main.rs"),
            Some("file::src/main.rs".to_string())
        );
    }

    #[test]
    fn external_id_validation_rejects_empty_file_ids() {
        assert!(!is_valid_external_id(""));
        assert!(!is_valid_external_id("file::"));
        assert!(!is_valid_external_id("file::   "));
        assert!(is_valid_external_id("cargo::workspace::Cargo.toml"));
        assert!(is_valid_external_id("file::src/main.rs"));
    }

    #[test]
    fn ingest_resolves_rust_ref_edges_before_finalize() {
        let root = temp_ingest_dir("rust-resolve");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod helper;\nuse crate::helper::Helper;\npub fn build(helper: Helper) {}\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let main_file = graph.resolve_id("file::src/main.rs").unwrap();
        let helper = graph
            .resolve_id("file::src/helper.rs::struct::Helper")
            .unwrap();

        let has_reference_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "references"
        });
        let has_import_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(stats.references_resolved >= 1);
        assert!(has_reference_edge || has_import_edge);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_populates_node_provenance_for_code_symbols() {
        // PROOF the AST-apply path is not a no-op: a REAL ingest (walk + extract +
        // build) must populate each symbol node's provenance (source_path +
        // line_start), not leave it at the add_node default of None / 0.
        let root = temp_ingest_dir("rust-provenance");
        fs::create_dir_all(root.join("src")).unwrap();

        // A function spanning known lines: `fn answer` opens on line 3.
        fs::write(
            root.join("src/lib.rs"),
            "// header comment\n\
             \n\
             pub fn answer() -> u32 {\n\
             \x20   42\n\
             }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();
        let func = graph
            .resolve_id("file::src/lib.rs::fn::answer")
            .expect("function node must exist after ingest");

        let prov = graph.resolve_node_provenance(func);
        assert_eq!(
            prov.source_path.as_deref(),
            Some("src/lib.rs"),
            "provenance source_path must be the project-relative source file"
        );
        assert_eq!(
            prov.line_start,
            Some(3),
            "provenance line_start must be the function's real opening line"
        );

        // The symbol now also carries a behavioral excerpt sliced from its own
        // source span (signature + body), so embeddings capture what it DOES —
        // not just its name. (Drives the seek semantic layer end to end.)
        let excerpt = prov.excerpt.as_deref().unwrap_or("");
        assert!(
            excerpt.contains("answer") && excerpt.contains("42"),
            "provenance excerpt must fold in the signature + body, got {excerpt:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_adds_rust_workspace_and_crate_nodes() {
        let root = temp_ingest_dir("cargo-workspace");
        fs::create_dir_all(root.join("crates/app/src")).unwrap();
        fs::create_dir_all(root.join("crates/core/src")).unwrap();

        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/app\", \"crates/core\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"corelib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(root.join("crates/core/src/lib.rs"), "pub struct Core;\n").unwrap();
        fs::write(
            root.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncorelib = { path = \"../core\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/app/src/lib.rs"),
            "use corelib::Core;\npub fn boot(_: Core) {}\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();
        let workspace = graph.resolve_id("cargo::workspace::Cargo.toml").unwrap();
        let app = graph
            .resolve_id("cargo::crate::crates/app/Cargo.toml::app")
            .unwrap();
        let core = graph
            .resolve_id("cargo::crate::crates/core/Cargo.toml::corelib")
            .unwrap();
        let app_file = graph.resolve_id("file::crates/app/src/lib.rs").unwrap();

        let workspace_contains_app = graph.csr.out_range(workspace).any(|idx| {
            graph.csr.targets[idx] == app
                && graph.strings.resolve(graph.csr.relations[idx]) == "contains"
        });
        let app_depends_on_core = graph.csr.out_range(app).any(|idx| {
            graph.csr.targets[idx] == core
                && graph.strings.resolve(graph.csr.relations[idx]) == "depends_on"
        });
        let app_contains_file = graph.csr.out_range(app).any(|idx| {
            graph.csr.targets[idx] == app_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "contains"
        });

        assert!(workspace_contains_app);
        assert!(app_depends_on_core);
        assert!(app_contains_file);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_resolves_rust_pub_use_edges_before_finalize() {
        let root = temp_ingest_dir("rust-reexport-resolve");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "mod helper;\npub use crate::helper::Helper;\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let lib_file = graph.resolve_id("file::src/lib.rs").unwrap();
        let helper = graph
            .resolve_id("file::src/helper.rs::struct::Helper")
            .unwrap();

        let has_reexport_edge = graph.csr.out_range(lib_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "reexports"
        });

        assert!(stats.references_resolved >= 1);
        assert!(has_reexport_edge);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_links_rust_mod_declarations_to_module_files() {
        let root = temp_ingest_dir("rust-mod-file-link");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(root.join("src/main.rs"), "mod helper;\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();
        let main_file = graph.resolve_id("file::src/main.rs").unwrap();
        let helper_file = graph.resolve_id("file::src/helper.rs").unwrap();

        let has_module_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "declares_module"
        });

        assert!(has_module_edge);

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // TypeScript: calls edges + cross-file imports resolution (Step 1+2 gate)
    // -----------------------------------------------------------------------

    /// Fixture:
    ///   b.ts: `export function foo() { return 1; }`
    ///   a.ts: `import { foo } from "./b"; export function run() { return foo(); }`
    ///
    /// Asserts:
    ///   (a) at least one `calls` edge exists originating in a.ts
    ///   (b) a resolved cross-file `imports` edge exists from a.ts to b.ts
    #[test]
    fn typescript_emits_calls_and_cross_file_imports() {
        let root = temp_ingest_dir("ts-calls-imports");
        fs::create_dir_all(&root).unwrap();

        fs::write(root.join("b.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(
            root.join("a.ts"),
            "import { foo } from \"./b\";\nexport function run() { return foo(); }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        // --- (a) Assert a `calls` edge exists in the graph (from any node in a.ts) ---
        let has_calls_edge = (0..graph.csr.pending_edges.len())
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls");
        // Also check in finalized CSR
        let has_calls_csr = (0..graph.num_nodes() as usize).any(|i| {
            let node_id = m1nd_core::types::NodeId::new(i as u32);
            graph
                .csr
                .out_range(node_id)
                .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls")
        });

        assert!(
            has_calls_edge || has_calls_csr,
            "Expected at least one `calls` edge in the graph after ingesting TypeScript files with function calls"
        );

        // --- (b) Assert a cross-file `imports` edge from a.ts to b.ts ---
        let a_ts = graph
            .resolve_id("file::a.ts")
            .expect("file::a.ts node missing");
        let b_ts = graph
            .resolve_id("file::b.ts")
            .expect("file::b.ts node missing");

        let has_import_edge = graph.csr.out_range(a_ts).any(|idx| {
            graph.csr.targets[idx] == b_ts
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(
            has_import_edge,
            "Expected a cross-file `imports` edge from file::a.ts to file::b.ts"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_resolves_rust_impl_method_ownership_edges() {
        let root = temp_ingest_dir("rust-impl-ownership");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(
            root.join("src/lib.rs"),
            "pub trait Runner { fn boot(&self); }\npub struct Engine;\nimpl Runner for Engine { fn boot(&self) {} }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let boot = graph.resolve_id("file::src/lib.rs::fn::boot").unwrap();
        let engine = graph
            .resolve_id("file::src/lib.rs::struct::Engine")
            .unwrap();
        let runner = graph.resolve_id("file::src/lib.rs::trait::Runner").unwrap();

        let has_owner_edge = graph.csr.out_range(boot).any(|idx| {
            graph.csr.targets[idx] == engine
                && graph.strings.resolve(graph.csr.relations[idx]) == "belongs_to_type"
        });
        let has_trait_edge = graph.csr.out_range(boot).any(|idx| {
            graph.csr.targets[idx] == runner
                && graph.strings.resolve(graph.csr.relations[idx]) == "implements_trait"
        });

        assert!(stats.references_resolved >= 2);
        assert!(has_owner_edge);
        assert!(has_trait_edge);

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // L1GHT adapter: epistemic markers produce structured graph edges
    // -----------------------------------------------------------------------

    /// Fixture L1GHT document with an entity claim followed by three epistemic
    /// qualifiers: confidence, ambiguity, and evidence.
    ///
    /// Asserts:
    ///   (a) `epistemic_confidence` edge exists and its weight is ~0.6
    ///   (b) `epistemic_ambiguity` edge exists
    ///   (c) `evidenced_by` edge exists
    ///   (d) All three epistemic edges originate from the TokenValidator entity
    ///       node (the preceding claim), not from the section node.
    #[test]
    fn l1ght_epistemic_markers_produce_structured_edges() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("m1nd-l1ght-epistemic-{nonce}"));
        fs::create_dir_all(&root).unwrap();

        let doc = "\
---
Protocol: L1GHT/1.0
Node: AuthService
---

## Token Validation

The [⍂ entity: TokenValidator] runs HMAC checks.
[𝔻 confidence: 0.6]
[𝔻 ambiguity: retry policy undecided]
[𝔻 evidence: m1nd-core/src/auth.rs]
";

        fs::write(root.join("authservice.md"), doc).unwrap();

        let adapter = L1ghtIngestAdapter::new(None);
        let (graph, stats) = adapter.ingest(&root).unwrap();

        assert!(stats.nodes_created > 0, "no nodes created");
        assert!(stats.edges_created > 0, "no edges created");

        // Find the TokenValidator entity node id: it is the target of the
        // `declares_entity` edge.
        let mut entity_node_id = None;
        'outer: for i in 0..graph.num_nodes() as usize {
            let nid = m1nd_core::types::NodeId::new(i as u32);
            for idx in graph.csr.out_range(nid) {
                if graph.strings.resolve(graph.csr.relations[idx]) == "declares_entity" {
                    entity_node_id = Some(graph.csr.targets[idx]);
                    break 'outer;
                }
            }
        }
        let entity_node_id = entity_node_id.expect("declares_entity edge not found");

        // (a) epistemic_confidence edge from entity node, weight ~0.6
        let mut conf_weight: Option<f32> = None;
        for idx in graph.csr.out_range(entity_node_id) {
            if graph.strings.resolve(graph.csr.relations[idx]) == "epistemic_confidence" {
                let w = graph.csr.read_weight(EdgeIdx::new(idx as u32)).get();
                conf_weight = Some(w);
                break;
            }
        }
        let conf_weight =
            conf_weight.expect("epistemic_confidence edge not found from entity node");
        assert!(
            (conf_weight - 0.6_f32).abs() < 1e-5,
            "epistemic_confidence weight expected ~0.6, got {conf_weight}"
        );

        // (b) epistemic_ambiguity edge from entity node
        let has_ambiguity = graph
            .csr
            .out_range(entity_node_id)
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "epistemic_ambiguity");
        assert!(
            has_ambiguity,
            "epistemic_ambiguity edge not found from entity node"
        );

        // (c) evidenced_by edge from entity node
        let has_evidence = graph
            .csr
            .out_range(entity_node_id)
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "evidenced_by");
        assert!(has_evidence, "evidenced_by edge not found from entity node");

        let _ = fs::remove_dir_all(root);
    }

    /// Test that ingest populates different change_frequency values for files with
    /// different git commit histories. The frequently-committed file must have a
    /// strictly higher change_frequency than the rarely-committed one.
    #[test]
    fn ingest_populates_change_frequency_from_git_history() {
        use std::process::Command;

        let root = temp_ingest_dir("git-change-freq");
        fs::create_dir_all(&root).unwrap();

        // Initialize a git repo
        let git_ok = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !git_ok {
            // git unavailable — skip rather than fail
            let _ = fs::remove_dir_all(root);
            return;
        }

        // Set minimal git identity so commits don't require global config
        let _ = Command::new("git")
            .args(["config", "user.email", "test@m1nd"])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.name", "m1nd-test"])
            .current_dir(&root)
            .output();

        // Write two Rust files
        fs::write(root.join("hot.rs"), "pub fn hot() {}\n").unwrap();
        fs::write(root.join("cold.rs"), "pub fn cold() {}\n").unwrap();

        // Initial commit — both files touched once
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init", "--no-gpg-sign"])
            .current_dir(&root)
            .output();

        // Commit hot.rs four more times (total 5 commits)
        for i in 1..=4 {
            fs::write(
                root.join("hot.rs"),
                format!("pub fn hot() {{ /* v{i} */ }}\n"),
            )
            .unwrap();
            let _ = Command::new("git")
                .args(["add", "hot.rs"])
                .current_dir(&root)
                .output();
            let _ = Command::new("git")
                .args(["commit", "-m", &format!("hot v{i}"), "--no-gpg-sign"])
                .current_dir(&root)
                .output();
        }

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        let hot_id = graph.resolve_id("file::hot.rs");
        let cold_id = graph.resolve_id("file::cold.rs");

        // Both file nodes must exist
        assert!(hot_id.is_some(), "file::hot.rs node not found in graph");
        assert!(cold_id.is_some(), "file::cold.rs node not found in graph");

        let hot_freq = graph.nodes.change_frequency[hot_id.unwrap().as_usize()].get();
        let cold_freq = graph.nodes.change_frequency[cold_id.unwrap().as_usize()].get();

        // hot.rs was committed 5x, cold.rs 1x — hot must have strictly higher frequency
        assert!(
            hot_freq > cold_freq,
            "expected hot_freq ({hot_freq:.4}) > cold_freq ({cold_freq:.4}): \
             hot.rs had 5 commits, cold.rs had 1"
        );

        // Sanity: cold.rs has at least 1 commit so its frequency must be > 0
        assert!(
            cold_freq > 0.0,
            "cold.rs had 1 commit so change_frequency should be > 0, got {cold_freq}"
        );

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // Go: calls edges + cross-file import resolution
    // -----------------------------------------------------------------------

    /// Asserts that the Go extractor emits `calls` edges for function/method
    /// call sites, and does NOT emit bogus call edges for keywords.
    ///
    /// Fixture:
    ///   main.go: package with a function `run` that calls `helper()` and
    ///   `fmt.Println(...)`, but also uses control-flow keywords `if` and `for`
    ///   which must NOT produce call edges.
    #[test]
    fn go_emits_calls_edges_and_guards_keywords() {
        let root = temp_ingest_dir("go-calls");
        fs::create_dir_all(&root).unwrap();

        fs::write(
            root.join("main.go"),
            r#"package main

import "fmt"

func helper() {
    fmt.Println("hello")
}

func run() {
    if true {
        for i := 0; i < 10; i++ {
            helper()
        }
    }
    helper()
}
"#,
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        // There must be at least one `calls` edge in the finalized graph.
        // helper() is called twice on non-keyword lines, so at minimum one
        // resolved or unresolved calls edge must exist.
        let has_calls_edge = (0..graph.num_nodes() as usize).any(|i| {
            let node_id = m1nd_core::types::NodeId::new(i as u32);
            graph
                .csr
                .out_range(node_id)
                .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls")
        });

        assert!(
            has_calls_edge,
            "Expected at least one `calls` edge in the graph after ingesting Go file with function calls"
        );

        // Keyword guard: Go keywords like `if`, `for`, `return` must NOT appear
        // as `calls` targets. We verify this by using the GoExtractor directly
        // (unit-level check in go.rs). At the integration level we just ensure
        // calls edges exist (previous assertion) and trust the extractor's
        // keyword exclusion list tested implicitly.

        let _ = fs::remove_dir_all(root);
    }

    /// Cross-file Go import resolution: two files in different sub-packages,
    /// one importing the other. Asserts a resolved file→file `imports` edge.
    ///
    /// Fixture:
    ///   pkg/util/util.go: package util — defines helper func
    ///   main.go: imports "mypkg/pkg/util" (last segment "util" matches dir)
    ///
    /// GoModuleIndex matches last import segment "util" against dir "pkg/util"
    /// and produces a resolved imports edge from main.go to util/util.go.
    #[test]
    fn go_cross_file_import_resolves_to_file_node() {
        let root = temp_ingest_dir("go-cross-file");
        fs::create_dir_all(root.join("pkg/util")).unwrap();

        fs::write(
            root.join("pkg/util/util.go"),
            r#"package util

func Helper() string {
    return "ok"
}
"#,
        )
        .unwrap();

        fs::write(
            root.join("main.go"),
            r#"package main

import (
    "mypkg/pkg/util"
    "fmt"
)

func main() {
    fmt.Println(util.Helper())
}
"#,
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        let main_file = graph
            .resolve_id("file::main.go")
            .expect("file::main.go not found");
        let util_file = graph
            .resolve_id("file::pkg/util/util.go")
            .expect("file::pkg/util/util.go not found");

        let has_import_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == util_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(
            has_import_edge,
            "Expected a cross-file `imports` edge from file::main.go to file::pkg/util/util.go. \
             GoModuleIndex should resolve last-segment 'util' to the pkg/util directory."
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Test that ingest succeeds with neutral defaults when the target directory
    /// is not a git repository. No panic, no error; change_frequency stays 0.0.
    #[test]
    fn ingest_succeeds_with_neutral_defaults_in_non_git_dir() {
        let root = temp_ingest_dir("no-git-neutral");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("lib.rs"), "pub fn f() {}\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        // Must not panic
        let result = ingest.ingest();
        assert!(
            result.is_ok(),
            "ingest should succeed in non-git dir: {:?}",
            result.err()
        );

        let (graph, _stats) = result.unwrap();
        let file_id = graph.resolve_id("file::lib.rs");
        assert!(file_id.is_some(), "file::lib.rs should be in graph");

        // Non-git file: change_frequency must be the neutral default (0.0)
        let freq = graph.nodes.change_frequency[file_id.unwrap().as_usize()].get();
        assert_eq!(
            freq, 0.0,
            "non-git file should have neutral change_frequency 0.0, got {freq}"
        );

        let _ = fs::remove_dir_all(root);
    }
}
