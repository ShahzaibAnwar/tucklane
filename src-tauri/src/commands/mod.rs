//! Typed Tauri commands. Every command validates its arguments; file paths
//! are accepted only as paths relative to the authorized root and are
//! resolved through the `AuthorizedRootGuard`. No generic filesystem access
//! is exposed.

pub mod dashboard;
pub mod duplicates;
pub mod files;
pub mod history;
pub mod monitoring;
pub mod organize;
pub mod root;
pub mod rules;
pub mod settings;

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// Runs blocking work off the main thread with access to app state.
pub async fn run<T, F>(app: AppHandle, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle, &AppState) -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let st = app.state::<AppState>();
        f(&app, &st)
    })
    .await
    .map_err(|e| AppError::Io(format!("background task failed: {e}")))?
}

/// Rejects oversized lists from the frontend.
pub fn limit_list<T>(v: &[T], max: usize) -> AppResult<()> {
    if v.len() > max {
        return Err(AppError::invalid(format!(
            "At most {max} items can be processed at once."
        )));
    }
    Ok(())
}
