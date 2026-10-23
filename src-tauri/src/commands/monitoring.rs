use tauri::{AppHandle, Emitter, Manager};

use super::run;
use crate::error::AppResult;
use crate::state::{start_monitor, stop_monitor, AppState, EV_CHANGED};
use crate::watcher::MonitorStatus;

/// Persists and applies the paused flag (shared by the tray and the UI).
pub fn set_paused(app: &AppHandle, paused: bool) {
    let st = app.state::<AppState>();
    let settings = {
        let mut s = st.settings.write();
        s.monitoring_paused = paused;
        s.clone()
    };
    if let Err(e) = st.db.lock().save_settings(&settings) {
        log::error!("failed to save pause state: {e}");
    }
    let has_monitor = st
        .monitor
        .lock()
        .as_ref()
        .map(|m| m.set_paused(paused))
        .is_some();
    if !has_monitor && !paused {
        start_monitor(app);
    }
    st.db.lock().log_event(
        "info",
        "monitor",
        if paused {
            "Monitoring paused."
        } else {
            "Monitoring resumed."
        },
        None,
    );
    crate::notify::send(
        app,
        if paused {
            "Monitoring paused"
        } else {
            "Monitoring resumed"
        },
        if paused {
            "New downloads will not be organized until you resume."
        } else {
            "Tucklane is watching for new downloads again."
        },
    );
    crate::tray::refresh(app);
    let _ = app.emit(EV_CHANGED, "settings");
}

#[tauri::command]
pub async fn get_monitoring_status(app: AppHandle) -> AppResult<MonitorStatus> {
    run(app, |_, st| Ok(st.monitor_status())).await
}

#[tauri::command]
pub async fn start_monitoring(app: AppHandle) -> AppResult<MonitorStatus> {
    start_monitor(&app);
    run(app, |_, st| Ok(st.monitor_status())).await
}

#[tauri::command]
pub async fn stop_monitoring(app: AppHandle) -> AppResult<MonitorStatus> {
    stop_monitor(&app);
    run(app, |_, st| Ok(st.monitor_status())).await
}

#[tauri::command]
pub async fn pause_monitoring(app: AppHandle) -> AppResult<MonitorStatus> {
    run(app, |app, st| {
        set_paused(app, true);
        Ok(st.monitor_status())
    })
    .await
}

#[tauri::command]
pub async fn resume_monitoring(app: AppHandle) -> AppResult<MonitorStatus> {
    run(app, |app, st| {
        set_paused(app, false);
        Ok(st.monitor_status())
    })
    .await
}

#[tauri::command]
pub async fn rescan_now(app: AppHandle) -> AppResult<()> {
    run(app, |_, st| {
        if let Some(m) = st.monitor.lock().as_ref() {
            m.rescan();
        }
        Ok(())
    })
    .await
}
