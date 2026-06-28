// === crates/m1nd-ingest/src/extract/typescript.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;

/// TypeScript/JavaScript extractor using regex.
/// Replaces: ingest.py TypeScriptExtractor
pub struct TypeScriptExtractor {
    re_func: Regex,
    re_class: Regex,
    re_interface: Regex,
    re_import: Regex,
    re_arrow: Regex,
    re_import_names: Regex, // Extract named imports: import { A, B } from ...
    re_type_ref: Regex,     // TypeScript type references
    re_method_call: Regex,  // receiver.method() calls (mirrors Python Task #7)
    re_fn_call: Regex,      // plain function calls: foo(
    re_method_def: Regex,   // class/object method definition opening a body: `foo(...) {`
}

impl TypeScriptExtractor {
    pub fn new() -> Self {
        Self {
            re_func: Regex::new(r"^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*[<(]").unwrap(),
            re_class: Regex::new(r"^\s*(?:export\s+)?(?:abstract\s+)?class\s+(\w+)").unwrap(),
            re_interface: Regex::new(r"^\s*(?:export\s+)?interface\s+(\w+)").unwrap(),
            re_import: Regex::new(r#"^\s*import\s+.*from\s+['"]([@\w./\-]+)['"]"#).unwrap(),
            re_arrow: Regex::new(r"^\s*(?:export\s+)?(?:const|let|var)\s+(\w+)\s*=\s*(?:async\s+)?(?:\([^)]*\)|[^=])\s*=>").unwrap(),
            re_import_names: Regex::new(r#"import\s*\{([^}]+)\}"#).unwrap(),
            re_type_ref: Regex::new(r":\s*([A-Z]\w+)").unwrap(),
            // Task: receiver.method() calls — mirrors Python re_method_call
            re_method_call: Regex::new(r"(\w+)\.(\w+)\s*\(").unwrap(),
            // Plain function calls: identifier( — used to detect bare calls like foo()
            re_fn_call: Regex::new(r"\b([a-z_]\w*)\s*\(").unwrap(),
            // Class/object method definition whose body brace opens on this line:
            // `name(args) {`, `async name(args) {`, `name(args): RetType {`,
            // `get name() {`, `static name() {`. Anchored at line start (after
            // indentation) and requiring a trailing `{` so call sites like
            // `foo(bar);` never match. Control-flow keywords are filtered in code.
            re_method_def: Regex::new(
                r"^\s*(?:public\s+|private\s+|protected\s+|static\s+|readonly\s+|abstract\s+|override\s+|async\s+|get\s+|set\s+|\*\s*)*([A-Za-z_$][\w$]*)\s*(?:<[^>]*>)?\s*\([^;]*\)\s*(?::\s*[^{};]+)?\s*\{\s*$",
            )
            .unwrap(),
        }
    }

    /// True if `name` is a control-flow / non-method keyword that can read like a
    /// method definition (`if (...) {`, `for (...) {`, `switch (...) {`, …). These
    /// must NOT be treated as enclosing functions. `function`/`class`/etc. are
    /// handled by their own branches before re_method_def, but listing them here is
    /// harmless and keeps the guard self-contained.
    fn is_control_keyword(name: &str) -> bool {
        matches!(
            name,
            "if" | "for"
                | "while"
                | "switch"
                | "catch"
                | "do"
                | "else"
                | "try"
                | "finally"
                | "with"
                | "return"
                | "function"
                | "class"
                | "interface"
                | "new"
                | "typeof"
                | "await"
                | "yield"
        )
    }
}

impl Default for TypeScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for TypeScriptExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let text = String::from_utf8_lossy(content);
        let cleaned_lines = strip_comments_and_strings(&text, CommentSyntax::C_STYLE);
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut unresolved_refs = Vec::new();

        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: vec!["typescript".into()],
            line: 1,
            end_line: text.lines().count() as u32,
        });

        // Enclosing-function tracking (mirrors rust_lang.rs). `calls` edges below
        // are sourced from the top of `fn_stack` so they read FUNCTION -> ref::callee
        // (function granularity) instead of file -> ref::callee. A stack handles
        // nested functions/closures and methods inside classes; we pop when
        // `brace_depth` falls back to/below the depth the body opened at. `pending_fn`
        // latches a function whose signature was seen but whose body `{` has not
        // opened yet (multi-line signatures), promoted to the stack on the line the
        // body brace finally opens.
        let mut brace_depth: i32 = 0;
        let mut fn_stack: Vec<(String, i32)> = Vec::new();
        let mut pending_fn: Option<(String, i32)> = None;

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;

            // Brace bookkeeping for this line. `depth_after` is the depth once this
            // line's braces apply, used to pop functions whose body has now closed.
            let open_count = line.chars().filter(|&c| c == '{').count() as i32;
            let close_count = line.chars().filter(|&c| c == '}').count() as i32;
            let depth_after = brace_depth + open_count - close_count;

            // Pop any enclosing functions whose body has now closed.
            while let Some((_, open_depth)) = fn_stack.last() {
                if depth_after <= *open_depth {
                    fn_stack.pop();
                } else {
                    break;
                }
            }

            if let Some(caps) = self.re_class.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::class::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Class,
                    tags: vec!["typescript".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
            } else if let Some(caps) = self.re_interface.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::interface::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Type,
                    tags: vec!["typescript".into(), "interface".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
            } else if let Some(caps) = self.re_func.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                // Disambiguate same-named defs in one file (TS function overloads
                // with bodies, or two class methods named `process`) so add_node
                // does not drop the sibling. First keeps the clean id.
                let node_id = super::unique_node_id(&nodes, &format!("{}::fn::{}", file_id, name));
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["typescript".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id.clone(),
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Become the enclosing function once the body `{` opens. Baseline is
                // the depth before this line; the body raises depth above it.
                pending_fn = Some((node_id, brace_depth));
            } else if let Some(caps) = self.re_arrow.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = super::unique_node_id(&nodes, &format!("{}::fn::{}", file_id, name));
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["typescript".into(), "arrow".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id.clone(),
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // An arrow assigned to a name (`const foo = (…) => {`) becomes the
                // enclosing scope when its block body opens. A single-expression
                // arrow (`const f = x => x+1;`) opens no block, so the pending latch
                // is dropped at end of line (depth does not rise) — its inline call
                // sites are then attributed to the OUTER scope, which is acceptable.
                pending_fn = Some((node_id, brace_depth));
            } else if let Some(caps) = self.re_method_def.captures(line) {
                // Class/object method definition (`name(args) {`). Not matched by
                // re_func (no `function` keyword) or re_arrow. Guard against
                // control-flow that also reads `kw (...) {` — those are NOT methods.
                let name = caps.get(1).unwrap().as_str();
                if !Self::is_control_keyword(name) {
                    let node_id =
                        super::unique_node_id(&nodes, &format!("{}::fn::{}", file_id, name));
                    nodes.push(ExtractedNode {
                        id: node_id.clone(),
                        label: name.to_string(),
                        node_type: NodeType::Function,
                        tags: vec!["typescript".into(), "method".into()],
                        line: ln,
                        end_line: ln,
                    });
                    edges.push(ExtractedEdge {
                        source: file_id.to_string(),
                        target: node_id.clone(),
                        relation: "contains".into(),
                        weight: 1.0,
                    });
                    pending_fn = Some((node_id, brace_depth));
                }
            }

            if let Some(caps) = self.re_import.captures(line) {
                let module = caps.get(1).unwrap().as_str();
                let ref_id = format!("ref::{}", module);
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: ref_id.clone(),
                    relation: "imports".into(),
                    weight: 0.5,
                });
                unresolved_refs.push(ref_id);

                // Also extract named imports: import { Foo, Bar } from '...'
                if let Some(names) = self.re_import_names.captures(line) {
                    let names_str = names.get(1).unwrap().as_str();
                    for name in names_str.split(',') {
                        let name = name.trim().split(" as ").next().unwrap_or("").trim();
                        if !name.is_empty() {
                            let ref_id = format!("ref::{}", name);
                            if !unresolved_refs.contains(&ref_id) {
                                edges.push(ExtractedEdge {
                                    source: file_id.to_string(),
                                    target: ref_id.clone(),
                                    relation: "imports".into(),
                                    weight: 0.5,
                                });
                                unresolved_refs.push(ref_id);
                            }
                        }
                    }
                }
            }

            // Type references in annotations (: TypeName)
            // Comments already stripped by pre-processor
            if !line.trim_start().starts_with("import") {
                for caps in self.re_type_ref.captures_iter(line) {
                    let type_name = caps.get(1).unwrap().as_str();
                    if !matches!(
                        type_name,
                        "String"
                            | "Number"
                            | "Boolean"
                            | "Promise"
                            | "Array"
                            | "Record"
                            | "Partial"
                            | "Required"
                            | "Readonly"
                            | "Map"
                            | "Set"
                            | "Date"
                            | "Error"
                            | "Function"
                            | "Object"
                            | "Omit"
                            | "Pick"
                    ) {
                        let ref_id = format!("ref::{}", type_name);
                        if !unresolved_refs.contains(&ref_id) {
                            edges.push(ExtractedEdge {
                                source: file_id.to_string(),
                                target: ref_id.clone(),
                                relation: "references".into(),
                                weight: 0.3,
                            });
                            unresolved_refs.push(ref_id);
                        }
                    }
                }
            }

            // Call-site detection — mirrors Python Task #7 re_method_call
            // Skip import lines and pure declaration-only lines (no body) to avoid
            // false positives, but allow lines that contain call expressions in
            // their body (e.g. `export function run() { return foo(); }`).
            let trimmed = line.trim_start();
            // A line is a "definition-only" line if it is a bare declaration without
            // an opening brace that would contain a function body with calls.
            let is_pure_decl = (trimmed.starts_with("function ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("interface ")
                || trimmed.starts_with("abstract class ")
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("export class ")
                || trimmed.starts_with("export interface ")
                || trimmed.starts_with("export abstract class "))
                && !trimmed.contains('{');
            if !is_pure_decl && !trimmed.starts_with("import ") && !trimmed.starts_with("//") {
                // Source for `calls` edges: the enclosing function if we are inside
                // one, else the file (top-level calls, e.g. module-init code). This
                // is what makes `calls` edges FUNCTION -> ref::callee so impact/why
                // can traverse the call graph at function granularity. `fn_stack`
                // does NOT yet contain a function defined ON this line (it is still
                // in `pending_fn`, promoted at end of line), so a definition line's
                // own name still attributes to the OUTER scope — preserving the
                // pre-fix source for that case (mirrors rust_lang.rs ordering).
                let call_source: &str = fn_stack
                    .last()
                    .map(|(id, _)| id.as_str())
                    .unwrap_or(file_id);

                // receiver.method() — emit calls ref to the method name
                for caps in self.re_method_call.captures_iter(line) {
                    let receiver = caps.get(1).unwrap().as_str();
                    let method = caps.get(2).unwrap().as_str();
                    // Skip common non-call patterns
                    if matches!(
                        receiver,
                        "this"
                            | "super"
                            | "window"
                            | "document"
                            | "console"
                            | "Math"
                            | "JSON"
                            | "Object"
                            | "Array"
                            | "Promise"
                            | "String"
                            | "Number"
                            | "Boolean"
                    ) {
                        continue;
                    }
                    // If receiver starts with uppercase it's likely a type; ref to receiver
                    // Otherwise ref to the method name (mirrors Python pattern)
                    let ref_target = if receiver.chars().next().is_some_and(|c| c.is_uppercase()) {
                        format!("ref::{}", receiver)
                    } else {
                        format!("ref::{}", method)
                    };
                    // Dedup per (source, target, relation) — NOT globally per file —
                    // so distinct callers of the same callee each get their own
                    // function-sourced edge (true function granularity).
                    if !edges.iter().any(|e| {
                        e.source == call_source && e.target == ref_target && e.relation == "calls"
                    }) {
                        edges.push(ExtractedEdge {
                            source: call_source.to_string(),
                            target: ref_target.clone(),
                            relation: "calls".into(),
                            weight: 0.4,
                        });
                    }
                    if !unresolved_refs.contains(&ref_target) {
                        unresolved_refs.push(ref_target);
                    }
                }

                // Plain function calls: foo() — emit calls ref to the function name
                // Only lowercase-starting identifiers to avoid constructor calls (new Foo())
                for caps in self.re_fn_call.captures_iter(line) {
                    let fn_name = caps.get(1).unwrap().as_str();
                    // Skip JS/TS keywords that look like function calls
                    if matches!(
                        fn_name,
                        "if" | "for"
                            | "while"
                            | "switch"
                            | "catch"
                            | "function"
                            | "return"
                            | "typeof"
                            | "instanceof"
                            | "await"
                            | "async"
                            | "throw"
                            | "new"
                            | "delete"
                            | "void"
                            | "in"
                            | "of"
                            | "from"
                            | "import"
                            | "export"
                            | "let"
                            | "const"
                            | "var"
                            | "require"
                    ) {
                        continue;
                    }
                    let ref_target = format!("ref::{}", fn_name);
                    if !edges.iter().any(|e| {
                        e.source == call_source && e.target == ref_target && e.relation == "calls"
                    }) {
                        edges.push(ExtractedEdge {
                            source: call_source.to_string(),
                            target: ref_target.clone(),
                            relation: "calls".into(),
                            weight: 0.4,
                        });
                    }
                    if !unresolved_refs.contains(&ref_target) {
                        unresolved_refs.push(ref_target);
                    }
                }
            }

            // Promote a pending function to the enclosing-function stack once its
            // body brace has opened (depth rose above the signature baseline).
            // Handles multi-line signatures: the function keyword/name and the body
            // `{` may be on different lines.
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
                // Otherwise: a one-line body (`function f() { .. }`, opened and
                // closed on this line), a single-expression arrow (no block), or a
                // `;`-terminated signature (overload/abstract decl). None becomes a
                // multi-line enclosing scope; drop the latch.
            }

            brace_depth += open_count - close_count;
        }

        Ok(ExtractionResult {
            nodes,
            edges,
            unresolved_refs,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx", "mjs", "cjs"]
    }
}

#[cfg(test)]
mod tests {
    use super::TypeScriptExtractor;
    use crate::extract::Extractor;
    use m1nd_core::types::NodeType;

    fn calls_edges(result: &crate::extract::ExtractionResult) -> Vec<(&str, &str)> {
        result
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .map(|e| (e.source.as_str(), e.target.as_str()))
            .collect()
    }

    #[test]
    fn ts_function_call_emits_function_sourced_calls_edge() {
        // A call inside a function body must produce a `calls` edge sourced from the
        // ENCLOSING FUNCTION node (not the file), targeting `ref::<callee>` so the
        // resolver can bind it to the callee fn node. This is the core fix.
        // `callee` is declared as a named arrow so its own declaration line does not
        // read as a `callee(` call (which would add an unrelated self-edge from the
        // file and muddy the negative assertion). The call we care about is the one
        // INSIDE `caller`.
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"const callee = () => {};\nfunction caller() {\n    callee();\n}\n",
                "file::src/server.ts",
            )
            .unwrap();

        let caller_id = "file::src/server.ts::fn::caller";
        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == caller_id
                && e.target == "ref::callee"),
            "expected `caller -> ref::callee` calls edge, got {:?}",
            calls_edges(&result)
        );

        // The call inside `caller` must NOT be sourced from the file (the bug being
        // fixed): there is no file -> ref::callee edge at all here.
        assert!(
            !result.edges.iter().any(|e| {
                e.relation == "calls"
                    && e.source == "file::src/server.ts"
                    && e.target == "ref::callee"
            }),
            "calls edge should be function-sourced, not file-sourced: {:?}",
            calls_edges(&result)
        );

        // The callee fn node exists with label `callee`, so the resolver binds
        // ref::callee -> that node (label-based resolution).
        assert!(result
            .nodes
            .iter()
            .any(|n| n.label == "callee" && n.node_type == NodeType::Function));
    }

    #[test]
    fn ts_arrow_function_sources_calls_from_enclosing_arrow() {
        // `const caller = () => { callee(); }` — the arrow assigned to a name is the
        // enclosing scope for calls in its block body.
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"const callee = () => {};\nconst caller = () => {\n    callee();\n};\n",
                "file::src/handlers.ts",
            )
            .unwrap();

        let caller_id = "file::src/handlers.ts::fn::caller";
        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == caller_id
                && e.target == "ref::callee"),
            "expected arrow caller -> ref::callee, got {:?}",
            calls_edges(&result)
        );
    }

    #[test]
    fn ts_class_method_sources_calls_from_enclosing_method() {
        // A method defined inside a class (`process(x) { helper(); }`) is tracked as
        // an enclosing function so its calls are method-sourced, not file-sourced.
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"function helper() {}\nclass Service {\n    process(x: number) {\n        helper();\n    }\n}\n",
                "file::src/service.ts",
            )
            .unwrap();

        let method_id = "file::src/service.ts::fn::process";
        // The method node exists.
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.label == "process" && n.node_type == NodeType::Function),
            "expected a `process` function node"
        );
        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == method_id
                && e.target == "ref::helper"),
            "expected process -> ref::helper, got {:?}",
            calls_edges(&result)
        );
    }

    #[test]
    fn ts_calls_attributed_to_nearest_enclosing_function() {
        // Two functions; each call is attributed to its OWN enclosing fn, and a call
        // shared by both yields a distinct edge per caller (function granularity,
        // not the old global per-file dedup that dropped the second).
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"function a() {\n    one();\n    shared();\n}\nfunction b() {\n    two();\n    shared();\n}\n",
                "file::src/dup.ts",
            )
            .unwrap();

        let a_id = "file::src/dup.ts::fn::a";
        let b_id = "file::src/dup.ts::fn::b";
        let calls = calls_edges(&result);
        assert!(
            calls.contains(&(a_id, "ref::one")),
            "a -> one missing: {calls:?}"
        );
        assert!(
            calls.contains(&(b_id, "ref::two")),
            "b -> two missing: {calls:?}"
        );
        // Both callers of `shared` get their own edge — the second is not dropped.
        assert!(
            calls.contains(&(a_id, "ref::shared")),
            "a -> shared missing: {calls:?}"
        );
        assert!(
            calls.contains(&(b_id, "ref::shared")),
            "b -> shared missing (per-source dedup regression): {calls:?}"
        );
        // No cross-attribution.
        assert!(
            !calls.contains(&(a_id, "ref::two")),
            "unexpected a -> two cross-attribution: {calls:?}"
        );
    }

    #[test]
    fn ts_control_flow_not_treated_as_enclosing_function() {
        // `if (...) {`, `for (...) {`, etc. must NOT become enclosing functions, and
        // a call inside a control block still attributes to the real enclosing fn.
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"function caller() {\n    if (cond) {\n        doWork();\n    }\n    for (let i = 0; i < n; i++) {\n        step();\n    }\n}\n",
                "file::src/flow.ts",
            )
            .unwrap();

        let caller_id = "file::src/flow.ts::fn::caller";
        // No `if`/`for` function nodes were created.
        assert!(
            !result
                .nodes
                .iter()
                .any(|n| (n.label == "if" || n.label == "for")
                    && n.node_type == NodeType::Function),
            "control-flow keywords must not create function nodes"
        );
        let calls = calls_edges(&result);
        // Calls inside the control blocks attribute to `caller`, not the file.
        assert!(
            calls.contains(&(caller_id, "ref::doWork")),
            "doWork should be caller-sourced: {calls:?}"
        );
        assert!(
            calls.contains(&(caller_id, "ref::step")),
            "step should be caller-sourced: {calls:?}"
        );
    }

    #[test]
    fn ts_same_name_methods_in_one_file_get_distinct_ids() {
        // Two classes in one file each declare a `process` method. Both must survive
        // as DISTINCT function nodes — without id disambiguation the second collides
        // on `…::fn::process` and add_node drops it (the method vanishes).
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(
                b"class A {\n    process() {\n    }\n}\nclass B {\n    process() {\n    }\n}\n",
                "file::src/dup.ts",
            )
            .unwrap();
        let ids: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.label == "process" && n.node_type == NodeType::Function)
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(
            ids.len(),
            2,
            "expected two distinct `process` method nodes, got ids {:?}",
            ids
        );
        // First keeps the clean (line-less) id for back-compat; the sibling is `#2`.
        assert!(
            ids.contains(&"file::src/dup.ts::fn::process"),
            "first occurrence should keep the clean id: {:?}",
            ids
        );
        assert!(
            ids.contains(&"file::src/dup.ts::fn::process#2"),
            "second occurrence should be disambiguated to `#2`: {:?}",
            ids
        );
    }

    #[test]
    fn ts_top_level_call_falls_back_to_file_source() {
        // A call at module top level (no enclosing function) still sources from the
        // file — the documented fallback.
        let ext = TypeScriptExtractor::new();
        let result = ext
            .extract(b"bootstrap();\n", "file::src/index.ts")
            .unwrap();

        assert!(
            result.edges.iter().any(|e| e.relation == "calls"
                && e.source == "file::src/index.ts"
                && e.target == "ref::bootstrap"),
            "top-level call should be file-sourced, got {:?}",
            calls_edges(&result)
        );
    }
}
