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
            // Detect Type::method( and .method( calls
            re_method_call: Regex::new(r"(?:(\w+)::(\w+)|\.(\w+))\s*[(<]").unwrap(),
            // UpperCamelCase type references (2+ chars, starts upper)
            // FIX #4: Allow second char to be uppercase (catches CSR, XLR, PPMI)
            re_type_ref: Regex::new(r"\b([A-Z][A-Za-z]\w+)\b").unwrap(),
            // Types in fn signatures: after :, ->, in <>, etc.
            // FIX: was `->s*` (missing backslash), now `->\s*`
            re_fn_sig_types: Regex::new(r"(?::\s*&?(?:mut\s+)?|->\s*&?(?:mut\s+)?|<\s*)([A-Z]\w+)")
                .unwrap(),
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
                        let method_id =
                            if let Some(existing) = Self::find_node_id(result, line, &name) {
                                existing
                            } else {
                                let node_id = format!("{}::fn::{}", file_id, name);
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

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;
            // Use the cleaned line for regex matching (comments/strings stripped)
            // but we still need to track brace depth across lines.

            // Update brace depth
            let open_count = line.chars().filter(|&c| c == '{').count() as i32;
            let close_count = line.chars().filter(|&c| c == '}').count() as i32;

            // Check if we're exiting the current enum or impl block
            if in_enum.is_some() && brace_depth + open_count - close_count <= block_start_depth {
                in_enum = None;
            }
            if in_impl_block && brace_depth + open_count - close_count <= block_start_depth {
                in_impl_block = false;
                impl_is_trait = false;
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
                    let node_id = format!("{}::fn::{}", file_id, name);
                    // Only add if not already added by the general fn branch below
                    if !result.nodes.iter().any(|n| n.id == node_id) {
                        result.nodes.push(ExtractedNode {
                            id: node_id.clone(),
                            label: name.to_string(),
                            node_type: NodeType::Function,
                            tags: {
                                let mut tags = Self::symbol_tags(module_path_ref, name);
                                tags.push("impl_method".into());
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
                let node_id = format!("{}::fn::{}", file_id, name);
                result.nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
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
                // Type::method( calls -- create ref to Type
                for caps in self.re_method_call.captures_iter(line) {
                    if let Some(type_match) = caps.get(1) {
                        let type_name = type_match.as_str();
                        // Only UpperCamelCase (likely a type, not a variable)
                        if type_name.chars().next().is_some_and(|c| c.is_uppercase())
                            && type_name.len() > 1
                        {
                            let ref_id = format!("ref::{}", type_name);
                            Self::push_unique_ref(&mut result, file_id, "calls", ref_id, 0.4);
                        }
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
