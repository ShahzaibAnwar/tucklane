//! Safe file movement inside the authorized root.
//!
//! Moves are performed by handle: the source file is opened without following
//! links (and without sharing write/delete access, so files still being
//! written are reported as locked), validated, and renamed through that same
//! handle with "replace if exists" disabled. Existing files are never
//! overwritten. If an atomic rename is impossible (different volume), a
//! copy-and-verify path is used with a staging file inside the root.

pub mod names;
pub mod stability;

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::security::{query_entry, AuthorizedRootGuard, NameRule};

pub const STAGING_PREFIX: &str = ".tucklane-staging-";

/// What the caller believes about the source file (from the scan).
#[derive(Debug, Clone, Default)]
pub struct Expectation {
    pub size: Option<u64>,
    pub modified_ms: Option<i64>,
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnCollision {
    /// Pick `name (n).ext`.
    Rename,
    /// Fail with [`AppError::Conflict`].
    Fail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveMethod {
    Rename,
    CopyVerify,
}

#[derive(Debug, Clone)]
pub struct MoveOutcome {
    pub dest_rel: String,
    pub final_name: String,
    pub size: u64,
    pub modified_ms: i64,
    pub file_id: String,
    pub sha256: Option<String>,
    pub method: MoveMethod,
}

/// Options for [`move_within_root`].
#[derive(Debug, Clone)]
pub struct MoveRequest<'a> {
    pub source_rel: &'a str,
    /// Destination directory relative to the root; empty string = the root itself.
    pub dest_dir_rel: &'a str,
    pub desired_name: &'a str,
    pub on_collision: OnCollision,
    pub expect: Expectation,
    /// Testing hook: skip the atomic rename and use copy-and-verify.
    pub force_copy: bool,
}

fn resolve_dir(guard: &AuthorizedRootGuard, rel: &str) -> AppResult<PathBuf> {
    if rel.is_empty() {
        guard.verify_root()?;
        Ok(guard.root().to_path_buf())
    } else {
        guard.ensure_dir(rel)
    }
}

/// Moves one file inside the authorized root. Never overwrites.
pub fn move_within_root(guard: &AuthorizedRootGuard, req: &MoveRequest) -> AppResult<MoveOutcome> {
    guard.verify_root()?;
    crate::security::validate_component(req.desired_name, NameRule::Existing)?;

    let (src_path, src_info) = guard.resolve_existing(req.source_rel)?;
    if src_info.is_dir {
        return Err(AppError::invalid("Folders are not moved by Tucklane."));
    }
    check_expectation(
        &req.expect,
        src_info.size,
        src_info.modified_ms,
        &src_info.file_id,
    )?;

    let dest_dir = resolve_dir(guard, req.dest_dir_rel)?;
    platform::move_file(guard, &src_path, &src_info.file_id, &dest_dir, req)
}

fn check_expectation(e: &Expectation, size: u64, modified_ms: i64, file_id: &str) -> AppResult<()> {
    if e.size.is_some_and(|s| s != size)
        || e.modified_ms
            .is_some_and(|m| (m - modified_ms).abs() > 2_000)
        || e.file_id
            .as_deref()
            .is_some_and(|id| !id.is_empty() && id != file_id)
    {
        return Err(AppError::Changed);
    }
    Ok(())
}

/// Streams `reader` into `writer`, returning (bytes, sha256-hex).
fn copy_hashing(reader: &mut impl Read, writer: &mut impl Write) -> std::io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        writer.write_all(&buf[..n])?;
        total += n as u64;
    }
    Ok((total, hex::encode(hasher.finalize())))
}

/// SHA-256 of a file, streamed (never loads the file into memory).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let (_, h) = copy_hashing(&mut f, &mut std::io::sink())?;
    Ok(h)
}

fn staging_name() -> String {
    format!("{STAGING_PREFIX}{}.tmp", uuid::Uuid::new_v4().simple())
}

/// Removes leftover staging files (from an interrupted copy) when the source
/// still exists. Only files with Tucklane's staging prefix are touched.
pub fn is_staging_name(name: &str) -> bool {
    name.starts_with(STAGING_PREFIX) && name.ends_with(".tmp")
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::security::{compare_key, is_within, winapi};

    const ERROR_NOT_SAME_DEVICE: i32 = 17;

    pub fn move_file(
        guard: &AuthorizedRootGuard,
        src_path: &Path,
        expected_id: &str,
        dest_dir: &Path,
        req: &MoveRequest,
    ) -> AppResult<MoveOutcome> {
        // Pin the destination directory for the duration of the move.
        let dir_handle = winapi::open_dir_pinned(dest_dir)?;
        let dir_info = winapi::handle_info(&dir_handle)?;
        if !dir_info.is_dir
            || dir_info
                .reparse_tag
                .is_some_and(|t| !crate::security::is_benign_reparse_tag(t))
            || !is_within(guard.root(), &dir_info.final_path)
        {
            return Err(AppError::unauthorized(
                "Destination folder is not inside the authorized folder.",
            ));
        }

        // Open the source by handle; this fails if it is still being written.
        let src = winapi::open_for_move(src_path)?;
        let info = winapi::handle_info(&src)?;
        if info.is_dir
            || info
                .reparse_tag
                .is_some_and(|t| !crate::security::is_benign_reparse_tag(t))
        {
            return Err(AppError::unauthorized(
                "Tucklane does not move links or folders.",
            ));
        }
        if info.file_id != expected_id || !is_within(guard.root(), &info.final_path) {
            return Err(AppError::Changed);
        }
        check_expectation(&req.expect, info.size, info.modified_ms, &info.file_id)?;

        let mut reserved = HashSet::new();
        for _ in 0..64 {
            let name = match req.on_collision {
                OnCollision::Rename => {
                    names::next_free_name(Some(dest_dir), req.desired_name, &reserved)
                }
                OnCollision::Fail => req.desired_name.to_string(),
            };
            let dest = dest_dir.join(&name);
            if compare_key(&dest) == compare_key(&info.final_path) {
                return Err(AppError::Conflict(
                    "The file is already in that location.".into(),
                ));
            }
            if !req.force_copy {
                match winapi::rename_by_handle_no_replace(&src, &dest) {
                    Ok(()) => return finish(guard, &dest, &info, None, MoveMethod::Rename),
                    Err(e) if matches!(e.raw_os_error(), Some(80) | Some(183)) => {
                        if req.on_collision == OnCollision::Fail {
                            return Err(AppError::Conflict(format!(
                                "\"{name}\" already exists in the destination."
                            )));
                        }
                        reserved.insert(names::name_key(&name));
                        continue;
                    }
                    Err(e) if e.raw_os_error() == Some(ERROR_NOT_SAME_DEVICE) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            return copy_verify(guard, &src, src_path, &info, dest_dir, &dest, req);
        }
        Err(AppError::Conflict(
            "Could not find a free filename in the destination.".into(),
        ))
    }

    /// Builds the outcome after the filesystem has already completed the
    /// move. This must not fail: the file *has* moved, and reporting an error
    /// here would leave history claiming otherwise. Metadata that cannot be
    /// re-read falls back to what was known before the move.
    fn finish(
        guard: &AuthorizedRootGuard,
        dest: &Path,
        before: &winapi::HandleInfo,
        sha256: Option<String>,
        method: MoveMethod,
    ) -> AppResult<MoveOutcome> {
        let after = query_entry(dest).ok();
        if let Some(a) = &after {
            // FAT/exFAT derive file IDs from the directory entry, so an ID
            // change after a successful rename is expected there.
            if method == MoveMethod::Rename && a.file_id != before.file_id {
                log::info!("file ID changed across rename (non-NTFS volume?)");
            }
        }
        let rel = after
            .as_ref()
            .and_then(|a| guard.rel_of(&a.final_path).ok())
            .or_else(|| guard.rel_of(dest).ok())
            .unwrap_or_default();
        let final_name = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(MoveOutcome {
            dest_rel: rel,
            final_name,
            size: after.as_ref().map(|a| a.size).unwrap_or(before.size),
            modified_ms: after
                .as_ref()
                .map(|a| a.modified_ms)
                .unwrap_or(before.modified_ms),
            file_id: after
                .map(|a| a.file_id)
                .unwrap_or_else(|| before.file_id.clone()),
            sha256,
            method,
        })
    }

    fn set_delete_disposition(file: &std::fs::File) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FileDispositionInfo, SetFileInformationByHandle,
        };
        let delete: u32 = 1; // FILE_DISPOSITION_INFO { BOOLEAN DeleteFile } (padded)
                             // SAFETY: valid handle opened with DELETE access; 4-byte buffer covers the struct.
        let ok = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as _,
                FileDispositionInfo,
                &delete as *const u32 as *const _,
                1,
            )
        };
        if ok == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn copy_verify(
        guard: &AuthorizedRootGuard,
        src_handle: &std::fs::File,
        src_path: &Path,
        info: &winapi::HandleInfo,
        dest_dir: &Path,
        dest: &Path,
        _req: &MoveRequest,
    ) -> AppResult<MoveOutcome> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};

        // A read-only source could be copied but not removed afterwards,
        // leaving two copies. Refuse before copying anything.
        const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
        if info.attributes & FILE_ATTRIBUTE_READONLY != 0 {
            return Err(AppError::PermissionDenied);
        }
        let staging = dest_dir.join(staging_name());
        let cleanup = |p: &Path| {
            let _ = std::fs::remove_file(p);
        };
        let result = (|| -> AppResult<String> {
            let mut reader = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
                .open(src_path)?;
            let mut out = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staging)?;
            let (n, src_hash) = copy_hashing(&mut reader, &mut out)?;
            out.sync_all()?;
            if let Ok(t) = std::fs::metadata(src_path).and_then(|m| m.modified()) {
                let _ = out.set_modified(t);
            }
            drop(out);
            if n != info.size {
                return Err(AppError::Changed);
            }
            let verify = sha256_file(&staging)?;
            if verify != src_hash {
                return Err(AppError::Io("Copy verification failed.".into()));
            }
            Ok(src_hash)
        })();
        let hash = match result {
            Ok(h) => h,
            Err(e) => {
                cleanup(&staging);
                return Err(e);
            }
        };
        // Publish the staging file under its final name (never replacing).
        let publish = (|| -> AppResult<()> {
            let st = winapi::open_for_move(&staging)?;
            winapi::rename_by_handle_no_replace(&st, dest).map_err(AppError::from)
        })();
        if let Err(e) = publish {
            cleanup(&staging);
            return Err(e);
        }
        // Only now, with a verified destination, remove the source by handle.
        if let Err(e) = set_delete_disposition(src_handle) {
            log::warn!("copy-verify: destination verified but source removal failed: {e}");
            return Err(AppError::Io(
                "The file was copied but the original could not be removed. Both copies are kept."
                    .into(),
            ));
        }
        finish(guard, dest, info, Some(hash), MoveMethod::CopyVerify)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn move_file(
        guard: &AuthorizedRootGuard,
        src_path: &Path,
        _expected_id: &str,
        dest_dir: &Path,
        req: &MoveRequest,
    ) -> AppResult<MoveOutcome> {
        let mut reserved = HashSet::new();
        for _ in 0..64 {
            let name = match req.on_collision {
                OnCollision::Rename => {
                    names::next_free_name(Some(dest_dir), req.desired_name, &reserved)
                }
                OnCollision::Fail => req.desired_name.to_string(),
            };
            let dest = dest_dir.join(&name);
            // hard_link fails if the destination exists: an atomic no-replace move.
            match std::fs::hard_link(src_path, &dest) {
                Ok(()) => {
                    std::fs::remove_file(src_path)?;
                    let after = query_entry(&dest)?;
                    return Ok(MoveOutcome {
                        dest_rel: guard.rel_of(&after.final_path)?,
                        final_name: name,
                        size: after.size,
                        modified_ms: after.modified_ms,
                        file_id: after.file_id,
                        sha256: None,
                        method: MoveMethod::Rename,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if req.on_collision == OnCollision::Fail {
                        return Err(AppError::Conflict(format!(
                            "\"{name}\" already exists in the destination."
                        )));
                    }
                    reserved.insert(names::name_key(&name));
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(AppError::Conflict(
            "Could not find a free filename in the destination.".into(),
        ))
    }
}
