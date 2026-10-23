//! Deterministic classification pipeline.
//!
//! Priority order:
//! 1. Exact-file exclusions and protected filename patterns
//! 2. User rules (by priority)
//! 3. Verified file signature (opt-in, only for missing/unknown/ambiguous extensions)
//! 4. Filename keyword hints (opt-in)
//! 5. Extension
//! 6. Other / manual review

pub mod kinds;
pub mod signature;
pub mod smart;

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use crate::fsops::names::extension_of;
use crate::rules::{wildcard_match, FileFacts, Rule, RuleAction};
use crate::settings::{Settings, UnknownFiles};
use kinds::{preset_folder, FileKind, Preset};
use smart::{ClassificationProvider, KeywordProvider};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Exclusion,
    Protected,
    IgnoredExtension,
    Rule,
    Signature,
    FilenameHint,
    Extension,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Decision {
    /// Move into `folder`. `confirm` means: never automatically; previews
    /// show it unselected.
    Move { folder: String, confirm: bool },
    /// Leave in place, deliberately.
    Ignore,
    /// Leave in place and flag for the user.
    Review,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub decision: Decision,
    pub kind: Option<FileKind>,
    pub kind_label: Option<String>,
    pub source: Source,
    pub reason: String,
    /// 1.0 for deterministic decisions; lower for hints.
    pub confidence: f32,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    /// Other enabled rules that also matched (lower priority).
    pub also_matched: Vec<String>,
}

pub struct ClassifyContext<'a> {
    pub preset: Preset,
    /// Enabled rules sorted by ascending priority.
    pub rules: &'a [Rule],
    pub settings: &'a Settings,
    /// Uppercased relative paths the user excluded.
    pub excluded: &'a HashSet<String>,
    /// Minimum confidence required for non-deterministic sources. Automatic
    /// organization uses a high bar so hints never move files on their own.
    pub min_confidence: f32,
    pub now_ms: i64,
}

pub struct FileInput<'a> {
    pub rel: &'a str,
    pub name: &'a str,
    pub size: u64,
    pub modified_ms: i64,
    pub created_ms: i64,
    /// Absolute path, used only for opt-in signature sniffing.
    pub abs: Option<&'a Path>,
}

fn base(decision: Decision, source: Source, reason: String) -> Classification {
    Classification {
        decision,
        kind: None,
        kind_label: None,
        source,
        reason,
        confidence: 1.0,
        rule_id: None,
        rule_name: None,
        also_matched: vec![],
    }
}

pub fn classify(ctx: &ClassifyContext, f: &FileInput) -> Classification {
    let ext = extension_of(f.name);
    let ext_kind = ext.as_deref().and_then(FileKind::from_extension);
    let mut kind = ext_kind;

    // 1. Exclusions and protection.
    if ctx.excluded.contains(&f.rel.to_uppercase()) {
        return base(
            Decision::Ignore,
            Source::Exclusion,
            "You excluded this file from organization.".into(),
        );
    }
    if let Some(p) = ctx
        .settings
        .protected_patterns
        .iter()
        .find(|p| wildcard_match(p, f.name))
    {
        return base(
            Decision::Ignore,
            Source::Protected,
            format!("Protected by the pattern \"{p}\"."),
        );
    }
    if let Some(e) = ext.as_deref() {
        let last = e.rsplit('.').next().unwrap_or(e);
        if ctx
            .settings
            .ignored_extensions
            .iter()
            .any(|i| i == e || i == last)
        {
            return base(
                Decision::Ignore,
                Source::IgnoredExtension,
                format!("Files ending in .{e} are ignored in your settings."),
            );
        }
    }

    // Opt-in signature check, used both to inform rules (KindIs) and as step 3.
    let ambiguous_ext = matches!(ext.as_deref(), Some("ts") | Some("bin") | Some("dat"));
    let mut sniffed: Option<(FileKind, &'static str)> = None;
    if ctx.settings.content_signatures && (ext_kind.is_none() || ambiguous_ext) {
        if let Some(abs) = f.abs {
            sniffed = signature::sniff_path(abs);
            if let Some((k, _)) = sniffed {
                kind = Some(k);
            }
        }
    }

    // 2. Rules.
    let facts = FileFacts {
        name: f.name,
        ext: ext.as_deref(),
        size: f.size,
        modified_ms: f.modified_ms,
        created_ms: f.created_ms,
        kind,
        now_ms: ctx.now_ms,
    };
    let mut matched = ctx.rules.iter().filter(|r| r.matches(&facts));
    if let Some(rule) = matched.next() {
        let also: Vec<String> = matched.map(|r| r.name.clone()).collect();
        let (decision, reason) = match &rule.action {
            RuleAction::MoveTo { folder, confirm } => (
                Decision::Move {
                    folder: folder.clone(),
                    confirm: *confirm,
                },
                format!("Matched rule \"{}\".", rule.name),
            ),
            RuleAction::Ignore => (
                Decision::Ignore,
                format!("Rule \"{}\" says to leave it here.", rule.name),
            ),
            RuleAction::Review => (
                Decision::Review,
                format!("Rule \"{}\" flags it for review.", rule.name),
            ),
        };
        let mut c = base(decision, Source::Rule, reason);
        c.kind = kind;
        c.kind_label = kind.map(|k| k.label().to_string());
        c.rule_id = Some(rule.id.clone());
        c.rule_name = Some(rule.name.clone());
        c.also_matched = also;
        return c;
    }

    // 3. Verified signature.
    if let Some((k, label)) = sniffed {
        if ext_kind != Some(k) && 0.95 >= ctx.min_confidence {
            let mut c = base(
                Decision::Move {
                    folder: preset_folder(ctx.preset, k).into(),
                    confirm: false,
                },
                Source::Signature,
                match ext.as_deref() {
                    Some(e) => {
                        format!("File header identifies a {label}, despite the .{e} extension.")
                    }
                    None => format!("No extension; the file header identifies a {label}."),
                },
            );
            c.kind = Some(k);
            c.kind_label = Some(k.label().into());
            c.confidence = 0.95;
            return c;
        }
    }

    // 4. Filename hints.
    if ctx.settings.smart_filename_hints {
        if let Some(h) = KeywordProvider.suggest(f.name, kind, ctx.preset) {
            if h.confidence >= ctx.min_confidence {
                let mut c = base(
                    Decision::Move {
                        folder: h.folder,
                        confirm: false,
                    },
                    Source::FilenameHint,
                    format!(
                        "Filename contains \"{}\" (filename hint, not content analysis).",
                        h.keyword
                    ),
                );
                c.kind = kind;
                c.kind_label = kind.map(|k| k.label().to_string());
                c.confidence = h.confidence;
                return c;
            }
        }
    }

    // 5. Extension.
    if let Some(k) = kind {
        let mut c = base(
            Decision::Move {
                folder: preset_folder(ctx.preset, k).into(),
                confirm: false,
            },
            Source::Extension,
            match ext.as_deref() {
                Some(e) => format!("{} (.{e} extension).", k.label()),
                None => format!("{}.", k.label()),
            },
        );
        c.kind = Some(k);
        c.kind_label = Some(k.label().into());
        return c;
    }

    // 6. Unknown.
    match ctx.settings.unknown_files {
        UnknownFiles::MoveToOther => base(
            Decision::Move {
                folder: "Other".into(),
                confirm: false,
            },
            Source::Unknown,
            match ext {
                Some(e) => {
                    format!("Unrecognized .{e} file; your settings send unknown files to Other.")
                }
                None => "No extension; your settings send unknown files to Other.".into(),
            },
        ),
        UnknownFiles::Leave => base(
            Decision::Review,
            Source::Unknown,
            match ext {
                Some(e) => format!("Unrecognized .{e} file — left in place for you to review."),
                None => "No extension — left in place for you to review.".into(),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Condition, MatchMode};

    fn ctx<'a>(s: &'a Settings, rules: &'a [Rule], ex: &'a HashSet<String>) -> ClassifyContext<'a> {
        ClassifyContext {
            preset: s.preset,
            rules,
            settings: s,
            excluded: ex,
            min_confidence: 0.0,
            now_ms: 0,
        }
    }

    fn input(name: &str) -> FileInput<'_> {
        FileInput {
            rel: name,
            name,
            size: 10,
            modified_ms: 0,
            created_ms: 0,
            abs: None,
        }
    }

    #[test]
    fn extension_defaults() {
        let s = Settings::default();
        let ex = HashSet::new();
        let c = ctx(&s, &[], &ex);
        let cases = [
            ("report.pdf", "Documents/PDF"),
            ("setup.exe", "Software/Installers"),
            ("movie.mp4", "Media/Videos"),
            ("photo.JPEG", "Media/Images"),
            ("song.flac", "Media/Audio"),
            ("subs.srt", "Media/Subtitles"),
            ("data.tar.gz", "Archives"),
            ("app.ts", "Developer/Code"),
            ("config.yaml", "Developer/Data"),
            ("budget.xlsx", "Documents/Excel"),
            ("deck.pptx", "Documents/Presentations"),
            ("ubuntu.torrent", "Other/Torrents"),
        ];
        for (n, folder) in cases {
            let r = classify(&c, &input(n));
            assert_eq!(
                r.decision,
                Decision::Move {
                    folder: folder.into(),
                    confirm: false
                },
                "{n}"
            );
            assert_eq!(r.source, Source::Extension);
        }
        let unknown = classify(&c, &input("mystery.qqq"));
        assert_eq!(unknown.decision, Decision::Review);
    }

    #[test]
    fn priority_order() {
        let s = Settings {
            smart_filename_hints: true,
            ..Settings::default()
        };
        let rules = vec![Rule {
            id: "r1".into(),
            name: "Big videos".into(),
            description: String::new(),
            priority: 1,
            enabled: true,
            match_mode: MatchMode::All,
            conditions: vec![Condition::NameContains {
                value: "invoice".into(),
            }],
            action: RuleAction::MoveTo {
                folder: "Finance".into(),
                confirm: false,
            },
            created_at: String::new(),
            updated_at: String::new(),
            last_triggered_at: None,
            match_count: 0,
        }];
        let mut ex = HashSet::new();
        ex.insert("KEEP.PDF".to_string());
        let c = ctx(&s, &rules, &ex);
        assert_eq!(classify(&c, &input("keep.pdf")).source, Source::Exclusion);
        assert_eq!(
            classify(&c, &input("desktop.ini")).source,
            Source::Protected
        );
        assert_eq!(classify(&c, &input("Invoice-1.pdf")).source, Source::Rule);
        let r = classify(&c, &input("Resume-John.pdf"));
        assert_eq!(r.source, Source::FilenameHint);
        // Automatic mode ignores low-confidence hints.
        let auto = ClassifyContext {
            min_confidence: 0.9,
            ..ctx(&s, &rules, &ex)
        };
        assert_eq!(
            classify(&auto, &input("Resume-John.pdf")).source,
            Source::Extension
        );
    }
}
