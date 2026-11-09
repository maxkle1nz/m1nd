// === crates/m1nd-ingest/src/extract/python.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Python extractor using regex.
/// FM-ING-009 fix: patterns allow leading whitespace (captures indented defs).
/// Replaces: ingest.py PythonExtractor
pub struct PythonExtractor {
    re_func: Regex,
    re_class: Regex,
    re_import: Regex,
    re_from_import: Regex,
    re_from_import_names: Regex, // from X import A, B, C
    re_type_hint: Regex,         // Type hints: -> Type, : Type
    re_class_inherit: Regex,     // class Foo(Bar, Baz):
    re_decorator: Regex,         // @decorator_name (Task #3)
    re_method_call: Regex,       // receiver.method() calls (Task #7)
    re_plain_header: Regex,
    re_bare_call: Regex,
    re_module_binding: Regex,
}

impl PythonExtractor {
    pub fn new() -> Self {
        Self {
            re_func: Regex::new(r"^\s*(?:async\s+)?def\s+(\w+)\s*\(").unwrap(),
            re_class: Regex::new(r"^\s*class\s+(\w+)\s*[:\(]").unwrap(),
            re_import: Regex::new(r"^\s*import\s+([\w.]+)").unwrap(),
            re_from_import: Regex::new(r"^\s*from\s+([\w.]+)\s+import").unwrap(),
            re_from_import_names: Regex::new(r"from\s+[\w.]+\s+import\s+(.+)").unwrap(),
            // FIX #2: was `->s*\s*` (missing backslash), now `->\s*`
            re_type_hint: Regex::new(r"(?::\s*|->\s*)([A-Z]\w+)").unwrap(),
            re_class_inherit: Regex::new(r"class\s+\w+\(([^)]+)\)").unwrap(),
            // Task #3: Python decorator extraction
            re_decorator: Regex::new(r"^\s*@(\w+(?:\.\w+)*)").unwrap(),
            // Task #7: receiver.method() calls
            re_method_call: Regex::new(r"(\w+)\.(\w+)\s*\(").unwrap(),
            re_plain_header: Regex::new(
                r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^()]*)\)\s*:\s*(.*)$",
            )
            .unwrap(),
            re_bare_call: Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap(),
            re_module_binding: Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*(?:=|[+*/%&|^-]=)")
                .unwrap(),
        }
    }

    /// Additional, deliberately narrow static call edges. Legacy extraction is
    /// unchanged. Only unique undecorated module functions with simple headers
    /// are targets; origins also need flat, assignment-free, quote-free bodies.
    /// Nested/control/dynamic scopes and multiline lexical forms are omitted,
    /// never sent through the global name resolver as guessed bare references.
    fn plain_call_edges(&self, text: &str, nodes: &[ExtractedNode]) -> Vec<ExtractedEdge> {
        let identities: HashMap<_, _> = nodes
            .iter()
            .filter(|node| node.node_type == NodeType::Function)
            .map(|node| ((node.line, node.label.as_str()), node.id.as_str()))
            .collect();
        let mut functions: Vec<PlainFunction> = Vec::new();
        let mut declarations: HashMap<String, usize> = HashMap::new();
        let mut other_bindings = HashSet::new();
        let mut current: Option<usize> = None;
        let mut class_body = false;
        let mut decorated = false;

        for (index, raw) in text.lines().enumerate() {
            // The shared stripper does not handle triple-single strings, and
            // preserves some standalone quoted lines. Do not trust it for the
            // new lexical ownership analysis. Omit multiline forms altogether.
            if raw.trim_start().starts_with('#') {
                continue;
            }
            if raw.contains("'''")
                || raw.contains("\"\"\"")
                || raw.contains('\\')
                || python_words(raw).any(|word| {
                    matches!(
                        word,
                        "global"
                            | "nonlocal"
                            | "exec"
                            | "eval"
                            | "globals"
                            | "locals"
                            | "__import__"
                    )
                })
            {
                return Vec::new();
            }
            let code = raw.split('#').next().unwrap_or("").trim_end();
            let line = code.trim_start_matches(' ');
            if line.is_empty() {
                continue;
            }
            if line.chars().next().is_some_and(char::is_whitespace) {
                return Vec::new();
            }
            let indent = code.len() - line.len();
            if indent > 0 {
                if let Some(origin) = current {
                    functions[origin].push_body(line, indent);
                } else if !class_body {
                    return Vec::new();
                }
                continue;
            }
            current = None;
            class_body = false;
            if raw.contains(';') {
                return Vec::new();
            }
            if line.starts_with('@') {
                decorated = true;
                continue;
            }
            if let Some(caps) = self.re_func.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                *declarations.entry(name.to_string()).or_default() += 1;
                let header = self.re_plain_header.captures(line);
                let params = header.as_ref().and_then(|caps| plain_parameters(&caps[2]));
                let id = identities.get(&((index + 1) as u32, name)).copied();
                let supported =
                    !decorated && plain_identifier(name) && params.is_some() && id.is_some();
                let mut function = PlainFunction {
                    name: name.to_string(),
                    id: id.unwrap_or("").to_string(),
                    parameters: params.unwrap_or_default(),
                    definition_supported: supported,
                    body_supported: true,
                    body_indent: None,
                    body: Vec::new(),
                };
                if let Some(header) = header {
                    if !header[3].is_empty() {
                        function.push_body(&header[3], 0);
                    }
                }
                current = Some(functions.len());
                functions.push(function);
            } else if let Some(caps) = self.re_class.captures(line) {
                other_bindings.insert(caps[1].to_string());
                class_body = true;
            } else if let Some(bindings) = plain_import_bindings(line) {
                match bindings {
                    Some(bindings) => other_bindings.extend(bindings),
                    None => return Vec::new(), // wildcard / complex import may bind any target
                }
            } else if let Some(caps) = self.re_module_binding.captures(line) {
                if raw.matches('=').count() != 1 {
                    return Vec::new();
                }
                other_bindings.insert(caps[1].to_string());
            } else if line != "pass" {
                // Conditional module declarations and other unmodelled bindings
                // cannot safely establish a unique module function target.
                return Vec::new();
            }
            decorated = false;
        }

        let targets: HashMap<_, _> = functions
            .iter()
            .filter(|function| {
                function.definition_supported
                    && declarations.get(&function.name) == Some(&1)
                    && !other_bindings.contains(&function.name)
            })
            .map(|function| (function.name.as_str(), function))
            .collect();
        let mut seen = BTreeSet::new();
        let mut edges = Vec::new();
        for origin in &functions {
            if !origin.body_supported || !targets.contains_key(origin.name.as_str()) {
                continue;
            }
            for body in &origin.body {
                for caps in self.re_bare_call.captures_iter(body) {
                    let called = caps.get(1).unwrap();
                    let name = called.as_str();
                    if body[..called.start()].trim_end().ends_with('.')
                        || origin.parameters.contains(name)
                        || matches!(name, "self" | "cls" | "super")
                    {
                        continue;
                    }
                    if let Some(target) = targets.get(name) {
                        // Relation is always calls here; distinct caller IDs
                        // must retain their own edge to the same target.
                        if seen.insert((origin.id.clone(), target.id.clone())) {
                            edges.push(ExtractedEdge {
                                source: origin.id.clone(),
                                target: target.id.clone(),
                                relation: "calls".into(),
                                weight: 0.4,
                            });
                        }
                    }
                }
            }
        }
        edges
    }
}

struct PlainFunction {
    name: String,
    id: String,
    parameters: HashSet<String>,
    definition_supported: bool,
    body_supported: bool,
    body_indent: Option<usize>,
    body: Vec<String>,
}

impl PlainFunction {
    fn push_body(&mut self, line: &str, indent: usize) {
        if self.body_indent.is_some_and(|expected| expected != indent) {
            self.body_supported = false;
        }
        self.body_indent.get_or_insert(indent);
        if line.contains(['\'', '"', '=', ':', ';', '`'])
            || !balanced_inline(line)
            || python_words(line).any(|word| {
                matches!(
                    word,
                    "def"
                        | "class"
                        | "lambda"
                        | "for"
                        | "while"
                        | "if"
                        | "elif"
                        | "else"
                        | "with"
                        | "as"
                        | "try"
                        | "except"
                        | "finally"
                        | "import"
                        | "from"
                        | "del"
                        | "match"
                        | "yield"
                )
            })
        {
            self.body_supported = false;
        }
        self.body.push(line.to_string());
    }
}

fn python_words(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|word| !word.is_empty())
}

fn plain_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && !matches!(
            name,
            "None"
                | "True"
                | "False"
                | "and"
                | "as"
                | "assert"
                | "async"
                | "await"
                | "break"
                | "class"
                | "continue"
                | "def"
                | "del"
                | "elif"
                | "else"
                | "except"
                | "finally"
                | "for"
                | "from"
                | "global"
                | "if"
                | "import"
                | "in"
                | "is"
                | "lambda"
                | "nonlocal"
                | "not"
                | "or"
                | "pass"
                | "raise"
                | "return"
                | "try"
                | "while"
                | "with"
                | "yield"
        )
}

fn plain_parameters(parameters: &str) -> Option<HashSet<String>> {
    let mut names = HashSet::new();
    if parameters.trim().is_empty() {
        return Some(names);
    }
    for parameter in parameters.split(',') {
        let name = parameter.trim();
        if !plain_identifier(name) || !names.insert(name.to_string()) {
            return None;
        }
    }
    Some(names)
}

// Some(None) is an unsupported import, None is not an import line.
fn plain_import_bindings(line: &str) -> Option<Option<Vec<String>>> {
    let (names, modules) = if let Some(rest) = line.strip_prefix("import ") {
        (rest, true)
    } else if line.starts_with("from ") {
        let Some((_, names)) = line.split_once(" import ") else {
            return Some(None);
        };
        (names, false)
    } else {
        return None;
    };
    let mut bindings = Vec::new();
    for item in names.split(',') {
        let parts: Vec<_> = item.split_whitespace().collect();
        let bound = match parts.as_slice() {
            [name] if modules => name.split('.').next().unwrap_or(""),
            [name] => name,
            [_, "as", alias] => alias,
            _ => return Some(None),
        };
        if !plain_identifier(bound) {
            return Some(None);
        }
        bindings.push(bound.to_string());
    }
    Some(Some(bindings))
}

fn balanced_inline(line: &str) -> bool {
    let mut stack = Vec::new();
    for ch in line.chars() {
        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' | ']' | '}' => {
                if stack.pop()
                    != Some(match ch {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    })
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for PythonExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let text = String::from_utf8_lossy(content);
        let cleaned_lines = strip_comments_and_strings(&text, CommentSyntax::PYTHON);
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut unresolved_refs = Vec::new();

        // File node
        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: vec!["python".into()],
            line: 1,
            end_line: text.lines().count() as u32,
        });

        // Track pending decorator for association with the next def/class
        let mut pending_decorators: Vec<String> = Vec::new();
        // Track last defined function/class node_id for decorator edge emission
        let mut last_node_id: Option<String> = None;

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;

            // Task #3: Decorator extraction
            if let Some(caps) = self.re_decorator.captures(line) {
                let decorator_name = caps.get(1).unwrap().as_str().to_string();
                pending_decorators.push(decorator_name);
                continue; // decorators are standalone lines, proceed to next
            }

            if let Some(caps) = self.re_class.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::class::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Class,
                    tags: vec!["python".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id.clone(),
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Emit decorator reference edges
                for dec in pending_decorators.drain(..) {
                    let ref_id = format!("ref::{}", dec);
                    edges.push(ExtractedEdge {
                        source: node_id.clone(),
                        target: ref_id.clone(),
                        relation: "references".into(),
                        weight: 0.4,
                    });
                    if !unresolved_refs.contains(&ref_id) {
                        unresolved_refs.push(ref_id);
                    }
                }
                last_node_id = Some(node_id);
            } else if let Some(caps) = self.re_func.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                // Disambiguate same-named defs in one file (two class methods named
                // `save`, a redefined top-level fn) so add_node does not drop the
                // sibling. First keeps the clean id; later siblings get `#2`/`#3`.
                let node_id = super::unique_node_id(&nodes, &format!("{}::fn::{}", file_id, name));
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["python".into()],
                    line: ln,
                    end_line: ln,
                });
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: node_id.clone(),
                    relation: "contains".into(),
                    weight: 1.0,
                });
                // Emit decorator reference edges
                for dec in pending_decorators.drain(..) {
                    let ref_id = format!("ref::{}", dec);
                    edges.push(ExtractedEdge {
                        source: node_id.clone(),
                        target: ref_id.clone(),
                        relation: "references".into(),
                        weight: 0.4,
                    });
                    if !unresolved_refs.contains(&ref_id) {
                        unresolved_refs.push(ref_id);
                    }
                }
                last_node_id = Some(node_id);
            } else {
                // If we accumulated decorators but hit a non-def/non-class line,
                // clear them (stale decorators should not bleed into later defs).
                if !pending_decorators.is_empty() && !line.trim().is_empty() {
                    pending_decorators.clear();
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
            } else if let Some(caps) = self.re_from_import.captures(line) {
                let module = caps.get(1).unwrap().as_str();
                let ref_id = format!("ref::{}", module);
                edges.push(ExtractedEdge {
                    source: file_id.to_string(),
                    target: ref_id.clone(),
                    relation: "imports".into(),
                    weight: 0.5,
                });
                unresolved_refs.push(ref_id);

                // Extract named imports: from X import A, B, C
                if let Some(names_caps) = self.re_from_import_names.captures(line) {
                    let names_str = names_caps.get(1).unwrap().as_str();
                    for name in names_str.split(',') {
                        let name = name.trim().split(" as ").next().unwrap_or("").trim();
                        if !name.is_empty() && name != "*" {
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

            // Class inheritance: class Foo(Bar, Baz)
            if let Some(caps) = self.re_class_inherit.captures(line) {
                let bases = caps.get(1).unwrap().as_str();
                for base in bases.split(',') {
                    let base = base.trim();
                    if !base.is_empty() && base != "object" {
                        let ref_id = format!("ref::{}", base);
                        if !unresolved_refs.contains(&ref_id) {
                            edges.push(ExtractedEdge {
                                source: file_id.to_string(),
                                target: ref_id.clone(),
                                relation: "implements".into(),
                                weight: 0.7,
                            });
                            unresolved_refs.push(ref_id);
                        }
                    }
                }
            }

            // Type hints: : TypeName, -> TypeName
            // Also: method calls (Task #7)
            if !line.trim_start().starts_with("import") && !line.trim_start().starts_with("from") {
                for caps in self.re_type_hint.captures_iter(line) {
                    let type_name = caps.get(1).unwrap().as_str();
                    if !matches!(
                        type_name,
                        "None"
                            | "True"
                            | "False"
                            | "Any"
                            | "Optional"
                            | "List"
                            | "Dict"
                            | "Tuple"
                            | "Set"
                            | "Union"
                            | "Type"
                            | "Callable"
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

                // Task #7: receiver.method() calls -> "calls" edges
                if !line.trim_start().starts_with("def ")
                    && !line.trim_start().starts_with("class ")
                    && !line.trim_start().starts_with("@")
                {
                    for caps in self.re_method_call.captures_iter(line) {
                        let receiver = caps.get(1).unwrap().as_str();
                        let method = caps.get(2).unwrap().as_str();
                        // Skip self.method and cls.method (internal calls, not cross-refs)
                        if receiver == "self" || receiver == "cls" || receiver == "super" {
                            continue;
                        }
                        // If receiver starts with uppercase, it's likely a type call: Type.method(
                        let ref_target =
                            if receiver.chars().next().is_some_and(|c| c.is_uppercase()) {
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
                }
            }
        }

        edges.extend(self.plain_call_edges(&text, &nodes));

        Ok(ExtractionResult {
            nodes,
            edges,
            unresolved_refs,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["py", "pyi"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Extractor;

    #[test]
    fn python_plain_calls_exact_sentinel_has_direct_function_origin() {
        let src = "def probe_target_9c32(): return 1\ndef probe_caller_9c32(): return probe_target_9c32()\n";
        let result = PythonExtractor::new()
            .extract(src.as_bytes(), "file::probe.py")
            .unwrap();
        assert!(result.edges.iter().any(|edge| edge.source
            == "file::probe.py::fn::probe_caller_9c32"
            && edge.target == "file::probe.py::fn::probe_target_9c32"
            && edge.relation == "calls"));
    }

    fn plain_calls(src: &str) -> Vec<(String, String)> {
        let result = PythonExtractor::new()
            .extract(src.as_bytes(), "file::probe.py")
            .unwrap();
        result
            .edges
            .into_iter()
            .filter(|edge| edge.relation == "calls" && edge.source != "file::probe.py")
            .map(|edge| (edge.source, edge.target))
            .collect()
    }

    fn pair(caller: &str, target: &str) -> (String, String) {
        (
            format!("file::probe.py::fn::{caller}"),
            format!("file::probe.py::fn::{target}"),
        )
    }

    #[test]
    fn python_plain_calls_forward_multiple_origins_and_repeated_sites() {
        let src = "def caller_a():\n    return helper() + helper()\ndef caller_b(value):\n    return helper(value)\ndef helper(value):\n    return value\n";
        assert_eq!(
            plain_calls(src),
            vec![pair("caller_a", "helper"), pair("caller_b", "helper")]
        );
    }

    #[test]
    fn python_plain_calls_recursion_async_and_unrelated_imports() {
        let src = "import unrelated.module\nfrom somewhere import other as unrelated_name\ndef recursive(value): return recursive(value)\nasync def caller():\n    return await helper()\ndef helper(): return 1\n";
        assert_eq!(
            plain_calls(src),
            vec![pair("recursive", "recursive"), pair("caller", "helper")]
        );
    }

    #[test]
    fn python_plain_calls_use_existing_declaration_identity_after_class_method() {
        let src = "class Worker:\n    def helper(self): return 0\ndef helper(): return 1\ndef caller(): return helper()\n";
        let result = PythonExtractor::new()
            .extract(src.as_bytes(), "file::probe.py")
            .unwrap();
        let actual = result
            .nodes
            .iter()
            .find(|node| node.node_type == NodeType::Function && node.line == 3)
            .unwrap();
        assert_eq!(actual.id, "file::probe.py::fn::helper#2");
        assert_eq!(
            plain_calls(src),
            vec![("file::probe.py::fn::caller".into(), actual.id.clone())]
        );
    }

    #[test]
    fn python_plain_calls_remove_only_call_keeps_function_nodes() {
        let before = "def helper(): return 1\ndef caller(): return helper()\n";
        let after = "def helper(): return 1\ndef caller(): return 1\n";
        let extractor = PythonExtractor::new();
        let old = extractor
            .extract(before.as_bytes(), "file::probe.py")
            .unwrap();
        let new = extractor
            .extract(after.as_bytes(), "file::probe.py")
            .unwrap();
        assert_eq!(
            old.nodes.iter().map(|node| &node.id).collect::<Vec<_>>(),
            new.nodes.iter().map(|node| &node.id).collect::<Vec<_>>()
        );
        assert_eq!(plain_calls(before), vec![pair("caller", "helper")]);
        assert!(plain_calls(after).is_empty());
    }

    #[test]
    fn python_plain_calls_omit_shadowed_or_locally_rebound_targets() {
        for caller in [
            "def caller(helper): return helper()\n",
            "def caller():\n    helper = replacement\n    return helper()\n",
            "def caller():\n    helper()\n    helper = replacement\n",
            "def caller():\n    helper: Callable\n    return helper()\n",
            "def caller():\n    import helper\n    return helper()\n",
            "def caller():\n    from elsewhere import helper\n    return helper()\n",
            "def caller():\n    def helper(): return 2\n    return helper()\n",
            "def caller():\n    def inner(): return helper()\n    return inner()\n",
            "def caller():\n    global helper\n    return helper()\n",
        ] {
            assert!(
                plain_calls(&format!("def helper(): return 1\n{caller}")).is_empty(),
                "{caller}"
            );
        }
    }

    #[test]
    fn python_plain_calls_omit_module_rebindings_duplicates_and_imported_names() {
        for binding in [
            "helper = replacement\n",
            "helper += replacement\n",
            "import helper\n",
            "import elsewhere as helper\n",
            "from elsewhere import helper\n",
            "from elsewhere import other as helper\n",
            "from elsewhere import *\n",
            "def helper(): return 2\n",
            "class helper: pass\n",
            "other = helper = replacement\n",
            "other = (helper := replacement)\n",
            "other = 0; helper = replacement\n",
            "\"doc\"; helper = replacement\n",
            "\"x\" or (helper := replacement)\n",
            "constant = \"#\"; helper = replacement\n",
            "constant = \"#\" or (helper := replacement)\n",
        ] {
            let src = format!("def helper(): return 1\n{binding}def caller(): return helper()\n");
            assert!(plain_calls(&src).is_empty(), "{binding}");
        }
    }

    #[test]
    fn python_plain_calls_do_not_infer_attributes_headers_or_decorated_bindings() {
        for body in [
            "def caller(): return obj.helper()\n",
            "def caller(): return self.helper() + cls.helper() + super().helper()\n",
            "def caller(value=helper()): return 1\n",
            "def caller(value: Helper): return helper()\n",
            "@helper()\ndef caller(): return 1\n",
            "@decorator\ndef caller(): return helper()\n",
            "class Worker:\n    def caller(self): return helper()\n",
        ] {
            assert!(
                plain_calls(&format!("def helper(): return 1\n{body}")).is_empty(),
                "{body}"
            );
        }
        assert!(
            plain_calls("@decorator\ndef helper(): return 1\ndef caller(): return helper()\n")
                .is_empty()
        );
    }

    #[test]
    fn python_plain_calls_omit_strings_comments_and_unsupported_lexical_scopes() {
        for body in [
            "def caller(): return \"helper()\"\n",
            "def caller():\n    'helper()'\n    return 1\n",
            "def caller():\n    # helper()\n    return 1\n",
            "def caller():\n    '''\n    helper()\n    '''\n    return 1\n",
            "def caller():\n    \"\"\"\n    helper()\n    \"\"\"\n    return 1\n",
            "def caller(): return lambda: helper()\n",
            "def caller(): return [helper() for item in items]\n",
            "def caller():\n    if condition:\n        return helper()\n",
            "def caller():\n    return helper(\n    )\n",
            "def caller():\n    return \\\n        helper()\n",
        ] {
            assert!(
                plain_calls(&format!("def helper(): return 1\n{body}")).is_empty(),
                "{body}"
            );
        }
    }

    #[test]
    fn python_plain_calls_never_emit_unqualified_external_references() {
        let result = PythonExtractor::new()
            .extract(
                b"def caller(): return unique_external()\n",
                "file::probe.py",
            )
            .unwrap();
        assert!(!result.edges.iter().any(|edge| edge.relation == "calls"));
        assert!(!result
            .unresolved_refs
            .iter()
            .any(|reference| reference.contains("unique_external")));
    }

    #[test]
    fn python_same_name_methods_in_one_file_get_distinct_ids() {
        // Two classes in one file each define a `save` method. Both must survive as
        // DISTINCT function nodes — without id disambiguation the second collides on
        // `…::fn::save` and add_node drops it (the method vanishes from the graph).
        let src = "class A:\n    def save(self):\n        pass\nclass B:\n    def save(self):\n        pass\n";
        let result = PythonExtractor::new()
            .extract(src.as_bytes(), "file::testfile.py")
            .unwrap();
        let save_ids: Vec<&str> = result
            .nodes
            .iter()
            .filter(|n| n.label == "save" && n.node_type == NodeType::Function)
            .map(|n| n.id.as_str())
            .collect();
        assert_eq!(
            save_ids.len(),
            2,
            "expected two distinct `save` method nodes, got ids {:?}",
            save_ids
        );
        assert!(
            save_ids.contains(&"file::testfile.py::fn::save"),
            "first occurrence should keep the clean id: {:?}",
            save_ids
        );
        assert!(
            save_ids.contains(&"file::testfile.py::fn::save#2"),
            "second occurrence should be disambiguated to `#2`: {:?}",
            save_ids
        );
    }
}
