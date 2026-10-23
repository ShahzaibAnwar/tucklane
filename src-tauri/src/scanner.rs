//! Directory listing inside the authorized root. Never follows links.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::security::{query_entry, AuthorizedRootGuard};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub hidden: bool,
    /// Symbolic link, junction or other reparse point Tucklane will not follow.
    pub is_link: bool,
}

fn to_ms(t: std::io::Result<std::time::SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn read_entries(
    guard: &AuthorizedRootGuard,
    dir: &Path,
    rel_prefix: &str,
) -> AppResult<Vec<Entry>> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().into_owned();
        // DirEntry::metadata does not follow links on Windows.
        let Ok(md) = item.metadata() else { continue };
        #[cfg(windows)]
        let (attrs, is_reparse) = {
            use std::os::windows::fs::MetadataExt;
            let a = md.file_attributes();
            (a, a & 0x400 != 0)
        };
        #[cfg(not(windows))]
        let (attrs, is_reparse) = (0u32, md.file_type().is_symlink());
        let mut is_link = false;
        if is_reparse {
            // Benign reparse points (e.g. OneDrive placeholders) are fine.
            is_link = query_entry(&item.path())
                .map(|i| i.is_unsafe_link)
                .unwrap_or(true);
        }
        #[cfg(windows)]
        let hidden = attrs & 0x6 != 0; // HIDDEN | SYSTEM
        #[cfg(not(windows))]
        let hidden = {
            let _ = attrs;
            name.starts_with('.')
        };
        let rel = if rel_prefix.is_empty() {
            name.clone()
        } else {
            format!("{rel_prefix}/{name}")
        };
        out.push(Entry {
            rel,
            name,
            is_dir: md.is_dir(),
            size: if md.is_dir() { 0 } else { md.len() },
            modified_ms: to_ms(md.modified()),
            created_ms: to_ms(md.created()),
            hidden,
            is_link,
        });
    }
    let _ = guard;
    Ok(out)
}

/// Lists one directory (`""` = the root).
pub fn list_dir(guard: &AuthorizedRootGuard, rel_dir: &str) -> AppResult<Vec<Entry>> {
    guard.verify_root()?;
    let dir = if rel_dir.is_empty() {
        guard.root().to_path_buf()
    } else {
        let (p, info) = guard.resolve_existing(rel_dir)?;
        if !info.is_dir {
            return Err(AppError::invalid("That is not a folder."));
        }
        p
    };
    read_entries(guard, &dir, rel_dir)
}

/// Recursively visits regular files (not links, not hidden/system), up to
/// `max_depth` levels below the root. Stops early when `cancel` is set.
pub fn walk_files(
    guard: &AuthorizedRootGuard,
    max_depth: usize,
    cancel: &AtomicBool,
    mut visit: impl FnMut(&Entry),
) -> AppResult<()> {
    guard.verify_root()?;
    let mut stack: Vec<(std::path::PathBuf, String, usize)> =
        vec![(guard.root().to_path_buf(), String::new(), 0)];
    while let Some((dir, rel, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let entries = match read_entries(guard, &dir, &rel) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("skipping unreadable folder: {e}");
                continue;
            }
        };
        for e in entries {
            if e.is_link || e.hidden {
                continue;
            }
            if e.is_dir {
                if depth < max_depth {
                    stack.push((dir.join(&e.name), e.rel.clone(), depth + 1));
                }
            } else {
                visit(&e);
            }
        }
    }
    Ok(())
}
