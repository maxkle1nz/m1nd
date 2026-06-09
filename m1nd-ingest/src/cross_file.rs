// === crates/m1nd-ingest/src/cross_file.rs ===
//
// Layer 1: Cross-File Edge Resolution
//
// Post-ingest pass that resolves ref:: edges into deterministic
// file-to-file cross-file edges. This is the keystone layer --
// without it, every file in the graph is an island.
//
// Edge types produced:
//   "imports"    — file A imports module from file B
//   "tests"      — test file A tests module B (naming convention)
//   "registers"  — A registers B as a route/plugin (include_router)
//
// Zero new dependencies -- uses only std + existing Graph API.

use std::collections::HashMap;
use std::path::Path;

use m1nd_core::error::M1ndResult;
use m1nd_core::graph::Graph;
use m1nd_core::types::*;
use regex::Regex;

// ---------------------------------------------------------------------------
// CrossFileStats — result statistics for the cross-file resolution pass
// ---------------------------------------------------------------------------

/// Statistics from cross-file edge resolution.
#[derive(Clone, Debug, Default)]
pub struct CrossFileStats {
    /// Total import edges resolved (file-to-file).
    pub imports_resolved: u64,
    /// Total import refs that could not be resolved (stdlib/third-party).
    pub imports_unresolved: u64,
    /// Test-to-module edges created via naming convention.
    pub test_edges_created: u64,
    /// Route registration edges created (include_router).
    pub register_edges_created: u64,
    /// Total cross-file edges created (sum of all types).
    pub total_edges_created: u64,
    /// Python files indexed in the module index.
    pub files_indexed: u64,
}

// ---------------------------------------------------------------------------
// PythonModuleIndex — maps module paths to file paths
// ---------------------------------------------------------------------------

/// Maps Python dotted module paths to file external IDs.
///
/// Built from discovered files during walk phase. Handles:
/// - Dotted paths: `backend.config` -> `file::backend/config.py`
/// - Bare names: `config` -> `file::backend/config.py` (flat layout)
/// - Package inits: `backend` -> `file::backend/__init__.py`
///
/// Flat Python projects (no `__init__.py` at root) need bare module
/// names like `config` to resolve directly to `config.py` within the
/// source directory.
struct PythonModuleIndex {
    /// dotted.module.path -> file external ID ("file::relative/path.py")
    module_to_file: HashMap<String, String>,
}

impl PythonModuleIndex {
    /// Build the module index from all file external IDs in the graph.
    ///
    /// Scans existing file nodes (NodeType::File) whose IDs end in `.py`.
    /// Registers each under multiple lookup keys:
    /// 1. Full dotted path: `backend.config` (from `backend/config.py`)
    /// 2. Bare filename (no extension): `config` (for flat-layout resolution)
    /// 3. Package init: `backend` (from `backend/__init__.py`)
    fn build(graph: &Graph) -> Self {
        let mut module_to_file: HashMap<String, String> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            // Recover the external ID for this node
            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) => p,
                None => continue,
            };

            if !rel_path.ends_with(".py") {
                continue;
            }

            // Strip .py extension
            let without_ext = &rel_path[..rel_path.len() - 3];

            // Full dotted path: "backend/config" -> "backend.config"
            let dotted = without_ext.replace('/', ".");

            // Handle __init__.py: "backend/__init__" -> package "backend"
            if dotted.ends_with(".__init__") {
                let package = &dotted[..dotted.len() - 9]; // strip ".__init__"
                module_to_file.insert(package.to_string(), ext_id.clone());
                continue;
            }

            // Register full dotted path
            module_to_file.insert(dotted.clone(), ext_id.clone());

            // Register bare filename for flat-layout resolution.
            // "backend.config" -> also register "config" if it does not
            // collide with an existing entry.
            if let Some(bare) = without_ext.rsplit('/').next() {
                // Only register bare name if it is different from the dotted path
                // (i.e., the file is nested, not already at root level).
                if bare != dotted {
                    // Use entry API: first registration wins.
                    // In flat layouts with a single source dir, this ensures
                    // `config` -> `backend/config.py` is deterministic.
                    module_to_file
                        .entry(bare.to_string())
                        .or_insert_with(|| ext_id.clone());
                }
            }
        }

        Self { module_to_file }
    }

    /// Resolve a Python import path to a file external ID.
    ///
    /// Tries exact match first, then progressively shorter prefixes
    /// to handle `from backend.config import Settings` -> resolve
    /// `backend.config` even if the ref was `backend.config.Settings`.
    ///
    /// Returns None for stdlib/third-party modules not in the project.
    fn resolve(&self, import_path: &str) -> Option<&str> {
        // Exact match: "config" or "backend.config"
        if let Some(file_id) = self.module_to_file.get(import_path) {
            return Some(file_id.as_str());
        }

        // Progressively shorter prefixes for dotted paths:
        // "backend.config.Settings" -> try "backend.config" -> "backend"
        if import_path.contains('.') {
            let mut parts: Vec<&str> = import_path.split('.').collect();
            while parts.len() > 1 {
                parts.pop();
                let prefix = parts.join(".");
                if let Some(file_id) = self.module_to_file.get(&prefix) {
                    return Some(file_id.as_str());
                }
            }
            // Also try the last segment alone (bare name fallback)
            if let Some(last) = import_path.rsplit('.').next() {
                if let Some(file_id) = self.module_to_file.get(last) {
                    return Some(file_id.as_str());
                }
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// JsModuleIndex — maps JS/TS relative specifiers to file external IDs
// ---------------------------------------------------------------------------

/// Maps JS/TS import specifiers (relative paths) to file external IDs.
///
/// Built from discovered file nodes tagged "typescript" or having a JS/TS
/// extension. Only resolves RELATIVE specifiers (starting with "./" or "../").
/// Bare/scoped specifiers (node_modules) are intentionally skipped.
///
/// Probes these suffixes in order: .ts .tsx .js .jsx .mjs .cjs
/// Also probes index files: /index.ts /index.tsx /index.js /index.jsx
struct JsModuleIndex {
    /// Canonical path (no extension, relative to project root) -> file external ID
    /// e.g. "src/utils" -> "file::src/utils.ts"
    path_to_file: HashMap<String, String>,
}

const JS_TS_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];

impl JsModuleIndex {
    /// Build the JS/TS module index from all file external IDs in the graph.
    ///
    /// Scans file nodes whose IDs end in a JS/TS extension and registers them
    /// under their path without extension, enabling specifier -> file resolution.
    fn build(graph: &Graph) -> Self {
        let mut path_to_file: HashMap<String, String> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) => p,
                None => continue,
            };

            // Only handle JS/TS files
            let is_js_ts = JS_TS_EXTENSIONS
                .iter()
                .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
            if !is_js_ts {
                continue;
            }

            // Strip extension to get canonical path key
            // e.g. "src/utils.ts" -> "src/utils"
            let without_ext = JS_TS_EXTENSIONS
                .iter()
                .find_map(|ext| rel_path.strip_suffix(&format!(".{}", ext)))
                .unwrap_or(rel_path);

            // Register under canonical path; first registration wins to avoid
            // e.g. both utils.ts and utils.js registering as "utils"
            path_to_file
                .entry(without_ext.to_string())
                .or_insert_with(|| ext_id.clone());
        }

        Self { path_to_file }
    }

    /// Resolve a JS/TS import specifier to a file external ID.
    ///
    /// `specifier`: the raw import specifier (e.g. "./utils", "../lib/helper").
    /// `source_dir`: the directory of the importing file (e.g. "src/components").
    ///
    /// Only resolves RELATIVE specifiers (starts with "./" or "../").
    /// Returns None for bare specifiers (node_modules / built-ins).
    fn resolve(&self, specifier: &str, source_dir: &str) -> Option<&str> {
        // Only handle relative imports
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            return None;
        }

        // Resolve the specifier against the source directory to get a canonical path
        let joined = join_path(source_dir, specifier);

        // If the specifier already has an extension, try exact match first
        let has_ext = JS_TS_EXTENSIONS
            .iter()
            .any(|ext| joined.ends_with(&format!(".{}", ext)));

        if has_ext {
            // Try the exact path (strip extension to look up in index)
            if let Some(without_ext) = JS_TS_EXTENSIONS
                .iter()
                .find_map(|ext| joined.strip_suffix(&format!(".{}", ext)))
            {
                if let Some(file_id) = self.path_to_file.get(without_ext) {
                    return Some(file_id.as_str());
                }
            }
        }

        // No extension in specifier — probe suffixes in priority order
        if let Some(file_id) = self.path_to_file.get(joined.as_str()) {
            return Some(file_id.as_str());
        }

        // Probe /index.{ext} variants
        let index_path = format!("{}/index", joined);
        if let Some(file_id) = self.path_to_file.get(index_path.as_str()) {
            return Some(file_id.as_str());
        }

        None
    }
}

// ---------------------------------------------------------------------------
// GoModuleIndex — maps Go import paths to file external IDs
// ---------------------------------------------------------------------------

/// Maps Go import package paths to file external IDs.
///
/// Go import paths look like:
///   `"github.com/org/repo/pkg/foo"` — remote module (external dep)
///   `"mymodule/internal/bar"`        — local sub-package
///
/// Resolution heuristic (honest/simple — no go.mod parsing):
///   1. Extract the LAST path segment of the import string (e.g. `"foo"` from
///      `"github.com/org/repo/pkg/foo"`).
///   2. Walk all `.go` file nodes in the graph; for each file, extract its
///      containing directory name (the last segment of its relative dir path).
///   3. If the import's last segment matches the file's parent directory name,
///      that file is a candidate. Among candidates, prefer the one whose full
///      relative path contains the most import-segment overlap (longest suffix
///      match of the import against the file's directory path).
///
/// Limitations (honest):
///   • No go.mod / go.sum parsing — module root is unknown.
///   • Ambiguous when two packages share the same last-segment name.
///   • External dependencies (stdlib, github.com with no local files) stay
///     unresolved; this is correct — we never fabricate edges.
///   • Alias imports (`myalias "pkg/foo"`) are not detected here; the extractor
///     registers the raw path and we resolve against it.
struct GoModuleIndex {
    /// last_segment -> Vec<(file_external_id, dir_path_slash_separated)>
    /// e.g. "foo" -> [("file::pkg/foo/bar.go", "pkg/foo"), ...]
    segment_to_files: HashMap<String, Vec<(String, String)>>,
}

const GO_EXTENSION: &str = "go";

impl GoModuleIndex {
    fn build(graph: &Graph) -> Self {
        let mut segment_to_files: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) if p.ends_with(&format!(".{}", GO_EXTENSION)) => p,
                _ => continue,
            };

            // Directory of this .go file (e.g. "pkg/foo" from "pkg/foo/bar.go")
            let dir = match rel_path.rfind('/') {
                Some(idx) => &rel_path[..idx],
                None => "", // file is at repo root — dir is ""
            };

            // Last segment of the directory path is the package name by convention
            let last_seg = match dir.rfind('/') {
                Some(idx) => &dir[idx + 1..],
                None if dir.is_empty() => "",
                None => dir,
            };

            if last_seg.is_empty() {
                continue;
            }

            segment_to_files
                .entry(last_seg.to_string())
                .or_default()
                .push((ext_id.clone(), dir.to_string()));
        }

        Self { segment_to_files }
    }

    /// Resolve a Go import path to a file external ID.
    ///
    /// `import_path`: the raw string from the import statement, e.g.
    ///   `"github.com/org/repo/pkg/foo"` or `"mymodule/bar"`.
    ///
    /// Returns None when the package cannot be matched to an ingested file
    /// (external dep, stdlib, or ambiguous with no clear winner).
    fn resolve(&self, import_path: &str) -> Option<&str> {
        // Extract the last path segment of the import
        let last_seg = import_path
            .split('/')
            .last()
            .filter(|s| !s.is_empty())?;

        let candidates = self.segment_to_files.get(last_seg)?;

        if candidates.is_empty() {
            return None;
        }

        // Single candidate: accept it (common case for local sub-packages)
        if candidates.len() == 1 {
            return Some(candidates[0].0.as_str());
        }

        // Multiple candidates: pick the one whose directory path has the longest
        // suffix match against the import path segments.
        // e.g. import "mymodule/internal/bar" → dir "internal/bar" > dir "bar"
        let import_segs: Vec<&str> = import_path.split('/').collect();
        let mut best: Option<&(String, String)> = None;
        let mut best_overlap = 0usize;

        for candidate in candidates {
            let dir_segs: Vec<&str> = candidate.1.split('/').collect();
            // Count how many trailing segments of the import path match the
            // trailing segments of the candidate dir path.
            let overlap = import_segs
                .iter()
                .rev()
                .zip(dir_segs.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            if overlap > best_overlap {
                best_overlap = overlap;
                best = Some(candidate);
            }
        }

        // Only return if there was at least one matching segment (the last_seg)
        // — which is guaranteed since all candidates share the last_seg key.
        best.map(|(id, _)| id.as_str())
    }
}

// ---------------------------------------------------------------------------
// JavaPackageIndex — maps Java fully-qualified class names to file external IDs
// ---------------------------------------------------------------------------

/// Maps Java import paths (fully-qualified class names) to file external IDs.
///
/// Java imports look like:
///   `import com.example.foo.Bar;`         — specific class
///   `import static com.example.foo.Bar;`  — static import (same format after stripping "static ")
///   `import com.example.foo.*;`           — wildcard (all classes in package)
///
/// Convention: class `com.example.foo.Bar` lives at `.../com/example/foo/Bar.java`.
///
/// Resolution heuristic (honest/conservative — no classpath or build tool parsing):
///   1. Extract the LAST segment of the import path:
///      `com.example.foo.Bar` → `Bar`; wildcard `com.example.foo.*` → `*` (special).
///   2. Walk all `.java` file nodes in the graph; for each file, extract its
///      filename stem (e.g. `Bar` from `com/example/foo/Bar.java`).
///   3. If the import's last segment matches the file's stem, that file is a candidate.
///   4. Among candidates, prefer the one whose directory path has the longest suffix
///      match against the package path of the import (mirrors GoModuleIndex heuristic).
///
/// Wildcard imports (`com.example.foo.*`): resolve to ALL `.java` files whose
///   directory path matches the package prefix (`com/example/foo`).
///
/// Limitations (honest):
///   • No pom.xml / build.gradle parsing — source roots unknown.
///   • Ambiguous when two classes share the same name in different packages with
///     equal suffix overlap; first entry wins.
///   • External/stdlib classes (java.util.*, org.springframework.*) with no ingested
///     files stay unresolved — never fabricate edges.
struct JavaPackageIndex {
    /// stem -> Vec<(file_external_id, dir_path_slash_separated)>
    /// e.g. "Bar" -> [("file::com/example/foo/Bar.java", "com/example/foo"), ...]
    stem_to_files: HashMap<String, Vec<(String, String)>>,
    /// All java file entries for wildcard resolution:
    /// dir_path -> Vec<file_external_id>
    dir_to_files: HashMap<String, Vec<String>>,
}

const JAVA_EXTENSION: &str = "java";

impl JavaPackageIndex {
    fn build(graph: &Graph) -> Self {
        let mut stem_to_files: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut dir_to_files: HashMap<String, Vec<String>> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) if p.ends_with(&format!(".{}", JAVA_EXTENSION)) => p,
                _ => continue,
            };

            // Directory of this .java file (e.g. "com/example/foo" from "com/example/foo/Bar.java")
            let dir = match rel_path.rfind('/') {
                Some(idx) => &rel_path[..idx],
                None => "", // file is at repo root
            };

            // Stem: filename without extension ("Bar" from "Bar.java")
            let stem = match rel_path.rfind('/') {
                Some(idx) => &rel_path[idx + 1..rel_path.len() - JAVA_EXTENSION.len() - 1],
                None => &rel_path[..rel_path.len() - JAVA_EXTENSION.len() - 1],
            };

            if stem.is_empty() {
                continue;
            }

            stem_to_files
                .entry(stem.to_string())
                .or_default()
                .push((ext_id.clone(), dir.to_string()));

            dir_to_files
                .entry(dir.to_string())
                .or_default()
                .push(ext_id.clone());
        }

        Self {
            stem_to_files,
            dir_to_files,
        }
    }

    /// Resolve a Java fully-qualified import path to file external ID(s).
    ///
    /// `import_path`: the raw path from the import statement, e.g.
    ///   `com.example.foo.Bar` or `com.example.foo.*`.
    ///
    /// Returns a Vec (possibly empty). Empty when the class/package cannot be
    /// matched to any ingested `.java` file (external dep, stdlib).
    ///
    /// Wildcard imports return ALL matching files in the resolved package directory.
    fn resolve(&self, import_path: &str) -> Vec<&str> {
        // Wildcard: com.example.foo.*
        if import_path.ends_with(".*") {
            let package = &import_path[..import_path.len() - 2]; // strip ".*"
            // Convert dotted package to slash-separated dir
            let dir_path = package.replace('.', "/");
            // Find all .java files in that directory (exact match or suffix match)
            let mut results: Vec<&str> = Vec::new();
            // Try exact dir match first
            if let Some(files) = self.dir_to_files.get(dir_path.as_str()) {
                for f in files {
                    results.push(f.as_str());
                }
            }
            if results.is_empty() {
                // Try suffix match: any dir whose last segments match dir_path
                for (dir, files) in &self.dir_to_files {
                    if dir.ends_with(dir_path.as_str())
                        && (dir.len() == dir_path.len()
                            || dir.as_bytes()[dir.len() - dir_path.len() - 1] == b'/')
                    {
                        for f in files {
                            results.push(f.as_str());
                        }
                    }
                }
            }
            return results;
        }

        // Specific class: com.example.foo.Bar
        let last_seg = import_path.split('.').last().filter(|s| !s.is_empty());
        let last_seg = match last_seg {
            Some(s) => s,
            None => return Vec::new(),
        };

        let candidates = match self.stem_to_files.get(last_seg) {
            Some(c) => c,
            None => return Vec::new(),
        };

        if candidates.is_empty() {
            return Vec::new();
        }

        // Single candidate: accept it
        if candidates.len() == 1 {
            return vec![candidates[0].0.as_str()];
        }

        // Multiple candidates: pick the one with the longest suffix overlap
        // between the import's package path and the file's directory path.
        // e.g. import "com.example.foo.Bar" → dir "com/example/foo" > dir "foo"
        let import_dir = if let Some(last_dot) = import_path.rfind('.') {
            import_path[..last_dot].replace('.', "/")
        } else {
            String::new()
        };
        let import_dir_segs: Vec<&str> = import_dir.split('/').filter(|s| !s.is_empty()).collect();

        let mut best: Option<&(String, String)> = None;
        let mut best_overlap = 0usize;

        for candidate in candidates {
            let dir_segs: Vec<&str> = candidate
                .1
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            let overlap = import_dir_segs
                .iter()
                .rev()
                .zip(dir_segs.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            if overlap > best_overlap
                || (overlap == best_overlap && best.is_none())
            {
                best_overlap = overlap;
                best = Some(candidate);
            }
        }

        match best {
            Some((id, _)) => vec![id.as_str()],
            None => vec![candidates[0].0.as_str()],
        }
    }
}

// ---------------------------------------------------------------------------
// CHeaderIndex — maps C/C++ #include paths to file external IDs
// ---------------------------------------------------------------------------

/// Maps C/C++ `#include` target strings to file external IDs.
///
/// The tree-sitter extractor produces the following ref:: targets for C/C++:
///   - `#include "util.h"`   → `util.h`   (string_literal child, quotes stripped)
///   - `#include <stdio.h>`  → `stdio.h`  (system_lib_string, angle brackets stripped
///                                          via fallback word extraction in extract_import_target)
///
/// Resolution strategy (QUOTED includes only):
///   1. If the target contains a path separator (`/`), join it against the
///      including file's directory and look up the normalized relative path.
///   2. If the target is a bare filename (no `/`), try basename match among all
///      ingested C/C++ files — e.g. `util.h` → `include/util.h`.
///
/// System / angle-bracket includes (`stdio.h`, `stdlib.h`, etc.) are left
/// UNRESOLVED — they will simply not be found in the project's file index,
/// which is the correct behaviour.  We never fabricate edges to external headers.
///
/// Limitations (honest):
///   • No `-I` / include-path awareness — only project-local headers resolve.
///   • Bare-name collision: if two headers share the same filename, first-win.
///   • C# `using_directive` is NOT resolved here (see separate comment in
///     resolve_cross_file_edges for the rationale).
struct CHeaderIndex {
    /// Canonical relative path (no leading `./`) → file external ID
    /// e.g. "include/util.h" → "file::include/util.h"
    path_to_file: HashMap<String, String>,
    /// Basename → Vec<(file_external_id, dir_slash_separated)>
    /// Used for bare-name resolution when path join misses.
    /// e.g. "util.h" → [("file::include/util.h", "include"), ...]
    basename_to_files: HashMap<String, Vec<(String, String)>>,
}

const C_EXTENSIONS: &[&str] = &["h", "hpp", "hxx", "hh", "c", "cc", "cpp", "cxx"];

impl CHeaderIndex {
    fn build(graph: &Graph) -> Self {
        let mut path_to_file: HashMap<String, String> = HashMap::new();
        let mut basename_to_files: HashMap<String, Vec<(String, String)>> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) => p,
                None => continue,
            };

            let is_c = C_EXTENSIONS
                .iter()
                .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
            if !is_c {
                continue;
            }

            // Register under the full relative path (normalized, no ./)
            path_to_file
                .entry(rel_path.to_string())
                .or_insert_with(|| ext_id.clone());

            // Register under the basename for bare-name fallback
            let dir = match rel_path.rfind('/') {
                Some(idx) => &rel_path[..idx],
                None => "",
            };
            let basename = match rel_path.rfind('/') {
                Some(idx) => &rel_path[idx + 1..],
                None => rel_path,
            };

            if !basename.is_empty() {
                basename_to_files
                    .entry(basename.to_string())
                    .or_default()
                    .push((ext_id.clone(), dir.to_string()));
            }
        }

        Self {
            path_to_file,
            basename_to_files,
        }
    }

    /// Resolve a C/C++ `#include` target to a file external ID.
    ///
    /// `target`: the string produced by extract_import_target, e.g.
    ///   `util.h`, `../include/types.h`, `stdio.h`.
    /// `source_dir`: the directory of the including file (e.g. `src/net`).
    ///
    /// Returns None when the header cannot be matched to any ingested file
    /// (system header, external dep, or path not in project).
    fn resolve(&self, target: &str, source_dir: &str) -> Option<&str> {
        // Strip any leading ./ for normalization
        let target = target.trim_start_matches("./");

        if target.contains('/') {
            // Path-relative include: join against source directory
            let joined = join_path(source_dir, target);
            if let Some(file_id) = self.path_to_file.get(joined.as_str()) {
                return Some(file_id.as_str());
            }
            // Fallback: try the basename only (in case the path isn't rooted at project root)
            let basename = joined.rsplit('/').next().unwrap_or(target);
            return self.resolve_by_basename(basename);
        }

        // Bare filename (no path separator): exact path match first
        // e.g. target = "util.h" and file at "util.h" (root level)
        if let Some(file_id) = self.path_to_file.get(target) {
            return Some(file_id.as_str());
        }

        // Try joining with source_dir: `util.h` in `src/net` → `src/net/util.h`
        if !source_dir.is_empty() {
            let joined = format!("{}/{}", source_dir, target);
            if let Some(file_id) = self.path_to_file.get(joined.as_str()) {
                return Some(file_id.as_str());
            }
        }

        // Basename fallback: find any ingested header with this filename
        self.resolve_by_basename(target)
    }

    /// Resolve by basename alone, returning the unique or best-match file.
    fn resolve_by_basename(&self, basename: &str) -> Option<&str> {
        let candidates = self.basename_to_files.get(basename)?;
        if candidates.is_empty() {
            return None;
        }
        // Single candidate: accept it
        if candidates.len() == 1 {
            return Some(candidates[0].0.as_str());
        }
        // Multiple: prefer header files (.h/.hpp) over source files, then first wins
        for (file_id, _) in candidates {
            let is_header = file_id.ends_with(".h") || file_id.ends_with(".hpp")
                || file_id.ends_with(".hxx") || file_id.ends_with(".hh");
            if is_header {
                return Some(file_id.as_str());
            }
        }
        Some(candidates[0].0.as_str())
    }
}

// ---------------------------------------------------------------------------
// KotlinPackageIndex — maps Kotlin import paths to file external IDs
// ---------------------------------------------------------------------------

/// Maps Kotlin import paths (fully-qualified class/object names) to file external IDs.
///
/// Kotlin imports look like:
///   `import com.example.foo.Bar`     — specific class
///   `import com.example.foo.*`       — wildcard (all declarations in package)
///
/// Convention mirrors Java: class `com.example.foo.Bar` lives in a file
/// whose stem is `Bar` inside a directory path matching `com/example/foo/`.
///
/// Resolution heuristic (mirrors JavaPackageIndex — honest/conservative):
///   1. Extract the LAST segment of the import path:
///      `com.example.foo.Bar` → `Bar`; wildcard `com.example.foo.*` → `*`.
///   2. Walk all `.kt` file nodes; match stem (e.g. `Bar` from `Bar.kt`).
///   3. Among candidates, prefer the one with the longest suffix overlap
///      between the import's package path and the file's directory path.
///
/// Wildcard imports (`com.example.foo.*`): resolve to ALL `.kt` files whose
///   directory path matches the package prefix.
///
/// Limitations (honest):
///   • No build.gradle / pom.xml parsing — source roots unknown.
///   • Ambiguous same-stem class names resolved by suffix overlap; first wins.
///   • External/stdlib classes (kotlin.collections.*, java.io.*, etc.) with no
///     ingested files stay unresolved — never fabricate edges.
///   • Kotlin allows top-level functions and multiple classes per file, so a
///     stem match is a heuristic — single-class-per-file convention is assumed.
struct KotlinPackageIndex {
    /// stem -> Vec<(file_external_id, dir_path_slash_separated)>
    /// e.g. "Bar" -> [("file::com/example/foo/Bar.kt", "com/example/foo"), ...]
    stem_to_files: HashMap<String, Vec<(String, String)>>,
    /// dir_path -> Vec<file_external_id> for wildcard resolution
    dir_to_files: HashMap<String, Vec<String>>,
}

const KOTLIN_EXTENSIONS: &[&str] = &["kt", "kts"];

impl KotlinPackageIndex {
    fn build(graph: &Graph) -> Self {
        let mut stem_to_files: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut dir_to_files: HashMap<String, Vec<String>> = HashMap::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) => p,
                None => continue,
            };

            let is_kt = KOTLIN_EXTENSIONS
                .iter()
                .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
            if !is_kt {
                continue;
            }

            // Directory of this .kt file
            let dir = match rel_path.rfind('/') {
                Some(idx) => &rel_path[..idx],
                None => "",
            };

            // Stem: filename without extension
            let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
            let stem = KOTLIN_EXTENSIONS
                .iter()
                .find_map(|ext| filename.strip_suffix(&format!(".{}", ext)))
                .unwrap_or(filename);

            if stem.is_empty() {
                continue;
            }

            stem_to_files
                .entry(stem.to_string())
                .or_default()
                .push((ext_id.clone(), dir.to_string()));

            dir_to_files
                .entry(dir.to_string())
                .or_default()
                .push(ext_id.clone());
        }

        Self {
            stem_to_files,
            dir_to_files,
        }
    }

    /// Resolve a Kotlin fully-qualified import path to file external ID(s).
    ///
    /// `import_path`: e.g. `com.example.foo.Bar` or `com.example.foo.*`.
    ///
    /// Returns a Vec (possibly empty). Empty when the class/package cannot be
    /// matched to any ingested `.kt` file (external dep, stdlib).
    fn resolve(&self, import_path: &str) -> Vec<&str> {
        // Wildcard: com.example.foo.*
        if import_path.ends_with(".*") {
            let package = &import_path[..import_path.len() - 2]; // strip ".*"
            let dir_path = package.replace('.', "/");
            let mut results: Vec<&str> = Vec::new();
            // Exact dir match
            if let Some(files) = self.dir_to_files.get(dir_path.as_str()) {
                for f in files {
                    results.push(f.as_str());
                }
            }
            if results.is_empty() {
                // Suffix match: any dir whose trailing segments match dir_path
                for (dir, files) in &self.dir_to_files {
                    if dir.ends_with(dir_path.as_str())
                        && (dir.len() == dir_path.len()
                            || dir.as_bytes()[dir.len() - dir_path.len() - 1] == b'/')
                    {
                        for f in files {
                            results.push(f.as_str());
                        }
                    }
                }
            }
            return results;
        }

        // Specific class: com.example.foo.Bar
        let last_seg = import_path.split('.').last().filter(|s| !s.is_empty());
        let last_seg = match last_seg {
            Some(s) => s,
            None => return Vec::new(),
        };

        let candidates = match self.stem_to_files.get(last_seg) {
            Some(c) => c,
            None => return Vec::new(),
        };

        if candidates.is_empty() {
            return Vec::new();
        }

        if candidates.len() == 1 {
            return vec![candidates[0].0.as_str()];
        }

        // Multiple candidates: pick by longest suffix overlap of package path vs dir path
        let import_dir = if let Some(last_dot) = import_path.rfind('.') {
            import_path[..last_dot].replace('.', "/")
        } else {
            String::new()
        };
        let import_dir_segs: Vec<&str> = import_dir
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut best: Option<&(String, String)> = None;
        let mut best_overlap = 0usize;

        for candidate in candidates {
            let dir_segs: Vec<&str> = candidate
                .1
                .split('/')
                .filter(|s| !s.is_empty())
                .collect();
            let overlap = import_dir_segs
                .iter()
                .rev()
                .zip(dir_segs.iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            if overlap > best_overlap || (overlap == best_overlap && best.is_none()) {
                best_overlap = overlap;
                best = Some(candidate);
            }
        }

        match best {
            Some((id, _)) => vec![id.as_str()],
            None => vec![candidates[0].0.as_str()],
        }
    }
}

// ---------------------------------------------------------------------------
// RustModuleIndex — maps Rust module paths to file external IDs
// ---------------------------------------------------------------------------

/// Maps Rust module paths to file external IDs.
///
/// Rust module system rules (conservative subset we resolve):
///
/// 1. **`mod NAME;`** — `dir/parent.rs` declares module `NAME`; resolves to
///    `dir/NAME.rs` OR `dir/NAME/mod.rs` (first one that exists in the graph).
///
/// 2. **`use crate::a::b::...`** — maps to `<crate_root>/a/b.rs` or
///    `<crate_root>/a/b/mod.rs`, stopping at the deepest path segment that
///    maps to an actual ingested file (trailing segments may be item names).
///
/// 3. **`use super::X`** — parent module's file. **`use self::X`** — current
///    module's file (treated as self-import, skipped).
///
/// 4. External crates (`use serde::...`, `use std::...`, etc.) — UNRESOLVED.
///    Any path whose first segment isn't `crate`/`super`/`self` and doesn't
///    match a known module path is left unresolved. Never fabricate edges.
///
/// Limitations (honest):
///   • Inline `mod x { ... }` blocks are NOT resolved to a file (no file to map to).
///   • `#[path = "..."]` overrides are NOT followed.
///   • Re-exports (`pub use ...`) are tracked as `imports` to the source file
///     but the re-export semantic is not propagated.
///   • Glob `use a::*` — resolved to the file owning module `a` (the `*` is dropped),
///     but individual item membership is not tracked.
///   • Ambiguous multi-crate workspaces: crate root detection uses the first
///     `src/lib.rs` or `src/main.rs` found in the same directory subtree as the
///     importing file (closest ancestor wins).
struct RustModuleIndex {
    /// Maps "crate_root_dir/module/path" -> file external ID
    /// e.g. "src/foo/bar" -> "file::src/foo/bar.rs"
    ///
    /// Keys are slash-separated paths relative to the project root,
    /// WITHOUT the .rs extension (i.e., the canonical path key).
    path_to_file: HashMap<String, String>,

    /// Set of all crate root directories found in the graph.
    /// A crate root dir is the directory containing lib.rs or main.rs.
    /// e.g. "src" for "src/lib.rs", "" for "lib.rs".
    crate_root_dirs: Vec<String>,
}

const RUST_EXTENSION: &str = "rs";

impl RustModuleIndex {
    /// Build the Rust module index from all file external IDs in the graph.
    ///
    /// Scans file nodes whose IDs end in `.rs`. Registers each under its
    /// canonical path (without extension) for crate-path resolution.
    /// Also identifies crate root directories (containing lib.rs or main.rs).
    fn build(graph: &Graph) -> Self {
        let mut path_to_file: HashMap<String, String> = HashMap::new();
        let mut crate_root_dirs: Vec<String> = Vec::new();

        for i in 0..graph.num_nodes() as usize {
            if graph.nodes.node_type[i] != NodeType::File {
                continue;
            }

            let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
                Some(id) => id,
                None => continue,
            };

            let rel_path = match ext_id.strip_prefix("file::") {
                Some(p) if p.ends_with(&format!(".{}", RUST_EXTENSION)) => p,
                _ => continue,
            };

            // Strip .rs extension to get canonical path key
            let without_ext = &rel_path[..rel_path.len() - 3]; // strip ".rs"

            // Handle mod.rs: "src/foo/mod.rs" -> register under "src/foo"
            let key = if without_ext.ends_with("/mod") {
                without_ext[..without_ext.len() - 4].to_string() // strip "/mod"
            } else {
                without_ext.to_string()
            };

            // Register: first registration wins (mod.rs and foo.rs may both exist
            // but we prefer non-mod.rs since it's the more modern form)
            path_to_file
                .entry(key)
                .or_insert_with(|| ext_id.clone());

            // Detect crate roots: files named lib.rs or main.rs
            let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
            if filename == "lib.rs" || filename == "main.rs" {
                let dir = match rel_path.rfind('/') {
                    Some(idx) => rel_path[..idx].to_string(),
                    None => String::new(), // file is at repo root
                };
                if !crate_root_dirs.contains(&dir) {
                    crate_root_dirs.push(dir);
                }
            }
        }

        Self {
            path_to_file,
            crate_root_dirs,
        }
    }

    /// Find the best crate root directory for a given source file path.
    ///
    /// Walks up the directory tree from `source_dir` and returns the first
    /// crate root dir that is an ancestor (or equal) to `source_dir`.
    /// Prefers the most specific (deepest) ancestor.
    fn crate_root_for(&self, source_dir: &str) -> Option<&str> {
        // Find all crate root dirs that are a prefix of source_dir (or equal)
        let mut best: Option<&str> = None;
        for root_dir in &self.crate_root_dirs {
            let is_ancestor = if root_dir.is_empty() {
                true // repo root is ancestor of everything
            } else {
                source_dir == root_dir
                    || source_dir.starts_with(&format!("{}/", root_dir))
            };
            if is_ancestor {
                // Prefer deepest ancestor (longest path)
                if best.map_or(true, |b: &str| root_dir.len() > b.len()) {
                    best = Some(root_dir.as_str());
                }
            }
        }
        best
    }

    /// Resolve a Rust crate-relative path to a file external ID.
    ///
    /// `crate_path`: slash-separated path segments relative to crate root,
    ///   e.g. `"foo/bar"` (from `use crate::foo::bar::Baz` → `"foo/bar"`).
    /// `crate_root_dir`: the directory of the crate root file (e.g. `"src"`).
    ///
    /// Tries progressively shorter prefixes to handle trailing item segments.
    fn resolve_crate_path(&self, crate_path: &str, crate_root_dir: &str) -> Option<&str> {
        // Build the full path: crate_root_dir + "/" + crate_path
        let full_path = if crate_root_dir.is_empty() {
            crate_path.to_string()
        } else {
            format!("{}/{}", crate_root_dir, crate_path)
        };

        // Try exact match
        if let Some(file_id) = self.path_to_file.get(full_path.as_str()) {
            return Some(file_id.as_str());
        }

        // Progressively shorter prefixes (trailing segments may be items, not modules)
        if crate_path.contains('/') {
            let mut parts: Vec<&str> = crate_path.split('/').collect();
            while parts.len() > 1 {
                parts.pop();
                let prefix = parts.join("/");
                let candidate = if crate_root_dir.is_empty() {
                    prefix.clone()
                } else {
                    format!("{}/{}", crate_root_dir, prefix)
                };
                if let Some(file_id) = self.path_to_file.get(candidate.as_str()) {
                    return Some(file_id.as_str());
                }
            }
        }

        None
    }

    /// Resolve a `super::` import to the parent module's file external ID.
    ///
    /// `source_file_rel_path`: relative path of the importing file (e.g. `"src/foo/bar.rs"`).
    ///
    /// Returns the parent module's file ID, or None if already at crate root.
    fn resolve_super(&self, source_file_rel_path: &str) -> Option<&str> {
        // Strip .rs and determine the module's parent
        let without_ext = source_file_rel_path.strip_suffix(".rs")?;

        // If this is a mod.rs, the parent is the grandparent dir
        let canonical = if without_ext.ends_with("/mod") {
            &without_ext[..without_ext.len() - 4] // "src/foo"
        } else {
            without_ext // "src/foo/bar" -> parent is "src/foo"
        };

        // Parent = everything before the last slash
        let parent_path = match canonical.rfind('/') {
            Some(idx) => &canonical[..idx],
            None => return None, // already at root, no super
        };

        // Try "parent_path" directly (for mod.rs case) or "parent_path.rs"
        // Look up in path_to_file
        self.path_to_file.get(parent_path).map(|s| s.as_str())
    }
}

/// Join a base directory path with a relative specifier, normalizing `..` and `.`.
/// Both paths use forward slashes. Returns a normalized relative path.
fn join_path(base_dir: &str, relative: &str) -> String {
    // Build the initial path by concatenating base_dir and relative
    let combined = if base_dir.is_empty() {
        relative.to_string()
    } else {
        format!("{}/{}", base_dir, relative)
    };

    // Normalize: split on '/', resolve '.' and '..'
    let mut parts: Vec<&str> = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {} // skip empty and self-ref segments
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

// ---------------------------------------------------------------------------
// resolve_cross_file_edges — main entry point
// ---------------------------------------------------------------------------

/// Post-ingest pass that resolves ref:: edges into deterministic
/// file-to-file cross-file edges.
///
/// Must be called AFTER all nodes and intra-file edges have been added
/// to the graph, but BEFORE finalize().
///
/// Produces three edge types:
/// 1. **imports** — from import ref:: edges already in the graph
/// 2. **tests** — from test file naming convention (test_X.py -> X.py)
/// 3. **registers** — from include_router() patterns in source files
///
/// All edges are Forward direction with appropriate causal strengths.
pub fn resolve_cross_file_edges(graph: &mut Graph, root: &Path) -> M1ndResult<CrossFileStats> {
    let mut stats = CrossFileStats::default();

    // Step 1: Build the Python module index from file nodes already in the graph.
    let module_index = PythonModuleIndex::build(graph);
    stats.files_indexed = module_index.module_to_file.len() as u64;

    // Step 2: Collect existing import ref:: edges from pending edges.
    // We scan the pending edges (pre-finalize) looking for "imports" relation
    // edges whose targets are ref:: prefixed. Since ref:: edges were skipped
    // during graph building (lib.rs:296-299), we need a different approach:
    // scan the extraction results. But we do not have access to extraction
    // results here -- we only have the graph.
    //
    // Alternative approach: scan all existing file nodes and reconstruct
    // import relationships by examining the source files on disk.
    // This is cleaner and avoids coupling to the extraction pipeline.
    let import_edges = collect_import_edges_from_files(graph, root, &module_index);

    for (source_file_id, target_file_id, relation) in &import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 3: Test-to-module edges via naming convention.
    let test_edges = infer_test_edges(graph, &module_index);
    for (test_file_id, source_file_id) in &test_edges {
        if add_cross_file_edge(graph, test_file_id, source_file_id, "tests", &mut stats) {
            stats.test_edges_created += 1;
        }
    }

    // Step 4: Route registration edges from include_router() in source.
    let register_edges = detect_route_registrations(graph, root, &module_index);
    for (main_file_id, route_file_id) in &register_edges {
        if add_cross_file_edge(graph, main_file_id, route_file_id, "registers", &mut stats) {
            stats.register_edges_created += 1;
        }
    }

    // Step 5: JS/TS cross-file import resolution via JsModuleIndex.
    let js_index = JsModuleIndex::build(graph);
    stats.files_indexed += js_index.path_to_file.len() as u64;
    let js_import_edges = collect_js_import_edges_from_files(graph, root, &js_index);
    for (source_file_id, target_file_id, relation) in &js_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 6: Go cross-file import resolution via GoModuleIndex.
    let go_index = GoModuleIndex::build(graph);
    stats.files_indexed += go_index.segment_to_files.len() as u64;
    let go_import_edges = collect_go_import_edges_from_files(graph, root, &go_index);
    for (source_file_id, target_file_id, relation) in &go_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 7: Java cross-file import resolution via JavaPackageIndex.
    let java_index = JavaPackageIndex::build(graph);
    stats.files_indexed += java_index.stem_to_files.len() as u64;
    let java_import_edges = collect_java_import_edges_from_files(graph, root, &java_index);
    for (source_file_id, target_file_id, relation) in &java_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 8: Rust cross-file import/module resolution via RustModuleIndex.
    let rust_index = RustModuleIndex::build(graph);
    stats.files_indexed += rust_index.path_to_file.len() as u64;
    let rust_import_edges = collect_rust_import_edges_from_files(graph, root, &rust_index);
    for (source_file_id, target_file_id, relation) in &rust_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 9: C/C++ cross-file #include resolution via CHeaderIndex.
    // Only QUOTED includes (`#include "..."`) resolve to project-local files.
    // System / angle-bracket includes (`#include <stdio.h>`) produce a bare
    // basename like `stdio.h` that will simply not be found in the index —
    // they stay unresolved naturally without any special-casing.
    //
    // NOTE — C# `using_directive` is intentionally NOT resolved here.
    // C# namespaces do not map 1:1 to files: multiple files can declare the
    // same namespace, and a single file can declare any namespace.  Resolving
    // `using Foo.Bar;` to a file would require a full Roslyn-style compilation
    // model.  Without that, any edge we produced would be fabricated.
    // C# imports are left unresolved until a proper namespace index is available.
    let c_index = CHeaderIndex::build(graph);
    stats.files_indexed += c_index.path_to_file.len() as u64;
    let c_import_edges = collect_c_import_edges_from_files(graph, root, &c_index);
    for (source_file_id, target_file_id, relation) in &c_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    // Step 10: Kotlin cross-file import resolution via KotlinPackageIndex.
    // Mirrors JavaPackageIndex: last-segment stem match + package-path suffix overlap.
    // Wildcard `import com.foo.*` resolves to all `.kt` files in that package dir.
    // External / stdlib imports stay unresolved.
    let kotlin_index = KotlinPackageIndex::build(graph);
    stats.files_indexed += kotlin_index.stem_to_files.len() as u64;
    let kotlin_import_edges = collect_kotlin_import_edges_from_files(graph, root, &kotlin_index);
    for (source_file_id, target_file_id, relation) in &kotlin_import_edges {
        if add_cross_file_edge(graph, source_file_id, target_file_id, relation, &mut stats) {
            stats.imports_resolved += 1;
        } else {
            stats.imports_unresolved += 1;
        }
    }

    Ok(stats)
}

// ---------------------------------------------------------------------------
// Import edge collection — scan Python files for import statements
// ---------------------------------------------------------------------------

/// Scan all Python file nodes in the graph, read their source from disk,
/// extract import statements, and resolve them to file-to-file edges.
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    module_index: &PythonModuleIndex,
) -> Vec<(String, String, String)> {
    let re_import = Regex::new(r"^\s*import\s+([\w.]+)").unwrap();
    let re_from_import = Regex::new(r"^\s*from\s+([\w.]+)\s+import").unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    // Track (source, target) pairs to avoid duplicate edges
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    // Skip list: imports that should not generate cross-file edges
    let skip_modules: &[&str] = &[
        "__future__",
        "typing",
        "typing_extensions",
        "abc",
        "collections",
        "dataclasses",
        "enum",
        "functools",
        "os",
        "sys",
        "io",
        "re",
        "json",
        "time",
        "datetime",
        "pathlib",
        "logging",
        "asyncio",
        "contextlib",
        "traceback",
        "uuid",
        "hashlib",
        "base64",
        "copy",
        "math",
        "random",
        "subprocess",
        "shutil",
        "tempfile",
        "signal",
        "socket",
        "urllib",
        "http",
        "ssl",
        "inspect",
        "importlib",
        "threading",
        "multiprocessing",
        "concurrent",
        "unittest",
        "pytest",
        "textwrap",
        "string",
    ];

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".py") => p,
            _ => continue,
        };

        // Read the source file from disk
        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let module_path = if let Some(caps) = re_from_import.captures(trimmed) {
                caps.get(1).unwrap().as_str().to_string()
            } else if let Some(caps) = re_import.captures(trimmed) {
                caps.get(1).unwrap().as_str().to_string()
            } else {
                continue;
            };

            // Skip stdlib/third-party modules
            let first_segment = module_path.split('.').next().unwrap_or(&module_path);
            if skip_modules.contains(&first_segment) {
                continue;
            }

            // Resolve to file
            if let Some(target_file_id) = module_index.resolve(&module_path) {
                // Skip self-imports
                if target_file_id == ext_id {
                    continue;
                }

                let key = (ext_id.clone(), target_file_id.to_string());
                if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                    e.insert(true);
                    edges.push((
                        ext_id.clone(),
                        target_file_id.to_string(),
                        "imports".to_string(),
                    ));
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Test-to-module inference — naming convention
// ---------------------------------------------------------------------------

/// Infer test-to-module edges from the naming convention:
///   `test_X.py` -> `X.py`
///   `tests/test_X.py` -> `X.py`
///   `backend/tests/test_X.py` -> `backend/X.py`
///
/// Returns Vec<(test_file_id, source_file_id)>.
fn infer_test_edges(graph: &Graph, module_index: &PythonModuleIndex) -> Vec<(String, String)> {
    let mut edges = Vec::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".py") => p,
            _ => continue,
        };

        // Extract the bare filename without extension
        let filename = match Path::new(rel_path).file_stem().and_then(|s| s.to_str()) {
            Some(f) => f,
            None => continue,
        };

        // Must start with "test_"
        let target_module = match filename.strip_prefix("test_") {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        // Try to resolve the tested module
        if let Some(target_file_id) = module_index.resolve(target_module) {
            // Skip if test file points to itself somehow
            if target_file_id != ext_id {
                edges.push((ext_id.clone(), target_file_id.to_string()));
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Route registration detection — include_router() pattern
// ---------------------------------------------------------------------------

/// Detect FastAPI route registrations via `include_router()` calls.
///
/// Scans Python files for patterns like:
///   `app.include_router(module_name.router)`
///   `router.include_router(sub_router.router)`
///
/// Returns Vec<(main_file_id, route_file_id)>.
fn detect_route_registrations(
    graph: &Graph,
    root: &Path,
    module_index: &PythonModuleIndex,
) -> Vec<(String, String)> {
    // Match: app.include_router(module_name.router)
    // Captures the module name before the dot
    let re_include_router =
        Regex::new(r"(?:app|router)\s*\.\s*include_router\s*\(\s*(\w+)\s*\.").unwrap();

    let mut edges = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".py") => p,
            _ => continue,
        };

        // Read the source file
        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Scan for include_router calls
        for line in content.lines() {
            for caps in re_include_router.captures_iter(line) {
                let module_name = caps.get(1).unwrap().as_str();

                // Resolve the module name to a file
                if let Some(target_file_id) = module_index.resolve(module_name) {
                    if target_file_id != ext_id {
                        let key = (ext_id.clone(), target_file_id.to_string());
                        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                            e.insert(true);
                            edges.push((ext_id.clone(), target_file_id.to_string()));
                        }
                    }
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// JS/TS import edge collection
// ---------------------------------------------------------------------------

/// Scan all JS/TS file nodes in the graph, read their source from disk,
/// extract import statements (ES module `import ... from "..."` and
/// `require("...")` CJS), and resolve relative specifiers to file-to-file edges.
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_js_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    js_index: &JsModuleIndex,
) -> Vec<(String, String, String)> {
    // ES module: import ... from "./specifier"
    let re_esm_import =
        Regex::new(r#"^\s*import\s+.*from\s+['"]([^'"]+)['"]"#).unwrap();
    // CJS: require("./specifier") or require('./specifier')
    let re_require = Regex::new(r#"\brequire\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) => p,
            None => continue,
        };

        // Only process JS/TS files
        let is_js_ts = JS_TS_EXTENSIONS
            .iter()
            .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
        if !is_js_ts {
            continue;
        }

        // Compute the directory of the source file for relative resolution
        let source_dir = match rel_path.rfind('/') {
            Some(idx) => &rel_path[..idx],
            None => "",
        };

        // Read the source file from disk
        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Collect specifiers from this line
            let mut specifiers: Vec<String> = Vec::new();
            if let Some(caps) = re_esm_import.captures(trimmed) {
                specifiers.push(caps.get(1).unwrap().as_str().to_string());
            }
            for caps in re_require.captures_iter(trimmed) {
                specifiers.push(caps.get(1).unwrap().as_str().to_string());
            }

            for specifier in specifiers {
                // Skip non-relative specifiers (bare / scoped / absolute)
                if !specifier.starts_with("./") && !specifier.starts_with("../") {
                    continue;
                }

                if let Some(target_file_id) = js_index.resolve(&specifier, source_dir) {
                    // Skip self-imports
                    if target_file_id == ext_id {
                        continue;
                    }
                    let key = (ext_id.clone(), target_file_id.to_string());
                    if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                        e.insert(true);
                        edges.push((
                            ext_id.clone(),
                            target_file_id.to_string(),
                            "imports".to_string(),
                        ));
                    }
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Go import edge collection
// ---------------------------------------------------------------------------

/// Scan all Go file nodes in the graph, read their source from disk,
/// extract import statements, and resolve them to file-to-file edges via
/// GoModuleIndex.
///
/// Go import forms handled:
///   Single:  import "pkg/path"
///   Block:   import ( \n "pkg/path" \n alias "pkg/path2" \n )
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_go_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    go_index: &GoModuleIndex,
) -> Vec<(String, String, String)> {
    // Matches the quoted path inside an import statement or block line.
    // Captures group 1: the raw import path string (without quotes).
    // Handles optional alias prefix: `alias "pkg/path"` or just `"pkg/path"`.
    let re_import_path = Regex::new(r#"(?:\w+\s+)?"([^"]+)""#).unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".go") => p,
            _ => continue,
        };

        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut in_import_block = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Block import start
            if trimmed.starts_with("import (") {
                in_import_block = true;
                continue;
            }

            if in_import_block {
                if trimmed == ")" {
                    in_import_block = false;
                    continue;
                }
                // Each line inside the block is: `"pkg/path"` or `alias "pkg/path"`
                if let Some(caps) = re_import_path.captures(trimmed) {
                    let import_path = caps.get(1).unwrap().as_str();
                    if let Some(target_id) = go_index.resolve(import_path) {
                        if target_id != ext_id {
                            let key = (ext_id.clone(), target_id.to_string());
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                seen.entry(key)
                            {
                                e.insert(true);
                                edges.push((
                                    ext_id.clone(),
                                    target_id.to_string(),
                                    "imports".to_string(),
                                ));
                            }
                        }
                    }
                }
                continue;
            }

            // Single-line import: import "pkg/path"  or  import alias "pkg/path"
            if trimmed.starts_with("import ") && !trimmed.contains('(') {
                if let Some(caps) = re_import_path.captures(trimmed) {
                    let import_path = caps.get(1).unwrap().as_str();
                    if let Some(target_id) = go_index.resolve(import_path) {
                        if target_id != ext_id {
                            let key = (ext_id.clone(), target_id.to_string());
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                seen.entry(key)
                            {
                                e.insert(true);
                                edges.push((
                                    ext_id.clone(),
                                    target_id.to_string(),
                                    "imports".to_string(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Java import edge collection
// ---------------------------------------------------------------------------

/// Scan all Java file nodes in the graph, read their source from disk,
/// extract import statements, and resolve them to file-to-file edges via
/// JavaPackageIndex.
///
/// Java import forms handled:
///   Specific:  import com.example.foo.Bar;
///   Static:    import static com.example.foo.Bar.method;
///   Wildcard:  import com.example.foo.*;
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_java_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    java_index: &JavaPackageIndex,
) -> Vec<(String, String, String)> {
    // Matches: import [static] <path>;
    // Group 1 captures the path (may end with .* for wildcards).
    let re_import = Regex::new(r"^\s*import\s+(?:static\s+)?([\w.*]+)\s*;").unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".java") => p,
            _ => continue,
        };

        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            // Only look at import lines
            if !trimmed.starts_with("import ") {
                continue;
            }

            if let Some(caps) = re_import.captures(trimmed) {
                let import_path = caps.get(1).unwrap().as_str();

                let targets = java_index.resolve(import_path);
                for target_id in targets {
                    // Skip self-imports (shouldn't happen but guard anyway)
                    if target_id == ext_id {
                        continue;
                    }
                    let key = (ext_id.clone(), target_id.to_string());
                    if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                        e.insert(true);
                        edges.push((
                            ext_id.clone(),
                            target_id.to_string(),
                            "imports".to_string(),
                        ));
                    }
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Rust import/module edge collection
// ---------------------------------------------------------------------------

/// Scan all Rust file nodes in the graph, read their source from disk,
/// and produce file-to-file `imports` edges for:
///
/// 1. `mod NAME;` declarations → sibling `NAME.rs` or `NAME/mod.rs`
/// 2. `use crate::a::b::...;` → `<crate_root>/a/b.rs` or `<crate_root>/a/b/mod.rs`
/// 3. `use super::...;` → parent module's file
///
/// External crates (`use std::`, `use serde::`, etc.) are left UNRESOLVED.
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_rust_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    rust_index: &RustModuleIndex,
) -> Vec<(String, String, String)> {
    // Matches: mod NAME; (not inline mod NAME { ... })
    // We detect the semicolon at end-of-statement to distinguish file-module decls
    // from inline module blocks.
    let re_mod_decl = Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;").unwrap();
    // Matches: use PATH; (captures everything between `use` and `;`)
    let re_use = Regex::new(r"^\s*(?:pub\s+)?use\s+(.+?)\s*;").unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) if p.ends_with(".rs") => p,
            _ => continue,
        };

        // Source directory for this file (used for mod; resolution)
        let source_dir = match rel_path.rfind('/') {
            Some(idx) => &rel_path[..idx],
            None => "",
        };

        // Source filename stem (lib/main/mod → treat dir as module root)
        let filename = rel_path.rsplit('/').next().unwrap_or(rel_path);
        let stem = filename.strip_suffix(".rs").unwrap_or(filename);
        // For a mod.rs, the module "root" directory is `source_dir` itself.
        // For lib.rs/main.rs, same.
        // For foo.rs, the "submodule root" is `source_dir/foo`.
        let mod_root_dir = match stem {
            "lib" | "main" | "mod" => source_dir.to_string(),
            other => {
                if source_dir.is_empty() {
                    other.to_string()
                } else {
                    format!("{}/{}", source_dir, other)
                }
            }
        };

        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }

            // --- Handle `mod NAME;` declarations ---
            if let Some(caps) = re_mod_decl.captures(trimmed) {
                let mod_name = caps.get(1).unwrap().as_str();

                // Candidate paths: NAME.rs and NAME/mod.rs relative to mod_root_dir
                let candidate_a = if mod_root_dir.is_empty() {
                    format!("{}.rs", mod_name)
                } else {
                    format!("{}/{}.rs", mod_root_dir, mod_name)
                };
                let candidate_b = if mod_root_dir.is_empty() {
                    format!("{}/mod.rs", mod_name)
                } else {
                    format!("{}/{}/mod.rs", mod_root_dir, mod_name)
                };

                // Check which candidate exists in the graph (resolve to file node)
                for candidate_path in &[candidate_a, candidate_b] {
                    let candidate_id = format!("file::{}", candidate_path);
                    if graph.resolve_id(&candidate_id).is_some() {
                        // Found — emit an imports edge
                        let key = (ext_id.clone(), candidate_id.clone());
                        if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                            e.insert(true);
                            edges.push((
                                ext_id.clone(),
                                candidate_id,
                                "imports".to_string(),
                            ));
                        }
                        break; // Only resolve to the first existing candidate
                    }
                }
                continue; // `mod NAME;` and `use` can't be on same line
            }

            // --- Handle `use PATH;` imports ---
            if let Some(caps) = re_use.captures(trimmed) {
                let use_spec = caps.get(1).unwrap().as_str().trim();

                // Expand brace imports: `use a::{b, c}` → ["a::b", "a::c"]
                // We reuse the extractor's expand_use_path logic by re-implementing
                // a minimal version inline (we can't call the extractor from here).
                let paths = expand_use_paths(use_spec);

                for path in &paths {
                    let path = path.trim();
                    if path.is_empty() {
                        continue;
                    }

                    // Determine path type by first segment
                    let first_seg = path.split("::").next().unwrap_or("");

                    if first_seg == "crate" {
                        // `use crate::a::b::Foo` → resolve `a::b` against crate root
                        let rest = match path.strip_prefix("crate::") {
                            Some(r) if !r.is_empty() => r,
                            _ => continue,
                        };
                        // Convert `::` to `/` to get slash-separated module path
                        let slash_path = rest.replace("::", "/");
                        // Remove glob suffix: `a::*` → `a`
                        let slash_path = slash_path.trim_end_matches("/*");
                        let slash_path = slash_path.trim_end_matches('/');

                        // Find crate root directory for this source file
                        let crate_root = rust_index.crate_root_for(source_dir);

                        if let Some(target_id) = crate_root
                            .and_then(|cr| rust_index.resolve_crate_path(slash_path, cr))
                        {
                            if target_id != ext_id {
                                let key = (ext_id.clone(), target_id.to_string());
                                if let std::collections::hash_map::Entry::Vacant(e) =
                                    seen.entry(key)
                                {
                                    e.insert(true);
                                    edges.push((
                                        ext_id.clone(),
                                        target_id.to_string(),
                                        "imports".to_string(),
                                    ));
                                }
                            }
                        }
                    } else if first_seg == "super" {
                        // `use super::X` → parent module's file
                        if let Some(target_id) = rust_index.resolve_super(rel_path) {
                            if target_id != ext_id {
                                let key = (ext_id.clone(), target_id.to_string());
                                if let std::collections::hash_map::Entry::Vacant(e) =
                                    seen.entry(key)
                                {
                                    e.insert(true);
                                    edges.push((
                                        ext_id.clone(),
                                        target_id.to_string(),
                                        "imports".to_string(),
                                    ));
                                }
                            }
                        }
                    } else if first_seg == "self" {
                        // `use self::X` → current module's file; skip (self-import)
                    }
                    // else: external crate (std, serde, etc.) → leave unresolved
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// C/C++ #include edge collection
// ---------------------------------------------------------------------------

/// Scan all C/C++ file nodes in the graph, read their source from disk,
/// extract `#include` directives, and resolve QUOTED includes to file-to-file
/// edges via CHeaderIndex.
///
/// Only quoted includes (`#include "..."`) are resolved.  Angle-bracket
/// includes (`#include <...>`) target system/external headers which will not
/// be found in the project index and therefore stay unresolved naturally.
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_c_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    c_index: &CHeaderIndex,
) -> Vec<(String, String, String)> {
    // Matches: #include "path"  (quoted — project-local)
    // Group 1: the path string without quotes.
    let re_quoted = Regex::new(r#"^\s*#\s*include\s+"([^"]+)""#).unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) => p,
            None => continue,
        };

        // Only process C/C++ files
        let is_c = C_EXTENSIONS
            .iter()
            .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
        if !is_c {
            continue;
        }

        // Source directory for relative resolution
        let source_dir = match rel_path.rfind('/') {
            Some(idx) => &rel_path[..idx],
            None => "",
        };

        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();

            // Skip empty lines and pure comment lines
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }

            // Only handle quoted includes (angle-bracket includes go unresolved)
            if let Some(caps) = re_quoted.captures(trimmed) {
                let target = caps.get(1).unwrap().as_str();

                if let Some(target_file_id) = c_index.resolve(target, source_dir) {
                    // Skip self-includes (shouldn't happen but guard anyway)
                    if target_file_id == ext_id {
                        continue;
                    }
                    let key = (ext_id.clone(), target_file_id.to_string());
                    if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                        e.insert(true);
                        edges.push((
                            ext_id.clone(),
                            target_file_id.to_string(),
                            "imports".to_string(),
                        ));
                    }
                }
            }
        }
    }

    edges
}

// ---------------------------------------------------------------------------
// Kotlin import edge collection
// ---------------------------------------------------------------------------

/// Scan all Kotlin file nodes in the graph, read their source from disk,
/// extract `import` statements, and resolve them to file-to-file edges via
/// KotlinPackageIndex.
///
/// Kotlin import forms handled:
///   Specific:  import com.example.foo.Bar
///   Wildcard:  import com.example.foo.*
///   Aliased:   import com.example.foo.Bar as B  (alias stripped, path resolved)
///
/// Returns Vec<(source_file_id, target_file_id, relation)>.
fn collect_kotlin_import_edges_from_files(
    graph: &Graph,
    root: &Path,
    kotlin_index: &KotlinPackageIndex,
) -> Vec<(String, String, String)> {
    // Matches: import <path>  or  import <path> as <alias>
    // Group 1: the dotted import path (before optional `as` alias).
    let re_import =
        Regex::new(r"^\s*import\s+([\w.*]+)(?:\s+as\s+\w+)?\s*;?\s*$").unwrap();

    let mut edges: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for i in 0..graph.num_nodes() as usize {
        if graph.nodes.node_type[i] != NodeType::File {
            continue;
        }

        let ext_id = match find_external_id(graph, NodeId::new(i as u32)) {
            Some(id) => id,
            None => continue,
        };

        let rel_path = match ext_id.strip_prefix("file::") {
            Some(p) => p,
            None => continue,
        };

        // Only process Kotlin files
        let is_kt = KOTLIN_EXTENSIONS
            .iter()
            .any(|ext| rel_path.ends_with(&format!(".{}", ext)));
        if !is_kt {
            continue;
        }

        let file_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }

            // Only look at import lines
            if !trimmed.starts_with("import ") {
                continue;
            }

            if let Some(caps) = re_import.captures(trimmed) {
                let import_path = caps.get(1).unwrap().as_str();

                let targets = kotlin_index.resolve(import_path);
                for target_id in targets {
                    if target_id == ext_id {
                        continue;
                    }
                    let key = (ext_id.clone(), target_id.to_string());
                    if let std::collections::hash_map::Entry::Vacant(e) = seen.entry(key) {
                        e.insert(true);
                        edges.push((
                            ext_id.clone(),
                            target_id.to_string(),
                            "imports".to_string(),
                        ));
                    }
                }
            }
        }
    }

    edges
}

/// Minimal expansion of Rust use path specs (without pulling in extractor).
///
/// Handles:
///   `a::b` → ["a::b"]
///   `a::{b, c::d}` → ["a::b", "a::c::d"]
///   `a::{self, b}` → ["a", "a::b"]
///   `a::*` → ["a::*"]
///
/// Does not handle deeply nested braces with commas in generics — but
/// Rust use paths don't have generics, so this is safe.
fn expand_use_paths(spec: &str) -> Vec<String> {
    let mut out = Vec::new();
    expand_use_paths_inner("", spec.trim(), &mut out);
    out.retain(|s| !s.is_empty());
    out.sort();
    out.dedup();
    out
}

fn expand_use_paths_inner(prefix: &str, spec: &str, out: &mut Vec<String>) {
    let spec = spec.trim().trim_end_matches(';').trim();
    if spec.is_empty() {
        return;
    }

    // Handle brace group: "prefix::a::{b, c}"
    if let Some(brace_start) = spec.find('{') {
        if let Some(brace_end) = spec.rfind('}') {
            let base = spec[..brace_start].trim().trim_end_matches("::").trim();
            let next_prefix = if base.is_empty() {
                prefix.to_string()
            } else if prefix.is_empty() {
                base.to_string()
            } else {
                format!("{}::{}", prefix, base)
            };
            let inner = &spec[brace_start + 1..brace_end];
            // Split by top-level commas (not inside nested braces)
            let mut depth = 0usize;
            let mut start = 0;
            for (idx, ch) in inner.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        let item = inner[start..idx].trim();
                        expand_use_paths_inner(&next_prefix, item, out);
                        start = idx + 1;
                    }
                    _ => {}
                }
            }
            let last = inner[start..].trim();
            if !last.is_empty() {
                expand_use_paths_inner(&next_prefix, last, out);
            }
            return;
        }
    }

    // Strip alias: `Foo as Bar` → `Foo`
    let without_alias = spec.split(" as ").next().unwrap_or(spec).trim();

    match without_alias {
        "self" => {
            if !prefix.is_empty() {
                out.push(prefix.to_string());
            }
        }
        "*" => {
            if !prefix.is_empty() {
                out.push(format!("{}::*", prefix));
            }
        }
        other => {
            if prefix.is_empty() {
                out.push(other.to_string());
            } else {
                out.push(format!("{}::{}", prefix, other));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Add a cross-file edge between two file nodes.
///
/// Returns true if the edge was successfully added, false if either
/// node could not be resolved.
fn add_cross_file_edge(
    graph: &mut Graph,
    source_id: &str,
    target_id: &str,
    relation: &str,
    stats: &mut CrossFileStats,
) -> bool {
    let source = match graph.resolve_id(source_id) {
        Some(id) => id,
        None => return false,
    };
    let target = match graph.resolve_id(target_id) {
        Some(id) => id,
        None => return false,
    };

    // Causal strength and weight depend on edge type:
    // - imports: moderate causal (dependency changes -> importer affected)
    // - tests: moderate causal (test verifies module behavior)
    // - registers: strong causal (registration is explicit wiring)
    let (weight, causal_strength) = match relation {
        "imports" => (0.6, 0.6),
        "tests" => (0.5, 0.5),
        "registers" => (0.7, 0.7),
        _ => (0.5, 0.4),
    };

    match graph.add_edge(
        source,
        target,
        relation,
        FiniteF32::new(weight),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(causal_strength),
    ) {
        Ok(_) => {
            stats.total_edges_created += 1;
            true
        }
        Err(_) => false,
    }
}

/// Find the external ID string for a node.
///
/// Iterates the id_to_node map to find the interned string matching
/// the given NodeId. Returns None if not found.
fn find_external_id(graph: &Graph, node: NodeId) -> Option<String> {
    for (interned, &nid) in &graph.id_to_node {
        if nid == node {
            return Some(graph.strings.resolve(*interned).to_string());
        }
    }
    None
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::*;

    /// Helper: add a Python file node to the graph.
    fn add_file_node(graph: &mut Graph, rel_path: &str) -> NodeId {
        let ext_id = format!("file::{}", rel_path);
        let label = rel_path.rsplit('/').next().unwrap_or(rel_path);
        graph
            .add_node(&ext_id, label, NodeType::File, &["python"], 0.0, 0.3)
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // PythonModuleIndex tests
    // -----------------------------------------------------------------------

    #[test]
    fn module_index_resolves_dotted_path() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/config.py");
        add_file_node(&mut graph, "backend/models.py");

        let index = PythonModuleIndex::build(&graph);

        assert_eq!(
            index.resolve("backend.config"),
            Some("file::backend/config.py"),
            "Dotted path should resolve to file"
        );
        assert_eq!(
            index.resolve("backend.models"),
            Some("file::backend/models.py"),
        );
    }

    #[test]
    fn module_index_resolves_bare_name_flat_layout() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/config.py");
        add_file_node(&mut graph, "backend/worker_pool.py");

        let index = PythonModuleIndex::build(&graph);

        // Bare names should resolve (flat layout, no __init__.py)
        assert_eq!(
            index.resolve("config"),
            Some("file::backend/config.py"),
            "Bare module name should resolve in flat layout"
        );
        assert_eq!(
            index.resolve("worker_pool"),
            Some("file::backend/worker_pool.py"),
        );
    }

    #[test]
    fn module_index_resolves_dotted_name_import() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/config.py");

        let index = PythonModuleIndex::build(&graph);

        // "backend.config.Settings" should resolve to backend/config.py
        // via progressive prefix shortening
        assert_eq!(
            index.resolve("backend.config.Settings"),
            Some("file::backend/config.py"),
            "Dotted name with attribute should resolve via prefix"
        );
    }

    #[test]
    fn module_index_handles_init_py() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/__init__.py");
        add_file_node(&mut graph, "backend/config.py");

        let index = PythonModuleIndex::build(&graph);

        // Package init should resolve as the package name
        assert_eq!(
            index.resolve("backend"),
            Some("file::backend/__init__.py"),
            "__init__.py should register under package name"
        );
        assert_eq!(
            index.resolve("backend.config"),
            Some("file::backend/config.py"),
        );
    }

    #[test]
    fn module_index_returns_none_for_stdlib() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/config.py");

        let index = PythonModuleIndex::build(&graph);

        assert_eq!(index.resolve("os"), None, "stdlib should not resolve");
        assert_eq!(index.resolve("json"), None, "stdlib should not resolve");
        assert_eq!(
            index.resolve("fastapi"),
            None,
            "third-party should not resolve"
        );
    }

    #[test]
    fn module_index_root_level_files() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "main.py");
        add_file_node(&mut graph, "config.py");

        let index = PythonModuleIndex::build(&graph);

        // Root-level files: dotted path equals bare name
        assert_eq!(index.resolve("main"), Some("file::main.py"),);
        assert_eq!(index.resolve("config"), Some("file::config.py"),);
    }

    // -----------------------------------------------------------------------
    // Test-to-module inference tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_edge_naming_convention() {
        let mut graph = Graph::with_capacity(8, 8);
        add_file_node(&mut graph, "backend/config.py");
        add_file_node(&mut graph, "backend/doctor.py");
        add_file_node(&mut graph, "backend/tests/test_config.py");
        add_file_node(&mut graph, "backend/tests/test_doctor.py");
        add_file_node(&mut graph, "backend/tests/test_e2e_integration.py"); // no match

        let index = PythonModuleIndex::build(&graph);
        let edges = infer_test_edges(&graph, &index);

        // Should find test_config -> config and test_doctor -> doctor
        assert!(
            edges.iter().any(|(t, s)| t.contains("test_config")
                && s.contains("config.py")
                && !s.contains("test_")),
            "test_config.py should map to config.py. Edges: {:?}",
            edges
        );
        assert!(
            edges.iter().any(|(t, s)| t.contains("test_doctor")
                && s.contains("doctor.py")
                && !s.contains("test_")),
            "test_doctor.py should map to doctor.py"
        );
        // test_e2e_integration.py should NOT match anything
        assert!(
            !edges
                .iter()
                .any(|(t, _)| t.contains("test_e2e_integration")),
            "Integration test with no matching module should not create edge"
        );
    }

    // -----------------------------------------------------------------------
    // Cross-file edge addition test
    // -----------------------------------------------------------------------

    #[test]
    fn cross_file_edge_adds_to_graph() {
        let mut graph = Graph::with_capacity(4, 8);
        add_file_node(&mut graph, "backend/main.py");
        add_file_node(&mut graph, "backend/config.py");

        let initial_pending = graph.csr.pending_edges.len();
        let mut stats = CrossFileStats::default();

        let result = add_cross_file_edge(
            &mut graph,
            "file::backend/main.py",
            "file::backend/config.py",
            "imports",
            &mut stats,
        );

        assert!(result, "Edge should be added successfully");
        assert_eq!(stats.total_edges_created, 1);
        assert_eq!(graph.csr.pending_edges.len(), initial_pending + 1);

        // Verify edge properties
        let edge = &graph.csr.pending_edges[initial_pending];
        assert_eq!(edge.weight.get(), 0.6);
        assert_eq!(edge.causal_strength.get(), 0.6);
        assert_eq!(edge.direction, EdgeDirection::Forward);
        assert!(!edge.inhibitory);
    }

    #[test]
    fn cross_file_edge_fails_for_missing_node() {
        let mut graph = Graph::with_capacity(4, 4);
        add_file_node(&mut graph, "backend/main.py");

        let mut stats = CrossFileStats::default();
        let result = add_cross_file_edge(
            &mut graph,
            "file::backend/main.py",
            "file::nonexistent.py",
            "imports",
            &mut stats,
        );

        assert!(!result, "Edge should fail for non-existent target");
        assert_eq!(stats.total_edges_created, 0);
    }

    #[test]
    fn edge_weights_vary_by_relation() {
        let mut graph = Graph::with_capacity(6, 12);
        add_file_node(&mut graph, "a.py");
        add_file_node(&mut graph, "b.py");
        add_file_node(&mut graph, "c.py");

        let mut stats = CrossFileStats::default();

        // imports edge
        add_cross_file_edge(
            &mut graph,
            "file::a.py",
            "file::b.py",
            "imports",
            &mut stats,
        );
        let import_edge = &graph.csr.pending_edges[0];
        assert!((import_edge.weight.get() - 0.6).abs() < f32::EPSILON);

        // tests edge
        add_cross_file_edge(&mut graph, "file::a.py", "file::c.py", "tests", &mut stats);
        let test_edge = &graph.csr.pending_edges[1];
        assert!((test_edge.weight.get() - 0.5).abs() < f32::EPSILON);

        // registers edge
        add_cross_file_edge(
            &mut graph,
            "file::b.py",
            "file::c.py",
            "registers",
            &mut stats,
        );
        let reg_edge = &graph.csr.pending_edges[2];
        assert!((reg_edge.weight.get() - 0.7).abs() < f32::EPSILON);
    }

    // -----------------------------------------------------------------------
    // JavaPackageIndex tests
    // -----------------------------------------------------------------------

    /// Helper: add a Java file node to the graph.
    fn add_java_file_node(graph: &mut Graph, rel_path: &str) -> NodeId {
        let ext_id = format!("file::{}", rel_path);
        let label = rel_path.rsplit('/').next().unwrap_or(rel_path);
        graph
            .add_node(&ext_id, label, NodeType::File, &["java"], 0.0, 0.3)
            .unwrap()
    }

    #[test]
    fn java_package_index_resolves_fqcn() {
        let mut graph = Graph::with_capacity(4, 4);
        add_java_file_node(&mut graph, "com/example/foo/Bar.java");
        add_java_file_node(&mut graph, "com/example/baz/Qux.java");

        let index = JavaPackageIndex::build(&graph);

        let resolved = index.resolve("com.example.foo.Bar");
        assert_eq!(
            resolved,
            vec!["file::com/example/foo/Bar.java"],
            "FQCN should resolve to file"
        );

        let resolved2 = index.resolve("com.example.baz.Qux");
        assert_eq!(
            resolved2,
            vec!["file::com/example/baz/Qux.java"],
        );
    }

    #[test]
    fn java_package_index_suffix_match_disambiguates() {
        // Two classes named "Bar" in different packages — longest suffix match wins.
        let mut graph = Graph::with_capacity(6, 6);
        add_java_file_node(&mut graph, "com/example/foo/Bar.java");
        add_java_file_node(&mut graph, "org/other/Bar.java");

        let index = JavaPackageIndex::build(&graph);

        // Import "com.example.foo.Bar" should prefer com/example/foo/Bar.java
        let resolved = index.resolve("com.example.foo.Bar");
        assert_eq!(
            resolved,
            vec!["file::com/example/foo/Bar.java"],
            "Suffix match should pick the deeper match. Got: {:?}",
            resolved
        );
    }

    #[test]
    fn java_package_index_resolves_wildcard() {
        let mut graph = Graph::with_capacity(6, 6);
        add_java_file_node(&mut graph, "com/example/foo/Alpha.java");
        add_java_file_node(&mut graph, "com/example/foo/Beta.java");
        add_java_file_node(&mut graph, "com/example/bar/Gamma.java");

        let index = JavaPackageIndex::build(&graph);

        let mut resolved = index.resolve("com.example.foo.*");
        resolved.sort();
        assert_eq!(resolved.len(), 2, "Wildcard should resolve to both files in package. Got: {:?}", resolved);
        assert!(resolved.contains(&"file::com/example/foo/Alpha.java"));
        assert!(resolved.contains(&"file::com/example/foo/Beta.java"));

        // Different package wildcard should not include foo files
        let resolved_bar = index.resolve("com.example.bar.*");
        assert_eq!(resolved_bar.len(), 1);
        assert!(resolved_bar.contains(&"file::com/example/bar/Gamma.java"));
    }

    #[test]
    fn java_package_index_returns_empty_for_external_dep() {
        let mut graph = Graph::with_capacity(4, 4);
        add_java_file_node(&mut graph, "com/example/Service.java");

        let index = JavaPackageIndex::build(&graph);

        // java.util.List is external — no ingested file
        assert!(
            index.resolve("java.util.List").is_empty(),
            "External stdlib class should not resolve"
        );
        // org.springframework.* is external
        assert!(
            index.resolve("org.springframework.web.*").is_empty(),
            "External wildcard should not resolve"
        );
    }

    /// This is the cross-file parity test mirroring go_cross_file_import_resolves_to_file_node.
    /// It verifies that JavaPackageIndex resolves an import from one Java file to
    /// another ingested Java file, producing an imports edge in the graph.
    #[test]
    fn java_cross_file_import_resolves_to_file_node() {
        // Two Java files: UserService imports UserRepository.
        // We verify that JavaPackageIndex correctly resolves the import and that
        // add_cross_file_edge produces an imports edge between the two file nodes.
        let mut graph = Graph::with_capacity(8, 8);
        add_java_file_node(&mut graph, "com/example/service/UserService.java");
        add_java_file_node(&mut graph, "com/example/repo/UserRepository.java");

        let index = JavaPackageIndex::build(&graph);

        // Resolve the FQCN import
        let resolved = index.resolve("com.example.repo.UserRepository");
        assert_eq!(
            resolved,
            vec!["file::com/example/repo/UserRepository.java"],
            "Should resolve import to UserRepository.java. Got: {:?}",
            resolved
        );

        // Add the cross-file edge and verify it lands in the graph
        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::com/example/service/UserService.java",
            "file::com/example/repo/UserRepository.java",
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge should be successfully added");
        assert_eq!(stats.total_edges_created, 1);

        // The edge should be in pending_edges
        let edge = graph.csr.pending_edges.iter().find(|e| {
            let src = graph
                .resolve_id("file::com/example/service/UserService.java")
                .unwrap();
            let tgt = graph
                .resolve_id("file::com/example/repo/UserRepository.java")
                .unwrap();
            e.source == src && e.target == tgt
        });
        assert!(
            edge.is_some(),
            "imports edge from UserService → UserRepository must be in graph"
        );
    }

    // -----------------------------------------------------------------------
    // RustModuleIndex tests
    // -----------------------------------------------------------------------

    /// Helper: add a Rust file node to the graph.
    fn add_rust_file_node(graph: &mut Graph, rel_path: &str) -> NodeId {
        let ext_id = format!("file::{}", rel_path);
        let label = rel_path.rsplit('/').next().unwrap_or(rel_path);
        graph
            .add_node(&ext_id, label, NodeType::File, &["rust"], 0.0, 0.3)
            .unwrap()
    }

    #[test]
    fn rust_module_index_builds_path_map() {
        let mut graph = Graph::with_capacity(6, 6);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo.rs");
        add_rust_file_node(&mut graph, "src/bar/mod.rs");

        let index = RustModuleIndex::build(&graph);

        // lib.rs registers "src" as crate root dir
        assert!(
            index.crate_root_dirs.contains(&"src".to_string()),
            "src should be detected as crate root dir"
        );

        // foo.rs registers under "src/foo"
        assert_eq!(
            index.path_to_file.get("src/foo"),
            Some(&"file::src/foo.rs".to_string()),
            "src/foo.rs should register under key 'src/foo'"
        );

        // bar/mod.rs registers under "src/bar" (strip /mod)
        assert_eq!(
            index.path_to_file.get("src/bar"),
            Some(&"file::src/bar/mod.rs".to_string()),
            "src/bar/mod.rs should register under key 'src/bar'"
        );
    }

    /// Test that `mod foo;` in lib.rs resolves to sibling foo.rs via the index.
    #[test]
    fn rust_mod_decl_resolves_to_sibling_rs() {
        // Graph has lib.rs + foo.rs; lib.rs says `mod foo;`
        // The collect function reads from disk, so we test the index resolution
        // logic directly by checking that the candidate file ID is in the graph.
        let mut graph = Graph::with_capacity(4, 4);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo.rs");

        // Simulate what collect_rust_import_edges_from_files does for `mod foo;` in lib.rs:
        // mod_root_dir for lib.rs is "src" (stem == "lib")
        // candidate_a = "src/foo.rs", candidate_b = "src/foo/mod.rs"
        let candidate_a = "file::src/foo.rs";
        let candidate_b = "file::src/foo/mod.rs";

        assert!(
            graph.resolve_id(candidate_a).is_some(),
            "src/foo.rs must be in graph"
        );
        assert!(
            graph.resolve_id(candidate_b).is_none(),
            "src/foo/mod.rs must NOT be in graph"
        );

        // Verify that add_cross_file_edge succeeds for the resolved candidate
        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::src/lib.rs",
            candidate_a,
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge lib.rs -> foo.rs should be added");
        assert_eq!(stats.total_edges_created, 1);
    }

    /// Test that `mod foo;` in lib.rs resolves to foo/mod.rs form when that is
    /// what exists in the graph (not foo.rs).
    #[test]
    fn rust_mod_decl_resolves_to_mod_rs_form() {
        let mut graph = Graph::with_capacity(4, 4);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo/mod.rs");

        // candidate_a = "src/foo.rs" (not in graph), candidate_b = "src/foo/mod.rs" (in graph)
        let candidate_a = "file::src/foo.rs";
        let candidate_b = "file::src/foo/mod.rs";

        assert!(
            graph.resolve_id(candidate_a).is_none(),
            "src/foo.rs must NOT be in graph"
        );
        assert!(
            graph.resolve_id(candidate_b).is_some(),
            "src/foo/mod.rs must be in graph"
        );

        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::src/lib.rs",
            candidate_b,
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge lib.rs -> foo/mod.rs should be added");
        assert_eq!(stats.total_edges_created, 1);
    }

    /// Test that `use crate::foo::Bar` resolves to `src/foo.rs` via RustModuleIndex.
    #[test]
    fn rust_use_crate_path_resolves_to_file() {
        let mut graph = Graph::with_capacity(6, 6);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo.rs");
        add_rust_file_node(&mut graph, "src/bar.rs");

        let index = RustModuleIndex::build(&graph);

        // `use crate::foo::Bar` from a file in "src/" subdir
        // crate root dir = "src", crate path = "foo/Bar" → try "src/foo/Bar" then "src/foo"
        let crate_root = index.crate_root_for("src");
        assert_eq!(crate_root, Some("src"), "crate root should be 'src'");

        // "foo/Bar" → exact miss, then try "foo" → hit "src/foo"
        let resolved = index.resolve_crate_path("foo/Bar", "src");
        assert_eq!(
            resolved,
            Some("file::src/foo.rs"),
            "use crate::foo::Bar should resolve to src/foo.rs"
        );

        // "bar" → exact hit "src/bar"
        let resolved_bar = index.resolve_crate_path("bar", "src");
        assert_eq!(
            resolved_bar,
            Some("file::src/bar.rs"),
            "use crate::bar should resolve to src/bar.rs"
        );
    }

    /// Test that external crate imports (std, serde, etc.) stay unresolved.
    #[test]
    fn rust_external_crate_stays_unresolved() {
        let mut graph = Graph::with_capacity(4, 4);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo.rs");

        let index = RustModuleIndex::build(&graph);

        // "std" is not a path in our graph — should return None
        let resolved = index.resolve_crate_path("collections/HashMap", "src");
        assert!(
            resolved.is_none(),
            "std::collections::HashMap must not resolve to any ingested file"
        );

        // "serde" likewise
        let resolved_serde = index.resolve_crate_path("Serialize", "src");
        assert!(
            resolved_serde.is_none(),
            "serde::Serialize must not resolve to any ingested file"
        );
    }

    /// Test super:: resolution: bar.rs in src/submod/ -> super resolves to src/submod.rs or mod.rs.
    #[test]
    fn rust_super_resolves_to_parent_module() {
        let mut graph = Graph::with_capacity(6, 6);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/submod.rs");
        add_rust_file_node(&mut graph, "src/submod/child.rs");

        let index = RustModuleIndex::build(&graph);

        // `use super::something` from "src/submod/child.rs"
        // without_ext = "src/submod/child" → parent = "src/submod"
        // path_to_file["src/submod"] = "file::src/submod.rs"
        let resolved = index.resolve_super("src/submod/child.rs");
        assert_eq!(
            resolved,
            Some("file::src/submod.rs"),
            "super from src/submod/child.rs should resolve to src/submod.rs"
        );
    }

    /// Integration test: verify that an imports edge from lib.rs to foo.rs
    /// is correctly emitted after add_cross_file_edge for the resolved mod decl.
    /// This mirrors go_cross_file_import_resolves_to_file_node and
    /// java_cross_file_import_resolves_to_file_node.
    #[test]
    fn rust_cross_file_mod_decl_resolves_to_file_node() {
        // Crate: lib.rs declares `mod foo;` → foo.rs
        let mut graph = Graph::with_capacity(8, 8);
        add_rust_file_node(&mut graph, "src/lib.rs");
        add_rust_file_node(&mut graph, "src/foo.rs");

        // Verify RustModuleIndex recognizes the crate root
        let index = RustModuleIndex::build(&graph);
        assert!(
            index.crate_root_dirs.contains(&"src".to_string()),
            "src should be crate root"
        );

        // The mod declaration in lib.rs → mod_root_dir = "src" (stem is "lib")
        // candidate = "file::src/foo.rs" — check it resolves
        let candidate_id = "file::src/foo.rs";
        assert!(
            graph.resolve_id(candidate_id).is_some(),
            "foo.rs must be in graph"
        );

        // Emit the cross-file edge
        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::src/lib.rs",
            candidate_id,
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge lib.rs -> foo.rs must succeed");
        assert_eq!(stats.total_edges_created, 1);

        // Verify the edge is in pending_edges with correct src/tgt
        let lib_id = graph.resolve_id("file::src/lib.rs").unwrap();
        let foo_id = graph.resolve_id("file::src/foo.rs").unwrap();
        let edge = graph
            .csr
            .pending_edges
            .iter()
            .find(|e| e.source == lib_id && e.target == foo_id);
        assert!(
            edge.is_some(),
            "imports edge src/lib.rs → src/foo.rs must be in graph pending_edges"
        );
    }

    /// Test expand_use_paths helper for brace imports.
    #[test]
    fn rust_expand_use_paths_handles_braces() {
        let paths = expand_use_paths("crate::foo::{Bar, Baz}");
        assert!(
            paths.contains(&"crate::foo::Bar".to_string()),
            "should expand to crate::foo::Bar"
        );
        assert!(
            paths.contains(&"crate::foo::Baz".to_string()),
            "should expand to crate::foo::Baz"
        );
    }

    #[test]
    fn rust_expand_use_paths_handles_self() {
        let paths = expand_use_paths("crate::foo::{self, Bar}");
        assert!(
            paths.contains(&"crate::foo".to_string()),
            "self should expand to the prefix path"
        );
        assert!(
            paths.contains(&"crate::foo::Bar".to_string()),
            "Bar should expand to crate::foo::Bar"
        );
    }

    #[test]
    fn rust_expand_use_paths_handles_glob() {
        let paths = expand_use_paths("crate::utils::*");
        assert_eq!(paths, vec!["crate::utils::*".to_string()]);
    }

    // -----------------------------------------------------------------------
    // CHeaderIndex tests
    // -----------------------------------------------------------------------

    /// Helper: add a C file node to the graph.
    fn add_c_file_node(graph: &mut Graph, rel_path: &str) -> NodeId {
        let ext_id = format!("file::{}", rel_path);
        let label = rel_path.rsplit('/').next().unwrap_or(rel_path);
        graph
            .add_node(&ext_id, label, NodeType::File, &["c"], 0.0, 0.3)
            .unwrap()
    }

    /// Quoted `#include "callee.h"` (sibling) → resolved file→file imports edge.
    #[test]
    fn c_cross_file_quoted_include_resolves_to_file_node() {
        // caller.c and callee.h are siblings at the root level.
        let mut graph = Graph::with_capacity(8, 8);
        add_c_file_node(&mut graph, "caller.c");
        add_c_file_node(&mut graph, "callee.h");

        let index = CHeaderIndex::build(&graph);

        // `#include "callee.h"` from a root-level file: source_dir = ""
        let resolved = index.resolve("callee.h", "");
        assert_eq!(
            resolved,
            Some("file::callee.h"),
            "Quoted sibling include should resolve. Got: {:?}",
            resolved
        );

        // Add the cross-file edge and verify it lands in the graph
        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::caller.c",
            "file::callee.h",
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge caller.c → callee.h should be added");
        assert_eq!(stats.total_edges_created, 1);

        // Verify the edge is in pending_edges
        let caller_id = graph.resolve_id("file::caller.c").unwrap();
        let callee_id = graph.resolve_id("file::callee.h").unwrap();
        let edge = graph
            .csr
            .pending_edges
            .iter()
            .find(|e| e.source == caller_id && e.target == callee_id);
        assert!(
            edge.is_some(),
            "imports edge caller.c → callee.h must be in graph pending_edges"
        );
    }

    /// `#include <stdio.h>` (angle-bracket system header) stays unresolved.
    #[test]
    fn c_system_include_stays_unresolved() {
        let mut graph = Graph::with_capacity(4, 4);
        add_c_file_node(&mut graph, "main.c");
        // Note: we deliberately do NOT add a "stdio.h" file node — it's a
        // system header and must not be fabricated.

        let index = CHeaderIndex::build(&graph);

        // System header `stdio.h` should not resolve to anything in the project
        let resolved = index.resolve("stdio.h", "");
        assert!(
            resolved.is_none(),
            "System header stdio.h must not resolve to any project file"
        );

        // stdlib.h likewise
        let resolved2 = index.resolve("stdlib.h", "");
        assert!(
            resolved2.is_none(),
            "System header stdlib.h must not resolve"
        );
    }

    /// Quoted relative include: `#include "../include/types.h"` with path separator.
    #[test]
    fn c_relative_path_include_resolves() {
        let mut graph = Graph::with_capacity(6, 6);
        add_c_file_node(&mut graph, "src/main.c");
        add_c_file_node(&mut graph, "include/types.h");

        let index = CHeaderIndex::build(&graph);

        // `#include "../include/types.h"` from "src/" directory
        let resolved = index.resolve("../include/types.h", "src");
        assert_eq!(
            resolved,
            Some("file::include/types.h"),
            "Relative path include should resolve via join_path. Got: {:?}",
            resolved
        );
    }

    /// Bare basename include where the header lives in a subdirectory.
    #[test]
    fn c_bare_basename_include_resolves_to_subdir_header() {
        let mut graph = Graph::with_capacity(6, 6);
        add_c_file_node(&mut graph, "src/main.c");
        add_c_file_node(&mut graph, "include/util.h");

        let index = CHeaderIndex::build(&graph);

        // `#include "util.h"` from "src/" — not a sibling, but basename matches include/util.h
        let resolved = index.resolve("util.h", "src");
        assert_eq!(
            resolved,
            Some("file::include/util.h"),
            "Bare basename should fall back to basename index. Got: {:?}",
            resolved
        );
    }

    // -----------------------------------------------------------------------
    // KotlinPackageIndex tests
    // -----------------------------------------------------------------------

    /// Helper: add a Kotlin file node to the graph.
    fn add_kotlin_file_node(graph: &mut Graph, rel_path: &str) -> NodeId {
        let ext_id = format!("file::{}", rel_path);
        let label = rel_path.rsplit('/').next().unwrap_or(rel_path);
        graph
            .add_node(&ext_id, label, NodeType::File, &["kotlin"], 0.0, 0.3)
            .unwrap()
    }

    /// `import com.example.Callee` in `Caller.kt` → resolved to `Callee.kt`.
    #[test]
    fn kotlin_cross_file_import_resolves_to_file_node() {
        // Caller.kt is at root level; Callee.kt lives in com/example/.
        let mut graph = Graph::with_capacity(8, 8);
        add_kotlin_file_node(&mut graph, "Caller.kt");
        add_kotlin_file_node(&mut graph, "com/example/Callee.kt");

        let index = KotlinPackageIndex::build(&graph);

        // Resolve the import
        let resolved = index.resolve("com.example.Callee");
        assert_eq!(
            resolved,
            vec!["file::com/example/Callee.kt"],
            "import com.example.Callee should resolve to com/example/Callee.kt. Got: {:?}",
            resolved
        );

        // Add the cross-file edge and verify it lands in the graph
        let mut stats = CrossFileStats::default();
        let added = add_cross_file_edge(
            &mut graph,
            "file::Caller.kt",
            "file::com/example/Callee.kt",
            "imports",
            &mut stats,
        );
        assert!(added, "imports edge Caller.kt → Callee.kt should be added");
        assert_eq!(stats.total_edges_created, 1);

        // Verify the edge is in pending_edges
        let caller_id = graph.resolve_id("file::Caller.kt").unwrap();
        let callee_id = graph.resolve_id("file::com/example/Callee.kt").unwrap();
        let edge = graph
            .csr
            .pending_edges
            .iter()
            .find(|e| e.source == caller_id && e.target == callee_id);
        assert!(
            edge.is_some(),
            "imports edge Caller.kt → com/example/Callee.kt must be in graph pending_edges"
        );
    }

    /// External Kotlin/Java stdlib imports stay unresolved.
    #[test]
    fn kotlin_external_import_stays_unresolved() {
        let mut graph = Graph::with_capacity(4, 4);
        add_kotlin_file_node(&mut graph, "com/example/Service.kt");

        let index = KotlinPackageIndex::build(&graph);

        // kotlin.collections.List is external — no ingested file
        assert!(
            index.resolve("kotlin.collections.List").is_empty(),
            "kotlin.collections.List must not resolve"
        );
        // java.io.* is external
        assert!(
            index.resolve("java.io.*").is_empty(),
            "java.io.* wildcard must not resolve"
        );
    }

    /// Wildcard `import com.example.*` resolves to all `.kt` files in that package.
    #[test]
    fn kotlin_wildcard_import_resolves_to_all_package_files() {
        let mut graph = Graph::with_capacity(8, 8);
        add_kotlin_file_node(&mut graph, "com/example/Alpha.kt");
        add_kotlin_file_node(&mut graph, "com/example/Beta.kt");
        add_kotlin_file_node(&mut graph, "com/other/Gamma.kt");

        let index = KotlinPackageIndex::build(&graph);

        let mut resolved = index.resolve("com.example.*");
        resolved.sort();
        assert_eq!(
            resolved.len(),
            2,
            "Wildcard should resolve to both files in package. Got: {:?}",
            resolved
        );
        assert!(resolved.contains(&"file::com/example/Alpha.kt"));
        assert!(resolved.contains(&"file::com/example/Beta.kt"));

        // Different package wildcard should not include com/example files
        let resolved_other = index.resolve("com.other.*");
        assert_eq!(resolved_other.len(), 1);
        assert!(resolved_other.contains(&"file::com/other/Gamma.kt"));
    }

    /// Suffix overlap disambiguates same-stem Kotlin classes in different packages.
    #[test]
    fn kotlin_package_index_suffix_match_disambiguates() {
        let mut graph = Graph::with_capacity(6, 6);
        add_kotlin_file_node(&mut graph, "com/example/foo/Bar.kt");
        add_kotlin_file_node(&mut graph, "org/other/Bar.kt");

        let index = KotlinPackageIndex::build(&graph);

        // import "com.example.foo.Bar" should prefer com/example/foo/Bar.kt
        let resolved = index.resolve("com.example.foo.Bar");
        assert_eq!(
            resolved,
            vec!["file::com/example/foo/Bar.kt"],
            "Suffix match should pick the deeper match. Got: {:?}",
            resolved
        );
    }
}
