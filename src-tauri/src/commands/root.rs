use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::run;
use crate::error::{AppError, AppResult};
use crate::models::RootInfo;
use crate::security::{display_path, AuthorizedRootGuard};
use crate::state::{start_monitor, stop_monitor, EV_CHANGED};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderValidation {
    pub ok: bool,
    pub path: String,
    pub message: Option<String>,
}

/// The user's Downloads known folder (resolved via SHGetKnownFolderPath,
/// so redirected folders are honored).
#[tauri::command]
pub async fn get_default_downloads_folder() -> AppResult<Option<String>> {
    Ok(dirs::download_dir().map(|p| display_path(&p)))
}

#[tauri::command]
pub async fn validate_authorized_root(app: AppHandle, path: String) -> AppResult<FolderValidation> {
    run(app, move |_, st| {
        let p = PathBuf::from(path.trim());
        Ok(match st.policy.validate(&p) {
            Ok(c) => FolderValidation {
                ok: true,
                path: display_path(&c),
                message: None,
            },
            Err(e) => FolderValidation {
                ok: false,
                path: path.clone(),
                message: Some(e.to_string()),
            },
        })
    })
    .await
}

#[tauri::command]
pub async fn get_authorized_root(app: AppHandle) -> AppResult<Option<RootInfo>> {
    run(app, |_, st| {
        Ok(st
            .db
            .lock()
            .active_root()?
            .map(|(id, _, display, at)| RootInfo {
                id,
                path: display,
                selected_at: at,
            }))
    })
    .await
}

/// Authorizes a new root. Monitoring of the previous root stops immediately.
#[tauri::command]
pub async fn select_authorized_root(app: AppHandle, path: String) -> AppResult<RootInfo> {
    let info = run(app.clone(), move |app, st| {
        let p = PathBuf::from(path.trim());
        let canonical = st.policy.validate(&p)?;
        // Stop monitoring the old root first (its thread may be waiting for the job lock).
        stop_monitor(app);
        let _job = st.file_job.try_lock().ok_or_else(|| {
            AppError::Busy("Please wait for the current operation to finish.".into())
        })?;
        let display = display_path(&canonical);
        let id = st
            .db
            .lock()
            .set_active_root(&canonical.to_string_lossy(), &display)?;
        let guard = AuthorizedRootGuard::open(&canonical, id.clone(), &st.policy)?;
        *st.guard.write() = Some(guard);
        *st.root_error.write() = None;
        st.plans.lock().clear();
        st.notified_ready.lock().clear();
        st.db.lock().log_event(
            "info",
            "root",
            &format!("Authorized folder set to {display}."),
            None,
        );
        Ok(RootInfo {
            id,
            path: display,
            selected_at: crate::models::now_iso(),
        })
    })
    .await?;
    start_monitor(&app);
    let _ = app.emit(EV_CHANGED, "root");
    Ok(info)
}
