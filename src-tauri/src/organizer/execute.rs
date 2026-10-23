//! Executes a list of moves as one recorded session.
//!
//! For every item the operation intent is persisted *before* the move
//! (`planned`, then `in_progress` with the file's identity), and the result
//! is recorded only after the filesystem confirms it. If the process dies
//! mid-way, [`crate::history::recover`] reconciles the records with disk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::fsops::stability::{has_partial_sibling, is_temp_download};
use crate::fsops::{move_within_root, Expectation, MoveRequest, OnCollision};
use crate::models::{now_ms, OpKind, OpState, SessionKind, SessionRecord};
use crate::security::AuthorizedRootGuard;

#[derive(Debug, Clone)]
pub struct ExecItem {
    pub source_rel: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
    pub dest_dir: String,
    pub dest_name: String,
    pub on_collision: OnCollision,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub session_id: String,
    pub total: usize,
    pub done: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current: Option<String>,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecReport {
    pub session: SessionRecord,
    pub cancelled: bool,
    pub errors: Vec<String>,
}

/// Minimum quiet period before a manual move, so a file that is visibly
/// still changing is not moved even when the user selected it.
const MANUAL_MIN_QUIET_MS: i64 = 3_000;

pub fn execute(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    kind: SessionKind,
    items: Vec<ExecItem>,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(&Progress),
) -> AppResult<ExecReport> {
    guard.verify_root()?;
    let total = items.len();
    // 1. Persist the session and every operation intent up front.
    let (session_id, op_ids) = {
        let db = db.lock();
        let sid = db.create_session(guard.id(), kind, total as i64)?;
        let mut ids = Vec::with_capacity(total);
        for it in &items {
            ids.push(db.insert_planned_op(
                &sid,
                guard.id(),
                OpKind::Move,
                &it.source_rel,
                &it.dest_dir,
                &it.name,
                Some(it.size as i64),
                Some(it.modified_ms),
                it.rule_id.as_deref(),
                it.rule_name.as_deref(),
                it.reason.as_deref(),
                None,
            )?);
        }
        (sid, ids)
    };

    let mut p = Progress {
        session_id: session_id.clone(),
        total,
        ..Default::default()
    };
    on_progress(&p);
    let mut cancelled = false;
    let mut aborted: Option<String> = None;
    let mut errors = Vec::new();
    let mut rule_hits: HashMap<String, i64> = HashMap::new();

    for (it, op_id) in items.iter().zip(op_ids.iter()) {
        if cancel.load(Ordering::SeqCst) {
            cancelled = true;
        }
        if cancelled || aborted.is_some() {
            let why = if cancelled {
                "Cancelled before this file was processed."
            } else {
                aborted.as_deref().unwrap_or("")
            };
            db.lock().set_op_state(op_id, OpState::Skipped, Some(why))?;
            p.skipped += 1;
            p.done += 1;
            continue;
        }
        p.current = Some(it.name.clone());
        on_progress(&p);

        match move_one(guard, db, op_id, it) {
            Ok(()) => {
                p.completed += 1;
                if let Some(r) = &it.rule_id {
                    *rule_hits.entry(r.clone()).or_default() += 1;
                }
            }
            Err(Outcome::Skipped(msg)) => {
                db.lock()
                    .set_op_state(op_id, OpState::Skipped, Some(&msg))?;
                p.skipped += 1;
            }
            Err(Outcome::Failed(e)) => {
                let msg = e.to_string();
                if matches!(e, AppError::InvalidRoot(_)) {
                    aborted = Some(format!("Stopped: {msg}"));
                }
                db.lock().set_op_state(op_id, OpState::Failed, Some(&msg))?;
                errors.push(format!("{}: {msg}", it.name));
                p.failed += 1;
            }
        }
        p.done += 1;
        on_progress(&p);
    }

    let status = if cancelled {
        "cancelled"
    } else if aborted.is_some() {
        "failed"
    } else {
        "completed"
    };
    let session = {
        let db = db.lock();
        for (rule, n) in rule_hits {
            let _ = db.record_rule_triggered(&rule, n);
        }
        let s = db.finish_session(&session_id, status)?;
        let msg = format!(
            "{} session: {} moved, {} skipped, {} failed.",
            match kind {
                SessionKind::Auto => "Automatic organization",
                SessionKind::Retry => "Retry",
                _ => "Organization",
            },
            s.completed,
            s.skipped,
            s.failed
        );
        db.log_event(
            if s.failed > 0 { "warning" } else { "info" },
            "organize",
            &msg,
            Some(&session_id),
        );
        s
    };
    p.current = None;
    p.finished = true;
    on_progress(&p);
    Ok(ExecReport {
        session,
        cancelled,
        errors,
    })
}

enum Outcome {
    Skipped(String),
    Failed(AppError),
}

fn move_one(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    op_id: &str,
    it: &ExecItem,
) -> Result<(), Outcome> {
    if is_temp_download(&it.name) || has_partial_sibling(guard.root(), &it.name) {
        return Err(Outcome::Skipped("Download still in progress.".into()));
    }
    let (_, info) = guard
        .resolve_existing(&it.source_rel)
        .map_err(|e| match e {
            AppError::NotFound(_) => {
                Outcome::Skipped("The file no longer exists (it was moved or deleted).".into())
            }
            other => Outcome::Failed(other),
        })?;
    if info.size != it.size || (info.modified_ms - it.modified_ms).abs() > 2_000 {
        return Err(Outcome::Skipped(
            "The file changed since the preview. Scan again to include it.".into(),
        ));
    }
    if now_ms() - info.modified_ms < MANUAL_MIN_QUIET_MS {
        return Err(Outcome::Skipped("The file is still being written.".into()));
    }
    // Persist intent with the file's identity before touching it.
    db.lock()
        .mark_op_in_progress(op_id, &info.file_id, info.size as i64, info.modified_ms)
        .map_err(Outcome::Failed)?;

    let req = MoveRequest {
        source_rel: &it.source_rel,
        dest_dir_rel: &it.dest_dir,
        desired_name: &it.dest_name,
        on_collision: it.on_collision,
        expect: Expectation {
            size: Some(it.size),
            modified_ms: Some(it.modified_ms),
            file_id: Some(info.file_id.clone()),
        },
        force_copy: false,
    };
    match move_within_root(guard, &req) {
        Ok(out) => {
            let method = match out.method {
                crate::fsops::MoveMethod::Rename => "rename",
                crate::fsops::MoveMethod::CopyVerify => "copy_verify",
            };
            // The file has moved. If recording that fails, leave the op
            // `in_progress` so startup recovery reconciles it with disk,
            // rather than marking a successful move as failed.
            if let Err(e) = db.lock().complete_op(
                op_id,
                &out.dest_rel,
                &out.final_name,
                out.size as i64,
                out.modified_ms,
                &out.file_id,
                out.sha256.as_deref(),
                method,
            ) {
                log::error!("move succeeded but could not be recorded: {e}");
            }
            Ok(())
        }
        Err(AppError::NotFound(_)) => Err(Outcome::Skipped(
            "The file no longer exists (it was moved or deleted).".into(),
        )),
        Err(AppError::Changed) => Err(Outcome::Skipped(
            "The file changed since the preview. Scan again to include it.".into(),
        )),
        Err(AppError::Locked) => Err(Outcome::Skipped(
            "The file is open in another program.".into(),
        )),
        Err(AppError::Conflict(m)) if it.on_collision == OnCollision::Fail => {
            Err(Outcome::Skipped(m))
        }
        Err(e) => Err(Outcome::Failed(e)),
    }
}
