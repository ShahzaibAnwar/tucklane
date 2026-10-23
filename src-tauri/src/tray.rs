//! System tray icon and menu.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::state::{AppState, EV_NAVIGATE};
use crate::watcher::MonitorState;

const TRAY_ID: &str = "tucklane-tray";

pub struct TrayItems {
    pub pause: MenuItem<Wry>,
    pub resume: MenuItem<Wry>,
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if app.tray_by_id(TRAY_ID).is_some() {
        return Ok(());
    }
    let open = MenuItem::with_id(app, "open", "Open Tucklane", true, None::<&str>)?;
    let organize = MenuItem::with_id(app, "organize", "Organize Now", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause Monitoring", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume Monitoring", false, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Open Downloads", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Tucklane", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open, &organize, &sep1, &pause, &resume, &folder, &settings, &sep2, &quit,
        ],
    )?;

    app.manage(TrayItems {
        pause: pause.clone(),
        resume: resume.clone(),
    });

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Tucklane")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "organize" => {
                show_main(app);
                let _ = app.emit(EV_NAVIGATE, "organize");
            }
            "pause" => crate::commands::monitoring::set_paused(app, true),
            "resume" => crate::commands::monitoring::set_paused(app, false),
            "folder" => {
                let _ = crate::commands::files::open_root(app);
            }
            "settings" => {
                show_main(app);
                let _ = app.emit(EV_NAVIGATE, "settings");
            }
            "quit" => crate::tray::quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    refresh(app);
    Ok(())
}

pub fn remove(app: &AppHandle) {
    let _ = app.remove_tray_by_id(TRAY_ID);
}

/// Stops monitoring and exits without blocking the main thread: waiting for
/// the monitor thread (which may be finishing a move) happens on a worker.
pub fn quit(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        crate::state::stop_monitor(&app);
        app.exit(0);
    });
}

/// Updates tooltip and pause/resume availability from the monitor status.
///
/// Tray and menu updates are executed on the main thread. Calling them
/// directly from a worker blocks until the main thread answers, which
/// deadlocks if the main thread is itself waiting for that worker (e.g.
/// joining the monitor thread on quit). So the work is always queued.
pub fn refresh(app: &AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || refresh_now(&h));
}

fn refresh_now(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let state = app.state::<AppState>();
    let status = state.monitor_status();
    let auto = state.settings.read().auto_organize;
    let text = match status.state {
        MonitorState::Watching if auto => "Tucklane — organizing new downloads".to_string(),
        MonitorState::Watching => "Tucklane — watching for new downloads".to_string(),
        MonitorState::Paused => "Tucklane — monitoring paused".to_string(),
        MonitorState::Error => "Tucklane — folder unavailable".to_string(),
        MonitorState::Off => "Tucklane — monitoring off".to_string(),
    };
    let _ = tray.set_tooltip(Some(text));
    if let Some(items) = app.try_state::<TrayItems>() {
        let paused = status.state == MonitorState::Paused;
        let active = matches!(status.state, MonitorState::Watching | MonitorState::Error);
        let _ = items.pause.set_enabled(active);
        let _ = items.resume.set_enabled(paused);
    }
}
