//! Auto-detect document format and route to the correct ingest adapter.
//!
//! Usage:
//!   let (format, adapter) = DocumentRouter::detect(path);
//!   let (graph, stats) = adapter.ingest(root)?;

use crate::{
    BibTexAdapter, IngestAdapter, JatsArticleAdapter, L1ghtIngestAdapter, PatentIngestAdapter,
};
use std::path::Path;

/// Detected document format
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocumentFormat {
    /// USPTO/EPO patent XML
    Patent,
    /// PubMed NLM or JATS Z39.96 scientific article XML
    JatsArticle,
    /// BibTeX bibliography file
    BibTeX,
    /// L1GHT protocol Markdown
    L1ght,
    /// Source code or unknown format
    Code,
}

impl std::fmt::Display for DocumentFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Patent => write!(f, "patent"),
            Self::JatsArticle => write!(f, "article"),
            Self::BibTeX => write!(f, "bibtex"),
            Self::L1ght => write!(f, "light"),
            Self::Code => write!(f, "code"),
        }
    }
}

/// Document format router — detects format and returns the appropriate adapter.
pub struct DocumentRouter;

impl DocumentRouter {
    /// Detect format from a single file and return an adapter.
    /// Returns `None` for the adapter if format is Code (handled by M1nd's default pipeline).
    pub fn detect(path: &Path) -> (DocumentFormat, Option<Box<dyn IngestAdapter>>) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();

        // BibTeX by extension
        if matches!(ext.as_str(), "bib" | "bibtex") {
            return (
                DocumentFormat::BibTeX,
                Some(Box::new(BibTexAdapter::new(None))),
            );
        }

        // Markdown — check for L1GHT protocol
        if matches!(ext.as_str(), "md" | "markdown") {
            if let Ok(content) = std::fs::read_to_string(path) {
                if content.contains("Protocol: L1GHT") || content.contains("protocol: l1ght") {
                    return (
                        DocumentFormat::L1ght,
                        Some(Box::new(L1ghtIngestAdapter::new(None))),
                    );
                }
            }
            return (DocumentFormat::Code, None);
        }

        // XML — inspect content header
        if matches!(ext.as_str(), "xml" | "nxml") {
            if let Ok(content) = std::fs::read_to_string(path) {
                let header: String = content.chars().take(2000).collect();
                return Self::detect_from_xml(&header);
            }
        }

        (DocumentFormat::Code, None)
    }

    fn detect_from_xml(content: &str) -> (DocumentFormat, Option<Box<dyn IngestAdapter>>) {
        // Patent
        if content.contains("<us-patent-grant")
            || content.contains("<us-patent-application")
            || content.contains("<ep-patent-document")
        {
            return (
                DocumentFormat::Patent,
                Some(Box::new(PatentIngestAdapter::new(None))),
            );
        }

        // PubMed / JATS
        if content.contains("<PubmedArticle")
            || content.contains("<PubmedArticleSet")
            || (content.contains("<article") && content.contains("dtd-version"))
            || content.contains("NLM//DTD")
        {
            return (
                DocumentFormat::JatsArticle,
                Some(Box::new(JatsArticleAdapter::new(None))),
            );
        }

        (DocumentFormat::Code, None)
    }

    /// Detect dominant format for a directory by sampling up to 20 files.
    pub fn detect_directory(root: &Path) -> (DocumentFormat, Option<Box<dyn IngestAdapter>>) {
        if !root.is_dir() {
            return Self::detect(root);
        }

        let mut counts = [0u32; 4]; // Patent, Article, BibTeX, L1ght

        for entry in walkdir::WalkDir::new(root)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .take(20)
        {
            let (fmt, _) = Self::detect(entry.path());
            match fmt {
                DocumentFormat::Patent => counts[0] += 1,
                DocumentFormat::JatsArticle => counts[1] += 1,
                DocumentFormat::BibTeX => counts[2] += 1,
                DocumentFormat::L1ght => counts[3] += 1,
                DocumentFormat::Code => {}
            }
        }

        let max_idx = counts
            .iter()
            .enumerate()
            .max_by_key(|(_, &c)| c)
            .map(|(i, _)| i)
            .unwrap_or(4);
        let max_count = counts.get(max_idx).copied().unwrap_or(0);

        if max_count == 0 {
            return (DocumentFormat::Code, None);
        }

        match max_idx {
            0 => (
                DocumentFormat::Patent,
                Some(Box::new(PatentIngestAdapter::new(None))),
            ),
            1 => (
                DocumentFormat::JatsArticle,
                Some(Box::new(JatsArticleAdapter::new(None))),
            ),
            2 => (
                DocumentFormat::BibTeX,
                Some(Box::new(BibTexAdapter::new(None))),
            ),
            3 => (
                DocumentFormat::L1ght,
                Some(Box::new(L1ghtIngestAdapter::new(None))),
            ),
            _ => (DocumentFormat::Code, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_patent_xml() {
        let dir = std::env::temp_dir().join("router-patent");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("p.xml"),
            "<us-patent-grant><doc-number>123</doc-number></us-patent-grant>",
        )
        .unwrap();
        let (fmt, adapter) = DocumentRouter::detect(&dir.join("p.xml"));
        assert_eq!(fmt, DocumentFormat::Patent);
        assert!(adapter.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_pubmed_xml() {
        let dir = std::env::temp_dir().join("router-pubmed");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.xml"),
            "<PubmedArticleSet><PubmedArticle></PubmedArticle></PubmedArticleSet>",
        )
        .unwrap();
        let (fmt, adapter) = DocumentRouter::detect(&dir.join("a.xml"));
        assert_eq!(fmt, DocumentFormat::JatsArticle);
        assert!(adapter.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detects_bibtex() {
        let dir = std::env::temp_dir().join("router-bib");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("refs.bib"), "@article{test, title={Test}}").unwrap();
        let (fmt, adapter) = DocumentRouter::detect(&dir.join("refs.bib"));
        assert_eq!(fmt, DocumentFormat::BibTeX);
        assert!(adapter.is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_xml_is_code() {
        let dir = std::env::temp_dir().join("router-unknown");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.xml"), "<config><debug/></config>").unwrap();
        let (fmt, adapter) = DocumentRouter::detect(&dir.join("config.xml"));
        assert_eq!(fmt, DocumentFormat::Code);
        assert!(adapter.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn directory_detection() {
        let dir = std::env::temp_dir().join("router-dir-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bib"), "@article{x, title={A}}").unwrap();
        std::fs::write(dir.join("b.bib"), "@article{y, title={B}}").unwrap();
        let (fmt, _) = DocumentRouter::detect_directory(&dir);
        assert_eq!(fmt, DocumentFormat::BibTeX);
        std::fs::remove_dir_all(&dir).ok();
    }
}
