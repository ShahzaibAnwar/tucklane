use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use super::{limit_list, run};
use crate::classifier::kinds::{preset_folder, FileKind};
use crate::error::{AppError, AppResult};
use crate::fsops::names::extension_of;
use crate::models::ExclusionRecord;
use crate::scanner::{list_dir, walk_files, Entry};
use crate::security::{display_path, split_rel, NameRule};
use crate::state::{AppState, EV_CHANGED};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorerEntry {
    #[serde(flatten)]
    pub entry: Entry,
    pub ext: Option<String>,
    pub kind: Option<FileKind>,
    pub kind_label: Option<String>,
    /// Where the default rules would put this file (top-level files only).
    pub suggested_folder: Option<String>,
    pub excluded: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub rel: String,
    pub entries: Vec<ExplorerEntry>,
    pub total: usize,
}

fn enrich(st: &AppState, entries: Vec<Entry>, excluded: &HashSet<String>) -> Vec<ExplorerEntry> {
    let preset = st.settings.read().preset;
    entries
        .into_iter()
        .map(|e| {
            let ext = if e.is_dir {
                None
            } else {
                extension_of(&e.name)
            };
            let kind = ext.as_deref().and_then(FileKind::from_extension);
            ExplorerEntry {
                ext,
                kind,
                kind_label: kind.map(|k| k.label().to_string()),
                suggested_folder: (!e.is_dir && !e.rel.contains('/'))
                    .then(|| kind.map(|k| preset_folder(preset, k).to_string()))
                    .flatten(),
                excluded: excluded.contains(&e.rel.to_uppercase()),
                entry: e,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn list_directory(app: AppHandle, rel: String) -> AppResult<DirListing> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        let entries = list_dir(&guard, &rel)?;
        let excluded = st.excluded_set(guard.id())?;
        let entries: Vec<Entry> = entries.into_iter().filter(|e| !e.hidden).collect();
        let total = entries.len();
        Ok(DirListing {
            rel,
            entries: enrich(st, entries, &excluded),
            total,
        })
    })
    .await
}

/// Case-insensitive filename search across the root (links not followed).
#[tauri::command]
pub async fn search_files(
    app: AppHandle,
    query: String,
    limit: Option<usize>,
) -> AppResult<Vec<ExplorerEntry>> {
    run(app, move |_, st| {
        let q = query.trim().to_lowercase();
        if q.is_empty() || q.len() > 200 {
            return Ok(vec![]);
        }
        let guard = st.guard()?;
        let limit = limit.unwrap_or(500).min(5000);
        let mut found = Vec::new();
        let cancel = AtomicBool::new(false);
        walk_files(&guard, 12, &cancel, |e| {
            if found.len() < limit && e.name.to_lowercase().contains(&q) {
                found.push(e.clone());
            }
        })?;
        let excluded = st.excluded_set(guard.id())?;
        Ok(enrich(st, found, &excluded))
    })
    .await
}

/// Whether a file can be opened without an extra confirmation. This is an
/// allowlist of passive document/media types; programs, scripts, shortcuts,
/// disk images, macro-enabled documents and anything unrecognized ask first.
fn opens_without_confirmation(name: &str) -> bool {
    use FileKind::*;
    // Windows ignores trailing dots/spaces, so "x.exe." opens as "x.exe".
    let trimmed = name.trim_end_matches(['.', ' ']);
    let Some(ext) = extension_of(trimmed) else {
        return false;
    };
    let last = ext.rsplit('.').next().unwrap_or("");
    if ["xlsm", "xlsb", "docm", "pptm", "dotm", "svg", "html", "htm"].contains(&last) {
        return false;
    }
    matches!(
        FileKind::from_extension(last),
        Some(
            Pdf | Word
                | Text
                | Ebook
                | Spreadsheet
                | Presentation
                | Image
                | Video
                | Audio
                | Subtitle
                | Archive
                | Torrent
                | Font
        )
    )
}

/// Opens a file with its registered Windows application. Anything that is
/// not a passive document or media file requires explicit confirmation.
#[tauri::command]
pub async fn open_file(
    app: AppHandle,
    rel: String,
    confirmed_executable: Option<bool>,
) -> AppResult<()> {
    run(app, move |app, st| {
        let guard = st.guard()?;
        let (path, info) = guard.resolve_existing(&rel)?;
        if info.is_dir {
            return app
                .opener()
                .open_path(display_path(&path), None::<&str>)
                .map_err(|e| AppError::Io(e.to_string()));
        }
        let name = rel.rsplit('/').next().unwrap_or(&rel);
        if !opens_without_confirmation(name) && confirmed_executable != Some(true) {
            return Err(AppError::ConfirmRequired(
                "This file may run a program or contain active content. Only open it if you trust where it came from."
                    .into(),
            ));
        }
        app.opener()
            .open_path(display_path(&path), None::<&str>)
            .map_err(|e| AppError::Io(e.to_string()))
    })
    .await
}

#[tauri::command]
pub async fn reveal_in_explorer(app: AppHandle, rel: String) -> AppResult<()> {
    run(app, move |app, st| {
        let guard = st.guard()?;
        let (path, _) = guard.resolve_existing(&rel)?;
        app.opener()
            .reveal_item_in_dir(display_path(&path))
            .map_err(|e| AppError::Io(e.to_string()))
    })
    .await
}

pub fn open_root(app: &AppHandle) -> AppResult<()> {
    let st = app.state::<AppState>();
    let guard = st.guard()?;
    app.opener()
        .open_path(guard.display_root(), None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}

#[tauri::command]
pub async fn open_root_folder(app: AppHandle) -> AppResult<()> {
    run(app, |app, _| open_root(app)).await
}

pub const PROJECT_URL: &str = "https://github.com/tucklane/tucklane";

/// Opens the project's GitHub page in the default browser. The URL is fixed;
/// the frontend cannot open arbitrary URLs.
#[tauri::command]
pub async fn open_project_page(app: AppHandle) -> AppResult<()> {
    app.opener()
        .open_url(PROJECT_URL, None::<&str>)
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Full path for "Copy path". Only returned on explicit user request.
#[tauri::command]
pub async fn get_absolute_path(app: AppHandle, rel: String) -> AppResult<String> {
    run(app, move |_, st| {
        let guard = st.guard()?;
        let (path, _) = guard.resolve_existing(&rel)?;
        Ok(display_path(&path))
    })
    .await
}

const THUMB_EXTS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("avif", "image/avif"),
];
const THUMB_MAX_BYTES: u64 = 6 * 1024 * 1024;

/// Returns a data URL for small raster images so the webview can render a
/// preview. Nothing is executed; SVG (which can contain script) is excluded.
#[tauri::command]
pub async fn get_thumbnail(app: AppHandle, rel: String) -> AppResult<Option<String>> {
    use base64::Engine;
    run(app, move |_, st| {
        let ext = extension_of(&rel).unwrap_or_default();
        let Some((_, mime)) = THUMB_EXTS.iter().find(|(e, _)| *e == ext) else {
            return Ok(None);
        };
        let guard = st.guard()?;
        let (path, info) = guard.resolve_existing(&rel)?;
        if info.is_dir || info.size > THUMB_MAX_BYTES {
            return Ok(None);
        }
        let bytes = std::fs::read(&path)?;
        Ok(Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )))
    })
    .await
}

#[tauri::command]
pub async fn exclude_files(app: AppHandle, rels: Vec<String>) -> AppResult<()> {
    run(app.clone(), move |_, st| {
        limit_list(&rels, 10_000)?;
        let guard = st.guard()?;
        let db = st.db.lock();
        for r in &rels {
            split_rel(r, NameRule::Existing)?;
            db.add_exclusion(guard.id(), r, "user")?;
        }
        Ok(())
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "exclusions");
    Ok(())
}

#[tauri::command]
pub async fn include_files(app: AppHandle, rels: Vec<String>) -> AppResult<()> {
    run(app.clone(), move |_, st| {
        limit_list(&rels, 10_000)?;
        let guard = st.guard()?;
        let db = st.db.lock();
        for r in &rels {
            db.remove_exclusion(guard.id(), r)?;
        }
        Ok(())
    })
    .await?;
    let _ = app.emit(EV_CHANGED, "exclusions");
    Ok(())
}

#[tauri::command]
pub async fn get_exclusions(app: AppHandle) -> AppResult<Vec<ExclusionRecord>> {
    run(app, |_, st| {
        let guard = st.guard()?;
        st.db.lock().list_exclusions(guard.id())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::opens_without_confirmation as ok;

    #[test]
    fn only_passive_files_open_without_confirmation() {
        for n in [
            "report.pdf",
            "photo.JPG",
            "song.mp3",
            "notes.txt",
            "data.xlsx",
            "a.tar.gz",
        ] {
            assert!(ok(n), "{n}");
        }
        for n in [
            "setup.exe",
            "x.exe.",
            "x.exe ",
            "tool.cpl",
            "link.url",
            "app.appref-ms",
            "disk.iso",
            "run.js",
            "macro.xlsm",
            "page.html",
            "drawing.svg",
            "script.ps1",
            "unknown.qqq",
            "README",
            "x.msc",
            "help.chm",
        ] {
            assert!(!ok(n), "{n}");
        }
    }
}
