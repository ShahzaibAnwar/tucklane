use std::io::Write;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{limit_list, run};
use crate::error::{AppError, AppResult};
use crate::history::{self, UndoOptions, UndoReport};
use crate::models::{EventRecord, HistoryQuery, OperationRecord, SessionRecord};
use crate::organizer::ExecReport;
use crate::state::{EV_CHANGED, EV_ORGANIZE_PROGRESS};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub items: Vec<OperationRecord>,
    pub total: i64,
}

#[tauri::command]
pub async fn get_operation_history(app: AppHandle, query: HistoryQuery) -> AppResult<HistoryPage> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        let (items, total) = st.db.lock().query_ops(guard.id(), &query)?;
        Ok(HistoryPage { items, total })
    })
    .await
}

#[tauri::command]
pub async fn get_sessions(
    app: AppHandle,
    limit: Option<u32>,
    offset: Option<u32>,
) -> AppResult<Vec<SessionRecord>> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        st.db.lock().list_sessions(
            guard.id(),
            limit.unwrap_or(50).min(500),
            offset.unwrap_or(0),
        )
    })
    .await
}

#[tauri::command]
pub async fn get_session_operations(
    app: AppHandle,
    session_id: String,
) -> AppResult<Vec<OperationRecord>> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        let ops = st.db.lock().ops_for_session(&session_id)?;
        let db = st.db.lock();
        Ok(ops
            .into_iter()
            .filter(|o| db.op_root_id(&o.id).ok().as_deref() == Some(guard.id()))
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn get_file_history(app: AppHandle, rel: String) -> AppResult<Vec<OperationRecord>> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        crate::security::split_rel(&rel, crate::security::NameRule::Existing)?;
        st.db.lock().ops_for_file(guard.id(), &rel)
    })
    .await
}

#[tauri::command]
pub async fn undo_operations(
    app: AppHandle,
    operation_ids: Vec<String>,
    options: Option<UndoOptions>,
) -> AppResult<UndoReport> {
    let r = run(app.clone(), move |_, st| {
        limit_list(&operation_ids, 10_000)?;
        let guard = st.guard()?;
        let _job = st.file_job.try_lock().ok_or_else(|| {
            AppError::Busy("Please wait for the current operation to finish.".into())
        })?;
        history::undo_operations(&guard, &st.db, &operation_ids, &options.unwrap_or_default())
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "undo");
    Ok(r)
}

#[tauri::command]
pub async fn undo_session(
    app: AppHandle,
    session_id: String,
    options: Option<UndoOptions>,
) -> AppResult<UndoReport> {
    let r = run(app.clone(), move |_, st| {
        let guard = st.guard()?;
        let _job = st.file_job.try_lock().ok_or_else(|| {
            AppError::Busy("Please wait for the current operation to finish.".into())
        })?;
        history::undo_session(&guard, &st.db, &session_id, &options.unwrap_or_default())
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "undo");
    Ok(r)
}

#[tauri::command]
pub async fn retry_operations(app: AppHandle, operation_ids: Vec<String>) -> AppResult<ExecReport> {
    let r = run(app.clone(), move |app, st| {
        limit_list(&operation_ids, 10_000)?;
        let guard = st.guard()?;
        let _job = st.file_job.try_lock().ok_or_else(|| {
            AppError::Busy("Please wait for the current operation to finish.".into())
        })?;
        st.organize_cancel
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let app2 = app.clone();
        history::retry_operations(
            &guard,
            &st.db,
            &operation_ids,
            &st.organize_cancel,
            &move |p| {
                let _ = app2.emit(EV_ORGANIZE_PROGRESS, p);
            },
        )
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "retry");
    Ok(r)
}

#[tauri::command]
pub async fn get_activity_events(
    app: AppHandle,
    limit: Option<u32>,
    offset: Option<u32>,
    search: Option<String>,
    severity: Option<String>,
) -> AppResult<Vec<EventRecord>> {
    run(app, move |_, st| {
        st.db.lock().list_events(
            limit.unwrap_or(100),
            offset.unwrap_or(0),
            search.as_deref(),
            severity.as_deref(),
        )
    })
    .await
}

fn csv_field(s: &str) -> String {
    // Neutralize spreadsheet formula injection and quote every field.
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Writes the activity log as CSV into the authorized folder (the only
/// place Tucklane writes). Returns the created file's name.
#[tauri::command]
pub async fn export_activity_csv(app: AppHandle, query: HistoryQuery) -> AppResult<String> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        guard.verify_root()?;
        let q = HistoryQuery {
            limit: Some(1000),
            offset: Some(0),
            ..query
        };
        let mut rows = Vec::new();
        let mut offset = 0u32;
        loop {
            let (page, total) = st.db.lock().query_ops(
                guard.id(),
                &HistoryQuery {
                    offset: Some(offset),
                    ..q.clone()
                },
            )?;
            let n = page.len() as u32;
            rows.extend(page);
            offset += n;
            if n == 0 || offset as i64 >= total || offset >= 200_000 {
                break;
            }
        }
        let name = format!(
            "Tucklane-activity-{}.csv",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        let path = guard.root().join(&name);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        f.write_all(b"\xEF\xBB\xBF")?;
        writeln!(f, "Time,Type,Result,File,From,To,Rule,Reason,Error,Session")?;
        for o in rows {
            let fields = [
                o.created_at.clone(),
                o.kind_str().into(),
                o.state_str().into(),
                o.original_name.clone(),
                o.source_rel.clone(),
                o.dest_rel.clone().unwrap_or_else(|| o.dest_dir_rel.clone()),
                o.rule_name.clone().unwrap_or_default(),
                o.reason.clone().unwrap_or_default(),
                o.error.clone().unwrap_or_default(),
                o.session_id.clone(),
            ];
            writeln!(
                f,
                "{}",
                fields
                    .iter()
                    .map(|s| csv_field(s))
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
        }
        f.sync_all()?;
        let db = st.db.lock();
        let _ = db.add_exclusion(guard.id(), &name, "export");
        db.log_event(
            "info",
            "export",
            &format!("Activity exported to {name}."),
            None,
        );
        Ok(name)
    })
    .await
}

impl OperationRecord {
    fn kind_str(&self) -> &'static str {
        self.kind.as_str()
    }
    fn state_str(&self) -> &'static str {
        self.state.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::csv_field;

    #[test]
    fn csv_escaping() {
        assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_field("=cmd()"), "\"'=cmd()\"");
    }
}
