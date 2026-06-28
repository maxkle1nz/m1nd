// === crates/m1nd-ingest/src/extract/generic.rs ===

use super::{
    strip_comments_and_strings, CommentSyntax, ExtractedEdge, ExtractedNode, ExtractionResult,
    Extractor,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::types::NodeType;
use regex::Regex;

/// Fallback regex extractor for unsupported languages.
/// Matches common patterns: def/function/fn/func/sub/proc.
pub struct GenericExtractor {
    re_func: Regex,
    re_class: Regex,
}

impl GenericExtractor {
    pub fn new() -> Self {
        Self {
            re_func: Regex::new(r"^\s*(?:def|function|fn|func|sub|proc)\s+(\w+)").unwrap(),
            re_class: Regex::new(r"^\s*(?:class|struct|type|record)\s+(\w+)").unwrap(),
        }
    }
}

impl Default for GenericExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for GenericExtractor {
    fn extract(&self, content: &[u8], file_id: &str) -> M1ndResult<ExtractionResult> {
        let text = String::from_utf8_lossy(content);
        let cleaned_lines = strip_comments_and_strings(&text, CommentSyntax::GENERIC);
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        let file_label = file_id.rsplit("::").next().unwrap_or(file_id);
        nodes.push(ExtractedNode {
            id: file_id.to_string(),
            label: file_label.to_string(),
            node_type: NodeType::File,
            tags: vec!["generic".into()],
            line: 1,
            end_line: text.lines().count() as u32,
        });

        for (line_num, line) in cleaned_lines.iter().enumerate() {
            let ln = (line_num + 1) as u32;

            if let Some(caps) = self.re_class.captures(line) {
                let name = caps.get(1).unwrap().as_str();
                let node_id = format!("{}::type::{}", file_id, name);
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Struct,
                    tags: vec!["generic".into()],
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
                // Disambiguate same-named defs in one file (two `def save` under
                // different blocks, a redefined top-level fn) so add_node does not
                // drop the sibling. First keeps the clean id; later get `#2`/`#3`.
                let node_id = super::unique_node_id(&nodes, &format!("{}::fn::{}", file_id, name));
                nodes.push(ExtractedNode {
                    id: node_id.clone(),
                    label: name.to_string(),
                    node_type: NodeType::Function,
                    tags: vec!["generic".into()],
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
        }

        Ok(ExtractionResult {
            nodes,
            edges,
            unresolved_refs: Vec::new(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &[] // matches nothing by default; used as fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_same_name_fns_in_one_file_get_distinct_ids() {
        // Two same-named defs in one file. Both must survive as DISTINCT function
        // nodes — without id disambiguation the second collides on `…::fn::save`
        // and add_node drops it (the def vanishes from the graph).
        let src = "class A\n    def save\nclass B\n    def save\n";
        let result = GenericExtractor::new()
            .extract(src.as_bytes(), "file::testfile.txt")
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
            "expected two distinct `save` fn nodes, got ids {:?}",
            save_ids
        );
        assert!(
            save_ids.contains(&"file::testfile.txt::fn::save"),
            "first occurrence should keep the clean id: {:?}",
            save_ids
        );
        assert!(
            save_ids.contains(&"file::testfile.txt::fn::save#2"),
            "second occurrence should be disambiguated to `#2`: {:?}",
            save_ids
        );
    }
}
