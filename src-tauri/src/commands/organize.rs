use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::{limit_list, run};
use crate::error::{AppError, AppResult};
use crate::fsops::names::extension_of;
use crate::models::SessionKind;
use crate::organizer::{self, ExecReport, Plan, PlanMode, PlanStatus};
use crate::state::{EV_CHANGED, EV_ORGANIZE_PROGRESS};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub plan_id: String,
    pub total_files: usize,
    pub total_bytes: u64,
    pub categories: usize,
    pub suggested_changes: usize,
    pub unclassified: usize,
    pub in_use: usize,
    pub pending: usize,
    /// Groups of same-size files at the top level: *possible* duplicates,
    /// not verified by content.
    pub possible_duplicate_groups: usize,
    pub by_folder: Vec<(String, usize)>,
}

/// Scans the root without moving anything and summarizes what was found.
#[tauri::command]
pub async fn scan_downloads(app: AppHandle) -> AppResult<ScanSummary> {
    let r = run(app.clone(), |_, st| {
        let plan = st.plan(PlanMode::Manual, None)?;
        let mut sizes: HashMap<(u64, Option<String>), usize> = HashMap::new();
        for i in plan.items.iter().filter(|i| i.size > 0) {
            *sizes.entry((i.size, extension_of(&i.name))).or_default() += 1;
        }
        let s = &plan.summary;
        Ok(ScanSummary {
            plan_id: plan.id.clone(),
            total_files: s.files_scanned,
            total_bytes: s.bytes_scanned,
            categories: s.categories,
            suggested_changes: s.proposed,
            unclassified: plan
                .items
                .iter()
                .filter(|i| i.status == PlanStatus::Review)
                .count(),
            in_use: s.locked,
            pending: s.pending,
            possible_duplicate_groups: sizes.values().filter(|n| **n > 1).count(),
            by_folder: s.by_folder.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        })
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "scan");
    Ok(r)
}

#[tauri::command]
pub async fn preview_organization(app: AppHandle, only: Option<Vec<String>>) -> AppResult<Plan> {
    run(app, move |_, st| {
        if let Some(o) = &only {
            limit_list(o, 100_000)?;
        }
        let set: Option<HashSet<String>> =
            only.map(|v| v.iter().map(|s| s.to_uppercase()).collect());
        st.plan(PlanMode::Manual, set.as_ref())
    })
    .await
}

fn run_exec(
    app: &AppHandle,
    st: &crate::state::AppState,
    plan: &Plan,
    item_ids: &HashSet<String>,
    kind: SessionKind,
) -> AppResult<ExecReport> {
    let guard = st.guard()?;
    let settings = st.settings();
    let chosen: Vec<_> = plan
        .items
        .iter()
        .filter(|i| item_ids.contains(&i.id))
        .filter(|i| {
            matches!(
                i.status,
                PlanStatus::Ready
                    | PlanStatus::Renamed
                    | PlanStatus::NeedsDecision
                    | PlanStatus::Confirm
            )
        })
        .collect();
    if chosen.is_empty() {
        return Err(AppError::invalid("No movable files were selected."));
    }
    let items = organizer::exec_items_from_plan(chosen, settings.collision);
    let _job = st
        .file_job
        .try_lock()
        .ok_or_else(|| AppError::Busy("Another organization is already running.".into()))?;
    st.organize_cancel.store(false, Ordering::SeqCst);
    let app2 = app.clone();
    let last = std::sync::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
    let rep = organizer::execute(
        &guard,
        &st.db,
        kind,
        items,
        &st.organize_cancel,
        &move |p| {
            // Throttle UI updates to ~20/s, but always deliver the final one.
            let mut l = last.lock().unwrap();
            if p.finished || p.done == 0 || l.elapsed().as_millis() >= 50 {
                *l = std::time::Instant::now();
                let _ = app2.emit(EV_ORGANIZE_PROGRESS, p);
            }
        },
    )?;
    st.plans.lock().retain(|p| p.id != plan.id);
    let _ = st.plan(PlanMode::Auto, None);
    let n = rep.session.completed;
    if n > 0 {
        crate::notify::send(
            app,
            &format!("{n} download{} organized", if n == 1 { "" } else { "s" }),
            "You can undo this session from Tucklane at any time.",
        );
    }
    Ok(rep)
}

/// Executes selected items of a preview.
#[tauri::command]
pub async fn execute_organization(
    app: AppHandle,
    plan_id: String,
    item_ids: Vec<String>,
) -> AppResult<ExecReport> {
    let rep = run(app.clone(), move |app, st| {
        limit_list(&item_ids, 100_000)?;
        let plan = st.find_plan(&plan_id)?;
        let ids: HashSet<String> = item_ids.into_iter().collect();
        run_exec(app, st, &plan, &ids, SessionKind::Manual)
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "organize");
    Ok(rep)
}

/// Plans and immediately executes every file that is ready (used by the
/// dashboard "Organize Now" when previews are turned off).
#[tauri::command]
pub async fn organize_now(app: AppHandle) -> AppResult<ExecReport> {
    let rep = run(app.clone(), |app, st| {
        let plan = st.plan(PlanMode::Manual, None)?;
        let ids: HashSet<String> = plan
            .items
            .iter()
            .filter(|i| i.selected)
            .map(|i| i.id.clone())
            .collect();
        if ids.is_empty() {
            return Err(AppError::NotFound(
                "Everything is already in order — nothing to organize.".into(),
            ));
        }
        run_exec(app, st, &plan, &ids, SessionKind::Manual)
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "organize");
    Ok(rep)
}

/// Requests cancellation; the current file finishes moving first.
#[tauri::command]
pub async fn cancel_organization(app: AppHandle) -> AppResult<()> {
    run(app, |_, st| {
        st.organize_cancel.store(true, Ordering::SeqCst);
        Ok(())
    })
    .await
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedItem {
    source_rel: String,
    selected: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlanInfo {
    pub id: String,
    pub created_at: String,
    pub item_count: i64,
}

#[tauri::command]
pub async fn save_plan(
    app: AppHandle,
    plan_id: String,
    item_ids: Vec<String>,
) -> AppResult<SavedPlanInfo> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        let plan = st.find_plan(&plan_id)?;
        let ids: HashSet<String> = item_ids.into_iter().collect();
        let items: Vec<SavedItem> = plan
            .items
            .iter()
            .filter(|i| i.dest_dir.is_some())
            .map(|i| SavedItem {
                source_rel: i.source_rel.clone(),
                selected: ids.contains(&i.id),
            })
            .collect();
        let count = items.iter().filter(|i| i.selected).count() as i64;
        let id = st
            .db
            .lock()
            .save_plan(guard.id(), &serde_json::to_string(&items)?, count)?;
        Ok(SavedPlanInfo {
            id,
            created_at: crate::models::now_iso(),
            item_count: count,
        })
    })
    .await
}

#[tauri::command]
pub async fn list_saved_plans(app: AppHandle) -> AppResult<Vec<SavedPlanInfo>> {
    run(app, |_, st| {
        let guard = st.guard()?;
        Ok(st
            .db
            .lock()
            .list_saved_plans(guard.id())?
            .into_iter()
            .map(|(id, created_at, item_count)| SavedPlanInfo {
                id,
                created_at,
                item_count,
            })
            .collect())
    })
    .await
}

/// Re-plans a saved plan against the folder as it is now. Files that have
/// changed or disappeared simply drop out; nothing stale is executed.
#[tauri::command]
pub async fn load_saved_plan(app: AppHandle, id: String) -> AppResult<Plan> {
    run(app, move |_, st| {
        let json = st.db.lock().get_saved_plan(&id)?;
        let saved: Vec<SavedItem> = serde_json::from_str(&json)?;
        let only: HashSet<String> = saved.iter().map(|s| s.source_rel.to_uppercase()).collect();
        let selected: HashSet<String> = saved
            .iter()
            .filter(|s| s.selected)
            .map(|s| s.source_rel.to_uppercase())
            .collect();
        let mut plan = st.plan(PlanMode::Manual, Some(&only))?;
        for item in &mut plan.items {
            item.selected = item.selected && selected.contains(&item.source_rel.to_uppercase());
        }
        let mut plans = st.plans.lock();
        if let Some(p) = plans.iter_mut().find(|p| p.id == plan.id) {
            *p = plan.clone();
        }
        Ok(plan)
    })
    .await
}

#[tauri::command]
pub async fn delete_saved_plan(app: AppHandle, id: String) -> AppResult<()> {
    run(app, move |_, st| st.db.lock().delete_saved_plan(&id)).await
}
