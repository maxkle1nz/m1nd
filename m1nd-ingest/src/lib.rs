#![allow(unused)]
// === crates/m1nd-ingest/src/lib.rs ===

use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::types::*;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub mod cargo_workspace;
pub mod cross_file;
pub mod diff;
pub mod extract;
pub mod json_adapter;
pub mod l1ght_adapter;
pub mod memory_adapter;
pub mod merge;
pub mod resolve;
pub mod walker;

pub use l1ght_adapter::L1ghtIngestAdapter;

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

    Some(format!("file::{}", trimmed))
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
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            timeout: Duration::from_secs(300),
            max_nodes: 500_000,
            skip_dirs: vec![
                ".git".into(),
                "node_modules".into(),
                "__pycache__".into(),
                ".venv".into(),
                "target".into(),
                "dist".into(),
                "build".into(),
                ".next".into(),
                "vendor".into(),
            ],
            skip_files: vec![
                "package-lock.json".into(),
                "yarn.lock".into(),
                "Cargo.lock".into(),
                "poetry.lock".into(),
            ],
            parallelism: std::thread::available_parallelism()
                .map(|p| p.get().min(16))
                .unwrap_or(8),
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
        );
        let walk_result = dir_walker.walk(&self.config.root)?;
        stats.files_scanned = walk_result.files.len() as u64;
        stats.commit_groups = walk_result.commit_groups.clone();

        use rayon::prelude::*;
        let num_threads = self.config.parallelism.clamp(1, 64);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .map_err(|e| M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: format!("thread pool: {}", e),
            })?;

        let extraction_results: Vec<(String, extract::ExtractionResult)> = pool.install(|| {
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
                    Some((file_id, result))
                })
                .collect()
        });

        let mut all_nodes = Vec::new();
        let mut all_edges = Vec::new();
        for (file_id, result) in &extraction_results {
            if start.elapsed() > self.config.timeout {
                eprintln!("[m1nd-ingest] Timeout after {} files", stats.files_parsed);
                break;
            }

            stats.files_parsed += 1;
            all_nodes.extend_from_slice(&result.nodes);
            all_edges.extend_from_slice(&result.edges);
        }

        let mut graph = m1nd_core::graph::Graph::new();
        let mut skipped_invalid_nodes = 0u64;
        for node in &all_nodes {
            if !is_valid_external_id(&node.id) {
                skipped_invalid_nodes += 1;
                eprintln!(
                    "[m1nd-ingest] WARNING: skipping invalid external_id {:?}",
                    node.id
                );
                continue;
            }

            let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
            if graph
                .add_node(&node.id, &node.label, node.node_type, &tags, 0.0, 0.0)
                .is_ok()
            {
                stats.nodes_created += 1;
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
}
