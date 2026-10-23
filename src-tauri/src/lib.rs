//! Tucklane — a privacy-first downloads organizer for Windows.

pub mod classifier;
pub mod commands;
pub mod database;
pub mod duplicates;
pub mod error;
pub mod fsops;
pub mod history;
pub mod models;
pub mod notify;
pub mod organizer;
pub mod rules;
pub mod scanner;
pub mod security;
pub mod settings;
pub mod state;
pub mod tray;
pub mod watcher;

#[cfg(all(test, windows))]
mod acceptance_tests;
#[cfg(test)]
pub mod testutil;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_window_state::StateFlags;

use crate::database::Db;
use crate::security::{AuthorizedRootGuard, RootPolicy};
use crate::state::AppState;

/// Command-line flag used when Windows starts Tucklane at sign-in.
pub const MINIMIZED_FLAG: &str = "--minimized";

fn init_state(app: &AppHandle) -> Result<AppState, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    let log_dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let mut protected = vec![data_dir.clone(), log_dir.clone()];
    if let Ok(local) = app.path().app_local_data_dir() {
        protected.push(local);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            protected.push(dir.to_path_buf());
        }
    }
    let policy = RootPolicy::system(&protected);

    let mut notes = Vec::new();
    let opened = Db::open(&data_dir.join("tucklane.db"))?;
    if let Some(aside) = &opened.recovered_from {
        notes.push(format!(
            "Tucklane's history database was damaged and has been reset. The damaged copy was kept as {}.",
            aside.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
    }
    let db = opened.db;
    let settings = db.load_settings()?;
    let _ = db.prune_history(settings.history_retention_days);

    let mut root_error = None;
    let guard = match db.active_root()? {
        Some((id, canonical, display, _)) => {
            match AuthorizedRootGuard::open(std::path::Path::new(&canonical), id, &policy) {
                Ok(g) => Some(g),
                Err(e) => {
                    log::warn!("authorized folder unavailable: {e}");
                    root_error = Some(format!("{display} is not available: {e}"));
                    None
                }
            }
        }
        None => None,
    };

    let db = Arc::new(Mutex::new(db));
    let recovery = match history::recover(guard.as_ref(), &db) {
        Ok(r)
            if r.needs_attention
                + r.recovered_completed
                + r.recovered_untouched
                + r.sessions_interrupted
                > 0 =>
        {
            Some(r)
        }
        Ok(_) => None,
        Err(e) => {
            log::error!("recovery failed: {e}");
            None
        }
    };

    Ok(AppState {
        db,
        guard: RwLock::new(guard),
        root_error: RwLock::new(root_error),
        settings: RwLock::new(settings),
        policy,
        monitor: Mutex::new(None),
        file_job: Mutex::new(()),
        organize_cancel: Arc::new(AtomicBool::new(false)),
        dup_cancel: Arc::new(AtomicBool::new(false)),
        dup_running: AtomicBool::new(false),
        plans: Mutex::new(VecDeque::new()),
        startup_notes: Mutex::new(notes),
        recovery: Mutex::new(recovery),
        data_dir,
        log_dir,
        notified_ready: Mutex::new(Default::default()),
        close_hint_shown: AtomicBool::new(false),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_FLAG]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = match init_state(&handle) {
                Ok(s) => s,
                Err(e) => {
                    log::error!("startup failed: {e}");
                    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                    handle
                        .dialog()
                        .message(format!("Tucklane could not start:\n\n{e}"))
                        .kind(MessageDialogKind::Error)
                        .title("Tucklane")
                        .blocking_show();
                    std::process::exit(1);
                }
            };
            let settings = state.settings();
            app.manage(state);

            if settings.show_tray_icon {
                if let Err(e) = tray::create(&handle) {
                    log::warn!("tray icon unavailable: {e}");
                }
            }
            state::start_monitor(&handle);

            let launched_minimized = std::env::args().any(|a| a == MINIMIZED_FLAG);
            let stay_hidden = launched_minimized && settings.start_minimized && settings.onboarding_complete && settings.show_tray_icon;
            if let Some(w) = app.get_webview_window("main") {
                if !stay_hidden {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            // Refresh the file index in the background so the dashboard has data.
            let h2 = handle.clone();
            std::thread::spawn(move || {
                let st = h2.state::<AppState>();
                if st.settings.read().onboarding_complete && st.guard.read().is_some() {
                    let _ = st.plan(organizer::PlanMode::Auto, None);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let st = app.state::<AppState>();
                let s = st.settings.read().clone();
                if s.close_to_tray && s.show_tray_icon && s.onboarding_complete {
                    api.prevent_close();
                    let _ = window.hide();
                    if !st.close_hint_shown.swap(true, Ordering::SeqCst) {
                        notify::send(
                            app,
                            "Tucklane is still running",
                            "It keeps watching your downloads from the notification area. Use Quit in the tray menu to exit.",
                        );
                    }
                } else {
                    // Closing quits: stop monitoring off the main thread, then exit.
                    api.prevent_close();
                    let _ = window.hide();
                    tray::quit(app);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard::get_app_status,
            commands::dashboard::get_dashboard_stats,
            commands::dashboard::export_diagnostics,
            commands::root::get_default_downloads_folder,
            commands::root::validate_authorized_root,
            commands::root::get_authorized_root,
            commands::root::select_authorized_root,
            commands::organize::scan_downloads,
            commands::organize::preview_organization,
            commands::organize::execute_organization,
            commands::organize::organize_now,
            commands::organize::cancel_organization,
            commands::organize::save_plan,
            commands::organize::list_saved_plans,
            commands::organize::load_saved_plan,
            commands::organize::delete_saved_plan,
            commands::history::get_operation_history,
            commands::history::get_sessions,
            commands::history::get_session_operations,
            commands::history::get_file_history,
            commands::history::undo_operations,
            commands::history::undo_session,
            commands::history::retry_operations,
            commands::history::get_activity_events,
            commands::history::export_activity_csv,
            commands::rules::get_rules,
            commands::rules::create_rule,
            commands::rules::update_rule,
            commands::rules::delete_rule,
            commands::rules::reorder_rules,
            commands::rules::test_rule,
            commands::rules::export_rules,
            commands::rules::import_rules,
            commands::duplicates::find_duplicates,
            commands::duplicates::cancel_duplicate_scan,
            commands::duplicates::get_duplicate_groups,
            commands::duplicates::recycle_duplicates,
            commands::files::list_directory,
            commands::files::search_files,
            commands::files::open_file,
            commands::files::reveal_in_explorer,
            commands::files::open_root_folder,
            commands::files::open_about_link,
            commands::files::get_absolute_path,
            commands::files::get_thumbnail,
            commands::files::exclude_files,
            commands::files::include_files,
            commands::files::get_exclusions,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::get_presets,
            commands::settings::is_autostart_enabled,
            commands::settings::quit_app,
            commands::monitoring::get_monitoring_status,
            commands::monitoring::start_monitoring,
            commands::monitoring::stop_monitoring,
            commands::monitoring::pause_monitoring,
            commands::monitoring::resume_monitoring,
            commands::monitoring::rescan_now,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tucklane")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                state::stop_monitor(app);
            }
        });
}
