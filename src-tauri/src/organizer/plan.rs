//! Builds an organization plan: what would move where, and why. Planning
//! never modifies the filesystem.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::classifier::{classify, Classification, ClassifyContext, Decision, FileInput};
use crate::error::AppResult;
use crate::fsops::is_staging_name;
use crate::fsops::names::{name_key, next_free_name};
use crate::fsops::stability::{is_temp_download, readiness, Readiness};
use crate::rules::Rule;
use crate::scanner::{list_dir, Entry};
use crate::security::AuthorizedRootGuard;
use crate::settings::{CollisionPolicy, Settings};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Will move as proposed.
    Ready,
    /// Will move under a new name because the destination name is taken.
    Renamed,
    /// Destination name is taken and the user asked to decide.
    NeedsDecision,
    /// A rule requires explicit confirmation.
    Confirm,
    /// Still downloading or recently modified.
    Pending,
    /// Open in another program.
    Locked,
    /// Flagged for manual review.
    Review,
    /// Deliberately left in place (exclusion, protection, ignore rule).
    Ignored,
    /// Cannot be moved (e.g. collision with "skip" policy, unsafe destination).
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub id: String,
    pub source_rel: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
    pub dest_dir: Option<String>,
    pub dest_name: Option<String>,
    pub status: PlanStatus,
    /// Whether the item is selected by default in the preview.
    pub selected: bool,
    pub note: Option<String>,
    pub classification: Classification,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub files_scanned: usize,
    pub bytes_scanned: u64,
    pub folders_left: usize,
    pub hidden_skipped: usize,
    pub links_skipped: usize,
    pub proposed: usize,
    pub conflicts: usize,
    pub pending: usize,
    pub locked: usize,
    pub review: usize,
    pub ignored: usize,
    pub skipped: usize,
    pub categories: usize,
    pub by_folder: BTreeMap<String, usize>,
    pub by_rule: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub id: String,
    pub created_at: String,
    pub items: Vec<PlanItem>,
    pub summary: PlanSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanMode {
    /// Preview for the user: hints allowed, lock checks for movable files.
    Manual,
    /// Background organization: only high-confidence, non-confirm decisions.
    Auto,
}

pub struct PlanInputs<'a> {
    pub settings: &'a Settings,
    /// All rules; disabled ones are filtered here.
    pub rules: &'a [Rule],
    /// Uppercased relative paths excluded by the user.
    pub excluded: &'a HashSet<String>,
    /// Uppercased relative paths Tucklane itself put in the folder (restored
    /// by undo, exports). Never organized automatically; in a manual preview
    /// they are offered unselected.
    pub restored: &'a HashSet<String>,
    pub now_ms: i64,
    pub mode: PlanMode,
    /// Restrict planning to these top-level relative paths (None = all).
    pub only: Option<&'a HashSet<String>>,
}

/// Plans organization of the loose files at the top level of the root.
pub fn build_plan(guard: &AuthorizedRootGuard, inp: &PlanInputs) -> AppResult<Plan> {
    let entries = list_dir(guard, "")?;
    Ok(plan_entries(guard, entries, inp))
}

pub fn plan_entries(guard: &AuthorizedRootGuard, entries: Vec<Entry>, inp: &PlanInputs) -> Plan {
    let excluded: HashSet<String> = match inp.mode {
        PlanMode::Auto => inp.excluded.union(inp.restored).cloned().collect(),
        PlanMode::Manual => inp.excluded.clone(),
    };
    let mut rules: Vec<Rule> = inp.rules.iter().filter(|r| r.enabled).cloned().collect();
    rules.sort_by_key(|r| r.priority);
    let ctx = ClassifyContext {
        preset: inp.settings.preset,
        rules: &rules,
        settings: inp.settings,
        excluded: &excluded,
        min_confidence: if inp.mode == PlanMode::Auto { 0.9 } else { 0.0 },
        now_ms: inp.now_ms,
    };
    let mut summary = PlanSummary::default();
    let mut items = Vec::new();
    // Names claimed by earlier items in this plan, per destination folder.
    let mut reserved: HashMap<String, HashSet<String>> = HashMap::new();
    let mut probed: HashMap<String, AppResult<Option<std::path::PathBuf>>> = HashMap::new();
    let settle = inp.settings.settle_seconds as u64;

    let mut entries = entries;
    entries.sort_by_key(|a| a.name.to_lowercase());
    for e in entries {
        if let Some(only) = inp.only {
            if !only.contains(&e.rel.to_uppercase()) {
                continue;
            }
        }
        if e.is_dir {
            summary.folders_left += 1;
            continue;
        }
        if e.is_link {
            summary.links_skipped += 1;
            continue;
        }
        if e.hidden || is_staging_name(&e.name) {
            summary.hidden_skipped += 1;
            continue;
        }
        summary.files_scanned += 1;
        summary.bytes_scanned += e.size;

        let abs = guard.root().join(&e.name);
        let c = classify(
            &ctx,
            &FileInput {
                rel: &e.rel,
                name: &e.name,
                size: e.size,
                modified_ms: e.modified_ms,
                created_ms: e.created_ms,
                abs: Some(&abs),
            },
        );
        let mut item = PlanItem {
            id: uuid::Uuid::new_v4().to_string(),
            source_rel: e.rel.clone(),
            name: e.name.clone(),
            size: e.size,
            modified_ms: e.modified_ms,
            dest_dir: None,
            dest_name: None,
            status: PlanStatus::Ready,
            selected: false,
            note: None,
            classification: c.clone(),
        };

        if is_temp_download(&e.name) {
            // An in-progress download: never classify it as "unknown".
            item.status = PlanStatus::Pending;
            item.note = Some("Download still in progress.".into());
            summary.pending += 1;
            items.push(item);
            continue;
        }

        match &c.decision {
            Decision::Ignore => {
                item.status = PlanStatus::Ignored;
            }
            Decision::Review => {
                item.status = PlanStatus::Review;
            }
            Decision::Move { folder, confirm } => {
                item.dest_dir = Some(folder.clone());
                let ready = readiness(
                    guard.root(),
                    &e.name,
                    e.size,
                    e.modified_ms,
                    inp.now_ms,
                    settle,
                    true,
                );
                match ready {
                    Readiness::Ready => {}
                    Readiness::Locked => {
                        item.status = PlanStatus::Locked;
                        item.note = Some("Open in another program. Close it and try again.".into());
                    }
                    Readiness::TempDownload | Readiness::PartialSibling => {
                        item.status = PlanStatus::Pending;
                        item.note = Some("Download still in progress.".into());
                    }
                    Readiness::Settling { remaining_secs } => {
                        item.status = PlanStatus::Pending;
                        item.note = Some(format!(
                            "Modified moments ago; waiting {remaining_secs}s to make sure the download finished."
                        ));
                    }
                }
                if item.status == PlanStatus::Ready {
                    let probe = probed
                        .entry(folder.clone())
                        .or_insert_with(|| guard.probe_dir(folder));
                    match probe {
                        Err(err) => {
                            item.status = PlanStatus::Skipped;
                            item.note = Some(err.to_string());
                        }
                        Ok(dir) => {
                            let taken = reserved.entry(folder.to_uppercase()).or_default();
                            let free = match dir {
                                Some(d) => next_free_name(Some(d.as_path()), &e.name, taken),
                                None => next_free_name(None, &e.name, taken),
                            };
                            let collides = free != e.name;
                            if collides {
                                summary.conflicts += 1;
                                match inp.settings.collision {
                                    CollisionPolicy::Rename => {
                                        item.status = PlanStatus::Renamed;
                                        item.note = Some(format!(
                                            "\"{}\" already exists there; will be saved as \"{free}\".",
                                            e.name
                                        ));
                                    }
                                    CollisionPolicy::Skip => {
                                        item.status = PlanStatus::Skipped;
                                        item.note = Some(format!(
                                            "\"{}\" already exists there; skipped by your collision setting.",
                                            e.name
                                        ));
                                    }
                                    CollisionPolicy::Ask => {
                                        item.status = PlanStatus::NeedsDecision;
                                        item.note = Some(format!(
                                            "\"{}\" already exists there. Select to save as \"{free}\".",
                                            e.name
                                        ));
                                    }
                                }
                            }
                            if item.status != PlanStatus::Skipped {
                                taken.insert(name_key(&free));
                                item.dest_name = Some(free);
                            }
                            if *confirm
                                && matches!(item.status, PlanStatus::Ready | PlanStatus::Renamed)
                            {
                                item.status = PlanStatus::Confirm;
                                item.note = Some(format!(
                                    "Rule \"{}\" asks for confirmation before moving.",
                                    c.rule_name.clone().unwrap_or_default()
                                ));
                            }
                        }
                    }
                }
            }
        }

        if inp.mode == PlanMode::Manual
            && inp.restored.contains(&e.rel.to_uppercase())
            && matches!(item.status, PlanStatus::Ready | PlanStatus::Renamed)
        {
            item.status = PlanStatus::Confirm;
            item.note = Some(
                "Restored by undo (or created by Tucklane), so it isn't organized automatically. Select it to organize it now."
                    .into(),
            );
        }
        item.selected = matches!(item.status, PlanStatus::Ready | PlanStatus::Renamed);
        match item.status {
            PlanStatus::Ready
            | PlanStatus::Renamed
            | PlanStatus::NeedsDecision
            | PlanStatus::Confirm => {
                summary.proposed += 1;
                if let Some(d) = &item.dest_dir {
                    *summary.by_folder.entry(d.clone()).or_default() += 1;
                }
                if let Some(r) = &item.classification.rule_name {
                    *summary.by_rule.entry(r.clone()).or_default() += 1;
                }
            }
            PlanStatus::Pending => summary.pending += 1,
            PlanStatus::Locked => summary.locked += 1,
            PlanStatus::Review => summary.review += 1,
            PlanStatus::Ignored => summary.ignored += 1,
            PlanStatus::Skipped => summary.skipped += 1,
        }
        items.push(item);
    }
    summary.categories = summary.by_folder.len();
    Plan {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: crate::models::now_iso(),
        items,
        summary,
    }
}
