//! Shared application state and the glue between the core modules and Tauri.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::history::RecoveryReport;
use crate::models::now_ms;
use crate::organizer::{self, Plan, PlanInputs, PlanMode, PlanStatus};
use crate::security::{AuthorizedRootGuard, RootPolicy};
use crate::settings::Settings;
use crate::watcher::{Monitor, MonitorConfig, MonitorStatus};

pub const EV_ORGANIZE_PROGRESS: &str = "tucklane://organize-progress";
pub const EV_DUPLICATES_PROGRESS: &str = "tucklane://duplicates-progress";
pub const EV_MONITOR: &str = "tucklane://monitor-status";
pub const EV_CHANGED: &str = "tucklane://data-changed";
pub const EV_NAVIGATE: &str = "tucklane://navigate";

pub struct AppState {
    pub db: Arc<Mutex<Db>>,
    pub guard: RwLock<Option<AuthorizedRootGuard>>,
    pub root_error: RwLock<Option<String>>,
    pub settings: RwLock<Settings>,
    pub policy: RootPolicy,
    pub monitor: Mutex<Option<Monitor>>,
    /// Serializes every job that changes files (organize, undo, retry, recycle).
    pub file_job: Mutex<()>,
    pub organize_cancel: Arc<AtomicBool>,
    pub dup_cancel: Arc<AtomicBool>,
    pub dup_running: AtomicBool,
    pub plans: Mutex<VecDeque<Plan>>,
    pub startup_notes: Mutex<Vec<String>>,
    pub recovery: Mutex<Option<RecoveryReport>>,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    /// Files we already told the user about (to avoid repeated notifications).
    pub notified_ready: Mutex<HashSet<String>>,
    pub close_hint_shown: AtomicBool,
}

impl AppState {
    pub fn guard(&self) -> AppResult<AuthorizedRootGuard> {
        self.guard.read().clone().ok_or(AppError::NoRoot)
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Uppercased paths the user excluded themselves.
    pub fn excluded_set(&self, root_id: &str) -> AppResult<HashSet<String>> {
        Ok(self.exclusion_sets(root_id)?.0)
    }

    /// (user exclusions, files Tucklane placed: restored by undo or exports).
    pub fn exclusion_sets(&self, root_id: &str) -> AppResult<(HashSet<String>, HashSet<String>)> {
        let mut user = HashSet::new();
        let mut restored = HashSet::new();
        for e in self.db.lock().list_exclusions(root_id)? {
            if e.reason == "user" {
                user.insert(e.rel_path.to_uppercase());
            } else {
                restored.insert(e.rel_path.to_uppercase());
            }
        }
        Ok((user, restored))
    }

    /// Builds a plan and refreshes the file index used by the dashboard.
    pub fn plan(&self, mode: PlanMode, only: Option<&HashSet<String>>) -> AppResult<Plan> {
        let guard = self.guard()?;
        let settings = self.settings();
        let rules = self.db.lock().list_rules()?;
        let (excluded, restored) = self.exclusion_sets(guard.id())?;
        let plan = organizer::build_plan(
            &guard,
            &PlanInputs {
                settings: &settings,
                rules: &rules,
                excluded: &excluded,
                restored: &restored,
                now_ms: now_ms(),
                mode,
                only,
            },
        )?;
        if only.is_none() {
            let rows: Vec<(String, u64, i64, Option<String>, &str)> = plan
                .items
                .iter()
                .map(|i| {
                    let status = match i.status {
                        PlanStatus::Ready
                        | PlanStatus::Renamed
                        | PlanStatus::NeedsDecision
                        | PlanStatus::Confirm => "ready",
                        PlanStatus::Pending => "pending",
                        PlanStatus::Locked => "locked",
                        PlanStatus::Review => "review",
                        PlanStatus::Ignored => "ignored",
                        PlanStatus::Skipped => "skipped",
                    };
                    (
                        i.source_rel.clone(),
                        i.size,
                        i.modified_ms,
                        i.dest_dir.clone(),
                        status,
                    )
                })
                .collect();
            self.db.lock().replace_file_index(guard.id(), &rows)?;
        }
        if mode == PlanMode::Manual {
            let mut plans = self.plans.lock();
            plans.push_front(plan.clone());
            plans.truncate(4);
        }
        Ok(plan)
    }

    pub fn find_plan(&self, id: &str) -> AppResult<Plan> {
        self.plans
            .lock()
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or_else(|| {
                AppError::NotFound("That preview has expired. Please preview again.".into())
            })
    }

    pub fn monitor_status(&self) -> MonitorStatus {
        self.monitor
            .lock()
            .as_ref()
            .map(|m| m.status())
            .unwrap_or_default()
    }
}

/// Starts (or restarts) folder monitoring for the current root.
pub fn start_monitor(app: &AppHandle) {
    let state = app.state::<AppState>();
    stop_monitor(app);
    let Ok(guard) = state.guard() else { return };
    let s = state.settings();
    if !s.onboarding_complete {
        return;
    }
    let app_ready = app.clone();
    let app_status = app.clone();
    let monitor = Monitor::start(
        guard.root().to_path_buf(),
        MonitorConfig {
            settle_secs: s.settle_seconds as u64,
            scan_interval_mins: s.scan_interval_minutes as u64,
        },
        s.monitoring_paused,
        Arc::new(move |names| on_files_ready(&app_ready, names)),
        Arc::new(move |st| {
            let _ = app_status.emit(EV_MONITOR, st);
            crate::tray::refresh(&app_status);
        }),
    );
    *state.monitor.lock() = Some(monitor);
}

pub fn stop_monitor(app: &AppHandle) {
    let state = app.state::<AppState>();
    let m = state.monitor.lock().take();
    if let Some(m) = m {
        m.stop();
    }
    let _ = app.emit(EV_MONITOR, MonitorStatus::default());
}

/// Called by the monitor with files that look like completed downloads.
fn on_files_ready(app: &AppHandle, names: Vec<String>) {
    let state = app.state::<AppState>();
    let s = state.settings();
    let only: HashSet<String> = names.iter().map(|n| n.to_uppercase()).collect();
    let plan = match state.plan(PlanMode::Auto, Some(&only)) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("auto plan failed: {e}");
            return;
        }
    };
    let movable: Vec<_> = plan.items.iter().filter(|i| i.selected).collect();
    let review = plan
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.status,
                PlanStatus::Review | PlanStatus::Confirm | PlanStatus::NeedsDecision
            )
        })
        .count();

    if s.auto_organize && !s.monitoring_paused && !movable.is_empty() {
        let items = organizer::exec_items_from_plan(movable, s.collision);
        let Ok(guard) = state.guard() else { return };
        let _job = state.file_job.lock();
        let cancel = AtomicBool::new(false);
        match organizer::execute(
            &guard,
            &state.db,
            crate::models::SessionKind::Auto,
            items,
            &cancel,
            &|_| {},
        ) {
            Ok(rep) => {
                let n = rep.session.completed;
                if n > 0 {
                    crate::notify::send(
                        app,
                        &format!("{n} download{} organized", if n == 1 { "" } else { "s" }),
                        "Open Tucklane to see where they went or undo.",
                    );
                }
            }
            Err(e) => log::warn!("auto organize failed: {e}"),
        }
    } else if !movable.is_empty() {
        // Monitoring without auto-organize: let the user know, once per file.
        let mut seen = state.notified_ready.lock();
        let new = movable
            .iter()
            .filter(|i| seen.insert(i.source_rel.to_uppercase()))
            .count();
        if new > 0 {
            crate::notify::send(
                app,
                &format!(
                    "{new} new download{} ready to organize",
                    if new == 1 { "" } else { "s" }
                ),
                "Open Tucklane to review and organize them.",
            );
        }
    }
    if review > 0 && s.auto_organize {
        crate::notify::send(
            app,
            &format!(
                "{review} file{} need{} your review",
                if review == 1 { "" } else { "s" },
                if review == 1 { "s" } else { "" }
            ),
            "Open Tucklane to decide where they go.",
        );
    }
    // Refresh the index for the dashboard.
    let _ = state.plan(PlanMode::Auto, None);
    let _ = app.emit(EV_CHANGED, "monitor");
}

/// Keeps a small cache of `HashMap` lookups in tests without Tauri.
#[allow(dead_code)]
pub type PlanCache = HashMap<String, Plan>;
