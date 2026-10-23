//! Two-stage duplicate finder.
//!
//! Stage 1 groups files by size, then by a quick fingerprint (SHA-256 of the
//! first and last 64 KiB). Stage 2 confirms candidates with a full streamed
//! SHA-256. Matching names are never treated as evidence of duplication.
//!
//! Removal only ever uses the Windows Recycle Bin, requires explicit
//! confirmation in the UI, re-verifies content first, and always keeps at
//! least one copy of every group.

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::database::Db;
use crate::error::{AppError, AppResult};
use crate::fsops::sha256_file;
use crate::models::{now_iso, DuplicateGroup, DuplicateMember, OpKind, OpState, SessionKind};
use crate::scanner::{walk_files, Entry};
use crate::security::AuthorizedRootGuard;

const MAX_DEPTH: usize = 12;
const EDGE: u64 = 64 * 1024;
/// Files smaller than this are skipped: tiny files are commonly identical by
/// design (empty files, placeholders) and recover no meaningful space.
const MIN_SIZE: u64 = 1;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DupProgress {
    pub phase: String,
    pub files_seen: usize,
    pub candidates: usize,
    pub hashed: usize,
    pub to_hash: usize,
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DupSummary {
    pub groups: usize,
    pub duplicate_files: usize,
    pub recoverable_bytes: u64,
    pub scanned_at: Option<String>,
}

pub fn summarize(groups: &[DuplicateGroup]) -> DupSummary {
    DupSummary {
        groups: groups.len(),
        duplicate_files: groups
            .iter()
            .map(|g| g.members.len().saturating_sub(1))
            .sum(),
        recoverable_bytes: groups
            .iter()
            .map(|g| g.size * g.members.len().saturating_sub(1) as u64)
            .sum(),
        scanned_at: groups.first().map(|g| g.scanned_at.clone()),
    }
}

fn quick_fingerprint(path: &Path, size: u64) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; EDGE as usize];
    let n = read_full(&mut f, &mut buf)?;
    h.update(&buf[..n]);
    if size > EDGE * 2 {
        f.seek(SeekFrom::Start(size - EDGE))?;
        let n = read_full(&mut f, &mut buf)?;
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

fn read_full(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

/// Scans the root (recursively, not following links) for duplicate files.
pub fn find_duplicates(
    guard: &AuthorizedRootGuard,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(&DupProgress),
) -> AppResult<Vec<DuplicateGroup>> {
    let mut p = DupProgress {
        phase: "scanning".into(),
        ..Default::default()
    };
    let mut by_size: HashMap<u64, Vec<Entry>> = HashMap::new();
    walk_files(guard, MAX_DEPTH, cancel, |e| {
        if e.size >= MIN_SIZE && !crate::fsops::is_staging_name(&e.name) {
            by_size.entry(e.size).or_default().push(e.clone());
        }
        p.files_seen += 1;
        if p.files_seen % 500 == 0 {
            on_progress(&p);
        }
    })?;

    // Stage 1b: quick fingerprints within same-size groups.
    let size_groups: Vec<(u64, Vec<Entry>)> =
        by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    p.candidates = size_groups.iter().map(|(_, v)| v.len()).sum();
    p.phase = "fingerprinting".into();
    on_progress(&p);
    let mut candidates: Vec<(u64, Vec<Entry>)> = Vec::new();
    for (size, entries) in size_groups {
        let mut by_fp: HashMap<String, Vec<Entry>> = HashMap::new();
        for e in entries {
            if cancel.load(Ordering::Relaxed) {
                return Err(AppError::Cancelled);
            }
            let Ok((abs, _)) = guard.resolve_existing(&e.rel) else {
                continue;
            };
            if let Ok(fp) = quick_fingerprint(&abs, size) {
                by_fp.entry(fp).or_default().push(e);
            }
        }
        candidates.extend(
            by_fp
                .into_values()
                .filter(|v| v.len() > 1)
                .map(|v| (size, v)),
        );
    }

    // Stage 2: full hashes, at most two files at a time (gentle on HDDs).
    p.phase = "verifying".into();
    p.to_hash = candidates.iter().map(|(_, v)| v.len()).sum();
    on_progress(&p);
    let scanned_at = now_iso();
    let hashed = Mutex::new(0usize);
    let mut groups = Vec::new();
    for (size, entries) in candidates {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Cancelled);
        }
        let results: Vec<(Entry, Option<String>)> = std::thread::scope(|s| {
            let chunks: Vec<&[Entry]> = entries.chunks(entries.len().div_ceil(2)).collect();
            let handles: Vec<_> = chunks
                .into_iter()
                .map(|chunk| {
                    let hashed = &hashed;
                    s.spawn(move || {
                        chunk
                            .iter()
                            .map(|e| {
                                if cancel.load(Ordering::Relaxed) {
                                    return (e.clone(), None);
                                }
                                let h = guard
                                    .resolve_existing(&e.rel)
                                    .ok()
                                    .and_then(|(abs, _)| sha256_file(&abs).ok());
                                *hashed.lock() += 1;
                                (e.clone(), h)
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        p.hashed = *hashed.lock();
        on_progress(&p);
        let mut by_hash: HashMap<String, Vec<Entry>> = HashMap::new();
        for (e, h) in results {
            if let Some(h) = h {
                by_hash.entry(h).or_default().push(e);
            }
        }
        for (hash, mut members) in by_hash {
            if members.len() < 2 {
                continue;
            }
            members.sort_by_key(|m| m.modified_ms);
            groups.push(DuplicateGroup {
                id: uuid::Uuid::new_v4().to_string(),
                hash,
                size,
                members: members
                    .into_iter()
                    .map(|m| DuplicateMember {
                        rel: m.rel,
                        name: m.name,
                        size: m.size,
                        modified_ms: m.modified_ms,
                    })
                    .collect(),
                scanned_at: scanned_at.clone(),
            });
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(AppError::Cancelled);
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * g.members.len() as u64));
    p.phase = "done".into();
    p.finished = true;
    on_progress(&p);
    Ok(groups)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycleResult {
    pub rel: String,
    pub ok: bool,
    pub message: String,
}

/// Moves the selected duplicate copies to the Recycle Bin after verifying
/// that each still matches its group's hash and that at least one verified
/// copy of the group remains.
pub fn recycle_duplicates(
    guard: &AuthorizedRootGuard,
    db: &Mutex<Db>,
    rels: &[String],
) -> AppResult<Vec<RecycleResult>> {
    guard.verify_root()?;
    let groups = db.lock().list_duplicate_groups(guard.id())?;
    let mut results = Vec::new();
    let session = db
        .lock()
        .create_session(guard.id(), SessionKind::Manual, rels.len() as i64)?;

    for g in &groups {
        let selected: Vec<&DuplicateMember> = g
            .members
            .iter()
            .filter(|m| rels.iter().any(|r| r.eq_ignore_ascii_case(&m.rel)))
            .collect();
        if selected.is_empty() {
            continue;
        }
        // Verify the copies we keep: at least one must still match the hash.
        let keepers_ok = g
            .members
            .iter()
            .filter(|m| !selected.iter().any(|s| s.rel == m.rel))
            .any(|m| {
                guard
                    .resolve_existing(&m.rel)
                    .ok()
                    .and_then(|(p, _)| sha256_file(&p).ok())
                    .as_deref()
                    == Some(&g.hash)
            });
        for m in &selected {
            let mut r = RecycleResult {
                rel: m.rel.clone(),
                ok: false,
                message: String::new(),
            };
            if !keepers_ok {
                r.message = "Kept: no other verified copy of this file remains.".into();
                results.push(r);
                continue;
            }
            let op_id = db.lock().insert_planned_op(
                &session,
                guard.id(),
                OpKind::Recycle,
                &m.rel,
                "",
                &m.name,
                Some(m.size as i64),
                Some(m.modified_ms),
                None,
                None,
                Some("Duplicate copy moved to Recycle Bin"),
                None,
            )?;
            let outcome = (|| -> AppResult<()> {
                let (abs, info) = guard.resolve_existing(&m.rel)?;
                if info.is_dir || info.size != g.size {
                    return Err(AppError::Changed);
                }
                if sha256_file(&abs)? != g.hash {
                    return Err(AppError::Changed);
                }
                // Removable and other non-fixed volumes may have no Recycle
                // Bin, in which case Windows would delete permanently.
                #[cfg(windows)]
                if !crate::security::winapi::is_fixed_drive(&abs) {
                    return Err(AppError::invalid(
                        "This drive has no Recycle Bin, so Tucklane won't remove the file. Delete it yourself if you're sure.",
                    ));
                }
                trash::delete(&abs).map_err(|e| AppError::Io(format!("Recycle Bin: {e}")))?;
                Ok(())
            })();
            let db = db.lock();
            match outcome {
                Ok(()) => {
                    let _ = db.set_op_state(&op_id, OpState::Completed, None);
                    r.ok = true;
                    r.message = "Moved to the Recycle Bin.".into();
                }
                Err(e) => {
                    let _ = db.set_op_state(&op_id, OpState::Failed, Some(&e.to_string()));
                    r.message = e.to_string();
                }
            }
            results.push(r);
        }
    }
    {
        let db = db.lock();
        let s = db.finish_session(&session, "completed")?;
        let n = results.iter().filter(|r| r.ok).count();
        db.log_event(
            "info",
            "duplicates",
            &format!("{n} duplicate copies moved to the Recycle Bin."),
            Some(&s.id),
        );
        // Drop recycled members from the stored groups.
        let remaining: Vec<DuplicateGroup> = groups
            .into_iter()
            .map(|mut g| {
                g.members
                    .retain(|m| !results.iter().any(|r| r.ok && r.rel == m.rel));
                g
            })
            .filter(|g| g.members.len() > 1)
            .collect();
        db.replace_duplicate_groups(guard.id(), &remaining)?;
    }
    Ok(results)
}
