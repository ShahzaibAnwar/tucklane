use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt;

use super::run;
use crate::classifier::kinds::{preset_folders, Preset};
use crate::error::{AppError, AppResult};
use crate::settings::Settings;
use crate::state::{start_monitor, AppState, EV_CHANGED};
use crate::watcher::MonitorConfig;

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> AppResult<Settings> {
    run(app, |_, st| Ok(st.settings())).await
}

/// Applies settings that have side effects outside the database.
fn apply(app: &AppHandle, old: &Settings, new: &Settings) {
    if old.launch_at_startup != new.launch_at_startup {
        let al = app.autolaunch();
        let r = if new.launch_at_startup {
            al.enable()
        } else {
            al.disable()
        };
        if let Err(e) = r {
            log::warn!("autostart change failed: {e}");
        }
    }
    if old.show_tray_icon != new.show_tray_icon {
        if new.show_tray_icon {
            let _ = crate::tray::create(app);
        } else {
            crate::tray::remove(app);
        }
    }
    let state = app.state::<AppState>();
    if !old.onboarding_complete && new.onboarding_complete {
        start_monitor(app);
    } else if let Some(m) = state.monitor.lock().as_ref() {
        if old.settle_seconds != new.settle_seconds
            || old.scan_interval_minutes != new.scan_interval_minutes
        {
            m.set_config(MonitorConfig {
                settle_secs: new.settle_seconds as u64,
                scan_interval_mins: new.scan_interval_minutes as u64,
            });
        }
        if old.monitoring_paused != new.monitoring_paused {
            m.set_paused(new.monitoring_paused);
        }
        if !old.auto_organize && new.auto_organize {
            m.rescan();
        }
    }
    crate::tray::refresh(app);
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    let (old, new) = run(app.clone(), move |_, st| {
        let new = settings.validate()?;
        if new.onboarding_complete && st.guard.read().is_none() {
            return Err(AppError::NoRoot);
        }
        st.db.lock().save_settings(&new)?;
        let old = std::mem::replace(&mut *st.settings.write(), new.clone());
        if old.preset != new.preset || old.collision != new.collision {
            st.plans.lock().clear();
        }
        Ok((old, new))
    })
    .await?;
    apply(&app, &old, &new);
    let _ = app.emit(EV_CHANGED, "settings");
    Ok(new)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub id: Preset,
    pub folders: Vec<&'static str>,
}

#[tauri::command]
pub async fn get_presets() -> AppResult<Vec<PresetInfo>> {
    Ok([Preset::Simple, Preset::Balanced, Preset::Advanced]
        .into_iter()
        .map(|p| PresetInfo {
            id: p,
            folders: preset_folders(p),
        })
        .collect())
}

#[tauri::command]
pub async fn is_autostart_enabled(app: AppHandle) -> AppResult<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::Io(e.to_string()))
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) -> AppResult<()> {
    crate::tray::quit(&app);
    Ok(())
}
