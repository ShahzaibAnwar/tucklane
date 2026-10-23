//! User-defined organization rules: conditions, actions and validation.

use serde::{Deserialize, Serialize};

use crate::classifier::kinds::FileKind;
use crate::error::{AppError, AppResult};
use crate::security::{split_rel, NameRule};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Condition {
    ExtensionIs {
        values: Vec<String>,
    },
    NameContains {
        value: String,
    },
    NameStartsWith {
        value: String,
    },
    NameEndsWith {
        value: String,
    },
    /// Wildcard pattern: `*` any run of characters, `?` one character.
    NameMatches {
        pattern: String,
    },
    SizeGreaterThan {
        bytes: u64,
    },
    SizeLessThan {
        bytes: u64,
    },
    ModifiedWithinDays {
        days: u32,
    },
    ModifiedOlderThanDays {
        days: u32,
    },
    CreatedWithinDays {
        days: u32,
    },
    KindIs {
        kind: FileKind,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    /// Move into a folder inside the authorized root. When `confirm` is set,
    /// matching files are listed in previews but never moved automatically.
    MoveTo {
        folder: String,
        #[serde(default)]
        confirm: bool,
    },
    /// Leave the file where it is.
    Ignore,
    /// Leave the file and flag it for later review.
    Review,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub priority: i64,
    pub enabled: bool,
    #[serde(default)]
    pub match_mode: MatchMode,
    pub conditions: Vec<Condition>,
    pub action: RuleAction,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub last_triggered_at: Option<String>,
    #[serde(default)]
    pub match_count: i64,
}

/// The editable part of a rule, as sent by the frontend or an import file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub match_mode: MatchMode,
    pub conditions: Vec<Condition>,
    pub action: RuleAction,
}

fn default_true() -> bool {
    true
}

/// Facts about a file used for rule matching. Only metadata, never contents.
#[derive(Debug, Clone)]
pub struct FileFacts<'a> {
    pub name: &'a str,
    pub ext: Option<&'a str>,
    pub size: u64,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub kind: Option<FileKind>,
    pub now_ms: i64,
}

const DAY_MS: i64 = 86_400_000;

/// Case-insensitive wildcard match supporting `*` and `?`.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn norm_ext(e: &str) -> String {
    e.trim().trim_start_matches('.').to_lowercase()
}

impl Condition {
    pub fn matches(&self, f: &FileFacts) -> bool {
        let lname = f.name.to_lowercase();
        match self {
            Condition::ExtensionIs { values } => f
                .ext
                .is_some_and(|e| values.iter().any(|v| norm_ext(v) == e)),
            Condition::NameContains { value } => lname.contains(&value.to_lowercase()),
            Condition::NameStartsWith { value } => lname.starts_with(&value.to_lowercase()),
            Condition::NameEndsWith { value } => {
                let stem = match f.ext {
                    Some(e) => lname
                        .strip_suffix(&format!(".{e}"))
                        .unwrap_or(&lname)
                        .to_string(),
                    None => lname.clone(),
                };
                let v = value.to_lowercase();
                stem.ends_with(&v) || lname.ends_with(&v)
            }
            Condition::NameMatches { pattern } => wildcard_match(pattern, f.name),
            Condition::SizeGreaterThan { bytes } => f.size > *bytes,
            Condition::SizeLessThan { bytes } => f.size < *bytes,
            Condition::ModifiedWithinDays { days } => {
                f.now_ms - f.modified_ms <= *days as i64 * DAY_MS
            }
            Condition::ModifiedOlderThanDays { days } => {
                f.now_ms - f.modified_ms > *days as i64 * DAY_MS
            }
            Condition::CreatedWithinDays { days } => {
                f.now_ms - f.created_ms <= *days as i64 * DAY_MS
            }
            Condition::KindIs { kind } => f.kind == Some(*kind),
        }
    }

    fn validate(&self) -> AppResult<()> {
        let text_ok = |s: &str| !s.trim().is_empty() && s.len() <= 200;
        let ok = match self {
            Condition::ExtensionIs { values } => {
                !values.is_empty()
                    && values.len() <= 50
                    && values.iter().all(|v| {
                        let e = norm_ext(v);
                        !e.is_empty()
                            && e.len() <= 20
                            && e.chars().all(|c| {
                                c.is_alphanumeric() || c == '.' || c == '!' || c == '_' || c == '-'
                            })
                    })
            }
            Condition::NameContains { value }
            | Condition::NameStartsWith { value }
            | Condition::NameEndsWith { value } => text_ok(value),
            Condition::NameMatches { pattern } => text_ok(pattern),
            Condition::ModifiedWithinDays { days }
            | Condition::ModifiedOlderThanDays { days }
            | Condition::CreatedWithinDays { days } => *days <= 36_500,
            Condition::SizeGreaterThan { .. }
            | Condition::SizeLessThan { .. }
            | Condition::KindIs { .. } => true,
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::invalid(
                "One of the rule conditions is incomplete or invalid.",
            ))
        }
    }
}

/// Validates a destination folder for a rule: relative, inside the root,
/// at most 8 levels deep, no reserved or invalid names.
pub fn validate_folder(folder: &str) -> AppResult<String> {
    let trimmed = folder.trim();
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") || trimmed.contains(':') {
        return Err(AppError::unauthorized(
            "Rules can only move files into folders inside the authorized folder.",
        ));
    }
    let cleaned = trimmed.trim_matches(['/', '\\']).to_string();
    let parts = split_rel(&cleaned, NameRule::Create).map_err(|e| match e {
        AppError::Unauthorized(_) => AppError::unauthorized(
            "Rules can only move files into folders inside the authorized folder.",
        ),
        other => other,
    })?;
    if parts.len() > 8 {
        return Err(AppError::invalid(
            "Destination folders can be at most 8 levels deep.",
        ));
    }
    if parts
        .iter()
        .any(|p| p.to_lowercase().starts_with(".tucklane"))
    {
        return Err(AppError::invalid(
            "That folder name is reserved by Tucklane.",
        ));
    }
    Ok(parts.join("/"))
}

impl RuleDraft {
    pub fn validate(mut self) -> AppResult<RuleDraft> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 100 {
            return Err(AppError::invalid("Rule name must be 1–100 characters."));
        }
        if self.description.chars().count() > 500 {
            return Err(AppError::invalid(
                "Rule description must be at most 500 characters.",
            ));
        }
        if self.conditions.is_empty() || self.conditions.len() > 20 {
            return Err(AppError::invalid(
                "A rule needs between 1 and 20 conditions.",
            ));
        }
        for c in &self.conditions {
            c.validate()?;
        }
        if let RuleAction::MoveTo { folder, .. } = &mut self.action {
            *folder = validate_folder(folder)?;
        }
        Ok(self)
    }
}

impl Rule {
    pub fn matches(&self, f: &FileFacts) -> bool {
        if !self.enabled || self.conditions.is_empty() {
            return false;
        }
        match self.match_mode {
            MatchMode::All => self.conditions.iter().all(|c| c.matches(f)),
            MatchMode::Any => self.conditions.iter().any(|c| c.matches(f)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(name: &str, size: u64) -> FileFacts<'_> {
        FileFacts {
            name,
            ext: name.rsplit_once('.').map(|(_, e)| e),
            size,
            modified_ms: 0,
            created_ms: 0,
            kind: None,
            now_ms: 3 * DAY_MS,
        }
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("invoice*.pdf", "Invoice-2026.PDF"));
        assert!(wildcard_match("*report?.xlsx", "sales report1.xlsx"));
        assert!(!wildcard_match("invoice*.pdf", "my invoice.pdf"));
        assert!(wildcard_match("*", ""));
    }

    #[test]
    fn conditions() {
        let f = facts("Invoice-PIVF.pdf", 600 * 1024 * 1024);
        assert!(Condition::ExtensionIs {
            values: vec![".PDF".into()]
        }
        .matches(&f));
        assert!(Condition::NameContains {
            value: "invoice".into()
        }
        .matches(&f));
        assert!(Condition::NameStartsWith {
            value: "inv".into()
        }
        .matches(&f));
        assert!(Condition::NameEndsWith {
            value: "pivf".into()
        }
        .matches(&f));
        assert!(Condition::SizeGreaterThan {
            bytes: 500 * 1024 * 1024
        }
        .matches(&f));
        assert!(!Condition::ModifiedWithinDays { days: 2 }.matches(&f));
        assert!(Condition::ModifiedOlderThanDays { days: 2 }.matches(&f));
    }

    #[test]
    fn folder_validation_blocks_escape() {
        assert_eq!(
            validate_folder("Documents/Invoices/").unwrap(),
            "Documents/Invoices"
        );
        for bad in [
            "../Desktop",
            "C:\\Users\\x\\Desktop",
            "\\\\server\\x",
            "a/../../b",
            "CON",
            "a:b",
            ".tucklane",
        ] {
            assert!(validate_folder(bad).is_err(), "{bad}");
        }
    }
}
