//! Undo, retry and crash recovery.

use std::sync::atomic::AtomicBool;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::fsops::{
    is_staging_name, move_within_root, sha256_file, Expectation, MoveRequest, OnCollision,
};
use crate::models::{OpKind, OpState, OperationRecord, SessionKind};
use crate::organizer::{execute, ExecItem, ExecReport, Progress};
use crate::security::{query_entry, AuthorizedRootGuard};

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UndoConflict {
    /// Stop and ask the user.
    #[default]
    Ask,
    /// Restore under a new name such as `report (1).pdf`.
    Rename,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UndoOptions {
    pub on_conflict: UndoConflict,
    /// Restore even if the file was modified after Tucklane moved it.
    pub allow_modified: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UndoStatus {
    Restored,
    /// The moved file is no longer at its destination.
    Missing,
    /// The file at the destination changed since it was moved.
    Modified,
    /// Something now occupies the original location.
    Conflict,
    NotUndoable,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub operation_id: String,
    pub name: String,
    pub status: UndoStatus,
    pub message: String,
    pub restored_rel: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub session_id: Option<String>,
    pub results: Vec<UndoResult>,
}

fn parent_and_name(rel: &str) -> (String, String) {
    match rel.rsplit_once('/') {
        Some((p, n)) => (p.to_string(), n.to_string()),
        None => (String::new(), rel.to_string()),
    }
}

/// Undoes the given completed moves (newest first). Each undo is itself
/// recorded as an operation in a new "undo" session.
pub fn undo_operations(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    op_ids: &[String],
    opts: &UndoOptions,
) -> AppResult<UndoReport> {
    guard.verify_root()?;
    let mut ops: Vec<OperationRecord> = {
        let db = db.lock();
        // Only operations recorded for the currently authorized folder.
        op_ids
            .iter()
            .filter(|id| db.op_root_id(id).ok().as_deref() == Some(guard.id()))
            .filter_map(|id| db.get_op(id).ok())
            .collect()
    };
    if ops.is_empty() {
        return Err(AppError::NotFound("Nothing to undo.".into()));
    }
    ops.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));

    let session_id = db
        .lock()
        .create_session(guard.id(), SessionKind::Undo, ops.len() as i64)?;
    let mut results = Vec::new();
    for op in &ops {
        let r = undo_one(guard, db, &session_id, op, opts);
        results.push(r);
    }
    {
        let db = db.lock();
        let s = db.finish_session(&session_id, "completed")?;
        let restored = results
            .iter()
            .filter(|r| r.status == UndoStatus::Restored)
            .count();
        db.log_event(
            if restored == results.len() {
                "info"
            } else {
                "warning"
            },
            "undo",
            &format!("Undo: {restored} of {} files restored.", results.len()),
            Some(&s.id),
        );
        // Mark the original sessions as undone when nothing undoable remains.
        let mut sessions: Vec<&str> = ops.iter().map(|o| o.session_id.as_str()).collect();
        sessions.sort();
        sessions.dedup();
        for sid in sessions {
            if let Ok(s) = db.get_session(sid) {
                if s.undoable == 0 {
                    let _ = db.mark_session_undone(sid);
                }
            }
        }
    }
    Ok(UndoReport {
        session_id: Some(session_id),
        results,
    })
}

fn undo_one(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    session_id: &str,
    op: &OperationRecord,
    opts: &UndoOptions,
) -> UndoResult {
    let mut res = UndoResult {
        operation_id: op.id.clone(),
        name: op
            .final_name
            .clone()
            .unwrap_or_else(|| op.original_name.clone()),
        status: UndoStatus::Failed,
        message: String::new(),
        restored_rel: None,
    };
    let dest_rel = match (&op.kind, &op.state, &op.dest_rel) {
        (OpKind::Move, OpState::Completed, Some(d)) => d.clone(),
        (_, OpState::Undone, _) => {
            res.status = UndoStatus::NotUndoable;
            res.message = "Already undone.".into();
            return res;
        }
        _ => {
            res.status = UndoStatus::NotUndoable;
            res.message = "Only completed moves can be undone.".into();
            return res;
        }
    };

    // Is the file still where we put it, and unchanged?
    let info = match guard.resolve_existing(&dest_rel) {
        Ok((_, info)) => info,
        Err(AppError::NotFound(_)) => {
            res.status = UndoStatus::Missing;
            res.message = format!(
                "\"{}\" is no longer in {}. It may have been moved or deleted.",
                res.name, dest_rel
            );
            return res;
        }
        Err(e) => {
            res.message = e.to_string();
            return res;
        }
    };
    let same_identity = match op.method.as_deref() {
        Some("copy_verify") => op.sha256.as_deref().is_some_and(|h| {
            guard
                .resolve_existing(&dest_rel)
                .ok()
                .and_then(|(p, _)| sha256_file(&p).ok())
                .as_deref()
                == Some(h)
        }),
        _ => op.file_id.as_deref() == Some(info.file_id.as_str()),
    };
    if !same_identity {
        res.status = UndoStatus::Missing;
        res.message =
            "A different file now has that name; the original was moved or replaced.".into();
        return res;
    }
    let modified = op.size.is_some_and(|s| s as u64 != info.size)
        || op
            .mtime_ms
            .is_some_and(|m| (m - info.modified_ms).abs() > 2_000);
    if modified && !opts.allow_modified {
        res.status = UndoStatus::Modified;
        res.message = "The file was changed after Tucklane moved it. Restore anyway?".into();
        return res;
    }

    let (orig_dir, orig_name) = parent_and_name(&op.source_rel);
    let orig_taken = guard
        .join_rel(&op.source_rel, crate::security::NameRule::Existing)
        .ok()
        .is_some_and(|p| std::fs::symlink_metadata(p).is_ok());
    if orig_taken && opts.on_conflict == UndoConflict::Ask {
        res.status = UndoStatus::Conflict;
        res.message =
            format!("Another file named \"{orig_name}\" is now in the original location.");
        return res;
    }

    let undo_op = {
        let db = db.lock();
        match db.insert_planned_op(
            session_id,
            guard.id(),
            OpKind::Undo,
            &dest_rel,
            &orig_dir,
            &res.name,
            Some(info.size as i64),
            Some(info.modified_ms),
            None,
            None,
            Some("Undo"),
            Some(&op.id),
        ) {
            Ok(id) => {
                let _ =
                    db.mark_op_in_progress(&id, &info.file_id, info.size as i64, info.modified_ms);
                id
            }
            Err(e) => {
                res.message = e.to_string();
                return res;
            }
        }
    };

    let req = MoveRequest {
        source_rel: &dest_rel,
        dest_dir_rel: &orig_dir,
        desired_name: &orig_name,
        on_collision: if opts.on_conflict == UndoConflict::Rename {
            OnCollision::Rename
        } else {
            OnCollision::Fail
        },
        expect: Expectation {
            size: Some(info.size),
            modified_ms: Some(info.modified_ms),
            file_id: Some(info.file_id.clone()),
        },
        force_copy: false,
    };
    match move_within_root(guard, &req) {
        Ok(out) => {
            let db = db.lock();
            let method = if out.method == crate::fsops::MoveMethod::Rename {
                "rename"
            } else {
                "copy_verify"
            };
            let _ = db.complete_op(
                &undo_op,
                &out.dest_rel,
                &out.final_name,
                out.size as i64,
                out.modified_ms,
                &out.file_id,
                out.sha256.as_deref(),
                method,
            );
            let _ = db.set_op_state(&op.id, OpState::Undone, None);
            // A restored file should stay put: exclude it from automatic organization.
            let _ = db.add_exclusion(guard.id(), &out.dest_rel, "undo");
            res.status = UndoStatus::Restored;
            res.message = if out.final_name != orig_name {
                format!(
                    "Restored as \"{}\" because the original name was taken.",
                    out.final_name
                )
            } else {
                "Restored to its original location.".into()
            };
            res.restored_rel = Some(out.dest_rel);
        }
        Err(e) => {
            let (state, status) = match &e {
                AppError::Conflict(_) => (OpState::Skipped, UndoStatus::Conflict),
                AppError::NotFound(_) => (OpState::Skipped, UndoStatus::Missing),
                AppError::Changed => (OpState::Skipped, UndoStatus::Modified),
                _ => (OpState::Failed, UndoStatus::Failed),
            };
            let _ = db
                .lock()
                .set_op_state(&undo_op, state, Some(&e.to_string()));
            res.status = status;
            res.message = e.to_string();
        }
    }
    res
}

/// Undoes every completed move of a session.
pub fn undo_session(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    session_id: &str,
    opts: &UndoOptions,
) -> AppResult<UndoReport> {
    let ids: Vec<String> = db
        .lock()
        .ops_for_session(session_id)?
        .into_iter()
        .filter(|o| o.kind == OpKind::Move && o.state == OpState::Completed)
        .map(|o| o.id)
        .collect();
    if ids.is_empty() {
        return Err(AppError::NotFound(
            "This session has no moves left to undo.".into(),
        ));
    }
    undo_operations(guard, db, &ids, opts)
}

/// Retries failed or skipped moves using their original destination.
pub fn retry_operations(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    op_ids: &[String],
    cancel: &AtomicBool,
    on_progress: &dyn Fn(&Progress),
) -> AppResult<ExecReport> {
    let mut items = Vec::new();
    {
        let db = db.lock();
        for id in op_ids {
            if db.op_root_id(id).ok().as_deref() != Some(guard.id()) {
                continue;
            }
            let op = db.get_op(id)?;
            if op.kind != OpKind::Move || !matches!(op.state, OpState::Failed | OpState::Skipped) {
                continue;
            }
            let Ok((_, info)) = guard.resolve_existing(&op.source_rel) else {
                continue;
            };
            if info.is_dir {
                continue;
            }
            items.push(ExecItem {
                source_rel: op.source_rel.clone(),
                name: op.original_name.clone(),
                size: info.size,
                modified_ms: info.modified_ms,
                dest_dir: op.dest_dir_rel.clone(),
                dest_name: op.original_name.clone(),
                on_collision: OnCollision::Rename,
                rule_id: op.rule_id.clone(),
                rule_name: op.rule_name.clone(),
                reason: op.reason.clone(),
            });
        }
    }
    if items.is_empty() {
        return Err(AppError::NotFound(
            "None of the selected files can be retried (they may have been moved or deleted)."
                .into(),
        ));
    }
    execute(guard, db, SessionKind::Retry, items, cancel, on_progress)
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub sessions_interrupted: usize,
    pub recovered_completed: usize,
    pub recovered_untouched: usize,
    pub needs_attention: usize,
    pub staging_removed: usize,
}

/// Reconciles operations left `in_progress` by a crash or power loss with
/// what is actually on disk. Runs at startup before anything else.
pub fn recover(guard: Option<&AuthorizedRootGuard>, db: &Mutex<Db>) -> AppResult<RecoveryReport> {
    let mut rep = RecoveryReport::default();
    let db = db.lock();
    let in_progress = db.ops_in_state(OpState::InProgress)?;
    for op in in_progress {
        let Some(g) = guard.filter(|g| g.id() == op_root(&db, &op)) else {
            db.set_op_state(
                &op.id,
                OpState::RecoveryRequired,
                Some("Interrupted while the folder was unavailable; please check this file."),
            )?;
            rep.needs_attention += 1;
            continue;
        };
        let expected = op.file_id.clone().unwrap_or_default();
        // Still at the source?
        let at_source = g
            .resolve_existing(&op.source_rel)
            .ok()
            .filter(|(_, i)| i.file_id == expected);
        // Find it in the destination folder by identity.
        let mut found_dest: Option<(String, crate::security::EntryInfo)> = None;
        let mut staging = Vec::new();
        if let Ok(Some(dir)) = g.probe_dir(&op.dest_dir_rel) {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if is_staging_name(&name) {
                        staging.push(e.path());
                        continue;
                    }
                    if let Ok(info) = query_entry(&e.path()) {
                        if !expected.is_empty() && info.file_id == expected && !info.is_dir {
                            found_dest = Some((name, info));
                        }
                    }
                }
            }
        }
        match (at_source, found_dest) {
            (Some(_), _) => {
                // Not moved. Remove only our own partial staging copies.
                for s in staging {
                    if std::fs::remove_file(&s).is_ok() {
                        rep.staging_removed += 1;
                    }
                }
                db.set_op_state(
                    &op.id,
                    OpState::Failed,
                    Some("Interrupted before the move; the file was not touched."),
                )?;
                rep.recovered_untouched += 1;
            }
            (None, Some((name, info))) => {
                let dest_rel = if op.dest_dir_rel.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", op.dest_dir_rel, name)
                };
                db.complete_op(
                    &op.id,
                    &dest_rel,
                    &name,
                    info.size as i64,
                    info.modified_ms,
                    &info.file_id,
                    None,
                    "rename",
                )?;
                if op.kind == OpKind::Undo {
                    if let Some(orig) = &op.undo_of {
                        db.set_op_state(orig, OpState::Undone, None)?;
                    }
                }
                rep.recovered_completed += 1;
            }
            (None, None) => {
                db.set_op_state(
                    &op.id,
                    OpState::RecoveryRequired,
                    Some("Interrupted; the file was not found at its original or destination location."),
                )?;
                rep.needs_attention += 1;
            }
        }
    }
    // Close sessions that never finished.
    let running: Vec<String> = {
        let mut stmt_ids = Vec::new();
        for s in db.ops_in_state(OpState::Planned)? {
            db.set_op_state(
                &s.id,
                OpState::Skipped,
                Some("The session was interrupted before this file was processed."),
            )?;
            stmt_ids.push(s.session_id);
        }
        stmt_ids.sort();
        stmt_ids.dedup();
        stmt_ids
    };
    for sid in db.running_session_ids()?.into_iter().chain(running) {
        if db
            .get_session(&sid)
            .map(|s| s.status == "running")
            .unwrap_or(false)
        {
            db.finish_session(&sid, "interrupted")?;
            rep.sessions_interrupted += 1;
        }
    }
    if rep.sessions_interrupted
        + rep.needs_attention
        + rep.recovered_completed
        + rep.recovered_untouched
        > 0
    {
        db.log_event(
            if rep.needs_attention > 0 {
                "warning"
            } else {
                "info"
            },
            "recovery",
            &format!(
                "Recovered after an interruption: {} completed, {} untouched, {} need attention.",
                rep.recovered_completed, rep.recovered_untouched, rep.needs_attention
            ),
            None,
        );
    }
    Ok(rep)
}

fn op_root(db: &Db, op: &OperationRecord) -> String {
    db.op_root_id(&op.id).unwrap_or_default()
}
