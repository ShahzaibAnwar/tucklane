//! Local filename hints. These are simple, explainable keyword matches on the
//! filename — no file contents, no network, no model.
//!
//! [`ClassificationProvider`] is the extension point for a future local model;
//! no such model ships with Tucklane today.

use super::kinds::{FileKind, Preset};

pub struct Hint {
    pub folder: String,
    pub keyword: &'static str,
    pub confidence: f32,
}

/// Interface for optional classifiers that suggest a destination from file
/// metadata. Implementations must run locally and must not read file
/// contents unless content inspection is enabled by the user.
pub trait ClassificationProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn suggest(&self, name: &str, kind: Option<FileKind>, preset: Preset) -> Option<Hint>;
}

/// Splits a filename stem into lowercase alphanumeric tokens.
fn tokens(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in stem.chars() {
        if ch.is_alphanumeric() {
            // Split CamelCase boundaries: "SalesReport" -> sales, report.
            if ch.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = ch.is_lowercase();
            cur.extend(ch.to_lowercase());
        } else {
            prev_lower = false;
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub struct KeywordProvider;

impl ClassificationProvider for KeywordProvider {
    fn name(&self) -> &'static str {
        "Filename keywords"
    }

    fn suggest(&self, name: &str, kind: Option<FileKind>, preset: Preset) -> Option<Hint> {
        use FileKind::*;
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        let toks = tokens(stem);
        let has =
            |words: &[&'static str]| words.iter().copied().find(|w| toks.iter().any(|t| t == w));
        let doc_like = matches!(kind, Some(Pdf | Word | Spreadsheet | Text | Image));
        let docs = "Documents";

        if doc_like {
            if let Some(k) = has(&["invoice", "invoices", "inv", "factura", "rechnung"]) {
                return Some(Hint {
                    folder: format!("{docs}/Invoices"),
                    keyword: k,
                    confidence: 0.8,
                });
            }
            if let Some(k) = has(&["receipt", "receipts", "quittung"]) {
                return Some(Hint {
                    folder: format!("{docs}/Receipts"),
                    keyword: k,
                    confidence: 0.8,
                });
            }
        }
        if matches!(kind, Some(Pdf | Word)) {
            if let Some(k) = has(&["resume", "résumé", "cv", "lebenslauf"]) {
                return Some(Hint {
                    folder: format!("{docs}/Resumes"),
                    keyword: k,
                    confidence: 0.75,
                });
            }
        }
        if matches!(kind, Some(Pdf | Word | Spreadsheet | Presentation)) {
            if let Some(k) = has(&["report", "reports"]) {
                return Some(Hint {
                    folder: format!("{docs}/Reports"),
                    keyword: k,
                    confidence: 0.7,
                });
            }
        }
        if kind == Some(Image) && preset == Preset::Advanced {
            if let Some(k) = has(&["screenshot", "screenshots", "screen", "capture"]) {
                return Some(Hint {
                    folder: "Media/Images/Screenshots".into(),
                    keyword: k,
                    confidence: 0.75,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes() {
        assert_eq!(tokens("Invoice-PIVF"), vec!["invoice", "pivf"]);
        assert_eq!(tokens("SalesReport_2026"), vec!["sales", "report", "2026"]);
    }

    #[test]
    fn hints() {
        let p = KeywordProvider;
        let h = p
            .suggest("Invoice-PIVF.pdf", Some(FileKind::Pdf), Preset::Balanced)
            .unwrap();
        assert_eq!(h.folder, "Documents/Invoices");
        let h = p
            .suggest("Resume-John.pdf", Some(FileKind::Pdf), Preset::Balanced)
            .unwrap();
        assert_eq!(h.folder, "Documents/Resumes");
        let h = p
            .suggest(
                "Sales-Report.xlsx",
                Some(FileKind::Spreadsheet),
                Preset::Balanced,
            )
            .unwrap();
        assert_eq!(h.folder, "Documents/Reports");
        assert!(p
            .suggest("Setup.exe", Some(FileKind::Installer), Preset::Balanced)
            .is_none());
        assert!(p
            .suggest("reinvoice.pdf", Some(FileKind::Pdf), Preset::Balanced)
            .is_none());
    }
}
