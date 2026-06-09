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
        }
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

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;

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
                let node_id = format!("{}::fn::{}", file_id, name);
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
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
            } else if let Some(caps) = self.re_arrow.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::fn::{}", file_id, name);
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
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
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
                    if !unresolved_refs.contains(&ref_target) {
                        edges.push(ExtractedEdge {
                            source: file_id.to_string(),
                            target: ref_target.clone(),
                            relation: "calls".into(),
                            weight: 0.4,
                        });
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
                    if !unresolved_refs.contains(&ref_target) {
                        edges.push(ExtractedEdge {
                            source: file_id.to_string(),
                            target: ref_target.clone(),
                            relation: "calls".into(),
                            weight: 0.4,
                        });
                        unresolved_refs.push(ref_target);
                    }
                }
            }
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
