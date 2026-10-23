//! Native Windows notifications. Messages contain counts only, never file
//! names, so nothing sensitive appears on the lock screen.

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;

pub fn send(app: &AppHandle, title: &str, body: &str) {
    let enabled = app.state::<AppState>().settings.read().notifications;
    if !enabled {
        return;
    }
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        log::warn!("notification failed: {e}");
    }
}
