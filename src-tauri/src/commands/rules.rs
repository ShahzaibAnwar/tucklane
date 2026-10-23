use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::{limit_list, run};
use crate::error::{AppError, AppResult};
use crate::models::now_ms;
use crate::organizer::{plan_entries, PlanInputs, PlanMode};
use crate::rules::{Rule, RuleDraft};
use crate::scanner::list_dir;
use crate::state::EV_CHANGED;

#[tauri::command]
pub async fn get_rules(app: AppHandle) -> AppResult<Vec<Rule>> {
    run(app, |_, st| st.db.lock().list_rules()).await
}

#[tauri::command]
pub async fn create_rule(app: AppHandle, draft: RuleDraft) -> AppResult<Rule> {
    let r = run(app.clone(), move |_, st| {
        let d = draft.validate()?;
        if st.db.lock().list_rules()?.len() >= 500 {
            return Err(AppError::invalid("You can have at most 500 rules."));
        }
        st.db.lock().insert_rule(&d)
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "rules");
    Ok(r)
}

#[tauri::command]
pub async fn update_rule(app: AppHandle, id: String, draft: RuleDraft) -> AppResult<Rule> {
    let r = run(app.clone(), move |_, st| {
        let d = draft.validate()?;
        st.db.lock().update_rule(&id, &d)
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "rules");
    Ok(r)
}

#[tauri::command]
pub async fn delete_rule(app: AppHandle, id: String) -> AppResult<()> {
    run(app.clone(), move |_, st| st.db.lock().delete_rule(&id)).await?;
    let _ = app.emit(EV_CHANGED, "rules");
    Ok(())
}

#[tauri::command]
pub async fn reorder_rules(app: AppHandle, ids: Vec<String>) -> AppResult<Vec<Rule>> {
    run(app, move |_, st| {
        limit_list(&ids, 500)?;
        let db = st.db.lock();
        let existing: HashSet<String> = db.list_rules()?.into_iter().map(|r| r.id).collect();
        let given: HashSet<String> = ids.iter().cloned().collect();
        if existing != given {
            return Err(AppError::invalid("The rule list changed; please reload."));
        }
        db.reorder_rules(&ids)?;
        db.list_rules()
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleTestMatch {
    pub rel: String,
    pub name: String,
    pub size: u64,
    /// Whether this rule (rather than a higher-priority one) would decide.
    pub decides: bool,
    pub decided_by: Option<String>,
    pub destination: Option<String>,
}

/// Shows which files in the folder a draft rule would match, and whether a
/// higher-priority rule would win. Nothing is moved.
#[tauri::command]
pub async fn test_rule(
    app: AppHandle,
    draft: RuleDraft,
    rule_id: Option<String>,
) -> AppResult<Vec<RuleTestMatch>> {
    run(app, move |_, st| {
        let d = draft.validate()?;
        let guard = st.guard()?;
        let settings = st.settings();
        let mut rules = st.db.lock().list_rules()?;
        let test_id = rule_id.clone().unwrap_or_else(|| "__test__".into());
        let priority = rules
            .iter()
            .find(|r| r.id == test_id)
            .map(|r| r.priority)
            .unwrap_or(i64::MAX);
        rules.retain(|r| r.id != test_id);
        let test_rule = Rule {
            id: test_id.clone(),
            name: d.name.clone(),
            description: d.description.clone(),
            priority,
            enabled: true,
            match_mode: d.match_mode,
            conditions: d.conditions.clone(),
            action: d.action.clone(),
            created_at: String::new(),
            updated_at: String::new(),
            last_triggered_at: None,
            match_count: 0,
        };
        rules.push(test_rule.clone());
        let (excluded, restored) = st.exclusion_sets(guard.id())?;
        let entries = list_dir(&guard, "")?;
        let plan = plan_entries(
            &guard,
            entries,
            &PlanInputs {
                settings: &settings,
                rules: &rules,
                excluded: &excluded,
                restored: &restored,
                now_ms: now_ms(),
                mode: PlanMode::Manual,
                only: None,
            },
        );
        let out = plan
            .items
            .into_iter()
            .filter(|i| {
                let facts = crate::rules::FileFacts {
                    name: &i.name,
                    ext: None,
                    size: i.size,
                    modified_ms: i.modified_ms,
                    created_ms: i.modified_ms,
                    kind: i.classification.kind,
                    now_ms: now_ms(),
                };
                let ext = crate::fsops::names::extension_of(&i.name);
                let facts = crate::rules::FileFacts {
                    ext: ext.as_deref(),
                    ..facts
                };
                test_rule.matches(&facts)
            })
            .take(500)
            .map(|i| RuleTestMatch {
                decides: i.classification.rule_id.as_deref() == Some(test_id.as_str()),
                decided_by: i
                    .classification
                    .rule_name
                    .clone()
                    .or(Some(i.classification.reason.clone())),
                destination: i.dest_dir.clone(),
                rel: i.source_rel,
                name: i.name,
                size: i.size,
            })
            .collect();
        Ok(out)
    })
    .await
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RulesFile {
    format: String,
    version: u32,
    rules: Vec<RuleDraft>,
}

/// Exports rule definitions (no paths outside the root, no file data).
#[tauri::command]
pub async fn export_rules(app: AppHandle) -> AppResult<String> {
    run(app, |_, st| {
        let rules = st.db.lock().list_rules()?;
        let file = RulesFile {
            format: "tucklane-rules".into(),
            version: 1,
            rules: rules
                .into_iter()
                .map(|r| RuleDraft {
                    name: r.name,
                    description: r.description,
                    enabled: r.enabled,
                    match_mode: r.match_mode,
                    conditions: r.conditions,
                    action: r.action,
                })
                .collect(),
        };
        Ok(serde_json::to_string_pretty(&file)?)
    })
    .await
}

/// Imports rules from JSON text. Every rule is validated exactly like a
/// rule created in the editor; destinations outside the root are rejected.
#[tauri::command]
pub async fn import_rules(app: AppHandle, json: String) -> AppResult<usize> {
    let n = run(app.clone(), move |_, st| {
        if json.len() > 2 * 1024 * 1024 {
            return Err(AppError::invalid("The rules file is too large."));
        }
        let file: RulesFile = serde_json::from_str(&json)
            .map_err(|e| AppError::invalid(format!("Not a valid Tucklane rules file: {e}")))?;
        if file.format != "tucklane-rules" || file.version != 1 {
            return Err(AppError::invalid(
                "Not a Tucklane rules file (or an unsupported version).",
            ));
        }
        limit_list(&file.rules, 500)?;
        let drafts = file
            .rules
            .into_iter()
            .map(|d| d.validate())
            .collect::<AppResult<Vec<_>>>()?;
        let db = st.db.lock();
        for d in &drafts {
            db.insert_rule(d)?;
        }
        db.log_event(
            "info",
            "rules",
            &format!("Imported {} rules.", drafts.len()),
            None,
        );
        Ok(drafts.len())
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "rules");
    Ok(n)
}
