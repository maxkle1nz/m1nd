// === crates/m1nd-ingest/src/extract/java.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;

/// Java extractor using regex.
pub struct JavaExtractor {
    re_class: Regex,
    re_interface: Regex,
    re_enum: Regex,
    re_method: Regex,
    re_import: Regex,
    /// Qualified calls: receiver.Method( — captures (receiver, method)
    re_method_call: Regex,
    /// Bare function calls: identifier( — lowercase-or-underscore start
    re_fn_call: Regex,
}

impl JavaExtractor {
    pub fn new() -> Self {
        Self {
            re_class: Regex::new(r"^\s*(?:public|private|protected)?\s*(?:static\s+)?(?:abstract\s+)?(?:final\s+)?class\s+(\w+)").unwrap(),
            re_interface: Regex::new(r"^\s*(?:public|private|protected)?\s*interface\s+(\w+)").unwrap(),
            re_enum: Regex::new(r"^\s*(?:public|private|protected)?\s*enum\s+(\w+)").unwrap(),
            re_method: Regex::new(r"^\s*(?:public|private|protected)\s+(?:static\s+)?(?:final\s+)?(?:synchronized\s+)?(?:abstract\s+)?(?:\w+(?:<[^>]*>)?)\s+(\w+)\s*\(").unwrap(),
            re_import: Regex::new(r"^\s*import\s+(?:static\s+)?([\w.*]+)\s*;").unwrap(),
            // Qualified call: receiver.Method( — captures (receiver, method)
            // Handles both static calls (ClassName.method) and instance calls (obj.method)
            re_method_call: Regex::new(r"\b(\w+)\.(\w+)\s*\(").unwrap(),
            // Bare call: identifier( — only lowercase/underscore first char to avoid
            // matching type casts like MyClass( or constructor invocations after `new`.
            // \b ensures we don't match mid-word.
            re_fn_call: Regex::new(r"\b([a-z_]\w*)\s*\(").unwrap(),
        }
    }
}

impl Default for JavaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for JavaExtractor {
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
            tags: vec!["java".into()],
            line: 1,
            end_line: text.lines().count() as u32,
        });

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;
            let trimmed = line.trim();

            // Skip blank lines and comment-only lines (single-line comments
            // should have been stripped by strip_comments_and_strings, but
            // guard here defensively for `//` and `*` JavaDoc continuation lines).
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('*')
                || trimmed.starts_with("/*")
            {
                continue;
            }

            // Skip package and import declaration lines — no call sites here.
            if trimmed.starts_with("package ") {
                continue;
            }

            // ------------------------------------------------------------------
            // Import extraction (before is_definition check — import lines are
            // never call-site lines).
            // ------------------------------------------------------------------
            if let Some(caps) = self.re_import.captures(line) {
                let path = caps.get(1).unwrap().as_str();
                let ref_id = format!("ref::{}", path);
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: ref_id.clone(),
                    relation: "imports".into(),
                    weight: 0.5,
                });
                if !unresolved_refs.contains(&ref_id) {
                    unresolved_refs.push(ref_id);
                }
                // Import lines have no call sites.
                continue;
            }

            // ------------------------------------------------------------------
            // Definition extraction — class / interface / enum / method.
            // We track `is_definition` so call-site detection is skipped on
            // definition-only lines (conservative: avoids false-positive calls
            // from parameter type names in a method signature).
            // ------------------------------------------------------------------
            let is_definition = if let Some(caps) = self.re_class.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::class::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Class,
                    tags: vec!["java".into()],
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
                    tags: vec!["java".into(), "interface".into()],
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
            } else if let Some(caps) = self.re_enum.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::enum::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Enum,
                    tags: vec!["java".into()],
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
                    tags: vec!["java".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id,
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Conservatively skip call-site scan on the method signature line
                // to avoid false positives from parameter type names.
                true
            } else {
                false
            };

            // ------------------------------------------------------------------
            // Call-site detection
            //
            // Only runs on lines that are NOT:
            //   • definition-only lines (is_definition == true)
            //   • package / import lines (already `continue`d above)
            //   • comment lines (already `continue`d above)
            //   • blank lines (already `continue`d above)
            //
            // Two patterns:
            //   1. Qualified: Receiver.method( or ClassName.method(  → re_method_call
            //   2. Bare: methodName(                                  → re_fn_call
            // ------------------------------------------------------------------
            if !is_definition {
                // Pattern 1: receiver.method(
                // Skip single-char receivers (e.g. `e` in catch(Exception e))
                // and `err`-style noisy receivers.
                // If receiver is uppercase → it's a class/type call: ref to receiver.
                // If receiver is lowercase → instance call: ref to method.
                for caps in self.re_method_call.captures_iter(line) {
                    let receiver = caps.get(1).unwrap().as_str();
                    let method = caps.get(2).unwrap().as_str();

                    // Skip very short receiver names (single-char, `err`, `ex`, `e`)
                    if receiver.len() <= 1 {
                        continue;
                    }

                    // Skip Java keywords that can appear before '(' in qualified form
                    // (e.g. would not naturally appear as receiver.X but guard anyway)
                    if matches!(
                        receiver,
                        "if" | "for" | "while" | "switch" | "catch" | "return"
                            | "new" | "synchronized" | "super" | "this" | "assert"
                            | "throw" | "throws" | "try" | "else" | "finally"
                            | "instanceof" | "class" | "interface" | "enum"
                            | "import" | "package" | "extends" | "implements"
                    ) {
                        continue;
                    }

                    let ref_target =
                        if receiver.chars().next().is_some_and(|c| c.is_uppercase()) {
                            // Static/class call: ClassName.method( → ref to ClassName
                            format!("ref::{}", receiver)
                        } else {
                            // Instance call: obj.method( → ref to method
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

                // Pattern 2: bare methodName(
                // Only lowercase-starting identifiers (Java convention: method names
                // are camelCase, starting lowercase). Uppercase-starting identifiers
                // are handled by pattern 1 when called as ClassName.method(); bare
                // uppercase is typically a constructor call or type cast.
                // Explicit keyword + Java-builtin exclusion list is the primary guard.
                for caps in self.re_fn_call.captures_iter(line) {
                    let fn_name = caps.get(1).unwrap().as_str();

                    if matches!(
                        fn_name,
                        // Java control-flow keywords
                        "if"
                            | "for"
                            | "while"
                            | "switch"
                            | "catch"
                            | "return"
                            | "throw"
                            | "assert"
                            | "case"
                            | "default"
                            | "else"
                            | "try"
                            | "finally"
                            | "do"
                            // Java structural keywords that look like calls
                            | "new"
                            | "super"
                            | "this"
                            | "synchronized"
                            | "instanceof"
                            | "class"
                            | "interface"
                            | "enum"
                            | "extends"
                            | "implements"
                            | "import"
                            | "package"
                            // Common noisy single-letter loop vars or very-short names
                            // handled above in method_call guard, but also catch here
                            // for bare calls like `t(` in test code
                            | "t"
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
        &["java"]
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
        let extractor = JavaExtractor::new();
        extractor
            .extract(src.as_bytes(), "file::testfile.java")
            .unwrap()
    }

    fn has_edge(result: &ExtractionResult, relation: &str, target: &str) -> bool {
        result
            .edges
            .iter()
            .any(|e| e.relation == relation && e.target == target)
    }

    // -----------------------------------------------------------------------
    // Existing behavior: contains + imports
    // -----------------------------------------------------------------------

    #[test]
    fn java_extractor_emits_class_node() {
        let result = extract("public class Foo {\n}\n");
        assert!(
            result.nodes.iter().any(|n| n.label == "Foo"),
            "Should extract class node"
        );
        assert!(has_edge(&result, "contains", "file::testfile.java::class::Foo"));
    }

    #[test]
    fn java_extractor_emits_method_node() {
        let result = extract("public class Foo {\n    public void run() {}\n}\n");
        assert!(has_edge(&result, "contains", "file::testfile.java::fn::run"));
    }

    #[test]
    fn java_extractor_emits_import() {
        let result = extract("import com.example.Bar;\n");
        assert!(
            has_edge(&result, "imports", "ref::com.example.Bar"),
            "Should emit import edge. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn java_extractor_emits_wildcard_import() {
        let result = extract("import com.example.foo.*;\n");
        assert!(
            has_edge(&result, "imports", "ref::com.example.foo.*"),
            "Should emit wildcard import edge. Edges: {:?}",
            result.edges
        );
    }

    // -----------------------------------------------------------------------
    // TASK A: calls edges
    // -----------------------------------------------------------------------

    #[test]
    fn java_emits_calls_edges_and_guards_keywords() {
        // This is the primary calls test — mirrors go_emits_calls_edges_and_guards_keywords.
        // Contains:
        //   - a bare method call: helper()       → should emit calls ref::helper
        //   - a qualified call: service.send()   → should emit calls ref::send
        //   - a static call: Utils.format()      → should emit calls ref::Utils
        //   - keyword: if (x)                    → must NOT emit calls ref::if
        //   - keyword: for (int i                → must NOT emit calls ref::for
        //   - keyword: while (                   → must NOT emit calls ref::while
        //   - keyword: new Foo()                 → must NOT emit calls ref::new
        //   - keyword: switch (x)                → must NOT emit calls ref::switch
        //   - keyword: catch (Exception           → must NOT emit calls ref::catch
        //   - keyword: synchronized (            → must NOT emit calls ref::synchronized
        //   - super.init()                       → must NOT emit calls ref::super (len == 5 but keyword)
        //   - this.load()                        → must NOT emit calls ref::this (keyword)
        let src = r#"
public class Service {
    public void execute() {
        helper();
        service.send(data);
        Utils.format(value);
        if (condition) {
            doWork();
        }
        for (int i = 0; i < 10; i++) {
        }
        while (running) {
        }
        Object obj = new Foo();
        switch (state) {
            case 1: break;
        }
        try {
        } catch (Exception e) {
        }
        synchronized (lock) {
        }
        super.init();
        this.load();
    }
}
"#;
        let result = extract(src);

        // Positive assertions
        assert!(
            has_edge(&result, "calls", "ref::helper"),
            "Should emit calls for bare helper(). Edges: {:?}",
            result.edges
        );
        assert!(
            has_edge(&result, "calls", "ref::send"),
            "Should emit calls for service.send(). Edges: {:?}",
            result.edges
        );
        assert!(
            has_edge(&result, "calls", "ref::Utils"),
            "Should emit calls for Utils.format() (uppercase receiver → ref::Utils). Edges: {:?}",
            result.edges
        );
        assert!(
            has_edge(&result, "calls", "ref::doWork"),
            "Should emit calls for doWork(). Edges: {:?}",
            result.edges
        );

        // Negative assertions — keywords must not produce call edges
        assert!(
            !has_edge(&result, "calls", "ref::if"),
            "Must NOT emit calls for `if` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::for"),
            "Must NOT emit calls for `for` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::while"),
            "Must NOT emit calls for `while` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::new"),
            "Must NOT emit calls for `new` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::switch"),
            "Must NOT emit calls for `switch` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::catch"),
            "Must NOT emit calls for `catch` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::synchronized"),
            "Must NOT emit calls for `synchronized` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::super"),
            "Must NOT emit calls for `super` keyword. Edges: {:?}",
            result.edges
        );
        assert!(
            !has_edge(&result, "calls", "ref::this"),
            "Must NOT emit calls for `this` keyword. Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn java_emits_calls_for_qualified_method_call() {
        let src = "public class A {\n    public void run() {\n        client.send();\n    }\n}\n";
        let result = extract(src);
        // lowercase receiver → ref to method name
        assert!(
            has_edge(&result, "calls", "ref::send"),
            "Should emit calls edge for client.send(). Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn java_emits_calls_for_static_class_call() {
        let src = "public class A {\n    public void run() {\n        Logger.info(msg);\n    }\n}\n";
        let result = extract(src);
        // uppercase receiver → ref to class name (static call)
        assert!(
            has_edge(&result, "calls", "ref::Logger"),
            "Should emit calls edge for Logger.info() (static call). Edges: {:?}",
            result.edges
        );
    }

    #[test]
    fn java_does_not_emit_calls_on_definition_line() {
        // Method definition itself should not generate a calls edge
        let src = "public class A {\n    public void process(Data data) {}\n}\n";
        let result = extract(src);
        assert!(
            !has_edge(&result, "calls", "ref::process"),
            "Must NOT emit calls edge for the method being defined. Edges: {:?}",
            result.edges
        );
        assert!(
            has_edge(&result, "contains", "file::testfile.java::fn::process"),
            "Should emit contains edge for process"
        );
    }

    #[test]
    fn java_deduplicates_calls_edges() {
        // Same callee called multiple times → only one calls edge
        let src = r#"
public class A {
    public void run() {
        helper();
        helper();
        helper();
    }
}
"#;
        let result = extract(src);
        let count = result
            .edges
            .iter()
            .filter(|e| e.relation == "calls" && e.target == "ref::helper")
            .count();
        assert_eq!(
            count,
            1,
            "Duplicate calls to helper() should produce exactly 1 edge, got {}",
            count
        );
    }

    #[test]
    fn java_sample_edges_for_reporting() {
        // Demonstrates what edges are produced on a realistic Java snippet.
        // The test passes trivially; edge quality can be reviewed via --nocapture.
        let src = r#"
package com.example.service;

import com.example.repo.UserRepository;
import com.example.model.User;

public class UserService {
    private UserRepository repo;

    public UserService(UserRepository repo) {
        this.repo = repo;
    }

    public User findById(long id) {
        User user = repo.findById(id);
        if (user == null) {
            throw new IllegalArgumentException("not found");
        }
        return validator.validate(user);
    }

    public void save(User user) {
        User validated = validator.validate(user);
        repo.save(validated);
        eventBus.publish(UserSaved.of(validated));
    }
}
"#;
        let extractor = JavaExtractor::new();
        let result = extractor
            .extract(src.as_bytes(), "file::UserService.java")
            .unwrap();
        for e in &result.edges {
            if e.relation == "calls" || e.relation == "imports" || e.relation == "contains" {
                let _ = format!("{} --[{}]--> {}", e.source, e.relation, e.target);
            }
        }
        // Minimal assertions: must have calls, imports, and contains
        assert!(result.edges.iter().any(|e| e.relation == "calls"));
        assert!(result.edges.iter().any(|e| e.relation == "imports"));
        assert!(result.edges.iter().any(|e| e.relation == "contains"));
    }
}
