//! Download-completion heuristics. A file is only considered finished when it
//! is not a temporary download artifact, has no partial sibling, has not been
//! modified for the settling period, and is not open for writing elsewhere.

use std::path::Path;

/// Extensions browsers and download clients use while a transfer is running.
pub const TEMP_DOWNLOAD_EXTS: &[&str] = &[
    "crdownload",
    "part",
    "partial",
    "download",
    "tmp",
    "!qb",
    "opdownload",
    "filepart",
    "aria2",
];

pub fn is_temp_download(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower.starts_with("~$")
        || lower.starts_with(".~lock.")
        || lower.starts_with("unconfirmed ") && lower.ends_with(".crdownload")
    {
        return true;
    }
    match lower.rsplit_once('.') {
        Some((_, ext)) => TEMP_DOWNLOAD_EXTS.contains(&ext),
        None => false,
    }
}

/// Firefox creates the final file (empty) alongside `name.part` while it
/// downloads, so the presence of a partial sibling means "not finished".
pub fn has_partial_sibling(dir: &Path, name: &str) -> bool {
    ["part", "crdownload", "download", "partial", "!qB", "aria2"]
        .iter()
        .any(|ext| std::fs::symlink_metadata(dir.join(format!("{name}.{ext}"))).is_ok())
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Readiness {
    Ready,
    /// Modified too recently; check again later.
    Settling {
        remaining_secs: u64,
    },
    TempDownload,
    PartialSibling,
    Locked,
}

/// Checks whether a file looks like a completed download. `locked_check`
/// performs the (comparatively expensive) exclusive-open test.
pub fn readiness(
    dir: &Path,
    name: &str,
    size: u64,
    modified_ms: i64,
    now_ms: i64,
    settle_secs: u64,
    locked_check: bool,
) -> Readiness {
    if is_temp_download(name) {
        return Readiness::TempDownload;
    }
    if has_partial_sibling(dir, name) {
        return Readiness::PartialSibling;
    }
    let age_ms = now_ms.saturating_sub(modified_ms).max(0) as u64;
    // Empty files are often placeholders created at the start of a download;
    // give them a longer grace period.
    let settle_ms = if size == 0 {
        settle_secs.max(60) * 1000
    } else {
        settle_secs * 1000
    };
    if age_ms < settle_ms {
        return Readiness::Settling {
            remaining_secs: (settle_ms - age_ms).div_ceil(1000),
        };
    }
    if locked_check && is_locked(&dir.join(name)) {
        return Readiness::Locked;
    }
    Readiness::Ready
}

/// True if another process holds the file open in a way that prevents a move.
pub fn is_locked(path: &Path) -> bool {
    #[cfg(windows)]
    {
        match crate::security::winapi::open_for_move(path) {
            Ok(_) => false,
            Err(e) => matches!(e.raw_os_error(), Some(32) | Some(33)),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_extensions() {
        for n in [
            "a.crdownload",
            "Unconfirmed 1234.crdownload",
            "video.mp4.part",
            "x.PARTIAL",
            "f.download",
            "t.tmp",
            "m.!qB",
            "~$report.docx",
        ] {
            assert!(is_temp_download(n), "{n}");
        }
        for n in ["report.pdf", "partial-report.pdf", "download.zip", "tmp"] {
            assert!(!is_temp_download(n), "{n}");
        }
    }

    #[test]
    fn settling() {
        let dir = std::env::temp_dir();
        let now = 1_000_000_000;
        assert_eq!(
            readiness(&dir, "x-nonexistent.pdf", 10, now - 5_000, now, 15, false),
            Readiness::Settling { remaining_secs: 10 }
        );
        assert_eq!(
            readiness(&dir, "x-nonexistent.pdf", 10, now - 20_000, now, 15, false),
            Readiness::Ready
        );
        // Empty file gets at least 60s.
        assert!(matches!(
            readiness(&dir, "x-nonexistent.pdf", 0, now - 20_000, now, 15, false),
            Readiness::Settling { .. }
        ));
    }
}
