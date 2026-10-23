use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{limit_list, run};
use crate::duplicates::{self, DupSummary, RecycleResult};
use crate::error::{AppError, AppResult};
use crate::models::DuplicateGroup;
use crate::state::{EV_CHANGED, EV_DUPLICATES_PROGRESS};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateReport {
    pub summary: DupSummary,
    pub groups: Vec<DuplicateGroup>,
}

struct RunningGuard<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for RunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn find_duplicates(app: AppHandle) -> AppResult<DuplicateReport> {
    let r = run(app.clone(), |app, st| {
        let guard = st.guard()?;
        if st.dup_running.swap(true, Ordering::SeqCst) {
            return Err(AppError::Busy(
                "A duplicate scan is already running.".into(),
            ));
        }
        let _running = RunningGuard(&st.dup_running);
        st.dup_cancel.store(false, Ordering::SeqCst);
        let app2 = app.clone();
        let groups = duplicates::find_duplicates(&guard, &st.dup_cancel, &move |p| {
            let _ = app2.emit(EV_DUPLICATES_PROGRESS, p);
        })?;
        let db = st.db.lock();
        db.replace_duplicate_groups(guard.id(), &groups)?;
        let summary = duplicates::summarize(&groups);
        db.log_event(
            "info",
            "duplicates",
            &format!(
                "Duplicate scan completed: {} groups, {} extra copies.",
                summary.groups, summary.duplicate_files
            ),
            None,
        );
        Ok(DuplicateReport { summary, groups })
    })
    .await?;
    if r.summary.groups > 0 {
        crate::notify::send(
            &app,
            &format!(
                "Tucklane found {} duplicate group{}",
                r.summary.groups,
                if r.summary.groups == 1 { "" } else { "s" }
            ),
            "Nothing was deleted. Review them in Duplicates.",
        );
    }
    let _ = app.emit(EV_CHANGED, "duplicates");
    Ok(r)
}

#[tauri::command]
pub async fn cancel_duplicate_scan(app: AppHandle) -> AppResult<()> {
    run(app, |_, st| {
        st.dup_cancel.store(true, Ordering::SeqCst);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn get_duplicate_groups(app: AppHandle) -> AppResult<DuplicateReport> {
    run(app, |_, st| {
        let guard = st.guard()?;
        let groups = st.db.lock().list_duplicate_groups(guard.id())?;
        Ok(DuplicateReport {
            summary: duplicates::summarize(&groups),
            groups,
        })
    })
    .await
}

/// Moves the explicitly selected copies to the Recycle Bin. The frontend
/// asks for confirmation; the backend independently refuses to remove the
/// last verified copy of any group.
#[tauri::command]
pub async fn recycle_duplicates(
    app: AppHandle,
    rels: Vec<String>,
    confirmed: bool,
) -> AppResult<Vec<RecycleResult>> {
    if !confirmed {
        return Err(AppError::invalid(
            "Moving files to the Recycle Bin requires confirmation.",
        ));
    }
    let r = run(app.clone(), move |_, st| {
        limit_list(&rels, 10_000)?;
        for r in &rels {
            crate::security::split_rel(r, crate::security::NameRule::Existing)?;
        }
        let guard = st.guard()?;
        let _job = st.file_job.try_lock().ok_or_else(|| {
            AppError::Busy("Please wait for the current operation to finish.".into())
        })?;
        duplicates::recycle_duplicates(&guard, &st.db, &rels)
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "duplicates");
    Ok(r)
}
