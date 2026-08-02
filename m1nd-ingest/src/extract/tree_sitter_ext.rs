// === crates/m1nd-ingest/src/extract/tree_sitter_ext.rs ===
//
// Generic tree-sitter-based extractor for m1nd.
// One extractor struct handles ALL tree-sitter-backed languages by taking a
// LanguageConfig that maps language-specific node kinds to m1nd NodeTypes.

use super::{ExtractedEdge, ExtractedNode, ExtractionResult, Extractor};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use tree_sitter::{Language, Node, Parser};

// ---------------------------------------------------------------------------
// LanguageConfig — per-language node kind → m1nd NodeType mapping
// ---------------------------------------------------------------------------

/// Maps tree-sitter node kind strings to m1nd semantic concepts.
/// Each language grammar uses different kind names for the same concepts
/// (e.g., C uses "function_definition", Ruby uses "method").
#[derive(Clone, Debug)]
pub struct LanguageConfig {
    /// Language tag for node metadata (e.g., "c", "ruby", "kotlin").
    pub lang_tag: &'static str,
    /// File extensions handled by this config.
    pub extensions: &'static [&'static str],
    /// AST node kinds that represent function/method definitions.
    pub function_kinds: &'static [&'static str],
    /// AST node kinds that represent class definitions.
    pub class_kinds: &'static [&'static str],
    /// AST node kinds that represent struct definitions.
    pub struct_kinds: &'static [&'static str],
    /// AST node kinds that represent enum definitions.
    pub enum_kinds: &'static [&'static str],
    /// AST node kinds that represent type/interface/trait definitions.
    pub type_kinds: &'static [&'static str],
    /// AST node kinds that represent module/namespace definitions.
    pub module_kinds: &'static [&'static str],
    /// AST node kinds that represent import/require/use statements.
    pub import_kinds: &'static [&'static str],
    /// AST node kinds that represent a call/invocation expression.
    ///
    /// When a node's kind matches one of these, `walk_ast` calls
    /// `extract_callee` to get the function/method name being called, then
    /// emits a `calls` edge from the enclosing definition (or file) to
    /// `ref::<callee>`.  The existing resolver later binds `ref::` targets.
    ///
    /// **Only populate this for grammars where the node-kind string and
    /// callee-child structure have been verified by AST inspection.**
    /// Leave as `&[]` for all unverified grammars — they keep current
    /// behaviour exactly (no false-positive edges).
    pub call_kinds: &'static [&'static str],
    /// The field name used for the "name" of definitions (usually "name").
    pub name_field: &'static str,
    /// Alternative field names to try if `name_field` yields nothing.
    pub alt_name_fields: &'static [&'static str],
    /// Node kinds whose first named child is the name identifier
    /// (fallback when field access fails).
    pub name_from_first_child: bool,
}

// ---------------------------------------------------------------------------
// TreeSitterExtractor — implements Extractor trait
// ---------------------------------------------------------------------------

/// Tree-sitter-based code extractor. Parses source via tree-sitter and walks
/// the AST to extract functions, classes, structs, enums, imports, and
/// containment edges.
pub struct TreeSitterExtractor {
    language: Language,
    config: LanguageConfig,
}

impl TreeSitterExtractor {
    /// Create a new extractor for a given tree-sitter Language + config.
    pub fn new(language: Language, config: LanguageConfig) -> Self {
        Self { language, config }
    }

    /// Try to extract the name of a definition node.
    /// Strategy:
    ///   1. field("name") on the node itself
    ///   2. alt_name_fields on the node itself
    ///   3. Recursive drill: for nodes like C's function_definition where the
    ///      name is buried inside a declarator child, drill into children that
    ///      are themselves declarators and try name/identifier extraction on them
    ///   4. First named child that looks like an identifier
    fn extract_name<'a>(&self, node: Node<'a>, source: &'a [u8]) -> Option<String> {
        self.extract_name_inner(node, source, 0)
    }

    fn extract_name_inner<'a>(
        &self,
        node: Node<'a>,
        source: &'a [u8],
        depth: usize,
    ) -> Option<String> {
        if depth > 4 {
            return None; // Prevent infinite recursion
        }

        // Try primary name field
        if let Some(name_node) = node.child_by_field_name(self.config.name_field) {
            // If the name node is a simple identifier, use its text
            if name_node.kind().contains("identifier")
                || name_node.kind() == "name"
                || name_node.kind() == "constant"
                || name_node.kind() == "simple_identifier"
                || name_node.named_child_count() == 0
            {
                let text = name_node.utf8_text(source).ok()?;
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            // If name node is compound (like a scoped identifier), try to
            // extract a simpler name from it
            if let Some(name) = self.extract_name_inner(name_node, source, depth + 1) {
                return Some(name);
            }
        }

        // Try alternative name fields
        for field in self.config.alt_name_fields {
            if let Some(child) = node.child_by_field_name(field) {
                // Drill into declarator nodes (C/C++ pattern:
                // function_definition → declarator: function_declarator → name: identifier)
                if child.kind().contains("declarator") {
                    if let Some(name) = self.extract_name_inner(child, source, depth + 1) {
                        return Some(name);
                    }
                }
                // Simple identifier child
                if child.kind().contains("identifier")
                    || child.kind() == "name"
                    || child.named_child_count() == 0
                {
                    let text = child.utf8_text(source).ok()?;
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }

        // Fallback: scan named children for identifiers, declarators, or
        // name-bearing sub-nodes. When name_from_first_child is true, we
        // recursively drill into children to find the actual name. This
        // handles grammars where the name is nested (e.g., OCaml's
        // value_definition > let_binding > value_name, or TOML's
        // table > bare_key, or SQL's create_table > object_reference > identifier).
        if self.config.name_from_first_child {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                let kind = child.kind();
                // Direct identifier — return immediately
                if kind.contains("identifier")
                    || kind == "name"
                    || kind == "constant"
                    || kind == "simple_identifier"
                    || kind == "bare_key"
                    || kind == "value_name"
                    || kind == "type_constructor"
                    || kind == "constructor_name"
                {
                    let text = child.utf8_text(source).ok()?;
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
                // Skip keyword nodes (SQL: keyword_create, keyword_table, etc.)
                if kind.starts_with("keyword_") {
                    continue;
                }
                // Compound child — drill down recursively
                if kind.contains("declarator")
                    || kind.contains("binding")
                    || kind.contains("reference")
                    || kind.contains("definition")
                    || kind.contains("_name")
                    || kind.contains("spec")
                {
                    if let Some(name) = self.extract_name_inner(child, source, depth + 1) {
                        return Some(name);
                    }
                }
            }
        }

        None
    }

    /// Extract the import target string from an import node.
    fn extract_import_target<'a>(&self, node: Node<'a>, source: &'a [u8]) -> Vec<String> {
        let mut targets = Vec::new();

        // Try common patterns for import targets
        let mut cursor = node.walk();
        self.collect_import_identifiers(node, source, &mut targets, &mut cursor);

        // Fallback: use the full text of the import node, cleaned up
        if targets.is_empty() {
            if let Ok(text) = node.utf8_text(source) {
                // Strip the keyword and extract meaningful identifiers
                let cleaned = text.trim().replace('\n', " ");
                // Extract quoted strings (common in many languages)
                for part in cleaned.split('"').enumerate() {
                    if part.0 % 2 == 1 && !part.1.is_empty() {
                        targets.push(part.1.to_string());
                    }
                }
                // Extract single-quoted strings
                if targets.is_empty() {
                    for part in cleaned.split('\'').enumerate() {
                        if part.0 % 2 == 1 && !part.1.is_empty() {
                            targets.push(part.1.to_string());
                        }
                    }
                }
                // If still nothing, try to extract dotted/scoped names
                if targets.is_empty() {
                    for word in cleaned.split_whitespace() {
                        // Skip keywords
                        if matches!(
                            word,
                            "import"
                                | "from"
                                | "require"
                                | "use"
                                | "include"
                                | "using"
                                | "extern"
                                | "module"
                                | "package"
                                | "open"
                                | "static"
                                | "as"
                                | "*"
                                | "{"
                                | "}"
                                | "("
                                | ")"
                                | ";"
                        ) {
                            continue;
                        }
                        if word.contains('.') || word.contains("::") || word.len() > 1 {
                            targets.push(
                                word.trim_matches(|c: char| {
                                    !c.is_alphanumeric() && c != '.' && c != ':' && c != '_'
                                })
                                .to_string(),
                            );
                        }
                    }
                }
            }
        }

        targets.retain(|t| !t.is_empty());
        targets
    }

    fn is_import_node(&self, node: Node<'_>, source: &[u8]) -> bool {
        let Ok(text) = node.utf8_text(source) else {
            return false;
        };
        let trimmed = text.trim_start();
        match self.config.lang_tag {
            // These grammars represent every command/call with the same AST
            // kind. Only their actual loader verbs are imports; treating jq,
            // echo, or arbitrary calls as module targets emits unsafe refs.
            "bash" => {
                let command = trimmed.split_whitespace().next().unwrap_or_default();
                matches!(command, "source" | ".")
            }
            "lua" => trimmed.starts_with("require(") || trimmed.starts_with("require "),
            "r" => {
                trimmed.starts_with("library(")
                    || trimmed.starts_with("library ")
                    || trimmed.starts_with("require(")
                    || trimmed.starts_with("require ")
            }
            _ => true,
        }
    }

    /// Recursively collect identifier strings from import-related nodes.
    fn collect_import_identifiers<'a>(
        &self,
        node: Node<'a>,
        source: &'a [u8],
        targets: &mut Vec<String>,
        _cursor: &mut tree_sitter::TreeCursor<'a>,
    ) {
        let kind = node.kind();

        // Scoped identifiers, qualified names, etc.
        if kind.contains("identifier")
            || kind == "constant"
            || kind == "scope_resolution"
            || kind == "scoped_identifier"
            || kind == "qualified_name"
            || kind == "dotted_name"
            || kind == "member_expression"
        {
            if let Ok(text) = node.utf8_text(source) {
                let text = text.trim();
                if !text.is_empty()
                    && !matches!(
                        text,
                        "import"
                            | "from"
                            | "require"
                            | "use"
                            | "include"
                            | "using"
                            | "extern"
                            | "module"
                    )
                {
                    targets.push(text.to_string());
                    return; // Don't recurse into children of a qualified name
                }
            }
        }

        // String literals (for require("..."), include "...", etc.)
        if kind == "string_literal"
            || kind == "string"
            || kind == "string_content"
            || kind == "interpreted_string_literal"
        {
            if let Ok(text) = node.utf8_text(source) {
                let trimmed = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
                if !trimmed.is_empty() {
                    targets.push(trimmed.to_string());
                    return;
                }
            }
        }

        // Recurse into children
        let mut child_cursor = node.walk();
        for child in node.named_children(&mut child_cursor) {
            self.collect_import_identifiers(child, source, targets, &mut node.walk());
        }
    }

    /// Extract the callee name from a call/invocation expression node.
    ///
    /// Each grammar arranges the callee differently inside its call node.
    /// This method handles the patterns verified for each piloted grammar:
    ///
    /// - **C / C++** (`call_expression`):
    ///   - first named child = `identifier`         → simple call `foo()`
    ///   - first named child = `field_expression`   → member call `p->bar()` or `p.bar()`;
    ///     the `field_identifier` child of `field_expression` is the method name.
    ///
    /// - **C#** (`invocation_expression`):
    ///   - first named child = `identifier`              → simple call `Foo()`
    ///   - first named child = `member_access_expression` → qualified call
    ///     `Console.WriteLine()`; the *last* `identifier` child of the
    ///     `member_access_expression` is the method name.
    ///
    /// - **Kotlin / Scala** (`call_expression`):
    ///   - first named child = `identifier`          → simple call `foo(42)`
    ///   - first named child = `navigation_expression` → chained call
    ///     `obj.method()` — Kotlin skips (returns None). See Swift below.
    ///
    /// - **PHP** (`function_call_expression`):
    ///   - first named child = `name`                → simple call `foo()`
    ///
    /// - **PHP** (`member_call_expression` / `scoped_call_expression`):
    ///   - second named child = `name`               → method/static call `$obj->method()`
    ///     or `Class::method()`; first child is receiver, second is callee `name`.
    ///
    /// - **Swift** (`call_expression`):
    ///   - first named child = `simple_identifier`         → bare call `foo()`
    ///   - first named child = `navigation_expression` →
    ///     `navigation_suffix` child contains trailing `simple_identifier` = callee name.
    ///
    /// - **Ruby** (`call`):
    ///   - `method` field child = `identifier` → method name (e.g., `bar` in `obj.bar(1)`)
    ///   - `receiver` field child holds the receiver (skipped to avoid false positives)
    ///   - Bare calls without parens (e.g., `foo`) parse as plain `identifier` — they do
    ///     NOT produce a `call` node and are intentionally skipped.
    ///   - Verified via AST dump: `method` field is always a plain `identifier`.
    ///
    /// Returns `None` when the callee cannot be cleanly identified, which
    /// causes `walk_ast` to silently skip the edge (safe, no false positives).
    fn extract_callee<'a>(&self, node: Node<'a>, source: &'a [u8]) -> Option<String> {
        // Pattern Ruby: `call` node with a `method` field child.
        // Verified via AST dump: tree-sitter-ruby uses `call` for all receiver.method()
        // invocations. The `method` field is always a plain `identifier` containing
        // the method name. The `receiver` field (identifier or constant) is ignored.
        //
        // Bare calls without parens (e.g., `foo`) parse as `identifier` in Ruby and
        // do NOT produce a `call` node — so they are not captured here (safe).
        //
        // A method DEFINITION (`def foo ... end`) parses as `method` (not `call`), so
        // definitions never match this branch and never produce self-call edges.
        let node_kind = node.kind();
        if node_kind == "call" && self.config.lang_tag == "ruby" {
            if let Some(method_node) = node.child_by_field_name("method") {
                let text = method_node.utf8_text(source).ok()?;
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            return None; // `call` without clean `method` field → skip
        }

        // Pattern 0: PHP `member_call_expression` and `scoped_call_expression`.
        // Verified: these nodes have children [receiver, name(callee), arguments].
        // Must be checked BEFORE the generic `name` pattern (Pattern 2) because
        // the first child of `scoped_call_expression` is also `name` kind (class).
        // Second named child is the method/static callee `name`.
        if node_kind == "member_call_expression" || node_kind == "scoped_call_expression" {
            let mut nc = node.walk();
            let mut named_children = node.named_children(&mut nc);
            named_children.next(); // skip receiver (first child)
            if let Some(second_child) = named_children.next() {
                if second_child.kind() == "name" {
                    let text = second_child.utf8_text(source).ok()?;
                    let text = text.trim();
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            return None; // can't resolve cleanly
        }

        let mut cursor = node.walk();
        let first_child = node.named_children(&mut cursor).next()?;
        let first_kind = first_child.kind();

        // Pattern 1: simple identifier — covers C, C++, Kotlin, Scala, and Swift bare calls.
        if first_kind == "identifier" || first_kind == "simple_identifier" {
            let text = first_child.utf8_text(source).ok()?;
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        // Pattern 2: PHP `name` as first child — for `function_call_expression`.
        // Verified: PHP `function_call_expression` → first named child kind = "name".
        if first_kind == "name" {
            let text = first_child.utf8_text(source).ok()?;
            let text = text.trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }

        // Pattern 4: C `field_expression` (e.g., `ptr->method` or `obj.method`)
        // The rightmost child of field_expression with kind `field_identifier`
        // is the method name being called.
        if first_kind == "field_expression" {
            let mut fc = first_child.walk();
            for child in first_child.named_children(&mut fc) {
                if child.kind() == "field_identifier" {
                    let text = child.utf8_text(source).ok()?;
                    let text = text.trim();
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }

        // Pattern 5: C# `member_access_expression` (e.g., `Console.WriteLine`)
        // The *last* identifier child is the method name.
        if first_kind == "member_access_expression" {
            let mut fc = first_child.walk();
            let mut last_ident: Option<String> = None;
            for child in first_child.named_children(&mut fc) {
                if child.kind() == "identifier" {
                    if let Ok(text) = child.utf8_text(source) {
                        let text = text.trim();
                        if !text.is_empty() {
                            last_ident = Some(text.to_string());
                        }
                    }
                }
            }
            if last_ident.is_some() {
                return last_ident;
            }
        }

        // Pattern 6: Swift `navigation_expression` (e.g., `obj.method` or `a.b.c`)
        // Verified: navigation_expression has a `navigation_suffix` named child,
        // which contains the terminal `simple_identifier` (the method being called).
        // Works for both `obj.method()` (depth=1) and `a.b.c()` (nested navigation).
        if first_kind == "navigation_expression" {
            // Find the navigation_suffix child
            let mut fc = first_child.walk();
            for child in first_child.named_children(&mut fc) {
                if child.kind() == "navigation_suffix" {
                    // Inside navigation_suffix: find simple_identifier
                    let mut sc = child.walk();
                    for suffix_child in child.named_children(&mut sc) {
                        if suffix_child.kind() == "simple_identifier" {
                            let text = suffix_child.utf8_text(source).ok()?;
                            let text = text.trim();
                            if !text.is_empty() {
                                return Some(text.to_string());
                            }
                        }
                    }
                }
            }
            return None; // navigation_expression without clean suffix → skip
        }

        // All other patterns (subscript, etc.) → skip.
        // Returning None causes walk_ast to emit no edge — safe default.
        None
    }

    /// Walk the AST and extract nodes + edges.
    fn walk_ast(
        &self,
        root: Node<'_>,
        source: &[u8],
        file_id: &str,
        nodes: &mut Vec<ExtractedNode>,
        edges: &mut Vec<ExtractedEdge>,
        unresolved_refs: &mut Vec<String>,
    ) {
        // Use a stack to avoid deep recursion on large files
        let mut stack: Vec<(Node<'_>, Option<String>)> = vec![(root, None)];

        while let Some((node, parent_id)) = stack.pop() {
            let kind = node.kind();
            let start_line = node.start_position().row as u32 + 1;
            let end_line = node.end_position().row as u32 + 1;

            // Check if this node is a definition we care about
            let (node_type, id_prefix) = if self.config.function_kinds.contains(&kind) {
                (Some(NodeType::Function), "fn")
            } else if self.config.class_kinds.contains(&kind) {
                (Some(NodeType::Class), "class")
            } else if self.config.struct_kinds.contains(&kind) {
                (Some(NodeType::Struct), "struct")
            } else if self.config.enum_kinds.contains(&kind) {
                (Some(NodeType::Enum), "enum")
            } else if self.config.type_kinds.contains(&kind) {
                (Some(NodeType::Type), "type")
            } else if self.config.module_kinds.contains(&kind) {
                (Some(NodeType::Module), "module")
            } else {
                (None, "")
            };

            // Handle import statements
            if self.config.import_kinds.contains(&kind) && self.is_import_node(node, source) {
                let targets = self.extract_import_target(node, source);
                for target in targets {
                    // The full-text fallback splits on quotes, so a whitespace
                    // string literal in the source (`replace(/\s+/g," ")`) can
                    // come back as a target of pure whitespace — which becomes
                    // the invalid endpoint `ref:: ` and killed a real birth
                    // ceremony (2026-08-02). A blank reference names nothing;
                    // drop it here, at the producer.
                    let target = target.trim();
                    if target.is_empty() {
                        continue;
                    }
                    let ref_id = format!("ref::{}", target);
                    edges.push(ExtractedEdge {
                        source: file_id.to_string(),
                        target: ref_id.clone(),
                        relation: "imports".into(),
                        weight: 0.5,
                    });
                    if !unresolved_refs.contains(&ref_id) {
                        unresolved_refs.push(ref_id);
                    }
                }
            }

            // Handle call expressions — emit `calls` edges from the enclosing
            // definition (or file if top-level) to `ref::<callee>`.
            // We still recurse into children so that nested calls inside
            // argument lists are also detected with the same enclosing caller.
            if self.config.call_kinds.contains(&kind) {
                if let Some(callee) = self
                    .extract_callee(node, source)
                    .map(|callee| callee.trim().to_string())
                    .filter(|callee| !callee.is_empty())
                {
                    let caller = parent_id.as_deref().unwrap_or(file_id);
                    let ref_id = format!("ref::{}", callee);
                    edges.push(ExtractedEdge {
                        source: caller.to_string(),
                        target: ref_id.clone(),
                        relation: "calls".into(),
                        weight: 0.8,
                    });
                    if !unresolved_refs.contains(&ref_id) {
                        unresolved_refs.push(ref_id);
                    }
                }
                // Always recurse into call children (argument lists may contain
                // nested calls). The parent stays unchanged — nested calls are
                // also attributed to the same enclosing definition.
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    stack.push((child, parent_id.clone()));
                }
                continue; // Skip default child traversal below
            }

            // Handle definitions
            if let Some(nt) = node_type {
                if let Some(name) = self.extract_name(node, source) {
                    let base_id = format!("{}::{}::{}", file_id, id_prefix, name);
                    let node_id = if nodes.iter().any(|existing| existing.id == base_id) {
                        let line_id = format!("{base_id}#L{start_line}");
                        if nodes.iter().any(|existing| existing.id == line_id) {
                            format!("{line_id}B{}", node.start_byte())
                        } else {
                            line_id
                        }
                    } else {
                        base_id
                    };

                    nodes.push(ExtractedNode {
                        id: node_id.clone(),
                        label: name,
                        node_type: nt,
                        tags: vec![self.config.lang_tag.into()],
                        line: start_line,
                        end_line,
                    });

                    // Containment edge: parent → this node
                    let container = parent_id.as_deref().unwrap_or(file_id);
                    edges.push(ExtractedEdge {
                        source: container.to_string(),
                        target: node_id.clone(),
                        relation: "contains".into(),
                        weight: 1.0,
                    });

                    // Recurse into children with this node as parent
                    let mut cursor = node.walk();
                    for child in node.named_children(&mut cursor) {
                        stack.push((child, Some(node_id.clone())));
                    }
                    continue; // Skip the default child traversal below
                }
            }

            // Default: recurse into children with same parent
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                stack.push((child, parent_id.clone()));
            }
        }
    }
}

impl Extractor for TreeSitterExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let mut parser = Parser::new();
        parser.set_language(&self.language).map_err(|e| {
            m1nd_core::error::M1ndError::IngestError(format!(
                "Failed to set tree-sitter language for {}: {}",
                self.config.lang_tag, e
            ))
        })?;

        let tree = parser.parse(content, None).ok_or_else(|| {
            m1nd_core::error::M1ndError::IngestError(format!(
                "tree-sitter parse returned None for {}",
                file_id
            ))
        })?;

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut unresolved_refs = Vec::new();

        // File node (every extractor must emit one)
        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        let line_count = content.iter().filter(|&&b| b == b'\n').count() as u32;
        nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: vec![self.config.lang_tag.into()],
            line: 1,
            end_line: line_count.max(1),
        });

        // Walk AST
        self.walk_ast(
            tree.root_node(),
            content,
            file_id,
            &mut nodes,
            &mut edges,
            &mut unresolved_refs,
        );

        Ok(ExtractionResult {
            nodes,
            edges,
            unresolved_refs,
        })
    }

    fn extensions(&self) -> &[&str] {
        self.config.extensions
    }
}

// ---------------------------------------------------------------------------
// Language configs for all Tier 1 languages
// ---------------------------------------------------------------------------

/// C language config.
///
/// `call_kinds`: verified via AST dump — `call_expression` is the node kind for
/// both bare function calls (`foo(42)`) and struct-member calls (`p->bar()`).
/// `extract_callee` handles both: `identifier` first child for bare calls and
/// `field_expression` → `field_identifier` for member calls.
pub fn c_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "c",
        extensions: &["c", "h"],
        function_kinds: &["function_definition"],
        class_kinds: &[],
        struct_kinds: &["struct_specifier"],
        enum_kinds: &["enum_specifier"],
        type_kinds: &["type_definition"],
        module_kinds: &[],
        import_kinds: &["preproc_include"],
        // Verified: tree-sitter-c uses `call_expression` for all calls.
        // Callee is either a direct `identifier` child or a `field_expression`
        // child (for pointer/struct member calls). Confirmed via AST dump.
        call_kinds: &["call_expression"],
        name_field: "name",
        alt_name_fields: &["declarator"],
        name_from_first_child: true,
    }
}

/// C++ language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-cpp uses `call_expression`
/// for both bare function calls (`foo(42)`) and member calls (`obj.method()`).
/// The callee structure is identical to C:
///   - `identifier` first child → simple call
///   - `field_expression` → `field_identifier` → member call
///
/// Constructor calls (e.g., `Foo()`) and template instantiations that look like
/// calls resolve via `identifier` first child and are extracted cleanly.
/// Operator overloads (`operator()`) and complex template-argument calls whose
/// first child is neither `identifier` nor `field_expression` return `None`
/// from extract_callee — no bogus edge is emitted.
pub fn cpp_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "cpp",
        extensions: &["cpp", "cxx", "cc", "hpp", "hxx", "hh"],
        function_kinds: &["function_definition"],
        class_kinds: &["class_specifier"],
        struct_kinds: &["struct_specifier"],
        enum_kinds: &["enum_specifier"],
        type_kinds: &["type_definition", "alias_declaration"],
        module_kinds: &["namespace_definition"],
        import_kinds: &["preproc_include", "using_declaration"],
        // Verified: tree-sitter-cpp uses `call_expression` for all calls.
        // Same callee extraction patterns as C (identifier + field_expression).
        // Unresolvable shapes (template args, operators) return None safely.
        call_kinds: &["call_expression"],
        name_field: "name",
        alt_name_fields: &["declarator"],
        name_from_first_child: true,
    }
}

/// C# language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-c-sharp uses
/// `invocation_expression` for all method/function calls. The first named
/// child is either a plain `identifier` (bare call) or a
/// `member_access_expression` (qualified call like `Console.WriteLine`).
/// `extract_callee` handles both patterns.
pub fn csharp_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "csharp",
        extensions: &["cs"],
        function_kinds: &[
            "method_declaration",
            "constructor_declaration",
            "local_function_statement",
        ],
        class_kinds: &["class_declaration", "record_declaration"],
        struct_kinds: &["struct_declaration"],
        enum_kinds: &["enum_declaration"],
        type_kinds: &["interface_declaration", "delegate_declaration"],
        module_kinds: &["namespace_declaration"],
        import_kinds: &["using_directive"],
        // Verified: tree-sitter-c-sharp uses `invocation_expression` for calls.
        // Handles bare calls (identifier first child) and qualified calls
        // (member_access_expression → last identifier is the method name).
        call_kinds: &["invocation_expression"],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Ruby language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-ruby uses `call` for all
/// receiver.method() invocations (e.g., `obj.bar(1)`, `Helper.process(x)`).
/// The callee is extracted via the `method` field child (always an `identifier`).
/// The `receiver` field (identifier or constant) is ignored to avoid false positives.
///
/// Bare calls without parens (e.g., `foo`) parse as plain `identifier` in Ruby,
/// NOT as `call` nodes — they are intentionally skipped (conservative default).
///
/// Method DEFINITIONs (`def foo ... end`) parse as `method` node kind, never as
/// `call`, so a def never produces a self-call edge.
///
/// `import_kinds` remains `&[]` — Ruby imports are handled by the dedicated
/// require_relative scanner (cross_file.rs). Since `call` is no longer in
/// import_kinds, there is no dual-use conflict; `call` can safely go in call_kinds.
pub fn ruby_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "ruby",
        extensions: &["rb", "rake", "gemspec"],
        function_kinds: &["method", "singleton_method"],
        class_kinds: &["class"],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &["module"],
        // Ruby imports are handled by the dedicated require_relative scanner
        // (collect_ruby_import_edges_from_files in cross_file.rs). Using "call"
        // here mis-tagged EVERY method call as an import; removed to stop the noise.
        // No dual-use conflict: `call` in call_kinds is safe.
        import_kinds: &[],
        // Verified: tree-sitter-ruby uses `call` for receiver.method() invocations.
        // extract_callee uses the `method` field child to get the method name cleanly.
        // No overlap with import_kinds (which is empty for Ruby).
        call_kinds: &["call"],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// PHP language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-php uses three distinct node kinds
/// for different call shapes:
///   - `function_call_expression`: `name`(callee) + `arguments`
///     e.g., `foo()` → first named child = `name` "foo"
///   - `member_call_expression`: `variable_name`(receiver) + `name`(callee) + `arguments`
///     e.g., `$obj->method()` → second named child = `name` "method"
///   - `scoped_call_expression`: `name`(class) + `name`(callee) + `arguments`
///     e.g., `SomeClass::staticMethod()` → second named child = `name` "staticMethod"
///
/// None of these kinds appear in `import_kinds` (`namespace_use_declaration`).
/// No dual-use conflict.
pub fn php_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "php",
        extensions: &["php"],
        function_kinds: &["function_definition", "method_declaration"],
        class_kinds: &["class_declaration"],
        struct_kinds: &[],
        enum_kinds: &["enum_declaration"],
        type_kinds: &["interface_declaration", "trait_declaration"],
        module_kinds: &["namespace_definition"],
        import_kinds: &["namespace_use_declaration"],
        // Verified: tree-sitter-php uses three call node kinds (function_call_expression,
        // member_call_expression, scoped_call_expression). extract_callee handles all three.
        // No overlap with import_kinds.
        call_kinds: &[
            "function_call_expression",
            "member_call_expression",
            "scoped_call_expression",
        ],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Swift language config.
///
/// Note: Swift's tree-sitter grammar maps both `class` and `struct` to
/// `class_declaration` (struct has a `struct` keyword child). We map
/// class_declaration to Class. This is semantically correct for the m1nd
/// graph since both are type definitions with containment.
///
/// `call_kinds`: verified via AST dump — tree-sitter-swift uses `call_expression`
/// for all call sites. Two callee patterns:
///   - `simple_identifier` first child → bare call `foo()` — callee = that identifier
///   - `navigation_expression` first child → method call `obj.method()` or `a.b.c()`.
///     The navigation_expression always has a `navigation_suffix` named child which
///     contains the terminal `simple_identifier` (the method name). Verified for both
///     shallow (`obj.method`) and deep (`a.b.c`) chaining.
///
/// `call_expression` does NOT appear in `import_kinds` (`import_declaration`).
/// No dual-use conflict.
pub fn swift_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "swift",
        extensions: &["swift"],
        function_kinds: &["function_declaration", "init_declaration"],
        class_kinds: &["class_declaration"],
        struct_kinds: &[], // Swift grammar uses class_declaration for both
        enum_kinds: &["enum_declaration"],
        type_kinds: &["protocol_declaration", "typealias_declaration"],
        module_kinds: &[],
        import_kinds: &["import_declaration"],
        // Verified: tree-sitter-swift uses `call_expression` for all calls.
        // Bare calls (simple_identifier first child) and navigation calls
        // (navigation_expression → navigation_suffix → simple_identifier) both handled.
        // No overlap with import_kinds.
        call_kinds: &["call_expression"],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Kotlin language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-kotlin-ng uses
/// `call_expression` for all function/method calls. When the first named
/// child is an `identifier`, that is the callee (e.g., `foo(42)`,
/// `println(x)`). When the first named child is a `navigation_expression`
/// (e.g., `obj.method()`), the call is skipped to avoid ambiguity.
pub fn kotlin_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "kotlin",
        extensions: &["kt", "kts"],
        function_kinds: &["function_declaration"],
        class_kinds: &["class_declaration"],
        struct_kinds: &[],
        enum_kinds: &["enum_class_body"], // Kotlin enums are class_declaration with enum modifier
        type_kinds: &["interface_declaration", "type_alias"],
        module_kinds: &["package_header"],
        import_kinds: &["import_header"],
        // Verified: tree-sitter-kotlin-ng uses `call_expression` for calls.
        // Simple calls (identifier first child) are extracted cleanly.
        // Chained/navigation calls are skipped (navigation_expression first child).
        call_kinds: &["call_expression"],
        name_field: "name",
        alt_name_fields: &["simple_identifier"],
        name_from_first_child: true,
    }
}

/// Scala language config.
///
/// `call_kinds`: verified via AST dump — tree-sitter-scala uses `call_expression`
/// for function/method calls. The first named child is an `identifier` for simple
/// calls (e.g., `foo()`, `baz(42)`). Chained calls (e.g., `list.map(f)`) would
/// have a non-identifier first child and return `None` from extract_callee safely.
///
/// `call_expression` does NOT appear in `import_kinds` (`import_declaration`).
/// No dual-use conflict.
pub fn scala_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "scala",
        extensions: &["scala", "sc"],
        function_kinds: &["function_definition"],
        class_kinds: &["class_definition"],
        struct_kinds: &[],
        enum_kinds: &["enum_definition"],
        type_kinds: &["trait_definition", "type_definition"],
        module_kinds: &["object_definition", "package_clause"],
        import_kinds: &["import_declaration"],
        // Verified: tree-sitter-scala uses `call_expression` for calls.
        // First named child is `identifier` for simple calls; others return None.
        // No overlap with import_kinds.
        call_kinds: &["call_expression"],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Bash/Shell language config.
pub fn bash_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "bash",
        extensions: &["sh", "bash", "zsh"],
        function_kinds: &["function_definition"],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["command"], // source/. commands are regular commands
        // Deferred: Bash uses `command` for all invocations, same as import_kinds.
        // Disambiguation needed. Deferred.
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Lua language config.
pub fn lua_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "lua",
        extensions: &["lua"],
        function_kinds: &["function_declaration", "local_function_declaration"],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["function_call"], // require() calls
        // Deferred: Lua uses `function_call` for all calls, same as import_kinds.
        // Disambiguation (require vs other calls) needed. Deferred.
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// R language config.
pub fn r_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "r",
        extensions: &["r", "R", "Rmd"],
        // R doesn't have function_definition as a node kind;
        // functions are assigned via `name <- function(...)`.
        // We pick up left_assignment where the RHS is a function_definition.
        function_kinds: &["function_definition"],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["call"], // library() and require() are function calls
        // Deferred: R uses `call` for all invocations, same as import_kinds.
        // Disambiguation needed. Deferred.
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// HTML language config (structural extraction: tags as nodes is low value,
/// but we extract <script>/<link>/<style> as imports).
pub fn html_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "html",
        extensions: &["html", "htm"],
        function_kinds: &[],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["script_element", "style_element"],
        // N/A: HTML is structural; call graphs are handled by embedded JS extractor.
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: false,
    }
}

/// CSS language config.
pub fn css_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "css",
        extensions: &["css"],
        function_kinds: &[],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["import_statement"],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: false,
    }
}

/// JSON language config (structural only — top-level keys as nodes).
pub fn json_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "json",
        extensions: &["json"],
        function_kinds: &[],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &[],
        call_kinds: &[],
        name_field: "key",
        alt_name_fields: &[],
        name_from_first_child: false,
    }
}

// ---------------------------------------------------------------------------
// Factory functions — create configured extractors
// ---------------------------------------------------------------------------

/// Create a C extractor.
pub fn c_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_c::LANGUAGE.into(), c_config())
}

/// Create a C++ extractor.
pub fn cpp_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_cpp::LANGUAGE.into(), cpp_config())
}

/// Create a C# extractor.
pub fn csharp_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_c_sharp::LANGUAGE.into(), csharp_config())
}

/// Create a Ruby extractor.
pub fn ruby_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_ruby::LANGUAGE.into(), ruby_config())
}

/// Create a PHP extractor.
pub fn php_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_php::LANGUAGE_PHP.into(), php_config())
}

/// Create a Swift extractor.
pub fn swift_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_swift::LANGUAGE.into(), swift_config())
}

/// Create a Kotlin extractor.
pub fn kotlin_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_kotlin_ng::LANGUAGE.into(), kotlin_config())
}

/// Create a Scala extractor.
pub fn scala_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_scala::LANGUAGE.into(), scala_config())
}

/// Create a Bash extractor.
pub fn bash_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_bash::LANGUAGE.into(), bash_config())
}

/// Create a Lua extractor.
pub fn lua_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_lua::LANGUAGE.into(), lua_config())
}

/// Create an R extractor.
pub fn r_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_r::LANGUAGE.into(), r_config())
}

/// Create an HTML extractor.
pub fn html_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_html::LANGUAGE.into(), html_config())
}

/// Create a CSS extractor.
pub fn css_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_css::LANGUAGE.into(), css_config())
}

/// Create a JSON extractor.
pub fn json_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_json::LANGUAGE.into(), json_config())
}

/// JavaScript language config.
pub fn javascript_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "javascript",
        extensions: &["js", "mjs", "cjs"],
        function_kinds: &[
            "function_declaration",
            "function_expression",
            "arrow_function",
            "method_definition",
            "generator_function_declaration",
            "generator_function",
        ],
        class_kinds: &["class_declaration", "class_expression"],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["import_statement", "call_expression"],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: false,
    }
}

/// Create a JavaScript extractor.
pub fn javascript_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_javascript::LANGUAGE.into(), javascript_config())
}

// ---------------------------------------------------------------------------
// EmbeddedExtractor — parses host language (e.g. HTML) and routes embedded
// language blocks (e.g. <script>, <style>) to inner extractors.
// ---------------------------------------------------------------------------

/// Descriptor for an embedded language block found inside a host document.
struct EmbeddedBlock {
    /// Text content of the embedded block (without surrounding tags).
    content: String,
    /// Line offset in the original host file where this block starts (0-indexed).
    line_offset: u32,
    /// Language tag (e.g., "javascript", "css").
    lang: String,
}

/// Extracts embedded language blocks from a host language document (e.g. HTML).
///
/// Algorithm:
///   1. Parse host document with tree-sitter (e.g. tree_sitter_html).
///   2. Walk AST looking for `script_element` / `style_element` nodes.
///   3. For each block, extract raw_text child content.
///   4. Re-parse that content with the appropriate inner extractor.
///   5. Offset extracted node line numbers by the block's start line.
///   6. Merge all results into one ExtractionResult.
pub struct EmbeddedExtractor {
    host_language: tree_sitter::Language,
    host_config: LanguageConfig,
    /// (ast_node_kind, language_tag) pairs for embedded block detection.
    embedded_kinds: Vec<(&'static str, &'static str)>,
    /// Inner extractors keyed by language tag.
    inner_extractors: Vec<(String, Box<dyn super::Extractor>)>,
}

impl EmbeddedExtractor {
    /// Create an HTML embedded extractor that handles <script> and <style> blocks.
    pub fn html_embedded() -> Self {
        Self {
            host_language: tree_sitter_html::LANGUAGE.into(),
            host_config: html_config(),
            embedded_kinds: vec![("script_element", "javascript"), ("style_element", "css")],
            inner_extractors: vec![
                ("javascript".into(), Box::new(javascript_extractor())),
                ("css".into(), Box::new(css_extractor())),
            ],
        }
    }

    /// Find embedded blocks in the host AST.
    fn find_embedded_blocks(&self, source: &[u8]) -> Vec<EmbeddedBlock> {
        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&self.host_language).is_err() {
            return Vec::new();
        }
        let tree = match parser.parse(source, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut blocks = Vec::new();
        let mut stack = vec![tree.root_node()];

        while let Some(node) = stack.pop() {
            let kind = node.kind();

            // Check if this is an embedded block we care about
            let lang_opt = self
                .embedded_kinds
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, l)| *l);

            if let Some(lang_tag) = lang_opt {
                // For <script src="..."> (external), skip — no raw_text child
                // For <script type="..."> (inline), extract raw_text
                let content = self.extract_block_text(node, source, lang_tag);
                if let Some((text, line_offset)) = content {
                    if !text.trim().is_empty() {
                        blocks.push(EmbeddedBlock {
                            content: text,
                            line_offset,
                            lang: lang_tag.to_string(),
                        });
                    }
                }
                // Don't recurse into embedded blocks
                continue;
            }

            // Recurse into children
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                stack.push(child);
            }
        }

        blocks
    }

    /// Extract text from an embedded block node.
    /// Returns (content, line_offset_of_content_start) or None if external/empty.
    fn extract_block_text(
        &self,
        node: tree_sitter::Node<'_>,
        source: &[u8],
        _lang: &str,
    ) -> Option<(String, u32)> {
        // tree-sitter-html represents inline script/style content as a
        // `raw_text` child node inside script_element/style_element.
        // Check for `src` attribute (external script) — skip those.
        // We detect external scripts by checking if there's NO raw_text child.
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "raw_text" {
                let text = child.utf8_text(source).ok()?.to_string();
                let line_offset = child.start_position().row as u32;
                return Some((text, line_offset));
            }
        }
        None
    }
}

impl super::Extractor for EmbeddedExtractor {
    fn extract(
        &self,
        content: &[u8],
        file_id: &str,
    ) -> m1nd_core::error::M1ndResult<super::ExtractionResult> {
        let mut all_nodes = Vec::new();
        let mut all_edges = Vec::new();
        let mut all_refs = Vec::new();

        // File node (always present)
        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        let line_count = content.iter().filter(|&&b| b == b'\n').count() as u32;
        all_nodes.push(super::ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: m1nd_core::types::NodeType::File,
            tags: vec!["html".into()],
            line: 1,
            end_line: line_count.max(1),
        });

        // Find all embedded blocks
        let blocks = self.find_embedded_blocks(content);

        for block in &blocks {
            // Find the inner extractor for this language
            let extractor = self
                .inner_extractors
                .iter()
                .find(|(lang, _)| lang == &block.lang)
                .map(|(_, ext)| ext.as_ref());

            let extractor = match extractor {
                Some(e) => e,
                None => continue,
            };

            // Parse the embedded block content
            let block_bytes = block.content.as_bytes();
            let inner_result = match extractor.extract(block_bytes, file_id) {
                Ok(r) => r,
                Err(e) => {
                    // Graceful degradation: log and skip this block
                    eprintln!(
                        "[m1nd] EmbeddedExtractor: failed to parse {} block in {}: {}",
                        block.lang, file_id, e
                    );
                    continue;
                }
            };

            // Merge results, offsetting line numbers and skipping duplicate file node
            for node in inner_result.nodes {
                if node.node_type == m1nd_core::types::NodeType::File {
                    // Skip the inner file node — we already have one
                    continue;
                }
                // Offset line numbers by block start
                all_nodes.push(super::ExtractedNode {
                    line: node.line + block.line_offset,
                    end_line: node.end_line + block.line_offset,
                    ..node
                });
            }

            for edge in inner_result.edges {
                all_edges.push(edge);
            }
            for r in inner_result.unresolved_refs {
                if !all_refs.contains(&r) {
                    all_refs.push(r);
                }
            }
        }

        // Add containment edges from file to all top-level nodes
        for node in &all_nodes {
            if node.node_type != m1nd_core::types::NodeType::File {
                // Only add containment edge if one doesn't already exist
                let already_has_edge = all_edges
                    .iter()
                    .any(|e| e.target == node.id && e.relation == "contains");
                if !already_has_edge {
                    all_edges.push(super::ExtractedEdge {
                        source: file_id.to_string(),
                        target: node.id.clone(),
                        relation: "contains".into(),
                        weight: 1.0,
                    });
                }
            }
        }

        Ok(super::ExtractionResult {
            nodes: all_nodes,
            edges: all_edges,
            unresolved_refs: all_refs,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }
}

// ===========================================================================
// Tier 2 language configs + factory functions (8 languages)
// ===========================================================================
// All Tier 2 grammar crates use the new tree-sitter-language API (LanguageFn).
// No old-API crates allowed — they pull in separate tree-sitter C runtimes
// whose symbols collide with the main tree-sitter 0.24 runtime.
//
// Dropped: Dockerfile (tree-sitter-dockerfile 0.2 depends on tree-sitter 0.20)
// Replaced: tree-sitter-toml -> tree-sitter-toml-ng (same grammar, new API)
// Replaced: tree-sitter-sql -> tree-sitter-sequel (same grammar, new API)

/// Elixir language config.
/// Elixir's grammar represents def/defmodule as `call` nodes — the extractor
/// picks up function_signature patterns from the grammar's dedicated node kinds.
#[cfg(feature = "tier2")]
pub fn elixir_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "elixir",
        extensions: &["ex", "exs"],
        // Elixir uses `call` for everything, but the grammar has some
        // dedicated node types we can match on
        function_kinds: &["call"],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &["call"],
        import_kinds: &["call"],
        call_kinds: &[],
        name_field: "target",
        alt_name_fields: &["name"],
        name_from_first_child: true,
    }
}

/// Dart language config.
/// AST: class_declaration > identifier, function_signature > identifier.
#[cfg(feature = "tier2")]
pub fn dart_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "dart",
        extensions: &["dart"],
        function_kinds: &["function_signature", "method_signature"],
        class_kinds: &["class_declaration"],
        struct_kinds: &[],
        enum_kinds: &["enum_declaration"],
        type_kinds: &["mixin_declaration"],
        module_kinds: &[],
        import_kinds: &["import_or_export"],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        // class_declaration > identifier (first named child after skipping type_identifier)
        name_from_first_child: true,
    }
}

/// Zig language config.
#[cfg(feature = "tier2")]
pub fn zig_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "zig",
        extensions: &["zig"],
        function_kinds: &["function_declaration"],
        class_kinds: &[],
        struct_kinds: &["container_declaration"],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &["builtin_function"], // @import("...")
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        name_from_first_child: true,
    }
}

/// Haskell language config.
#[cfg(feature = "tier2")]
pub fn haskell_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "haskell",
        extensions: &["hs", "lhs"],
        function_kinds: &["function", "bind", "signature"],
        class_kinds: &["class_declaration"],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &["data_type", "newtype", "type_alias"],
        module_kinds: &["header"],
        import_kinds: &["import"],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &["module"],
        name_from_first_child: true,
    }
}

/// OCaml language config.
/// AST: value_definition > let_binding > value_name. The name is deep in children.
/// type_definition > type_binding > type_constructor.
#[cfg(feature = "tier2")]
pub fn ocaml_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "ocaml",
        extensions: &["ml", "mli"],
        function_kinds: &["value_definition"],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &["type_definition"],
        module_kinds: &["module_definition"],
        import_kinds: &["open_module"],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &["pattern"],
        // value_definition > let_binding > value_name — drill into children
        name_from_first_child: true,
    }
}

/// TOML language config (structural: tables as nodes).
/// toml-ng AST: table -> bare_key (first named child is the section name).
#[cfg(feature = "tier2")]
pub fn toml_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "toml",
        extensions: &["toml"],
        function_kinds: &[],
        class_kinds: &[],
        struct_kinds: &["table", "table_array_element"],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &[],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        // table's first named child is bare_key with the section name
        name_from_first_child: true,
    }
}

/// YAML language config (structural: limited extraction).
/// YAML has no functions or imports — we primarily extract the file node
/// for connectivity in the graph.
#[cfg(feature = "tier2")]
pub fn yaml_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "yaml",
        extensions: &["yml", "yaml"],
        function_kinds: &[],
        class_kinds: &[],
        struct_kinds: &[],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &[],
        import_kinds: &[],
        call_kinds: &[],
        name_field: "key",
        alt_name_fields: &[],
        name_from_first_child: false,
    }
}

/// SQL language config.
/// Uses tree-sitter-sequel grammar. AST: create_table -> object_reference -> identifier.
#[cfg(feature = "tier2")]
pub fn sql_config() -> LanguageConfig {
    LanguageConfig {
        lang_tag: "sql",
        extensions: &["sql"],
        function_kinds: &["create_function"],
        class_kinds: &[],
        struct_kinds: &["create_table", "create_view", "create_index"],
        enum_kinds: &[],
        type_kinds: &[],
        module_kinds: &["create_schema"],
        import_kinds: &[],
        call_kinds: &[],
        name_field: "name",
        alt_name_fields: &[],
        // create_table's named children include object_reference which has identifier
        name_from_first_child: true,
    }
}

// NOTE: Dockerfile extractor dropped — tree-sitter-dockerfile 0.2.0 depends on
// tree-sitter 0.20.10 (old C runtime) which causes symbol collisions with the
// main tree-sitter 0.24 runtime. No new-API Dockerfile grammar crate exists.
// Dockerfile files fall back to GenericExtractor.

// ---------------------------------------------------------------------------
// Tier 2 factory functions
// ---------------------------------------------------------------------------

/// Create an Elixir extractor.
#[cfg(feature = "tier2")]
pub fn elixir_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_elixir::LANGUAGE.into(), elixir_config())
}

/// Create a Dart extractor.
#[cfg(feature = "tier2")]
pub fn dart_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_dart::LANGUAGE.into(), dart_config())
}

/// Create a Zig extractor.
#[cfg(feature = "tier2")]
pub fn zig_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_zig::LANGUAGE.into(), zig_config())
}

/// Create a Haskell extractor.
#[cfg(feature = "tier2")]
pub fn haskell_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_haskell::LANGUAGE.into(), haskell_config())
}

/// Create an OCaml extractor.
#[cfg(feature = "tier2")]
pub fn ocaml_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_ocaml::LANGUAGE_OCAML.into(), ocaml_config())
}

/// Create a TOML extractor.
/// Uses tree-sitter-toml-ng (new API, no C symbol collisions).
#[cfg(feature = "tier2")]
pub fn toml_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_toml_ng::LANGUAGE.into(), toml_config())
}

/// Create a YAML extractor.
#[cfg(feature = "tier2")]
pub fn yaml_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_yaml::LANGUAGE.into(), yaml_config())
}

/// Create a SQL extractor.
/// Uses tree-sitter-sequel (new API, no C symbol collisions).
#[cfg(feature = "tier2")]
pub fn sql_extractor() -> TreeSitterExtractor {
    TreeSitterExtractor::new(tree_sitter_sequel::LANGUAGE.into(), sql_config())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Debug helper: dump all AST node kinds for a given source + language.
    #[allow(dead_code)]
    fn dump_ast(lang: Language, src: &[u8]) -> String {
        let mut parser = Parser::new();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let mut out = String::new();
        dump_node(&tree.root_node(), src, &mut out, 0);
        out
    }

    #[allow(dead_code)]
    fn dump_node(node: &Node, src: &[u8], out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = node.utf8_text(src).unwrap_or("?");
        let short_text = if text.len() > 60 { &text[..60] } else { text };
        out.push_str(&format!(
            "{}{} [{}..{}] {:?}\n",
            indent,
            node.kind(),
            node.start_position().row,
            node.end_position().row,
            short_text.replace('\n', "\\n")
        ));
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            dump_node(&child, src, out, depth + 1);
        }
    }

    /// A birth ceremony died on a REAL html file for this (2026-08-02, the HQ
    /// repo): an inline `<script>` is an import_kind, its structured targets
    /// come back empty, and the full-text fallback splits the script body on
    /// quotes — so a JS string literal `" "` became import target `" "`,
    /// edge `ref:: `, and the ingest validator refused the WHOLE payload.
    /// Every emitted reference must be non-whitespace; the degenerate ones are
    /// dropped, never emitted.
    #[test]
    fn html_inline_script_with_whitespace_string_literals_emits_no_blank_refs() {
        // The exact minimal shape bisected from the real file: a call chain
        // ending in `replace(/\s+/g," ")` inside a function body — the
        // structured collector finds no clean identifier, the full-text
        // fallback splits on quotes, and the `" "` literal becomes a target.
        let src = b"<!doctype html><html><head><script>\nfunction idOf(c){return c.querySelector(\".name\").textContent.trim().replace(/\\s+/g,\" \")}\n</script></head><body></body></html>";
        let ext = EmbeddedExtractor::html_embedded();
        let result = ext.extract(src, "file::page.html").unwrap();
        let blank: Vec<_> = result
            .edges
            .iter()
            .filter(|e| {
                e.target
                    .strip_prefix("ref::")
                    .is_some_and(|t| t.trim().is_empty())
            })
            .collect();
        assert!(
            blank.is_empty(),
            "no edge may carry a blank ref target; got {:?}",
            blank
                .iter()
                .map(|e| (&e.source, &e.target, &e.relation))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn c_extracts_function_and_struct() {
        let src = b"struct Point { int x; int y; };\nint add(int a, int b) { return a + b; }";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "add" && n.node_type == NodeType::Function),
            "Should extract C function. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Point" && n.node_type == NodeType::Struct),
            "Should extract C struct. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn c_extracts_enum() {
        let src = b"enum Color { RED, GREEN, BLUE };";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Color" && n.node_type == NodeType::Enum),
            "Should extract C enum. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn c_extracts_include() {
        let src = b"#include <stdio.h>\n#include \"myheader.h\"\nint main() { return 0; }";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();
        let import_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .collect();
        assert!(
            !import_edges.is_empty(),
            "Should have import edges for #include. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn cpp_extracts_class_and_namespace() {
        let src = b"namespace myns {\nclass Widget {\npublic:\n    void draw();\n};\n}";
        let ext = cpp_extractor();
        let result = ext.extract(src, "file::test.cpp").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "myns" && n.node_type == NodeType::Module),
            "Should extract C++ namespace. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Widget" && n.node_type == NodeType::Class),
            "Should extract C++ class. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn csharp_extracts_class_and_method() {
        let src = b"namespace MyApp {\n    public class UserService {\n        public void CreateUser(string name) { }\n    }\n}";
        let ext = csharp_extractor();
        let result = ext.extract(src, "file::UserService.cs").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "UserService" && n.node_type == NodeType::Class),
            "Should extract C# class. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "CreateUser" && n.node_type == NodeType::Function),
            "Should extract C# method. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ruby_extracts_class_and_method() {
        let src = b"class Dog\n  def bark\n    puts 'woof'\n  end\nend";
        let ext = ruby_extractor();
        let result = ext.extract(src, "file::dog.rb").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Dog" && n.node_type == NodeType::Class),
            "Should extract Ruby class. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "bark" && n.node_type == NodeType::Function),
            "Should extract Ruby method. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn ruby_extracts_module() {
        let src = b"module Animals\n  class Cat\n    def meow; end\n  end\nend";
        let ext = ruby_extractor();
        let result = ext.extract(src, "file::animals.rb").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Animals" && n.node_type == NodeType::Module),
            "Should extract Ruby module. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn php_extracts_class_and_function() {
        let src = b"<?php\nnamespace App;\nclass Controller {\n    public function index() { }\n}\nfunction helper() { }";
        let ext = php_extractor();
        let result = ext.extract(src, "file::Controller.php").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Controller" && n.node_type == NodeType::Class),
            "Should extract PHP class. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn swift_extracts_struct_and_func() {
        // Swift's tree-sitter grammar represents both struct and class as
        // class_declaration, so Point is extracted as Class (not Struct).
        let src = b"struct Point {\n    var x: Int\n    var y: Int\n}\n\nfunc add(_ a: Int, _ b: Int) -> Int {\n    return a + b\n}";
        let ext = swift_extractor();
        let result = ext.extract(src, "file::test.swift").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Point" && n.node_type == NodeType::Class),
            "Should extract Swift struct (as Class). Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "add" && n.node_type == NodeType::Function),
            "Should extract Swift function. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn swift_extracts_class_and_protocol() {
        let src = b"protocol Drawable {\n    func draw()\n}\n\nclass Circle: Drawable {\n    func draw() { }\n}";
        let ext = swift_extractor();
        let result = ext.extract(src, "file::test.swift").unwrap();
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Drawable" && n.node_type == NodeType::Type),
            "Should extract Swift protocol. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "Circle" && n.node_type == NodeType::Class),
            "Should extract Swift class. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bash_extracts_function() {
        let src = b"#!/bin/bash\n\nmy_func() {\n    echo 'hello'\n}\n\nfunction another_func {\n    echo 'world'\n}";
        let ext = bash_extractor();
        let result = ext.extract(src, "file::test.sh").unwrap();
        // At least one function should be found
        let funcs: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .collect();
        assert!(
            !funcs.is_empty(),
            "Should extract at least one bash function. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bash_imports_only_source_commands_not_arbitrary_quoted_commands() {
        let src =
            b"source ./env.sh\nprint -r -- \"$T\" | jq -r 'fromjson | \"\\(.id // \"?\")\"'\n";
        let result = bash_extractor().extract(src, "file::test.sh").unwrap();
        let imports = result
            .edges
            .iter()
            .filter(|edge| edge.relation == "imports")
            .collect::<Vec<_>>();
        assert!(!imports.is_empty(), "source command should emit an import");
        assert!(imports.iter().all(|edge| !edge.target.contains(".id")));
    }

    #[test]
    fn lua_extracts_function() {
        let src = b"function greet(name)\n    print('hello ' .. name)\nend\n\nlocal function helper()\n    return 42\nend";
        let ext = lua_extractor();
        let result = ext.extract(src, "file::test.lua").unwrap();
        let funcs: Vec<_> = result
            .nodes
            .iter()
            .filter(|n| n.node_type == NodeType::Function)
            .collect();
        assert!(
            !funcs.is_empty(),
            "Should extract Lua functions. Nodes: {:?}",
            result
                .nodes
                .iter()
                .map(|n| (&n.label, &n.node_type))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn file_node_always_present() {
        // Every tree-sitter extractor must produce a File node as the first node
        let extractors: Vec<(&str, Box<dyn Extractor>)> = vec![
            ("c", Box::new(c_extractor())),
            ("cpp", Box::new(cpp_extractor())),
            ("csharp", Box::new(csharp_extractor())),
            ("ruby", Box::new(ruby_extractor())),
            ("php", Box::new(php_extractor())),
            ("swift", Box::new(swift_extractor())),
            ("kotlin", Box::new(kotlin_extractor())),
            ("scala", Box::new(scala_extractor())),
            ("bash", Box::new(bash_extractor())),
            ("lua", Box::new(lua_extractor())),
            ("r", Box::new(r_extractor())),
            ("html", Box::new(html_extractor())),
            ("css", Box::new(css_extractor())),
            ("json", Box::new(json_extractor())),
        ];
        for (lang, ext) in extractors {
            let result = ext
                .extract(b"/* empty */", &format!("file::test.{}", lang))
                .unwrap();
            assert!(
                !result.nodes.is_empty(),
                "{} extractor should produce at least a file node",
                lang
            );
            assert_eq!(
                result.nodes[0].node_type,
                NodeType::File,
                "{} extractor first node should be File, got {:?}",
                lang,
                result.nodes[0].node_type
            );
        }
    }

    // ===================================================================
    // call_kinds pilot tests — C, C#, Kotlin
    // ===================================================================
    //
    // For each piloted grammar we assert:
    //   (a) expected `calls` edges appear with the correct callee names
    //   (b) no bogus `calls` edges (keyword/control-flow identifiers are not
    //       mistaken for callees; definitions are not emitted as calls)
    //
    // Node-kind verification method: AST dump via `dump_ast` helper (see above),
    // output inspected manually during development of this pilot.

    #[test]
    fn c_calls_simple_function() {
        // bar() calls foo() twice and baz() once.
        // Neither "int", "return", nor any keyword should appear as a callee.
        let src = b"int foo(int x) { return x; }\n\
                    int baz(int x) { return x * 2; }\n\
                    int bar(int a) { return foo(a) + baz(a); }";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) expected callees present
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo. call edges: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"baz"),
            "Should have calls edge to baz. call edges: {:?}",
            call_edges
        );

        // (b) no bogus callees — keywords / types must not appear
        let bogus = ["int", "return", "if", "while", "for"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear in calls edges. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn c_calls_struct_member() {
        // Member call via `->`: callee should be `draw`, not `self` or `node`.
        let src = b"struct Renderer { void (*draw)(void); };\n\
                    void render(struct Renderer* r) { r->draw(); }";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) method name extracted, not the receiver
        assert!(
            call_edges.contains(&"draw"),
            "Should have calls edge to 'draw' (field_identifier). Got: {:?}",
            call_edges
        );
        // (b) receiver name must not appear as callee
        assert!(
            !call_edges.contains(&"r"),
            "Receiver 'r' must not appear as callee. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn c_no_calls_from_definitions() {
        // A file with only definitions and no call sites must produce zero calls edges.
        let src = b"int square(int x) { return x * x; }\nstruct Point { int x; int y; };";
        let ext = c_extractor();
        let result = ext.extract(src, "file::test.c").unwrap();

        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected when no call sites present. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn csharp_calls_bare_and_qualified() {
        // Bar() calls Foo() (bare) and Console.WriteLine() (qualified).
        let src = b"class MyClass {\n\
                        void Foo() {}\n\
                        void Bar() {\n\
                            Foo();\n\
                            Console.WriteLine(\"hi\");\n\
                        }\n\
                    }";
        let ext = csharp_extractor();
        let result = ext.extract(src, "file::test.cs").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) both callees present
        assert!(
            call_edges.contains(&"Foo"),
            "Should have calls edge to Foo. Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"WriteLine"),
            "Should have calls edge to WriteLine (last ident of member_access). Got: {:?}",
            call_edges
        );

        // (b) no bogus callees — type names, keywords, receiver names
        let bogus = ["void", "class", "string", "Console", "MyClass"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear in calls edges. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn csharp_no_calls_from_class_definition() {
        // A class with only field declarations and no method bodies must produce zero calls edges.
        let src =
            b"class Config {\n    public int MaxRetries { get; set; }\n    public string Name;\n}";
        let ext = csharp_extractor();
        let result = ext.extract(src, "file::test.cs").unwrap();

        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from pure definition. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn kotlin_calls_simple_function() {
        // bar() calls foo() and println(); these should be captured.
        // The navigation_expression form (obj.method()) should NOT produce a callee
        // of "obj" or "method" erroneously — it is simply skipped.
        let src = b"fun foo(x: Int): Int = x\n\
                    fun bar(): Int {\n\
                        println(foo(42))\n\
                        return foo(1)\n\
                    }";
        let ext = kotlin_extractor();
        let result = ext.extract(src, "file::test.kt").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) simple calls must be extracted
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo. Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"println"),
            "Should have calls edge to println. Got: {:?}",
            call_edges
        );

        // (b) no bogus callees
        let bogus = ["fun", "return", "Int", "val", "var"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn kotlin_chained_calls_skipped_safely() {
        // A chained call like listOf(1).map { it } should NOT produce a bogus
        // callee of "listOf" attributed to "map" or vice versa in a confusing way.
        // We only expect `listOf` to appear (the first identifier child of the
        // outer navigation_expression's nested call_expression).
        let src = b"fun bar() {\n\
                        val x = listOf(1, 2, 3).map { it * 2 }\n\
                    }";
        let ext = kotlin_extractor();
        let result = ext.extract(src, "file::test.kt").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // "it" and "map" must not appear as callees (not simple call nodes)
        assert!(
            !call_edges.contains(&"it"),
            "'it' must not appear as a callee. Got: {:?}",
            call_edges
        );
        // No bogus keyword callees
        let bogus = ["fun", "val", "return"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn kotlin_no_calls_from_function_declaration() {
        // A function with no call sites must produce zero calls edges.
        let src = b"fun square(x: Int): Int = x * x";
        let ext = kotlin_extractor();
        let result = ext.extract(src, "file::test.kt").unwrap();

        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from pure definition. Got: {:?}",
            call_edges
        );
    }

    // ===================================================================
    // Temporary AST inspection tests — dump node kinds for verification.
    // These are #[ignore] so they never run in CI, but can be activated
    // with `cargo test -- --ignored` to inspect AST structure.
    // ===================================================================

    #[test]
    #[ignore]
    fn ast_dump_cpp_calls() {
        let src = b"void foo(int x) {}\nvoid bar() { foo(42); obj.method(); }";
        let dump = dump_ast(tree_sitter_cpp::LANGUAGE.into(), src);
        println!("=== C++ AST ===\n{}", dump);
    }

    #[test]
    #[ignore]
    fn ast_dump_php_calls() {
        let src = b"<?php\nfunction foo() {}\nfunction bar() { foo(); $obj->method(); SomeClass::staticMethod(); }";
        let dump = dump_ast(tree_sitter_php::LANGUAGE_PHP.into(), src);
        println!("=== PHP AST ===\n{}", dump);
    }

    #[test]
    #[ignore]
    fn ast_dump_scala_calls() {
        let src = b"def foo(): Int = 1\ndef bar(): Int = { foo(); baz(42) }";
        let dump = dump_ast(tree_sitter_scala::LANGUAGE.into(), src);
        println!("=== Scala AST ===\n{}", dump);
    }

    #[test]
    #[ignore]
    fn ast_dump_swift_calls() {
        let src = b"func foo() -> Int { return 1 }\nfunc bar() { foo(); obj.method() }";
        let dump = dump_ast(tree_sitter_swift::LANGUAGE.into(), src);
        println!("=== Swift AST ===\n{}", dump);
    }

    #[test]
    #[ignore]
    fn ast_dump_swift_navigation_detail() {
        // Check detailed structure of navigation_expression for method extraction
        let src = b"func bar() { obj.method(); a.b.c() }";
        let dump = dump_ast(tree_sitter_swift::LANGUAGE.into(), src);
        println!("=== Swift navigation detail ===\n{}", dump);
    }

    #[test]
    #[ignore]
    fn ast_dump_ruby_calls() {
        // Verify the exact node kind + children for Ruby call nodes.
        // Expected: `call` for method calls, with `method` field child = identifier.
        // Bare calls without parens (like `foo`) may parse as `identifier`.
        let src = b"def run\n  foo\n  obj.bar(1)\n  Helper.process(x)\nend";
        let dump = dump_ast(tree_sitter_ruby::LANGUAGE.into(), src);
        println!("=== Ruby calls AST ===\n{}", dump);

        // Also verify via field-based access that `method` field exists on `call`.
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        // Walk down to the `call` node (obj.bar(1)) and inspect its `method` field.
        let root = tree.root_node();
        // root > method(def run) > body_statement > call(obj.bar(1))
        let method_def = root.named_child(0).unwrap(); // method def `run`
        let body = method_def
            .child_by_field_name("body")
            .or_else(|| method_def.named_child(1))
            .unwrap();
        println!("=== body node kind: {} ===", body.kind());
        let mut cursor = body.walk();
        for (i, child) in body.named_children(&mut cursor).enumerate() {
            println!(
                "  body child[{}]: kind={} text={:?}",
                i,
                child.kind(),
                child.utf8_text(src).unwrap_or("?")
            );
            if child.kind() == "call" {
                // Check `method` field
                let mf = child.child_by_field_name("method");
                let rf = child.child_by_field_name("receiver");
                println!(
                    "    -> `method` field: {:?}",
                    mf.map(|n| (n.kind(), n.utf8_text(src).unwrap_or("?")))
                );
                println!(
                    "    -> `receiver` field: {:?}",
                    rf.map(|n| (n.kind(), n.utf8_text(src).unwrap_or("?")))
                );
                let mut nc = child.walk();
                for (j, c2) in child.named_children(&mut nc).enumerate() {
                    println!(
                        "    named_child[{}]: kind={} text={:?}",
                        j,
                        c2.kind(),
                        c2.utf8_text(src).unwrap_or("?")
                    );
                }
            }
        }
    }

    // ===================================================================
    // call_kinds tests — C++, PHP, Scala, Swift (new pilots)
    // ===================================================================

    #[test]
    fn cpp_calls_simple_function() {
        // bar() calls foo() (simple) and obj.method() (member via field_expression).
        let src = b"void foo(int x) {}\nvoid bar() { foo(42); obj.method(); }";
        let ext = cpp_extractor();
        let result = ext.extract(src, "file::test.cpp").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) expected callees present
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo. call edges: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"method"),
            "Should have calls edge to 'method' (field_expression → field_identifier). Got: {:?}",
            call_edges
        );

        // (b) no bogus callees
        let bogus = ["void", "int", "return", "if", "obj"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn cpp_no_calls_from_class_definition() {
        // Class + method declarations only — no call sites.
        let src = b"namespace myns {\nclass Widget {\npublic:\n    void draw();\n};\n}";
        let ext = cpp_extractor();
        let result = ext.extract(src, "file::test.cpp").unwrap();
        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from C++ class definition only. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn php_calls_function_and_member() {
        // bar() calls foo() (function_call_expression), $obj->method()
        // (member_call_expression), and SomeClass::staticMethod()
        // (scoped_call_expression).
        let src = b"<?php\nfunction foo() {}\nfunction bar() { foo(); $obj->method(); SomeClass::staticMethod(); }";
        let ext = php_extractor();
        let result = ext.extract(src, "file::test.php").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) all three callee forms present
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo (function_call_expression). Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"method"),
            "Should have calls edge to 'method' (member_call_expression). Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"staticMethod"),
            "Should have calls edge to 'staticMethod' (scoped_call_expression). Got: {:?}",
            call_edges
        );

        // (b) receiver names and keywords must not appear
        let bogus = ["SomeClass", "obj", "function", "php"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn php_no_calls_from_class_definition() {
        // A PHP class with only property declarations — no call sites.
        let src = b"<?php\nclass Config {\n    public int $maxRetries = 3;\n    public string $name = 'app';\n}";
        let ext = php_extractor();
        let result = ext.extract(src, "file::Config.php").unwrap();
        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from PHP class definition only. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn scala_calls_simple_function() {
        // bar() calls foo() and baz(42) — both are `call_expression` with
        // `identifier` first child.
        let src =
            b"def foo(): Int = 1\ndef baz(x: Int): Int = x\ndef bar(): Int = { foo(); baz(42) }";
        let ext = scala_extractor();
        let result = ext.extract(src, "file::test.scala").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) expected callees
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo. Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"baz"),
            "Should have calls edge to baz. Got: {:?}",
            call_edges
        );

        // (b) no bogus callees
        let bogus = ["def", "Int", "val", "return"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn scala_no_calls_from_function_definition() {
        // A Scala function with only a literal body — no call sites.
        let src = b"def square(x: Int): Int = x * x";
        let ext = scala_extractor();
        let result = ext.extract(src, "file::test.scala").unwrap();
        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from Scala pure definition. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn swift_calls_bare_and_navigation() {
        // bar() calls foo() (simple_identifier) and obj.method() (navigation_expression).
        let src = b"func foo() -> Int { return 1 }\nfunc bar() { foo(); obj.method() }";
        let ext = swift_extractor();
        let result = ext.extract(src, "file::test.swift").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) bare call extracted
        assert!(
            call_edges.contains(&"foo"),
            "Should have calls edge to foo. Got: {:?}",
            call_edges
        );
        // (b) navigation call: trailing method name extracted
        assert!(
            call_edges.contains(&"method"),
            "Should have calls edge to 'method' (navigation_expression → navigation_suffix). Got: {:?}",
            call_edges
        );

        // (c) receiver and keywords must not appear
        let bogus = ["obj", "func", "return", "Int", "let", "var"];
        for bad in bogus {
            assert!(
                !call_edges.contains(&bad),
                "Bogus callee '{}' must not appear. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn swift_calls_deep_chain() {
        // `a.b.c()` — deep navigation chain. The terminal identifier `c` should
        // be the callee, not `a` or `b`.
        let src = b"func bar() { a.b.c() }";
        let ext = swift_extractor();
        let result = ext.extract(src, "file::test.swift").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        assert!(
            call_edges.contains(&"c"),
            "Deep chain a.b.c() should extract 'c' as callee. Got: {:?}",
            call_edges
        );
        // Intermediate chain elements must not appear
        assert!(
            !call_edges.contains(&"a"),
            "'a' must not appear as callee in chain. Got: {:?}",
            call_edges
        );
        assert!(
            !call_edges.contains(&"b"),
            "'b' must not appear as callee in chain. Got: {:?}",
            call_edges
        );
    }

    #[test]
    fn swift_no_calls_from_struct_definition() {
        // A Swift struct/class with only stored properties — no call sites.
        let src = b"struct Point {\n    var x: Int\n    var y: Int\n}";
        let ext = swift_extractor();
        let result = ext.extract(src, "file::test.swift").unwrap();
        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            call_edges.is_empty(),
            "No calls edges expected from Swift struct definition only. Got: {:?}",
            call_edges
        );
    }

    // ===================================================================
    // Ruby call_kinds tests — verified via AST dump (ast_dump_ruby_calls)
    // ===================================================================

    #[test]
    fn ruby_calls_emit_edges() {
        // Verify that receiver.method() calls produce `calls` edges to the method name,
        // NOT the receiver. Both `identifier` receivers (obj) and `constant` receivers
        // (Helper) must work. Bare `foo` without parens parses as identifier (not `call`)
        // and is intentionally not captured — only actual `call` nodes are emitted.
        let src = b"def run\n  obj.bar(1)\n  Helper.process(x)\nend";
        let ext = ruby_extractor();
        let result = ext.extract(src, "file::test.rb").unwrap();

        let call_edges: Vec<&str> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| e.target.strip_prefix("ref::").unwrap_or(&e.target))
            .collect();

        // (a) method names extracted (not receivers)
        assert!(
            call_edges.contains(&"bar"),
            "Should have calls edge to 'bar' (method field, not receiver 'obj'). Got: {:?}",
            call_edges
        );
        assert!(
            call_edges.contains(&"process"),
            "Should have calls edge to 'process' (method field, not receiver 'Helper'). Got: {:?}",
            call_edges
        );

        // (b) receivers must NOT appear as callees
        let bogus_receivers = ["obj", "Helper"];
        for bad in bogus_receivers {
            assert!(
                !call_edges.contains(&bad),
                "Receiver '{}' must not appear as callee. Got: {:?}",
                bad,
                call_edges
            );
        }

        // (c) Ruby keywords and control-flow must not appear
        let bogus_keywords = ["def", "end", "if", "while", "return", "do"];
        for bad in bogus_keywords {
            assert!(
                !call_edges.contains(&bad),
                "Keyword '{}' must not appear as callee. Got: {:?}",
                bad,
                call_edges
            );
        }
    }

    #[test]
    fn ruby_no_calls_from_def() {
        // A method definition (`def foo ... end`) must NOT produce a self-call edge to `foo`.
        // `def` parses as `method` node kind, never as `call`, so no edge should appear.
        let src = b"def foo\n  42\nend\n\ndef bar\n  99\nend";
        let ext = ruby_extractor();
        let result = ext.extract(src, "file::test.rb").unwrap();

        let call_edges: Vec<_> = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();

        assert!(
            call_edges.is_empty(),
            "def definitions must not produce self-call edges. Got: {:?}",
            call_edges
        );
    }

    // Verify non-piloted languages (Bash, Lua, R) still produce
    // zero calls edges — they must be unchanged by this expansion.
    // Ruby, PHP, Scala, and Swift have been graduated to piloted status above.
    #[test]
    fn non_piloted_languages_emit_no_calls_edges() {
        let cases: &[(&str, Box<dyn Extractor>, &[u8])] = &[
            (
                "bash",
                Box::new(bash_extractor()),
                b"foo() { echo hi; }\nbar() { foo; }",
            ),
            (
                "lua",
                Box::new(lua_extractor()),
                b"function foo() end\nfunction bar() foo() end",
            ),
        ];
        for (lang, ext, src) in cases {
            let result = ext.extract(src, &format!("file::test.{}", lang)).unwrap();
            let call_edges: Vec<_> = result
                .edges
                .iter()
                .filter(|e| e.relation == "calls")
                .collect();
            assert!(
                call_edges.is_empty(),
                "Non-piloted language '{}' must produce no calls edges. Got: {:?}",
                lang,
                call_edges
            );
        }
    }

    #[test]
    fn containment_edges_have_correct_parent() {
        // Verify that a class containing a method produces correct containment
        let src = b"class Dog\n  def bark\n    puts 'woof'\n  end\nend";
        let ext = ruby_extractor();
        let result = ext.extract(src, "file::dog.rb").unwrap();

        // Find the Dog class node
        let dog_node = result.nodes.iter().find(|n| n.label == "Dog").unwrap();
        // Find containment edge from file to Dog
        let file_to_dog = result
            .edges
            .iter()
            .find(|e| e.target == dog_node.id && e.relation == "contains");
        assert!(
            file_to_dog.is_some(),
            "Should have contains edge to Dog class"
        );

        // Find the bark method node
        let bark_node = result.nodes.iter().find(|n| n.label == "bark").unwrap();
        // Find containment edge from Dog to bark
        let dog_to_bark = result.edges.iter().find(|e| {
            e.target == bark_node.id && e.relation == "contains" && e.source == dog_node.id
        });
        assert!(
            dog_to_bark.is_some(),
            "Should have contains edge from Dog to bark. Edges: {:?}",
            result
                .edges
                .iter()
                .filter(|e| e.relation == "contains")
                .collect::<Vec<_>>()
        );
    }

    // ===================================================================
    // Tier 2 language tests
    // ===================================================================

    #[cfg(feature = "tier2")]
    mod tier2 {
        use super::*;

        // -- Dart --
        #[test]
        fn dart_extracts_class_and_methods() {
            let src = b"class Worker {\n  void process(String data) {}\n  int _helper(int x) => x * 2;\n}";
            let ext = dart_extractor();
            let result = ext.extract(src, "file::worker.dart").unwrap();
            assert!(
                result
                    .nodes
                    .iter()
                    .any(|n| n.label == "Worker" && n.node_type == NodeType::Class),
                "Should extract Dart class. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn dart_file_node_first() {
            let ext = dart_extractor();
            let result = ext.extract(b"class X {}", "file::test.dart").unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // -- Zig --
        #[test]
        fn zig_extracts_functions() {
            let src = b"pub fn main() void {}\n\nfn helper(x: i32) i32 {\n    return x * 2;\n}";
            let ext = zig_extractor();
            let result = ext.extract(src, "file::main.zig").unwrap();
            assert!(
                result
                    .nodes
                    .iter()
                    .any(|n| n.label == "main" && n.node_type == NodeType::Function),
                "Should extract Zig pub fn. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
            assert!(
                result
                    .nodes
                    .iter()
                    .any(|n| n.label == "helper" && n.node_type == NodeType::Function),
                "Should extract Zig private fn"
            );
        }

        #[test]
        fn zig_extracts_import() {
            let src = b"const std = @import(\"std\");";
            let ext = zig_extractor();
            let result = ext.extract(src, "file::main.zig").unwrap();
            let import_edges: Vec<_> = result
                .edges
                .iter()
                .filter(|e| e.relation == "imports")
                .collect();
            assert!(
                !import_edges.is_empty(),
                "Zig @import should produce import edge. All edges: {:?}",
                result
                    .edges
                    .iter()
                    .map(|e| (&e.relation, &e.target))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn zig_file_node_first() {
            let ext = zig_extractor();
            let result = ext.extract(b"fn x() void {}", "file::test.zig").unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // -- Haskell --
        #[test]
        fn haskell_extracts_functions() {
            let src = b"module Main where\n\nmain :: IO ()\nmain = putStrLn \"hello\"\n\nhelper :: Int -> Int\nhelper x = x * 2";
            let ext = haskell_extractor();
            let result = ext.extract(src, "file::Main.hs").unwrap();
            // Should extract at least one function (main or helper)
            let fns: Vec<_> = result
                .nodes
                .iter()
                .filter(|n| n.node_type == NodeType::Function)
                .collect();
            assert!(
                !fns.is_empty(),
                "Haskell should extract functions. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn haskell_extracts_data_type() {
            let src = b"module Main where\n\ndata Color = Red | Blue | Green";
            let ext = haskell_extractor();
            let result = ext.extract(src, "file::Types.hs").unwrap();
            let types: Vec<_> = result
                .nodes
                .iter()
                .filter(|n| n.node_type == NodeType::Type)
                .collect();
            assert!(
                !types.is_empty(),
                "Haskell should extract data types. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn haskell_extracts_imports() {
            let src = b"module Main where\n\nimport Data.List\nimport qualified Data.Map as Map\n\nmain = return ()";
            let ext = haskell_extractor();
            let result = ext.extract(src, "file::Main.hs").unwrap();
            let import_edges: Vec<_> = result
                .edges
                .iter()
                .filter(|e| e.relation == "imports")
                .collect();
            assert!(
                !import_edges.is_empty(),
                "Haskell should have import edges. All edges: {:?}",
                result
                    .edges
                    .iter()
                    .map(|e| (&e.relation, &e.target))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn haskell_file_node_first() {
            let ext = haskell_extractor();
            let result = ext.extract(b"module Main where", "file::Main.hs").unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // -- OCaml --
        #[test]
        fn ocaml_extracts_let_bindings() {
            let src = b"let main () =\n  print_endline \"hello\"\n\nlet helper x = x * 2";
            let ext = ocaml_extractor();
            let result = ext.extract(src, "file::main.ml").unwrap();
            let fns: Vec<_> = result
                .nodes
                .iter()
                .filter(|n| n.node_type == NodeType::Function)
                .collect();
            assert!(
                !fns.is_empty(),
                "OCaml should extract let bindings as functions. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn ocaml_extracts_type_definition() {
            let src = b"type color = Red | Blue | Green";
            let ext = ocaml_extractor();
            let result = ext.extract(src, "file::types.ml").unwrap();
            let types: Vec<_> = result
                .nodes
                .iter()
                .filter(|n| n.node_type == NodeType::Type)
                .collect();
            assert!(
                !types.is_empty(),
                "OCaml should extract type definitions. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn ocaml_extracts_open() {
            let src = b"open Printf\n\nlet main () = printf \"hello\\n\"";
            let ext = ocaml_extractor();
            let result = ext.extract(src, "file::main.ml").unwrap();
            let import_edges: Vec<_> = result
                .edges
                .iter()
                .filter(|e| e.relation == "imports")
                .collect();
            assert!(
                !import_edges.is_empty(),
                "OCaml 'open' should produce import edge. All edges: {:?}",
                result
                    .edges
                    .iter()
                    .map(|e| (&e.relation, &e.target))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn ocaml_file_node_first() {
            let ext = ocaml_extractor();
            let result = ext.extract(b"let x = 1", "file::test.ml").unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // -- TOML --
        #[test]
        fn toml_extracts_tables() {
            let src = b"[package]\nname = \"myapp\"\n\n[dependencies]\nserde = \"1.0\"";
            let ext = toml_extractor();
            let result = ext.extract(src, "file::Cargo.toml").unwrap();
            assert!(
                result.nodes.iter().any(|n| n.label == "package"),
                "Should extract [package] table. Nodes: {:?}",
                result.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
            );
            assert!(
                result.nodes.iter().any(|n| n.label == "dependencies"),
                "Should extract [dependencies] table. Nodes: {:?}",
                result.nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
            );
        }

        #[test]
        fn toml_repeated_array_tables_receive_distinct_ids() {
            let src = b"[[bin]]\nname = \"one\"\npath = \"src/one.rs\"\n\n[[bin]]\nname = \"two\"\npath = \"src/two.rs\"\n";
            let result = toml_extractor().extract(src, "file::Cargo.toml").unwrap();
            let bins = result
                .nodes
                .iter()
                .filter(|node| node.label == "bin")
                .collect::<Vec<_>>();
            assert_eq!(bins.len(), 2);
            assert_ne!(bins[0].id, bins[1].id);
        }

        #[test]
        fn toml_file_node_first() {
            let ext = toml_extractor();
            let result = ext
                .extract(b"[section]\nkey = \"val\"", "file::test.toml")
                .unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // -- YAML --
        #[test]
        fn yaml_file_node_present() {
            let src = b"key: value\nlist:\n  - item1\n  - item2";
            let ext = yaml_extractor();
            let result = ext.extract(src, "file::config.yaml").unwrap();
            assert_eq!(
                result.nodes[0].node_type,
                NodeType::File,
                "YAML extractor should produce File node first"
            );
        }

        // -- SQL --
        #[test]
        fn sql_extracts_create_table() {
            let src = b"CREATE TABLE users (\n  id INTEGER PRIMARY KEY,\n  name TEXT NOT NULL\n);";
            let ext = sql_extractor();
            let result = ext.extract(src, "file::schema.sql").unwrap();
            assert!(
                result
                    .nodes
                    .iter()
                    .any(|n| n.label == "users" && n.node_type == NodeType::Struct),
                "Should extract CREATE TABLE as struct. Nodes: {:?}",
                result
                    .nodes
                    .iter()
                    .map(|n| (&n.label, &n.node_type))
                    .collect::<Vec<_>>()
            );
        }

        #[test]
        fn sql_file_node_first() {
            let ext = sql_extractor();
            let result = ext.extract(b"SELECT 1;", "file::test.sql").unwrap();
            assert_eq!(result.nodes[0].node_type, NodeType::File);
        }

        // NOTE: Dockerfile tests removed — grammar crate dropped due to
        // C symbol collisions (see Cargo.toml comment).

        // -- Integration tests --
        #[test]
        fn tier2_file_node_always_present() {
            let extractors: Vec<(&str, Box<dyn Extractor>, &[u8])> = vec![
                ("dart", Box::new(dart_extractor()), b"class X {}" as &[u8]),
                ("zig", Box::new(zig_extractor()), b"fn x() void {}"),
                (
                    "haskell",
                    Box::new(haskell_extractor()),
                    b"module Main where",
                ),
                ("ocaml", Box::new(ocaml_extractor()), b"let x = 1"),
                ("toml", Box::new(toml_extractor()), b"[section]"),
                ("yaml", Box::new(yaml_extractor()), b"key: value"),
                ("sql", Box::new(sql_extractor()), b"SELECT 1;"),
            ];
            for (lang, ext, src) in extractors {
                let result = ext.extract(src, &format!("file::test.{}", lang)).unwrap();
                assert!(
                    !result.nodes.is_empty(),
                    "{} extractor should produce at least a file node",
                    lang
                );
                assert_eq!(
                    result.nodes[0].node_type,
                    NodeType::File,
                    "{} extractor first node should be File, got {:?}",
                    lang,
                    result.nodes[0].node_type
                );
            }
        }

        // -- Verify existing regex extractors are NOT affected --
        #[test]
        fn regex_extractors_unchanged_by_tier2() {
            use crate::extract::go::GoExtractor;
            use crate::extract::java::JavaExtractor;
            use crate::extract::python::PythonExtractor;
            use crate::extract::rust_lang::RustExtractor;
            use crate::extract::typescript::TypeScriptExtractor;

            // Python
            let py = PythonExtractor::new();
            let r = py
                .extract(b"class Foo:\n    def bar(self): pass", "file::t.py")
                .unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "Foo" && n.node_type == NodeType::Class));
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "bar" && n.node_type == NodeType::Function));

            // Rust
            let rs = RustExtractor::new();
            let r = rs.extract(b"pub fn hello() {}", "file::t.rs").unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "hello" && n.node_type == NodeType::Function));

            // TypeScript
            let ts = TypeScriptExtractor::new();
            let r = ts
                .extract(b"export function greet() {}", "file::t.ts")
                .unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "greet" && n.node_type == NodeType::Function));

            // Go
            let go = GoExtractor::new();
            let r = go
                .extract(b"package main\nfunc main() {}", "file::main.go")
                .unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "main" && n.node_type == NodeType::Function));

            // Java
            let java = JavaExtractor::new();
            let r = java
                .extract(
                    b"public class App { public void run() {} }",
                    "file::App.java",
                )
                .unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "App" && n.node_type == NodeType::Class));
        }

        // -- Verify generic fallback still works --
        #[test]
        fn generic_fallback_still_works() {
            use crate::extract::generic::GenericExtractor;
            let ext = GenericExtractor::new();
            let r = ext
                .extract(
                    b"def helper():\n    pass\nclass Widget:\n    pass",
                    "file::unknown.xyz",
                )
                .unwrap();
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "helper" && n.node_type == NodeType::Function));
            assert!(r
                .nodes
                .iter()
                .any(|n| n.label == "Widget" && n.node_type == NodeType::Struct));
        }
    }
}
