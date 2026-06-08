// === crates/m1nd-ingest/src/extract/go.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;

/// Go extractor using regex.
pub struct GoExtractor {
    re_func: Regex,
    re_method: Regex,
    re_struct: Regex,
    re_interface: Regex,
    re_import: Regex,
    /// Qualified calls: pkg.Function( or receiver.Method(
    re_method_call: Regex,
    /// Plain function calls: identifier( not preceded by func/type keywords
    re_fn_call: Regex,
}

impl GoExtractor {
    pub fn new() -> Self {
        Self {
            re_func: Regex::new(r"^func\s+(\w+)\s*\(").unwrap(),
            re_method: Regex::new(r"^func\s+\([^)]+\)\s+(\w+)\s*\(").unwrap(),
            re_struct: Regex::new(r"^type\s+(\w+)\s+struct\b").unwrap(),
            re_interface: Regex::new(r"^type\s+(\w+)\s+interface\b").unwrap(),
            re_import: Regex::new(r#"^\s*"([^"]+)""#).unwrap(),
            // Qualified call: qualifier.Method( — captures (qualifier, method)
            // Handles both package-qualified calls (fmt.Println) and method calls (s.Run)
            re_method_call: Regex::new(r"\b(\w+)\.(\w+)\s*\(").unwrap(),
            // Plain bare call: identifier( — lowercase-or-mixed start, not a keyword
            // \b ensures we don't match mid-word; requires lowercase first char to avoid
            // matching type conversions like MyType( or struct literals.
            re_fn_call: Regex::new(r"\b([a-z_]\w*)\s*\(").unwrap(),
        }
    }
}

impl Default for GoExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for GoExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let text = String::from_utf8_lossy(content);
        let cleaned_lines = strip_comments_and_strings(&text, CommentSyntax::GO);
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut unresolved_refs = Vec::new();

        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: vec!["go".into()],
            line: 1,
            end_line: text.lines().count() as u32,
        });

        let mut in_import_block = false;

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;
            let trimmed = line.trim();

            // ------------------------------------------------------------------
            // Import block tracking
            // ------------------------------------------------------------------
            if trimmed.starts_with("import (") {
                in_import_block = true;
                continue;
            }
            if in_import_block {
                if trimmed == ")" {
                    in_import_block = false;
                    continue;
                }
                if let Some(caps) = self.re_import.captures(trimmed) {
                    let path = caps.get(1).unwrap().as_str();
                    let ref_id = format!("ref::{}", path);
                    edges.push(ExtractedEdge {
                        source: file_id.to_string(),
                        target: ref_id.clone(),
                        relation: "imports".into(),
                        weight: 0.5,
                    });
                    unresolved_refs.push(ref_id);
                }
                continue;
            }

            // ------------------------------------------------------------------
            // Definition extraction — struct / interface / method / func.
            // We track `is_definition` so call-site detection is skipped on
            // definition-only lines (conservative: avoids false-positive calls
            // from the parameter list of a func declaration).
            // ------------------------------------------------------------------
            let is_definition = if let Some(caps) = self.re_struct.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::struct::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Struct,
                    tags: vec!["go".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                true
            } else if let Some(caps) = self.re_interface.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::interface::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Type,
                    tags: vec!["go".into(), "interface".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                true
            } else if let Some(caps) = self.re_method.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::fn::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["go".into(), "method".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Conservatively skip call-site scan on the func/method signature
                // line to avoid false positives from parameter type names.
                true
            } else if let Some(caps) = self.re_func.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::fn::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["go".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                true
            } else {
                false
            };

            // Single-line import: import "pkg/path"
            if trimmed.starts_with("import ") && !trimmed.contains('(') {
                if let Some(caps) = self
                    .re_import
                    .captures(trimmed.trim_start_matches("import "))
                {
                    let path = caps.get(1).unwrap().as_str();
                    let ref_id = format!("ref::{}", path);
                    edges.push(ExtractedEdge {
                        source: file_id.to_string(),
                        target: ref_id.clone(),
                        relation: "imports".into(),
                        weight: 0.5,
                    });
                    unresolved_refs.push(ref_id);
                }
                // Import lines have no call sites.
                continue;
            }

            // ------------------------------------------------------------------
            // Call-site detection
            //
            // Only runs on lines that are NOT:
            //   • inside an import block (handled above via `continue`)
            //   • single-line imports (handled above via `continue`)
            //   • definition-only lines (is_definition == true)
            //   • package declaration lines
            //   • empty or comment-only lines
            //
            // Two patterns:
            //   1. Qualified: pkg.Func( or receiver.Method(  → re_method_call
            //   2. Bare: funcName(                            → re_fn_call
            // ------------------------------------------------------------------
            if !is_definition
                && !trimmed.starts_with("package ")
                && !trimmed.starts_with("//")
                && !trimmed.is_empty()
            {
                // Pattern 1: qualifier.Method(
                // qualifier len-1 and "err" are excluded (noisy: testing.T
                // receiver `t`, error value `err`).
                // For uppercase qualifier → ref to type; otherwise → ref to method.
                for caps in self.re_method_call.captures_iter(line) {
                    let qualifier = caps.get(1).unwrap().as_str();
                    let method = caps.get(2).unwrap().as_str();

                    if qualifier.len() == 1 || qualifier == "err" {
                        continue;
                    }

                    let ref_target =
                        if qualifier.chars().next().is_some_and(|c| c.is_uppercase()) {
                            format!("ref::{}", qualifier)
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

                // Pattern 2: bare funcName(
                // Only lowercase-starting identifiers (Go unexported functions).
                // Uppercase-starting identifiers handled by pattern 1 when called
                // as pkg.NewX(); bare uppercase is typically a type conversion.
                // Explicit keyword/builtin exclusion list is the primary false-positive gate.
                for caps in self.re_fn_call.captures_iter(line) {
                    let fn_name = caps.get(1).unwrap().as_str();

                    if matches!(
                        fn_name,
                        // Go control-flow keywords
                        "if"
                            | "for"
                            | "switch"
                            | "select"
                            | "return"
                            | "go"
                            | "defer"
                            | "range"
                            | "case"
                            | "default"
                            // Go built-in functions
                            | "make"
                            | "new"
                            | "len"
                            | "cap"
                            | "append"
                            | "copy"
                            | "delete"
                            | "close"
                            | "panic"
                            | "recover"
                            | "print"
                            | "println"
                            // Type / structural keywords
                            | "chan"
                            | "map"
                            | "func"
                            | "type"
                            | "struct"
                            | "interface"
                            | "var"
                            | "const"
                            // Common testing receiver names (too noisy)
                            | "t"
                            | "b"
                            | "m"
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
        &["go"]
    }
}

// ===========================================================================
// Unit tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Extractor;

    fn extract(src: &str) -> ExtractionResult {
        let extractor = GoExtractor::new();
        extractor
            .extract(src.as_bytes(), "file::testfile.go")
            .unwrap()
    }

    fn has_edge(result: &ExtractionResult, relation: &str, target: &str) -> bool {
        result
            .edges
            .iter()
            .any(|e| e.relation == relation && e.target == target)
    }

    fn has_any_relation(result: &ExtractionResult, relation: &str) -> bool {
        result.edges.iter().any(|e| e.relation == relation)
    }

    // -----------------------------------------------------------------------
    // Existing behavior: contains + imports
    // -----------------------------------------------------------------------

    #[test]
    fn go_extractor_emits_struct_node() {
        let result = extract("package main\n\ntype Config struct {\n    Host string\n}\n");
        assert!(
            result.nodes.iter().any(|n| n.label == "Config"),
            "Should extract struct node"
        );
        assert!(has_edge(&result, "contains", "file::testfile.go::struct::Config"));
    }

    #[test]
    fn go_extractor_emits_func_node() {
        let result = extract("package main\n\nfunc Run() {\n}\n");
        assert!(
            result.nodes.iter().any(|n| n.label == "Run"),
            "Should extract func node"
        );
        assert!(has_edge(&result, "contains", "file::testfile.go::fn::Run"));
    }

    #[test]
    fn go_extractor_emits_import() {
        let result = extract("package main\n\nimport (\n    \"fmt\"\n    \"github.com/org/repo/pkg/util\"\n)\n\nfunc main() {}\n");
        assert!(
            has_edge(&result, "imports", "ref::fmt"),
            "Should emit import edge for fmt"
        );
        assert!(
            has_edge(
                &result,
                "imports",
                "ref::github.com/org/repo/pkg/util"
            ),
            "Should emit import edge for github.com/org/repo/pkg/util"
        );
    }

    // -----------------------------------------------------------------------
    // TASK A: calls edges
    // -----------------------------------------------------------------------

    #[test]
    fn go_extractor_emits_calls_for_bare_function_call() {
        let result = extract("package main\n\nfunc run() {\n    helper()\n}\n");
        assert!(
            has_edge(&result, "calls", "ref::helper"),
            "Should emit calls edge for helper(). Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_emits_calls_for_qualified_call() {
        let result = extract("package main\n\nfunc run() {\n    client.Send()\n}\n");
        // qualifier `client` is lowercase → ref to method `Send`
        assert!(
            has_edge(&result, "calls", "ref::Send"),
            "Should emit calls edge for client.Send(). Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_does_not_emit_calls_for_if_keyword() {
        let result = extract("package main\n\nfunc run() {\n    if true {\n    }\n}\n");
        assert!(
            !has_edge(&result, "calls", "ref::if"),
            "Must NOT emit calls edge for `if` keyword. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_does_not_emit_calls_for_for_keyword() {
        let result = extract("package main\n\nfunc run() {\n    for i := 0; i < 10; i++ {\n    }\n}\n");
        assert!(
            !has_edge(&result, "calls", "ref::for"),
            "Must NOT emit calls edge for `for` keyword. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_does_not_emit_calls_for_switch() {
        let result = extract("package main\n\nfunc run(x int) {\n    switch x {\n    case 1:\n    }\n}\n");
        assert!(
            !has_edge(&result, "calls", "ref::switch"),
            "Must NOT emit calls edge for `switch`. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_does_not_emit_calls_for_make_builtin() {
        let result = extract("package main\n\nfunc run() {\n    s := make([]int, 10)\n    _ = s\n}\n");
        assert!(
            !has_edge(&result, "calls", "ref::make"),
            "Must NOT emit calls edge for built-in `make`. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_does_not_emit_calls_on_definition_line() {
        // The func definition line itself should not generate a calls edge
        // for the function name — only `contains` should appear.
        let result = extract("package main\n\nfunc process(data []byte) {\n}\n");
        // `process` should be a `contains` target, not a `calls` target
        assert!(
            !has_edge(&result, "calls", "ref::process"),
            "Must NOT emit calls edge for the function being defined. Edges: {:?}",
            result.edges
        );
        assert!(
            has_edge(&result, "contains", "file::testfile.go::fn::process"),
            "Should emit contains edge for process"
        );
    }

    #[test]
    fn go_extractor_emits_calls_from_non_definition_lines() {
        let src = "package main\n\nfunc run() {\n    processData()\n    pkg.Execute()\n}\n";
        let result = extract(src);
        // processData is a bare lowercase call
        assert!(
            has_edge(&result, "calls", "ref::processData"),
            "Should emit calls edge for processData(). Edges: {:?}",
            result.edges
        );
        // pkg.Execute — qualifier `pkg` lowercase → ref to `Execute`
        assert!(
            has_edge(&result, "calls", "ref::Execute"),
            "Should emit calls edge for pkg.Execute(). Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn go_extractor_sample_edges_for_reporting() {
        // This test exists to demonstrate what edges are produced on a
        // realistic Go snippet. It passes trivially; the println output
        // is visible with `cargo test -- --nocapture`.
        let src = r#"package main

import (
    "fmt"
    "github.com/org/repo/pkg/store"
)

type Server struct{}

func NewServer(db store.DB) *Server {
    return &Server{}
}

func (s *Server) HandleRequest() (string, error) {
    data, err := s.store.Query(42)
    if err != nil {
        return "", err
    }
    result := processData(data)
    fmt.Printf("result: %v\n", result)
    return buildResponse(result), nil
}

func processData(d []byte) []byte {
    return compress(d)
}
"#;
        let ext = GoExtractor::new();
        let result = ext.extract(src.as_bytes(), "file::main.go").unwrap();
        for e in &result.edges {
            if e.relation == "calls" || e.relation == "imports" || e.relation == "contains" {
                let _ = format!("{} --[{}]--> {}", e.source, e.relation, e.target);
            }
        }
        // Minimal assertion: we must have both calls and imports
        assert!(result.edges.iter().any(|e| e.relation == "calls"));
        assert!(result.edges.iter().any(|e| e.relation == "imports"));
        assert!(result.edges.iter().any(|e| e.relation == "contains"));
    }

    #[test]
    fn go_extractor_deduplicates_calls_edges() {
        // Same callee on multiple lines → only one calls edge (dedup via unresolved_refs)
        let src = "package main\n\nfunc run() {\n    helper()\n    helper()\n    helper()\n}\n";
        let result = extract(src);
        let count = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls" && e.target == "ref::helper")
            .count();
        assert_eq!(count, 1, "Duplicate calls to helper() should produce exactly 1 edge, got {}", count);
    }
}
