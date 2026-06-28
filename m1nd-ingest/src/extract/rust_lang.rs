// === crates/m1nd-ingest/src/extract/rust_lang.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;
#[cfg(feature = "tier1")]
use tree_sitter::{Node, Parser};

/// Rust extractor using regex.
/// Replaces: ingest.py RustExtractor
pub struct RustExtractor {
    re_fn: Regex,
    re_struct: Regex,
    re_enum: Regex,
    re_trait: Regex,
    re_impl: Regex,
    re_use: Regex,
    re_mod: Regex,
    // Call/reference detection (non-definition lines)
    re_method_call: Regex,  // .method_name( or ::method_name(
    re_type_ref: Regex,     // UpperCamelCase identifiers (type references)
    re_fn_sig_types: Regex, // Type names in fn signatures: &Type, Type, Box<Type>
    re_free_call: Regex,    // free-function call: bare `name(` not preceded by . or ::
    // Enum variant extraction
    re_variant: Regex, // Variant inside enum { } block
}

impl RustExtractor {
    pub fn new() -> Self {
        Self {
            re_fn: Regex::new(
                r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(?:const\s+)?fn\s+(\w+)",
            )
            .unwrap(),
            re_struct: Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)").unwrap(),
            re_enum: Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+(\w+)").unwrap(),
            re_trait: Regex::new(r"^\s*(?:pub(?:\([^)]*\))?\s+)?trait\s+(\w+)").unwrap(),
            re_impl: Regex::new(r"^\s*impl(?:<[^>]*>)?\s+(?:(\w+)\s+for\s+)?(\w+)").unwrap(),
            re_use: Regex::new(r"^\s*(?:pub\s+)?use\s+(.+);").unwrap(),
            re_mod: Regex::new(r"^\s*(?:pub\s+)?mod\s+(\w+)").unwrap(),
            // Detect Type::method( and receiver.method( calls. The second branch
            // captures the RECEIVER too (group 3) so a lowercase `var.method(` can
            // be told apart from an UpperCamelCase `Type::method(` and emit a
            // name-based `calls` edge to the method (mirrors typescript.rs).
            re_method_call: Regex::new(r"(?:(\w+)::(\w+)|(\w+)\.(\w+))\s*[(<]").unwrap(),
            // UpperCamelCase type references (2+ chars, starts upper)
            // FIX #4: Allow second char to be uppercase (catches CSR, XLR, PPMI)
            re_type_ref: Regex::new(r"\b([A-Z][A-Za-z]\w+)\b").unwrap(),
            // Types in fn signatures: after :, ->, in <>, etc.
            // FIX: was `->s*` (missing backslash), now `->\s*`
            re_fn_sig_types: Regex::new(r"(?::\s*&?(?:mut\s+)?|->\s*&?(?:mut\s+)?|<\s*)([A-Z]\w+)")
                .unwrap(),
            // Free-function calls: an identifier directly followed by `(`
            // (optionally whitespace). Macro calls `foo!(` are naturally excluded
            // because `!` sits between the ident and `(`. Method/path calls
            // (`.foo(`, `Type::foo(`) are rejected by inspecting the char before
            // the match (see the loop), which the regex crate (no lookbehind)
            // cannot express. `\b` anchors the ident start.
            re_free_call: Regex::new(r"\b([A-Za-z_]\w*)\s*\(").unwrap(),
            // Enum variants: identifiers at the start of a line (with optional whitespace)
            // inside an enum block, e.g. `    VariantName,` or `    VariantName(...)` or `    VariantName { ... }`
            re_variant: Regex::new(r"^\s+([A-Z]\w+)\s*(?:[,({]|$)").unwrap(),
        }
    }
}

impl Default for RustExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl RustExtractor {
    fn visibility_tags(item_text: &str) -> Vec<String> {
        let trimmed = item_text.trim_start();
        let mut tags = Vec::new();
        if trimmed.starts_with("pub(crate)") {
            tags.push("rust:visibility:pub(crate)".into());
        } else if trimmed.starts_with("pub(super)") {
            tags.push("rust:visibility:pub(super)".into());
        } else if trimmed.starts_with("pub(self)") {
            tags.push("rust:visibility:pub(self)".into());
        } else if trimmed.starts_with("pub(in ") {
            tags.push("rust:visibility:pub(in)".into());
        } else if trimmed.starts_with("pub ") || trimmed.starts_with("pub\n") {
            tags.push("rust:visibility:pub".into());
        } else {
            tags.push("rust:visibility:private".into());
        }
        tags
    }

    fn cfg_tags(item_text: &str) -> Vec<String> {
        let mut tags = Vec::new();
        for line in item_text.lines() {
            let trimmed = line.trim();
            if let Some(inner) = trimmed
                .strip_prefix("#[cfg(")
                .and_then(|rest| rest.strip_suffix(")]"))
            {
                tags.push(format!("rust:cfg:{}", inner.trim()));
            }
            if let Some(inner) = trimmed
                .strip_prefix("#[cfg_attr(")
                .and_then(|rest| rest.strip_suffix(")]"))
            {
                tags.push(format!("rust:cfg_attr:{}", inner.trim()));
            }
        }
        tags.sort();
        tags.dedup();
        tags
    }

    fn cfg_tags_before_line(source_text: &str, line: u32) -> Vec<String> {
        let lines: Vec<&str> = source_text.lines().collect();
        let mut tags = Vec::new();
        let mut idx = line.saturating_sub(1) as isize - 1;
        while idx >= 0 {
            let trimmed = lines[idx as usize].trim();
            if trimmed.is_empty() {
                idx -= 1;
                continue;
            }
            if let Some(inner) = trimmed
                .strip_prefix("#[cfg(")
                .and_then(|rest| rest.strip_suffix(")]"))
            {
                tags.push(format!("rust:cfg:{}", inner.trim()));
                idx -= 1;
                continue;
            }
            if let Some(inner) = trimmed
                .strip_prefix("#[cfg_attr(")
                .and_then(|rest| rest.strip_suffix(")]"))
            {
                tags.push(format!("rust:cfg_attr:{}", inner.trim()));
                idx -= 1;
                continue;
            }
            if trimmed.starts_with("#[") {
                idx -= 1;
                continue;
            }
            break;
        }
        tags.sort();
        tags.dedup();
        tags
    }

    fn split_top_level(input: &str, sep: char) -> Vec<String> {
        let mut parts = Vec::new();
        let mut start = 0;
        let mut depth_angle: usize = 0;
        let mut depth_brace: usize = 0;
        let mut depth_paren: usize = 0;
        for (idx, ch) in input.char_indices() {
            match ch {
                '<' => depth_angle += 1,
                '>' => depth_angle = depth_angle.saturating_sub(1),
                '{' => depth_brace += 1,
                '}' => depth_brace = depth_brace.saturating_sub(1),
                '(' => depth_paren += 1,
                ')' => depth_paren = depth_paren.saturating_sub(1),
                _ => {}
            }
            if ch == sep && depth_angle == 0 && depth_brace == 0 && depth_paren == 0 {
                parts.push(input[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
        }
        parts.push(input[start..].trim().to_string());
        parts.into_iter().filter(|part| !part.is_empty()).collect()
    }

    fn join_rust_path(prefix: &str, suffix: &str) -> String {
        match (prefix.trim(), suffix.trim()) {
            ("", suffix) => suffix.to_string(),
            (prefix, "") => prefix.to_string(),
            (prefix, suffix) => format!("{}::{}", prefix.trim_end_matches("::"), suffix),
        }
    }

    fn normalize_type_name(raw: &str) -> Option<String> {
        let mut text = raw.trim();
        if text.is_empty() {
            return None;
        }
        while let Some(rest) = text.strip_prefix('&') {
            text = rest.trim_start();
        }
        if let Some(rest) = text.strip_prefix("mut ") {
            text = rest.trim_start();
        }
        if let Some(rest) = text.strip_prefix("dyn ") {
            text = rest.trim_start();
        }
        if let Some(rest) = text.strip_prefix("impl ") {
            text = rest.trim_start();
        }

        let mut end = text.len();
        for needle in ["<", " ", "{", "(", "[", ","] {
            if let Some(idx) = text.find(needle) {
                end = end.min(idx);
            }
        }
        let text = text[..end].trim();
        let text = text.trim_matches(|ch: char| ch == ':' || ch == '&');
        let leaf = text.rsplit("::").next().unwrap_or(text).trim();
        if leaf.is_empty() {
            None
        } else {
            Some(leaf.to_string())
        }
    }

    fn parse_use_tree(prefix: &str, spec: &str, out: &mut Vec<String>) {
        let spec = spec.trim().trim_end_matches(';').trim();
        if spec.is_empty() {
            return;
        }

        if let Some(brace_start) = spec.find('{') {
            if let Some(brace_end) = spec.rfind('}') {
                let base = spec[..brace_start].trim().trim_end_matches("::").trim();
                let next_prefix = if base.is_empty() {
                    prefix.to_string()
                } else {
                    Self::join_rust_path(prefix, base)
                };
                let inner = &spec[brace_start + 1..brace_end];
                for item in Self::split_top_level(inner, ',') {
                    Self::parse_use_tree(&next_prefix, &item, out);
                }
                return;
            }
        }

        let without_alias = spec.split(" as ").next().unwrap_or(spec).trim();
        match without_alias {
            "self" => {
                if !prefix.is_empty() {
                    out.push(prefix.to_string());
                }
            }
            "*" => {
                if !prefix.is_empty() {
                    out.push(format!("{prefix}::*"));
                }
            }
            other => out.push(Self::join_rust_path(prefix, other)),
        }
    }

    /// Expand Rust use paths into normalized targets.
    /// Supports `pub use`, nested brace imports, `self`, and aliases.
    fn expand_use_path(path: &str) -> Vec<String> {
        let mut out = Vec::new();
        Self::parse_use_tree("", path, &mut out);
        out.retain(|item| !item.is_empty());
        out.sort();
        out.dedup();
        out
    }

    fn logical_module_path(file_id: &str) -> Option<String> {
        let rel = file_id.strip_prefix("file::")?;
        let rel = rel.strip_suffix(".rs").unwrap_or(rel);
        let parts: Vec<&str> = rel.split('/').collect();
        if parts.is_empty() {
            return None;
        }

        let start = parts
            .iter()
            .rposition(|part| *part == "src")
            .map(|idx| idx + 1)
            .unwrap_or(0);
        let mut module_parts: Vec<&str> = parts[start..].to_vec();
        if module_parts.is_empty() {
            return None;
        }

        match module_parts.last().copied() {
            Some("lib") | Some("main") => {
                module_parts.pop();
            }
            Some("mod") => {
                module_parts.pop();
            }
            Some(_) => {}
            None => return None,
        }

        if module_parts.is_empty() {
            None
        } else {
            Some(module_parts.join("::"))
        }
    }

    fn fq_name(module_path: Option<&str>, symbol: &str) -> String {
        match module_path {
            Some(path) if !path.is_empty() => format!("{path}::{symbol}"),
            _ => symbol.to_string(),
        }
    }

    fn base_tags(module_path: Option<&str>) -> Vec<String> {
        let mut tags = vec!["rust".into()];
        if let Some(path) = module_path {
            tags.push(format!("rust:module:{path}"));
        } else {
            tags.push("rust:module:crate".into());
        }
        tags
    }

    fn symbol_tags(module_path: Option<&str>, symbol: &str) -> Vec<String> {
        let mut tags = Self::base_tags(module_path);
        tags.push(format!("rust:fq:{}", Self::fq_name(module_path, symbol)));
        tags
    }

    fn module_file_targets(file_id: &str, module_name: &str) -> Vec<String> {
        let Some(rel) = file_id.strip_prefix("file::") else {
            return Vec::new();
        };
        let Some((dir, file_name)) = rel.rsplit_once('/') else {
            let stem = rel.strip_suffix(".rs").unwrap_or(rel);
            let base = match stem {
                "lib" | "main" | "mod" => String::new(),
                other => other.to_string(),
            };
            return if base.is_empty() {
                vec![
                    format!("file::{module_name}.rs"),
                    format!("file::{module_name}/mod.rs"),
                ]
            } else {
                vec![
                    format!("file::{base}/{module_name}.rs"),
                    format!("file::{base}/{module_name}/mod.rs"),
                ]
            };
        };

        let stem = file_name.strip_suffix(".rs").unwrap_or(file_name);
        let base_dir = match stem {
            "lib" | "main" | "mod" => dir.to_string(),
            other => format!("{dir}/{other}"),
        };

        vec![
            format!("file::{base_dir}/{module_name}.rs"),
            format!("file::{base_dir}/{module_name}/mod.rs"),
        ]
    }

    fn push_unique_ref(
        result: &mut ExtractionResult,
        source: &str,
        relation: &str,
        target: String,
        weight: f32,
    ) {
        if !result
            .edges
            .iter()
            .any(|edge| edge.source == source && edge.target == target && edge.relation == relation)
        {
            result.edges.push(ExtractedEdge {
                source: source.to_string(),
                target: target.clone(),
                relation: relation.to_string(),
                weight,
            });
        }
        if !result.unresolved_refs.contains(&target) {
            result.unresolved_refs.push(target);
        }
    }

    fn add_unique_tag(node: &mut ExtractedNode, tag: String) {
        if !node.tags.contains(&tag) {
            node.tags.push(tag);
        }
    }

    fn add_symbol_context_tags(
        result: &mut ExtractionResult,
        line: u32,
        label: &str,
        module_path: Option<&str>,
        extra_tags: &[String],
    ) {
        if let Some(node) = result
            .nodes
            .iter_mut()
            .find(|node| node.line == line && node.label == label)
        {
            if let Some(path) = module_path {
                Self::add_unique_tag(node, format!("rust:module:{path}"));
                Self::add_unique_tag(
                    node,
                    format!("rust:fq:{}", Self::fq_name(Some(path), label)),
                );
            }
            for tag in extra_tags {
                Self::add_unique_tag(node, tag.clone());
            }
        }
    }

    fn find_node_id(result: &ExtractionResult, line: u32, label: &str) -> Option<String> {
        result
            .nodes
            .iter()
            .find(|node| node.line == line && node.label == label)
            .map(|node| node.id.clone())
    }

    /// Make a function id unique within a single file's extraction. Same-named
    /// functions in one file (e.g. the 4 `propagate` impls in activation.rs)
    /// otherwise collide on one `file::…::fn::name` id, so `add_node` drops all
    /// but the first and call edges/queries bind to the wrong node. The FIRST
    /// occurrence keeps the clean id (back-compat: line-less `…::fn::name`
    /// queries still resolve to it); later same-name siblings get a `#2`, `#3`,
    /// … suffix so every distinct definition exists and is addressable. The
    /// node `label` stays `name`, so search/seek still match by label.
    fn unique_fn_id(result: &ExtractionResult, base_id: &str) -> String {
        if !result.nodes.iter().any(|n| n.id == base_id) {
            return base_id.to_string();
        }
        let mut n = 2u32;
        loop {
            let candidate = format!("{base_id}#{n}");
            if !result.nodes.iter().any(|node| node.id == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Scan the contiguous run of attribute lines (and blanks) immediately above
    /// `idx` in `cleaned_lines` and return `(has_cfg_test, has_test_fn)` — whether
    /// a `#[cfg(test)]` and/or a `#[…test]` runner attribute decorates the item at
    /// `idx`. Mirrors [`cfg_tags_before_line`]'s backward walk (stop at the first
    /// non-attribute, non-blank line) so the latch logic stays consistent with the
    /// tree-sitter path and is immune to attribute ordering / interleaving.
    fn test_attrs_before(cleaned_lines: &[String], idx: usize) -> (bool, bool) {
        let mut has_cfg_test = false;
        let mut has_test_fn = false;
        let mut i = idx as isize - 1;
        while i >= 0 {
            let t = cleaned_lines[i as usize].trim();
            if t.is_empty() {
                i -= 1;
                continue;
            }
            if Self::is_cfg_test_attr(t) {
                has_cfg_test = true;
                i -= 1;
                continue;
            }
            if Self::is_test_fn_attr(t) {
                has_test_fn = true;
                i -= 1;
                continue;
            }
            if t.starts_with("#[") || t.starts_with("#![") {
                // Some other attribute (e.g. #[inline]) — keep scanning past it.
                i -= 1;
                continue;
            }
            break;
        }
        (has_cfg_test, has_test_fn)
    }

    /// True if a (trimmed, comment/string-stripped) line is a `#[cfg(test)]`
    /// attribute — the gate that opens an in-file unit-test module. Matches the
    /// bare attribute exactly (the only form that toggles a whole test module);
    /// `#[cfg(all(test, …))]` and friends are deliberately NOT treated as the test
    /// gate here to stay conservative.
    fn is_cfg_test_attr(line: &str) -> bool {
        let t = line.trim();
        t == "#[cfg(test)]" || t == "#![cfg(test)]"
    }

    /// True if a (trimmed, comment/string-stripped) line is a test-runner
    /// attribute on a function: `#[test]`, `#[tokio::test]`, `#[…::test]`
    /// (e.g. `#[async_std::test]`, `#[rstest]`-style `::test` paths). The check is
    /// the attribute body's final path segment being `test`, so any `…::test`
    /// runner counts while ordinary attributes (`#[inline]`, `#[derive(...)]`) do
    /// not.
    fn is_test_fn_attr(line: &str) -> bool {
        let t = line.trim();
        let Some(inner) = t.strip_prefix("#[").and_then(|r| r.strip_suffix("]")) else {
            return false;
        };
        // Drop any argument list (`#[tokio::test(flavor = "…")]`) — keep the path.
        let path = inner.split('(').next().unwrap_or(inner).trim();
        path == "test" || path.rsplit("::").next() == Some("test")
    }

    /// True if `name` is a Rust keyword / control-flow construct / builtin macro-ish
    /// word that can appear as `name(` but is NOT a function call we want a `calls`
    /// edge for (e.g. `if (`, `while (`, `match (`, `return (`, `fn (`). Keeping
    /// this list conservative avoids spurious call edges; anything not listed is
    /// treated as a real free-function call target.
    fn is_call_keyword(name: &str) -> bool {
        matches!(
            name,
            "if" | "for"
                | "while"
                | "loop"
                | "match"
                | "return"
                | "fn"
                | "let"
                | "mut"
                | "move"
                | "as"
                | "in"
                | "where"
                | "impl"
                | "dyn"
                | "ref"
                | "else"
                | "break"
                | "continue"
                | "await"
                | "async"
                | "unsafe"
                | "const"
                | "static"
                | "type"
                | "use"
                | "mod"
                | "pub"
                | "struct"
                | "enum"
                | "trait"
                | "union"
        )
    }

    /// True if `method` is a very common stdlib / container / conversion method
    /// that, called as `receiver.method(`, is almost never a DOMAIN function call
    /// worth a `calls` edge (`.clone()`, `.unwrap()`, `.iter()`, `.push()`, …).
    /// Method calls on lowercase receivers are name-based (no receiver type), so a
    /// permissive list would flood the graph with stdlib noise and create label
    /// collisions; this conservative denylist keeps edges for real domain methods
    /// like `engine.propagate(` while dropping the ubiquitous boilerplate ones.
    fn is_noise_method(method: &str) -> bool {
        matches!(
            method,
            "clone"
                | "unwrap"
                | "expect"
                | "to_string"
                | "to_owned"
                | "as_str"
                | "as_ref"
                | "as_mut"
                | "iter"
                | "iter_mut"
                | "into_iter"
                | "len"
                | "is_empty"
                | "push"
                | "pop"
                | "insert"
                | "remove"
                | "get"
                | "get_mut"
                | "contains"
                | "map"
                | "filter"
                | "collect"
                | "unwrap_or"
                | "unwrap_or_else"
                | "unwrap_or_default"
                | "borrow"
                | "borrow_mut"
                | "lock"
                | "read"
                | "write"
                | "into"
                | "from"
                | "default"
                | "new"
                | "next"
                | "ok"
                | "err"
                | "and_then"
                | "or_else"
        )
    }
}

#[cfg(feature = "tier1")]
#[derive(Clone, Debug)]
struct ImplContext {
    self_ty: String,
    trait_ty: Option<String>,
    impl_node_id: Option<String>,
}

#[cfg(feature = "tier1")]
impl RustExtractor {
    fn semantic_extract_name(node: Node<'_>, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .map(|text| text.to_string())
    }

    fn impl_context(node: Node<'_>, source: &[u8]) -> Option<ImplContext> {
        let text = node.utf8_text(source).ok()?;
        let header = text.split('{').next()?.trim();
        let header = header.strip_prefix("impl")?.trim();
        if let Some((trait_part, self_part)) = header.split_once(" for ") {
            Some(ImplContext {
                self_ty: Self::normalize_type_name(self_part)?,
                trait_ty: Self::normalize_type_name(trait_part),
                impl_node_id: None,
            })
        } else {
            Some(ImplContext {
                self_ty: Self::normalize_type_name(header)?,
                trait_ty: None,
                impl_node_id: None,
            })
        }
    }

    fn impl_node_id(file_id: &str, line: u32, ctx: &ImplContext) -> String {
        match ctx.trait_ty.as_deref() {
            Some(trait_ty) => {
                format!(
                    "{file_id}::impl::{trait_ty}::for::{}::line::{line}",
                    ctx.self_ty
                )
            }
            None => format!("{file_id}::impl::{}::line::{line}", ctx.self_ty),
        }
    }

    fn node_module_path(root_module: Option<&str>, module_stack: &[String]) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(root) = root_module {
            if !root.is_empty() {
                parts.push(root.to_string());
            }
        }
        parts.extend(module_stack.iter().cloned());
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("::"))
        }
    }

    fn enrich_with_tree_sitter(
        &self,
        text: &str,
        file_id: &str,
        result: &mut ExtractionResult,
        root_module: Option<&str>,
    ) {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        if parser.set_language(&language).is_err() {
            return;
        }
        let Some(tree) = parser.parse(text, None) else {
            return;
        };
        let source = text.as_bytes();
        let mut module_stack = Vec::new();
        self.walk_semantic(
            tree.root_node(),
            source,
            file_id,
            result,
            root_module,
            &mut module_stack,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_semantic(
        &self,
        node: Node<'_>,
        source: &[u8],
        file_id: &str,
        result: &mut ExtractionResult,
        root_module: Option<&str>,
        module_stack: &mut Vec<String>,
        impl_ctx: Option<&ImplContext>,
    ) {
        match node.kind() {
            "use_declaration" => {
                if let Ok(text) = node.utf8_text(source) {
                    let relation = if text.trim_start().starts_with("pub use ") {
                        "reexports"
                    } else {
                        "imports"
                    };
                    let spec = text
                        .trim()
                        .trim_start_matches("pub ")
                        .trim_start_matches("use ")
                        .trim_end_matches(';')
                        .trim();
                    for target in Self::expand_use_path(spec) {
                        Self::push_unique_ref(
                            result,
                            file_id,
                            relation,
                            format!("ref::{target}"),
                            0.5,
                        );
                    }
                }
            }
            "impl_item" => {
                let mut next_impl = Self::impl_context(node, source);
                if let Some(ctx) = next_impl.as_mut() {
                    let line = node.start_position().row as u32 + 1;
                    let impl_node_id = Self::impl_node_id(file_id, line, ctx);
                    ctx.impl_node_id = Some(impl_node_id.clone());

                    if !result
                        .nodes
                        .iter()
                        .any(|existing| existing.id == impl_node_id)
                    {
                        let mut tags = vec![
                            "rust".to_string(),
                            "impl_block".to_string(),
                            format!("rust:impl:self:{}", ctx.self_ty),
                        ];
                        if let Some(root) = root_module {
                            tags.push(format!("rust:module:{root}"));
                        }
                        if let Some(trait_ty) = ctx.trait_ty.as_ref() {
                            tags.push(format!("rust:impl:trait:{trait_ty}"));
                        }
                        result.nodes.push(ExtractedNode {
                            id: impl_node_id.clone(),
                            label: match ctx.trait_ty.as_deref() {
                                Some(trait_ty) => format!("impl {trait_ty} for {}", ctx.self_ty),
                                None => format!("impl {}", ctx.self_ty),
                            },
                            node_type: NodeType::Module,
                            tags,
                            line,
                            end_line: node.end_position().row as u32 + 1,
                        });
                        result.edges.push(ExtractedEdge {
                            source: file_id.to_string(),
                            target: impl_node_id.clone(),
                            relation: "contains".into(),
                            weight: 1.0,
                        });
                    }

                    Self::push_unique_ref(
                        result,
                        &impl_node_id,
                        "belongs_to_type",
                        format!("ref::{}", ctx.self_ty),
                        0.8,
                    );
                    if let Some(trait_ty) = ctx.trait_ty.as_ref() {
                        Self::push_unique_ref(
                            result,
                            &impl_node_id,
                            "implements_trait",
                            format!("ref::{trait_ty}"),
                            0.85,
                        );
                    }
                }
                if let Some(ctx) = next_impl.as_ref() {
                    Self::push_unique_ref(
                        result,
                        file_id,
                        "references",
                        format!("ref::{}", ctx.self_ty),
                        0.45,
                    );
                    if let Some(trait_ty) = ctx.trait_ty.as_ref() {
                        Self::push_unique_ref(
                            result,
                            file_id,
                            "implements",
                            format!("ref::{trait_ty}"),
                            0.8,
                        );
                    }
                }
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.walk_semantic(
                        child,
                        source,
                        file_id,
                        result,
                        root_module,
                        module_stack,
                        next_impl.as_ref().or(impl_ctx),
                    );
                }
                return;
            }
            "mod_item" => {
                if let Some(name) = Self::semantic_extract_name(node, source) {
                    let line = node.start_position().row as u32 + 1;
                    module_stack.push(name.clone());
                    let module_path = Self::node_module_path(root_module, module_stack);
                    Self::add_symbol_context_tags(result, line, &name, module_path.as_deref(), &[]);
                    let mut cursor = node.walk();
                    for child in node.named_children(&mut cursor) {
                        self.walk_semantic(
                            child,
                            source,
                            file_id,
                            result,
                            root_module,
                            module_stack,
                            impl_ctx,
                        );
                    }
                    module_stack.pop();
                    return;
                }
            }
            "function_item" | "struct_item" | "enum_item" | "trait_item" | "type_item" => {
                if let Some(name) = Self::semantic_extract_name(node, source) {
                    let line = node.start_position().row as u32 + 1;
                    let module_path = Self::node_module_path(root_module, module_stack);
                    let mut extra_tags = Vec::new();
                    let source_text = std::str::from_utf8(source).unwrap_or("");
                    if let Ok(item_text) = node.utf8_text(source) {
                        extra_tags.extend(Self::visibility_tags(item_text));
                        extra_tags.extend(Self::cfg_tags(item_text));
                    }
                    extra_tags.extend(Self::cfg_tags_before_line(source_text, line));
                    extra_tags.sort();
                    extra_tags.dedup();
                    if let Some(ctx) = impl_ctx {
                        extra_tags.push(format!("rust:impl:self:{}", ctx.self_ty));
                        if let Some(trait_ty) = ctx.trait_ty.as_ref() {
                            extra_tags.push(format!("rust:impl:trait:{trait_ty}"));
                        }
                    }
                    Self::add_symbol_context_tags(
                        result,
                        line,
                        &name,
                        module_path.as_deref(),
                        &extra_tags,
                    );
                    if node.kind() == "function_item" {
                        let method_id = if let Some(existing) =
                            Self::find_node_id(result, line, &name)
                        {
                            existing
                        } else {
                            let node_id =
                                Self::unique_fn_id(result, &format!("{}::fn::{}", file_id, name));
                            let mut tags = Self::symbol_tags(module_path.as_deref(), &name);
                            for tag in &extra_tags {
                                Self::add_unique_tag(
                                    &mut ExtractedNode {
                                        id: String::new(),
                                        label: String::new(),
                                        node_type: NodeType::Function,
                                        tags: tags.clone(),
                                        line,
                                        end_line: line,
                                    },
                                    tag.clone(),
                                );
                            }
                            for tag in &extra_tags {
                                if !tags.contains(tag) {
                                    tags.push(tag.clone());
                                }
                            }
                            result.nodes.push(ExtractedNode {
                                id: node_id.clone(),
                                label: name.clone(),
                                node_type: NodeType::Function,
                                tags,
                                line,
                                end_line: line,
                            });
                            result.edges.push(ExtractedEdge {
                                source: file_id.to_string(),
                                target: node_id.clone(),
                                relation: "contains".into(),
                                weight: 1.0,
                            });
                            node_id
                        };
                        if let Some(ctx) = impl_ctx {
                            if let Some(impl_node_id) = ctx.impl_node_id.as_ref() {
                                Self::push_unique_ref(
                                    result,
                                    &method_id,
                                    "owned_by_impl",
                                    impl_node_id.clone(),
                                    0.85,
                                );
                            }
                            Self::push_unique_ref(
                                result,
                                &method_id,
                                "belongs_to_type",
                                format!("ref::{}", ctx.self_ty),
                                0.7,
                            );
                            if let Some(trait_ty) = ctx.trait_ty.as_ref() {
                                Self::push_unique_ref(
                                    result,
                                    &method_id,
                                    "implements_trait",
                                    format!("ref::{trait_ty}"),
                                    0.75,
                                );
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.walk_semantic(
                child,
                source,
                file_id,
                result,
                root_module,
                module_stack,
                impl_ctx,
            );
        }
    }
}

impl Extractor for RustExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let text = String::from_utf8_lossy(content);
        let cleaned_lines = strip_comments_and_strings(&text, CommentSyntax::RUST);
        let mut result = ExtractionResult {
            nodes: Vec::new(),
            edges: Vec::new(),
            unresolved_refs: Vec::new(),
        };
        let module_path = Self::logical_module_path(file_id);
        let module_path_ref = module_path.as_deref();

        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        result.nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: Self::base_tags(module_path_ref),
            line: 1,
            end_line: text.lines().count() as u32,
        });

        // Track enum/impl blocks for variant and trait-impl-method extraction
        let mut in_enum: Option<String> = None; // Some(enum_node_id) when inside enum { }
        let mut in_impl_block = false; // true when inside impl { }
        let mut impl_is_trait = false; // true when `impl Trait for Type`
        let mut brace_depth: i32 = 0;
        let mut block_start_depth: i32 = 0;
        // Depth at which a `#[cfg(test)] mod …` body opened, when we are inside one
        // (None otherwise). Tracked INDEPENDENTLY of block_start_depth (which enum/
        // impl share) because a test module nests impls/enums of its own. Every fn
        // defined while this is `Some` is an in-file unit test and gets tagged
        // `"test"` — this is what catches `#[cfg(test)] mod tests` living in a
        // non-test path (e.g. src/result_shaping.rs), which the path-only
        // `is_test_source` misses. Popped when brace_depth falls back to/below it
        // (same mechanism as fn_stack). A `#[cfg(test)] mod foo;` declaration (no
        // body) never opens a block, so it never sets this.
        let mut cfg_test_mod_depth: Option<i32> = None;
        // Stack of enclosing functions: (fn_node_id, brace_depth at which the fn
        // body opened). Call edges below are sourced from the top of this stack so
        // `calls` edges read FUNCTION -> ref::callee, not file -> ref::callee.
        // A stack (not a single slot) handles nested fns/closures and methods
        // inside impl blocks. We pop when brace_depth falls back to/below the
        // depth the body opened at (same mechanism as in_enum/in_impl_block).
        let mut fn_stack: Vec<(String, i32)> = Vec::new();
        // A function whose signature was seen but whose body `{` has not opened
        // yet (handles multi-line signatures). (fn_node_id, baseline brace_depth).
        // Pushed onto fn_stack on the line the body brace finally opens.
        let mut pending_fn: Option<(String, i32)> = None;

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;
            // Use the cleaned line for regex matching (comments/strings stripped)
            // but we still need to track brace depth across lines.

            // Update brace depth
            let open_count = line.chars().filter(|&c| c == '{').count() as i32;
            let close_count = line.chars().filter(|&c| c == '}').count() as i32;

            // Depth AFTER applying this line's braces — used for all block-exit
            // checks so a closing `}` on this line correctly pops its block.
            let depth_after = brace_depth + open_count - close_count;

            // Check if we're exiting the current enum or impl block
            if in_enum.is_some() && depth_after <= block_start_depth {
                in_enum = None;
            }
            if in_impl_block && depth_after <= block_start_depth {
                in_impl_block = false;
                impl_is_trait = false;
            }
            // Exit the `#[cfg(test)]` module once its body closes.
            if let Some(open_depth) = cfg_test_mod_depth {
                if depth_after <= open_depth {
                    cfg_test_mod_depth = None;
                }
            }
            // Pop any enclosing functions whose body has now closed.
            while let Some((_, open_depth)) = fn_stack.last() {
                if depth_after <= *open_depth {
                    fn_stack.pop();
                } else {
                    break;
                }
            }

            // --- Enum variant extraction (Task #5) ---
            if let Some(ref enum_id) = in_enum {
                if let Some(caps) = self.re_variant.captures(line) {
                    let variant_name = caps.get(1).unwrap().as_str();
                    // Skip common Rust keywords that might match
                    if !matches!(
                        variant_name,
                        "Self"
                            | "Some"
                            | "None"
                            | "Ok"
                            | "Err"
                            | "Box"
                            | "Vec"
                            | "String"
                            | "Option"
                            | "Result"
                    ) {
                        let variant_id = format!("{}::{}", enum_id, variant_name);
                        result.nodes.push(ExtractedNode {
                            id: variant_id.clone(),
                            label: variant_name.to_string(),
                            node_type: NodeType::Type,
                            tags: {
                                let mut tags = Self::symbol_tags(module_path_ref, variant_name);
                                tags.push("variant".into());
                                tags
                            },
                            line: ln,
                            end_line: ln,
                        });
                        result.edges.push(ExtractedEdge {
                            source: enum_id.clone(),
                            target: variant_id,
                            relation: "contains".into(),
                            weight: 1.0,
                        });
                    }
                }
            }

            // --- Trait impl method extraction (Task #6) ---
            if in_impl_block && impl_is_trait {
                if let Some(caps) = self.re_fn.captures(line) {
                    let name = caps.get(1).unwrap().as_str();
                    // Skip only a re-encounter of THIS exact method (same line+name);
                    // a same-name method in another impl of the same file is a
                    // distinct node and gets a `#N`-disambiguated id.
                    if Self::find_node_id(&result, ln, name).is_none() {
                        let node_id =
                            Self::unique_fn_id(&result, &format!("{}::fn::{}", file_id, name));
                        result.nodes.push(ExtractedNode {
                            id: node_id.clone(),
                            label: name.to_string(),
                            node_type: NodeType::Function,
                            tags: {
                                let mut tags = Self::symbol_tags(module_path_ref, name);
                                tags.push("impl_method".into());
                                // A trait-impl method inside a `#[cfg(test)]` module
                                // (or carrying a test attr) is also a test fn.
                                let (_, has_test_attr) =
                                    Self::test_attrs_before(&cleaned_lines, line_num);
                                if cfg_test_mod_depth.is_some() || has_test_attr {
                                    tags.push("test".into());
                                }
                                tags
                            },
                            line: ln,
                            end_line: ln,
                        });
                        result.edges.push(ExtractedEdge {
                            source: file_id.to_string(),
                            target: node_id,
                            relation: "contains".into(),
                            weight: 1.0,
                        });
                    }
                }
            }

            // --- Standard extraction (struct, enum, trait, impl, fn, mod) ---
            if let Some(caps) = self.re_struct.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::struct::{}", file_id, name);
                result.nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Struct,
                    tags: Self::symbol_tags(module_path_ref, name),
                    line: ln,
                    end_line: ln,
                });
                result.edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
            } else if let Some(caps) = self.re_enum.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::enum::{}", file_id, name);
                result.nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Enum,
                    tags: Self::symbol_tags(module_path_ref, name),
                    line: ln,
                    end_line: ln,
                });
                result.edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id.clone(),
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Start tracking enum block for variant extraction
                if line.contains('{') {
                    in_enum = Some(node_id);
                    block_start_depth = brace_depth;
                }
            } else if let Some(caps) = self.re_trait.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::trait::{}", file_id, name);
                result.nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Type,
                    tags: Self::symbol_tags(module_path_ref, name),
                    line: ln,
                    end_line: ln,
                });
                result.edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
            } else if let Some(caps) = self.re_impl.captures(line) {
                let type_name = caps.get(2).unwrap().as_str();
                Self::push_unique_ref(
                    &mut result,
                    file_id,
                    "references",
                    format!("ref::{type_name}"),
                    0.45,
                );
                if let Some(trait_name) = caps.get(1).map(|m| m.as_str()) {
                    Self::push_unique_ref(
                        &mut result,
                        file_id,
                        "implements",
                        format!("ref::{trait_name}"),
                        0.8,
                    );
                }

                // Track impl block for trait impl method extraction
                let is_trait_impl = caps.get(1).is_some();
                if line.contains('{') {
                    in_impl_block = true;
                    impl_is_trait = is_trait_impl;
                    block_start_depth = brace_depth;
                }
            } else if let Some(caps) = self.re_fn.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                // The trait-impl branch above may already have added THIS exact
                // function (same line+name). Reuse that node id rather than mint a
                // spurious `#2` sibling for one physical fn; only genuinely
                // distinct same-name fns (different line) get disambiguated.
                let node_id = match Self::find_node_id(&result, ln, name) {
                    Some(existing) => existing,
                    None => Self::unique_fn_id(&result, &format!("{}::fn::{}", file_id, name)),
                };
                let already_present = result.nodes.iter().any(|n| n.id == node_id);
                let mut tags = Self::symbol_tags(module_path_ref, name);
                // Tag unit-test functions so impact/ranking can deprioritize test
                // callers: a fn is a test if it sits inside a `#[cfg(test)]` module
                // OR carries a `#[test]`/`#[…::test]` runner attribute. This catches
                // in-file `#[cfg(test)] mod tests` in NON-test paths that the
                // path-only `is_test_source` cannot see.
                let (_, has_test_attr) = Self::test_attrs_before(&cleaned_lines, line_num);
                if cfg_test_mod_depth.is_some() || has_test_attr {
                    tags.push("test".into());
                }
                if !already_present {
                    result.nodes.push(ExtractedNode {
                        id: node_id.clone(),
                        label: name.to_string(),
                        node_type: NodeType::Function,
                        tags,
                        line: ln,
                        end_line: ln,
                    });
                    result.edges.push(ExtractedEdge {
                        source: file_id.to_string(),
                        target: node_id.clone(),
                        relation: "contains".into(),
                        weight: 1.0,
                    });
                }
                // Become the enclosing function once the body `{` opens. Baseline
                // is the depth before this line; the body raises depth above it.
                pending_fn = Some((node_id, brace_depth));
            } else if let Some(caps) = self.re_mod.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::mod::{}", file_id, name);
                result.nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Module,
                    tags: Self::symbol_tags(module_path_ref, name),
                    line: ln,
                    end_line: ln,
                });
                result.edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                if line.trim_end().ends_with(';') {
                    for target in Self::module_file_targets(file_id, name) {
                        result.edges.push(ExtractedEdge {
                            source: file_id.to_string(),
                            target,
                            relation: "declares_module".into(),
                            weight: 0.7,
                        });
                    }
                } else if line.contains('{') {
                    // An inline `mod … { … }` decorated with `#[cfg(test)]` opens a
                    // unit-test module: latch its body depth so every fn defined
                    // inside is tagged `"test"` (see cfg_test_mod_depth). Only the
                    // body form matters; the `;` declaration above never nests fns.
                    let (has_cfg_test, _) = Self::test_attrs_before(&cleaned_lines, line_num);
                    if has_cfg_test {
                        cfg_test_mod_depth = Some(brace_depth);
                    }
                }
            }

            if let Some(caps) = self.re_use.captures(line) {
                let path = caps.get(1).unwrap().as_str().trim();
                let relation = if line.trim_start().starts_with("pub use ") {
                    "reexports"
                } else {
                    "imports"
                };
                let refs = Self::expand_use_path(path);
                for r in refs {
                    Self::push_unique_ref(&mut result, file_id, relation, format!("ref::{r}"), 0.5);
                }
            }

            // Detect Type::method() calls and .method() calls (not on definition lines)
            if !line.trim_start().starts_with("pub")
                && !line.trim_start().starts_with("fn ")
                && !line.trim_start().starts_with("struct ")
                && !line.trim_start().starts_with("enum ")
                && !line.trim_start().starts_with("trait ")
                && !line.trim_start().starts_with("use ")
                && !line.trim_start().starts_with("mod ")
            {
                // Source for call edges: the enclosing function if we are inside
                // one, else the file (top-level calls, e.g. const initializers).
                // This is what makes `calls` edges FUNCTION -> ref::callee so
                // impact/why can traverse the call graph at function granularity.
                let call_source: &str = fn_stack
                    .last()
                    .map(|(id, _)| id.as_str())
                    .unwrap_or(file_id);

                // Path/method calls: `Qualifier::callee(` or `receiver.method(`.
                for caps in self.re_method_call.captures_iter(line) {
                    if let Some(type_match) = caps.get(1) {
                        let qualifier = type_match.as_str();
                        // UpperCamelCase qualifier -> a Type associated call
                        // (`Type::new(`): depend on the Type (existing behavior).
                        if qualifier.chars().next().is_some_and(|c| c.is_uppercase())
                            && qualifier.len() > 1
                        {
                            let ref_id = format!("ref::{}", qualifier);
                            Self::push_unique_ref(&mut result, call_source, "calls", ref_id, 0.4);
                        } else if let Some(callee_match) = caps.get(2) {
                            // Lowercase qualifier -> a module/path-qualified FREE
                            // FUNCTION call (`result_shaping::pack_to_budget(`,
                            // `crate::a::b::func(`). re_method_call captures only
                            // the final `qual::callee` pair, so group(2) is the
                            // callee. Emit a call edge to the callee fn (the same
                            // category as a bare free call, just namespaced) so the
                            // resolver binds ref::callee -> the `fn callee` node.
                            let callee = callee_match.as_str();
                            if callee.len() > 1
                                && callee
                                    .chars()
                                    .next()
                                    .is_some_and(|c| c == '_' || c.is_lowercase())
                                && !Self::is_call_keyword(callee)
                            {
                                let ref_id = format!("ref::{}", callee);
                                Self::push_unique_ref(
                                    &mut result,
                                    call_source,
                                    "calls",
                                    ref_id,
                                    0.35,
                                );
                            }
                        }
                    } else if let (Some(recv_match), Some(method_match)) =
                        (caps.get(3), caps.get(4))
                    {
                        // `receiver.method(` — a METHOD call on a value. When the
                        // receiver is LOWERCASE (a variable/field/`self`, not a
                        // Type), emit a name-based `calls` edge to the method so the
                        // resolver binds `ref::method` -> the `fn method` node by
                        // label (mirrors typescript.rs). This is what surfaces
                        // callers of e.g. `engine.propagate(` / `self.x.propagate(`.
                        // Skip noise methods (`.clone()`, `.iter()`, …), keywords,
                        // and 1-char names to avoid flooding the graph.
                        let receiver = recv_match.as_str();
                        let method = method_match.as_str();
                        if receiver
                            .chars()
                            .next()
                            .is_some_and(|c| c == '_' || c.is_lowercase())
                            && method.len() > 1
                            && !Self::is_call_keyword(method)
                            && !Self::is_noise_method(method)
                        {
                            let ref_id = format!("ref::{}", method);
                            Self::push_unique_ref(&mut result, call_source, "calls", ref_id, 0.35);
                        }
                    }
                }

                // Free-function calls: `name(` not preceded by `.` or `::` (those
                // are method / path calls handled above) and not a keyword.
                // Sourced from the enclosing function so the resolver binds
                // ref::name -> the `fn name` node, yielding a real caller -> callee
                // edge. We inspect the byte before the ident to reject `.`/`:`
                // (method/path call) since the regex crate has no lookbehind.
                //
                // Skip fn DEFINITION lines (incl. `async`/`unsafe`/`const fn`,
                // which the prefix guard above misses): the `name(` there is the
                // function being defined, not a call. The broad guard only covers
                // bare `fn `/`pub`, so re-check with re_fn.
                let bytes = line.as_bytes();
                let is_fn_def_line = self.re_fn.is_match(line);
                for m in self
                    .re_free_call
                    .captures_iter(line)
                    .filter(|_| !is_fn_def_line)
                {
                    let whole = m.get(0).unwrap();
                    let name = m.get(1).unwrap().as_str();
                    let start = whole.start();
                    let prev = if start == 0 {
                        None
                    } else {
                        Some(bytes[start - 1])
                    };
                    // Reject method calls (`.name(`) and path calls (`::name(`).
                    if matches!(prev, Some(b'.') | Some(b':')) {
                        continue;
                    }
                    if name.len() > 1 && !Self::is_call_keyword(name) {
                        let ref_id = format!("ref::{}", name);
                        Self::push_unique_ref(&mut result, call_source, "calls", ref_id, 0.35);
                    }
                }

                // Type references in fn signatures and type annotations
                for caps in self.re_fn_sig_types.captures_iter(line) {
                    if let Some(type_match) = caps.get(1) {
                        let type_name = type_match.as_str();
                        // Skip common std types and primitives
                        if !matches!(
                            type_name,
                            "Self"
                                | "String"
                                | "Vec"
                                | "Option"
                                | "Result"
                                | "Box"
                                | "Arc"
                                | "Rc"
                                | "HashMap"
                                | "HashSet"
                                | "BTreeMap"
                                | "Some"
                                | "None"
                                | "Ok"
                                | "Err"
                                | "Default"
                                | "Debug"
                                | "Clone"
                                | "Send"
                                | "Sync"
                                | "Sized"
                                | "Copy"
                                | "Display"
                                | "From"
                                | "Into"
                        ) {
                            let ref_id = format!("ref::{}", type_name);
                            Self::push_unique_ref(&mut result, file_id, "references", ref_id, 0.3);
                        }
                    }
                }
            }

            // Promote a pending function to the enclosing-function stack once its
            // body brace has opened (depth rose above the signature baseline).
            // Handles multi-line signatures: the `fn` keyword and the body `{`
            // may be on different lines.
            if let Some((id, baseline)) = pending_fn.take() {
                if depth_after > baseline {
                    // Body opened and is still open at end of line — this fn is now
                    // the enclosing scope.
                    fn_stack.push((id, baseline));
                } else if !line.contains('{') && !line.trim_end().ends_with(';') {
                    // Body not open yet (multi-line signature still inside `(...)`)
                    // and not a `;`-terminated declaration — keep waiting.
                    pending_fn = Some((id, baseline));
                }
                // Otherwise: a one-line body (`fn f() { .. }`, opened and closed
                // on this line) or a `;`-terminated signature (trait method decl).
                // Neither becomes a multi-line enclosing scope; drop the latch.
            }

            brace_depth += open_count - close_count;
        }

        #[cfg(feature = "tier1")]
        self.enrich_with_tree_sitter(&text, file_id, &mut result, module_path_ref);

        Ok(result)
    }

    fn extensions(&self) -> &[&str] {
        &["rs"]
    }
}

#[cfg(test)]
mod tests {
    use super::RustExtractor;
    use crate::extract::Extractor;
    use m1nd_core::types::NodeType;

    #[test]
    fn rust_symbols_include_module_and_fq_tags() {
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"pub struct Engine;\npub fn boot() {}\n",
                "file::src/runtime/core.rs",
            )
            .unwrap();

        let engine = result
            .nodes
            .iter()
            .find(|node| node.label == "Engine" && node.node_type == NodeType::Struct)
            .unwrap();
        let boot = result
            .nodes
            .iter()
            .find(|node| node.label == "boot" && node.node_type == NodeType::Function)
            .unwrap();

        assert!(engine
            .tags
            .iter()
            .any(|tag| tag == "rust:module:runtime::core"));
        assert!(engine
            .tags
            .iter()
            .any(|tag| tag == "rust:fq:runtime::core::Engine"));
        assert!(boot
            .tags
            .iter()
            .any(|tag| tag == "rust:fq:runtime::core::boot"));
    }

    #[test]
    fn rust_use_aliases_are_normalized() {
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"pub use crate::graph::{NodeId as GraphNodeId, Edge};\n",
                "file::src/runtime.rs",
            )
            .unwrap();

        assert!(result
            .unresolved_refs
            .iter()
            .any(|item| item == "ref::crate::graph::NodeId"));
        assert!(result
            .unresolved_refs
            .iter()
            .any(|item| item == "ref::crate::graph::Edge"));
        assert!(
            result
                .edges
                .iter()
                .any(|edge| edge.relation == "reexports"
                    && edge.target == "ref::crate::graph::NodeId")
        );
    }

    #[test]
    fn rust_free_function_call_emits_function_sourced_calls_edge() {
        // A free-function call inside a function body must produce a `calls` edge
        // sourced from the ENCLOSING FUNCTION node (not the file), targeting
        // `ref::<callee>` so the resolver can bind it to the callee's fn node.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn callee() {}\nfn caller() {\n    let x = callee();\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let caller_id = "file::src/lib.rs::fn::caller";
        let calls_edge = result
            .edges
            .iter()
            .find(|e| e.relation == "calls" && e.source == caller_id && e.target == "ref::callee");
        assert!(
            calls_edge.is_some(),
            "expected `caller -> ref::callee` calls edge, got edges: {:?}",
            result
                .edges
                .iter()
                .filter(|e| e.relation == "calls")
                .collect::<Vec<_>>()
        );

        // It must NOT be sourced from the file (the bug being fixed).
        assert!(
            !result.edges.iter().any(|e| {
                e.relation == "calls" && e.source == "file::src/lib.rs" && e.target == "ref::callee"
            }),
            "calls edge should be function-sourced, not file-sourced"
        );

        // The callee fn node exists with label `callee`, so the resolver can bind
        // ref::callee -> that node (label-based resolution).
        assert!(result
            .nodes
            .iter()
            .any(|n| n.label == "callee" && n.node_type == NodeType::Function));
    }

    #[test]
    fn rust_free_call_skips_keywords_and_macros() {
        // Control-flow `if (...)` / `while (...)` and macro `println!(...)` must
        // NOT produce call edges; a genuine free call on the same lines must.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn caller() {\n    if (compute()) {\n        println!(\"hi\");\n    }\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let calls: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.as_str())
            .collect();
        assert!(
            calls.contains(&"ref::compute"),
            "expected ref::compute, got {calls:?}"
        );
        assert!(!calls.contains(&"ref::if"), "must not emit `if` as a call");
        assert!(
            !calls.contains(&"ref::while"),
            "must not emit `while` as a call"
        );
        // `println!` is a macro: `!` sits between ident and `(`, so it is never
        // matched as a free call.
        assert!(
            !calls.contains(&"ref::println"),
            "macros must not be calls, got {calls:?}"
        );
    }

    #[test]
    fn rust_path_qualified_free_call_emits_function_sourced_calls_edge() {
        // `module::func(` and `crate::a::func(` are free-function calls reached via
        // a path; they must produce a function-sourced `calls` edge to the callee
        // (so e.g. handle_seek -> pack_to_budget links). A `Type::assoc(` call
        // still depends on the Type (UpperCamelCase qualifier).
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn caller() {\n    let y = result_shaping::pack_to_budget(x);\n    let z = Helper::new();\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let caller_id = "file::src/lib.rs::fn::caller";
        // path-qualified free-fn call -> edge to the callee fn
        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == caller_id
                && e.target == "ref::pack_to_budget"),
            "expected caller -> ref::pack_to_budget, got {:?}",
            result
                .edges
                .iter()
                .filter(|e| e.relation == "calls")
                .map(|e| (&e.source, &e.target))
                .collect::<Vec<_>>()
        );
        // UpperCamelCase qualifier still depends on the Type (not the lowercase rule)
        assert!(result
            .edges
            .iter()
            .any(|e| e.relation == "calls" && e.source == caller_id && e.target == "ref::Helper"));
        // and we did NOT emit a call to the (associated) `new` here.
        assert!(!result
            .edges
            .iter()
            .any(|e| e.relation == "calls" && e.target == "ref::new"));
    }

    #[test]
    fn rust_free_call_attributed_to_nearest_enclosing_function() {
        // Two functions; each call must be attributed to its OWN enclosing fn.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn a() {\n    one();\n}\nfn b() {\n    two();\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        assert!(result.edges.iter().any(|e| {
            e.relation == "calls" && e.source == "file::src/lib.rs::fn::a" && e.target == "ref::one"
        }));
        assert!(result.edges.iter().any(|e| {
            e.relation == "calls" && e.source == "file::src/lib.rs::fn::b" && e.target == "ref::two"
        }));
        // No cross-attribution.
        assert!(!result.edges.iter().any(|e| {
            e.relation == "calls" && e.source == "file::src/lib.rs::fn::a" && e.target == "ref::two"
        }));
    }

    #[test]
    fn rust_lowercase_receiver_method_call_emits_function_sourced_calls_edge() {
        // A method call on a LOWERCASE receiver (`x.propagate(`) — a variable, not
        // a Type — must produce a name-based `calls` edge from the enclosing fn to
        // `ref::propagate`, so impact/why can find callers of methods invoked via
        // a variable (the gap that left `impact propagate` at 0 callers). Mirrors
        // typescript.rs's receiver.method() handling.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn caller() {\n    x.propagate();\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let caller_id = "file::src/lib.rs::fn::caller";
        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == caller_id
                && e.target == "ref::propagate"),
            "expected caller -> ref::propagate (function-sourced), got {:?}",
            result
                .edges
                .iter()
                .filter(|e| e.relation == "calls")
                .map(|e| (&e.source, &e.target))
                .collect::<Vec<_>>()
        );
        // It must be function-sourced, not file-sourced.
        assert!(
            !result.edges.iter().any(|e| {
                e.relation == "calls"
                    && e.source == "file::src/lib.rs"
                    && e.target == "ref::propagate"
            }),
            "method-call edge must be function-sourced, not file-sourced"
        );
    }

    #[test]
    fn rust_method_call_skips_noise_and_keeps_type_assoc_calls() {
        // `.clone()`/`.iter()` (noise) must NOT become calls; an UpperCamelCase
        // `Type::assoc(` still depends on the Type (unchanged). This guards the
        // denylist against flooding while keeping domain method edges.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"fn caller() {\n    let v = data.clone();\n    let it = items.iter();\n    let e = Engine::build();\n    cfg.propagate();\n}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let calls: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.as_str())
            .collect();
        // Domain method on a lowercase receiver -> edge.
        assert!(
            calls.contains(&"ref::propagate"),
            "expected ref::propagate, got {calls:?}"
        );
        // Noise methods -> no edge.
        assert!(
            !calls.contains(&"ref::clone"),
            "`.clone()` must not be a call, got {calls:?}"
        );
        assert!(
            !calls.contains(&"ref::iter"),
            "`.iter()` must not be a call, got {calls:?}"
        );
        // UpperCamelCase Type::assoc still depends on the Type (existing behavior).
        assert!(
            calls.contains(&"ref::Engine"),
            "expected ref::Engine (Type assoc call unchanged), got {calls:?}"
        );
    }

    #[test]
    fn rust_in_file_cfg_test_module_fns_are_tagged_test() {
        // An in-file `#[cfg(test)] mod tests { fn t() {} }` lives in a NON-test
        // path; its fns must still be tagged `"test"` (the path-only is_test_source
        // can't see this), while a production fn in the same file must NOT be.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"pub fn prod() {}\n#[cfg(test)]\nmod tests {\n    fn helper() {}\n    #[test]\n    fn case_one() {}\n}\n",
                "file::src/result_shaping.rs",
            )
            .unwrap();

        let tagged = |label: &str| {
            result
                .nodes
                .iter()
                .find(|n| n.label == label && n.node_type == NodeType::Function)
                .unwrap_or_else(|| panic!("missing fn {label}"))
                .tags
                .iter()
                .any(|t| t == "test")
        };

        // Both the plain helper inside the cfg(test) module AND the #[test] fn are
        // tagged test.
        assert!(
            tagged("helper"),
            "fn inside #[cfg(test)] mod must be tagged"
        );
        assert!(tagged("case_one"), "#[test] fn must be tagged");
        // The production fn outside the test module is NOT tagged.
        assert!(!tagged("prod"), "production fn must NOT be tagged test");
    }

    #[test]
    fn rust_test_attribute_fn_outside_cfg_module_is_tagged() {
        // A `#[tokio::test]` (or `#[test]`) fn NOT wrapped in a cfg(test) module
        // still gets tagged; a neighbouring production fn does not. Also guards the
        // module-exit: a fn AFTER the test module closes is untagged.
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                b"#[tokio::test]\nasync fn async_case() {}\n#[cfg(test)]\nmod tests {\n    fn inner() {}\n}\nfn after_mod() {}\n",
                "file::src/lib.rs",
            )
            .unwrap();

        let tagged = |label: &str| {
            result
                .nodes
                .iter()
                .find(|n| n.label == label && n.node_type == NodeType::Function)
                .unwrap_or_else(|| panic!("missing fn {label}"))
                .tags
                .iter()
                .any(|t| t == "test")
        };

        assert!(tagged("async_case"), "#[tokio::test] fn must be tagged");
        assert!(tagged("inner"), "fn inside #[cfg(test)] mod must be tagged");
        // The module closed before `after_mod`: it is production code, untagged.
        assert!(
            !tagged("after_mod"),
            "fn after the test module closes must NOT be tagged"
        );
    }

    #[test]
    fn rust_mod_declarations_emit_candidate_module_file_edges() {
        let ext = RustExtractor::new();
        let result = ext.extract(b"mod helper;\n", "file::src/main.rs").unwrap();

        assert!(result.edges.iter().any(|edge| {
            edge.relation == "declares_module" && edge.target == "file::src/helper.rs"
        }));
        assert!(result.edges.iter().any(|edge| {
            edge.relation == "declares_module" && edge.target == "file::src/helper/mod.rs"
        }));
    }

    #[cfg(feature = "tier1")]
    #[test]
    fn rust_semantic_enrichment_extracts_visibility_and_cfg_tags() {
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                br#"
#[cfg(feature = "fast")]
pub(crate) struct Engine;
"#,
                "file::src/runtime.rs",
            )
            .unwrap();

        let engine = result
            .nodes
            .iter()
            .find(|node| node.label == "Engine" && node.node_type == NodeType::Struct)
            .unwrap();

        assert!(engine
            .tags
            .iter()
            .any(|tag| tag == "rust:visibility:pub(crate)"));
        assert!(engine
            .tags
            .iter()
            .any(|tag| tag == "rust:cfg:feature = \"fast\""));
    }

    #[cfg(feature = "tier1")]
    #[test]
    fn rust_semantic_enrichment_tracks_nested_modules_and_impl_context() {
        let ext = RustExtractor::new();
        let result = ext
            .extract(
                br#"
mod nested {
    pub struct Engine;

    impl Runner for Engine {
        fn boot(&self) {}
    }
}
"#,
                "file::src/runtime.rs",
            )
            .unwrap();

        let boot = result
            .nodes
            .iter()
            .find(|node| node.label == "boot" && node.node_type == NodeType::Function)
            .unwrap();

        assert!(boot
            .tags
            .iter()
            .any(|tag| tag == "rust:module:runtime::nested"));
        assert!(boot.tags.iter().any(|tag| tag == "rust:impl:self:Engine"));
        assert!(boot.tags.iter().any(|tag| tag == "rust:impl:trait:Runner"));
        assert!(result
            .edges
            .iter()
            .any(|edge| edge.relation == "implements" && edge.target == "ref::Runner"));
        let boot_id = boot.id.clone();
        assert!(result.edges.iter().any(|edge| {
            edge.source == boot_id
                && edge.relation == "belongs_to_type"
                && edge.target == "ref::Engine"
        }));
        assert!(result.edges.iter().any(|edge| {
            edge.source == boot_id
                && edge.relation == "implements_trait"
                && edge.target == "ref::Runner"
        }));
        let impl_node = result
            .nodes
            .iter()
            .find(|node| {
                node.label == "impl Runner for Engine" && node.node_type == NodeType::Module
            })
            .unwrap();
        assert!(result.edges.iter().any(|edge| {
            edge.source == boot_id
                && edge.relation == "owned_by_impl"
                && edge.target == impl_node.id
        }));
    }
}
