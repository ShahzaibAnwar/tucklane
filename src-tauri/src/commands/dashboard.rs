use std::io::Write;

use serde::Serialize;
use tauri::AppHandle;

use super::run;
use crate::error::AppResult;
use crate::history::RecoveryReport;
use crate::models::{HistoryQuery, OperationRecord, RootInfo};
use crate::settings::Settings;
use crate::watcher::MonitorStatus;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    pub root: Option<RootInfo>,
    pub root_available: bool,
    pub root_error: Option<String>,
    pub settings: Settings,
    pub monitor: MonitorStatus,
    pub startup_notes: Vec<String>,
    pub recovery: Option<RecoveryReport>,
}

#[tauri::command]
pub async fn get_app_status(app: AppHandle) -> AppResult<AppStatus> {
    run(app, |app, st| {
        let root = st
            .db
            .lock()
            .active_root()?
            .map(|(id, _, path, at)| RootInfo {
                id,
                path,
                selected_at: at,
            });
        let (available, err) = match st.guard.read().as_ref() {
            Some(g) => match g.verify_root() {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e.to_string())),
            },
            None => (false, st.root_error.read().clone()),
        };
        Ok(AppStatus {
            version: app.package_info().version.to_string(),
            root,
            root_available: available,
            root_error: err,
            settings: st.settings(),
            monitor: st.monitor_status(),
            startup_notes: st.startup_notes.lock().clone(),
            recovery: st.recovery.lock().clone(),
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFactor {
    pub label: String,
    pub count: i64,
    pub penalty: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub files_scanned: i64,
    pub bytes_reviewed: i64,
    pub files_organized: i64,
    pub ready_to_organize: i64,
    pub awaiting_review: i64,
    pub pending_downloads: i64,
    pub unresolved_failures: i64,
    pub duplicate_groups: usize,
    pub duplicate_bytes: u64,
    pub last_scan_at: Option<String>,
    pub health_score: i64,
    pub health_factors: Vec<HealthFactor>,
    pub recent: Vec<OperationRecord>,
}

/// Organization score: 100 minus explained penalties. It describes how tidy
/// the folder is, not the health of the PC.
fn health(ready: i64, review: i64, pending: i64, failed: i64) -> (i64, Vec<HealthFactor>) {
    let factors = vec![
        HealthFactor {
            label: "Loose files ready to organize".into(),
            count: ready,
            penalty: (ready * 2).min(45),
        },
        HealthFactor {
            label: "Files awaiting your review".into(),
            count: review,
            penalty: review.min(20),
        },
        HealthFactor {
            label: "Recent failed operations".into(),
            count: failed,
            penalty: (failed * 5).min(25),
        },
        HealthFactor {
            label: "Downloads still settling".into(),
            count: pending,
            penalty: pending.min(10),
        },
    ];
    let score = 100 - factors.iter().map(|f| f.penalty).sum::<i64>();
    (score.clamp(0, 100), factors)
}

#[tauri::command]
pub async fn get_dashboard_stats(app: AppHandle) -> AppResult<DashboardStats> {
    run(app, |_, st| {
        let guard = st.guard()?;
        let db = st.db.lock();
        let (files, bytes, ready, pending, review, last_scan) = db.index_stats(guard.id())?;
        let (_, _, organized) = db.op_counts(guard.id())?;
        let failures = db.unresolved_failures(guard.id())?;
        let groups = db.list_duplicate_groups(guard.id())?;
        let dup = crate::duplicates::summarize(&groups);
        let (recent, _) = db.query_ops(
            guard.id(),
            &HistoryQuery {
                kind: Some("move".into()),
                limit: Some(8),
                ..Default::default()
            },
        )?;
        let (score, factors) = health(ready, review, pending, failures);
        Ok(DashboardStats {
            files_scanned: files,
            bytes_reviewed: bytes,
            files_organized: organized,
            ready_to_organize: ready,
            awaiting_review: review,
            pending_downloads: pending,
            unresolved_failures: failures,
            duplicate_groups: dup.groups,
            duplicate_bytes: dup.recoverable_bytes,
            last_scan_at: last_scan,
            health_score: score,
            health_factors: factors,
            recent,
        })
    })
    .await
}

/// Writes a plain-text diagnostics report (no file names, no paths beyond
/// the authorized folder itself) into the authorized folder.
#[tauri::command]
pub async fn export_diagnostics(app: AppHandle) -> AppResult<String> {
    run(app, |app, st| {
        let guard = st.guard()?;
        guard.verify_root()?;
        let s = st.settings();
        let db = st.db.lock();
        let (files, bytes, ready, pending, review, last_scan) = db.index_stats(guard.id())?;
        let (moved, failed, _) = db.op_counts(guard.id())?;
        let events = db.list_events(50, 0, None, Some("error"))?;
        let name = format!("Tucklane-diagnostics-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(guard.root().join(&name))?;
        writeln!(f, "Tucklane diagnostics")?;
        writeln!(f, "Version: {}", app.package_info().version)?;
        writeln!(f, "OS: {} {}", std::env::consts::OS, std::env::consts::ARCH)?;
        writeln!(f, "Schema: {}", crate::database::SCHEMA_VERSION)?;
        writeln!(f, "Monitor: {:?}", st.monitor_status().state)?;
        writeln!(f, "Index: {files} files, {bytes} bytes, {ready} ready, {pending} pending, {review} review, last scan {last_scan:?}")?;
        writeln!(f, "Operations: {moved} completed moves, {failed} failed")?;
        writeln!(f, "Settings: {}", serde_json::to_string_pretty(&serde_json::json!({
            "preset": s.preset, "autoOrganize": s.auto_organize, "paused": s.monitoring_paused,
            "settleSeconds": s.settle_seconds, "scanIntervalMinutes": s.scan_interval_minutes,
            "collision": s.collision, "unknownFiles": s.unknown_files, "smartFilenameHints": s.smart_filename_hints,
            "contentSignatures": s.content_signatures, "closeToTray": s.close_to_tray,
        }))?)?;
        writeln!(f, "\nRecent errors:")?;
        for e in events {
            writeln!(f, "{} [{}] {}", e.created_at, e.kind, e.message)?;
        }
        let _ = db.add_exclusion(guard.id(), &name, "export");
        Ok(name)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::health;

    #[test]
    fn health_score_is_explained_and_bounded() {
        assert_eq!(health(0, 0, 0, 0).0, 100);
        let (score, f) = health(10, 3, 1, 1);
        assert_eq!(score, 100 - 20 - 3 - 5 - 1);
        assert_eq!(f.len(), 4);
        assert_eq!(health(1000, 1000, 1000, 1000).0, 0);
    }
}
