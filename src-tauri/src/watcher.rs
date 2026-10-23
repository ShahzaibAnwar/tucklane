//! Background monitoring of the authorized root's top level.
//!
//! Filesystem events only *nominate* files. A nominated file is handed to
//! the organizer only once it looks like a finished download (see
//! [`crate::fsops::stability`]). Events are debounced per file name. The
//! watch is non-recursive, so Tucklane moving a file into a category folder
//! never produces a new nomination; files Tucklane restores via undo are
//! nominated but excluded from organization by the undo itself.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use notify::{EventKind, RecursiveMode, Watcher as _};
use parking_lot::Mutex;
use serde::Serialize;

use crate::fsops::stability::{has_partial_sibling, is_locked, is_temp_download};
use crate::models::{now_iso, now_ms};
use crate::security::{compare_key, query_entry};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MonitorState {
    Off,
    Watching,
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorStatus {
    pub state: MonitorState,
    /// Files seen recently that are still settling or in use.
    pub pending: usize,
    pub last_scan_at: Option<String>,
    pub next_scan_at: Option<String>,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for MonitorStatus {
    fn default() -> Self {
        Self {
            state: MonitorState::Off,
            pending: 0,
            last_scan_at: None,
            next_scan_at: None,
            last_event_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MonitorConfig {
    pub settle_secs: u64,
    pub scan_interval_mins: u64,
}

/// Called with the names of top-level files that look complete.
pub type ReadyCallback = Arc<dyn Fn(Vec<String>) + Send + Sync>;
/// Called whenever the status changes.
pub type StatusCallback = Arc<dyn Fn(&MonitorStatus) + Send + Sync>;

enum Msg {
    Fs(notify::Result<notify::Event>),
    /// Automatic rescan (startup, resume, wake, schedule): files already
    /// reported unchanged are not reported again.
    Rescan,
    /// User-requested rescan: everything eligible is reported again.
    ForceRescan,
    Pause(bool),
    Config(MonitorConfig),
    Stop,
}

struct Pending {
    first_seen: Instant,
    next_check: Instant,
    size: u64,
    modified_ms: i64,
    attempts: u32,
}

/// Maximum stability checks before giving up on a file (~1 hour with backoff).
const MAX_ATTEMPTS: u32 = 120;

pub struct Monitor {
    tx: Sender<Msg>,
    handle: Option<JoinHandle<()>>,
    status: Arc<Mutex<MonitorStatus>>,
}

impl Monitor {
    pub fn start(
        root: PathBuf,
        config: MonitorConfig,
        paused: bool,
        on_ready: ReadyCallback,
        on_status: StatusCallback,
    ) -> Monitor {
        let (tx, rx) = unbounded();
        let status = Arc::new(Mutex::new(MonitorStatus::default()));
        let st = status.clone();
        let tx2 = tx.clone();
        let handle = std::thread::Builder::new()
            .name("tucklane-monitor".into())
            .spawn(move || run(root, config, paused, rx, tx2, st, on_ready, on_status))
            .expect("spawn monitor thread");
        Monitor {
            tx,
            handle: Some(handle),
            status,
        }
    }

    pub fn status(&self) -> MonitorStatus {
        self.status.lock().clone()
    }

    pub fn rescan(&self) {
        let _ = self.tx.send(Msg::ForceRescan);
    }

    pub fn set_paused(&self, paused: bool) {
        let _ = self.tx.send(Msg::Pause(paused));
    }

    pub fn set_config(&self, c: MonitorConfig) {
        let _ = self.tx.send(Msg::Config(c));
    }

    pub fn stop(mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    root: PathBuf,
    mut config: MonitorConfig,
    mut paused: bool,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    status: Arc<Mutex<MonitorStatus>>,
    on_ready: ReadyCallback,
    on_status: StatusCallback,
) {
    let mut pending: HashMap<String, Pending> = HashMap::new();
    // Files already handed to `on_ready`, with the (size, mtime) reported.
    let mut reported: HashMap<String, (u64, i64)> = HashMap::new();
    let mut watcher: Option<notify::RecommendedWatcher> = None;
    let mut last_tick = Instant::now();
    let mut next_health = Instant::now();
    let mut next_scan: Option<Instant> = None;
    let root_key = compare_key(&root);

    let publish = |status: &Arc<Mutex<MonitorStatus>>, f: &dyn Fn(&mut MonitorStatus)| {
        let snapshot = {
            let mut s = status.lock();
            f(&mut s);
            s.clone()
        };
        on_status(&snapshot);
    };

    let schedule_next = |config: &MonitorConfig| -> Option<Instant> {
        (config.scan_interval_mins > 0)
            .then(|| Instant::now() + Duration::from_secs(config.scan_interval_mins * 60))
    };

    // Initial full scan catches anything that arrived while Tucklane was closed.
    let _ = tx.send(Msg::Rescan);

    loop {
        // (Re)establish the OS watch when needed.
        if watcher.is_none() && Instant::now() >= next_health {
            let txw = tx.clone();
            let ok = query_entry(&root)
                .map(|i| i.is_dir && !i.is_unsafe_link)
                .unwrap_or(false);
            let result = if ok {
                notify::recommended_watcher(move |res| {
                    let _ = txw.send(Msg::Fs(res));
                })
                .and_then(|mut w| w.watch(&root, RecursiveMode::NonRecursive).map(|_| w))
                .map_err(|e| e.to_string())
            } else {
                Err("The authorized folder is not available.".to_string())
            };
            match result {
                Ok(w) => {
                    watcher = Some(w);
                    next_scan = schedule_next(&config);
                    publish(&status, &|s| {
                        s.state = if paused {
                            MonitorState::Paused
                        } else {
                            MonitorState::Watching
                        };
                        s.last_error = None;
                    });
                    let _ = tx.send(Msg::Rescan);
                }
                Err(e) => {
                    log::warn!("monitor: cannot watch root: {e}");
                    publish(&status, &|s| {
                        s.state = MonitorState::Error;
                        s.last_error = Some(e.clone());
                    });
                    next_health = Instant::now() + Duration::from_secs(30);
                }
            }
        }

        match rx.recv_timeout(Duration::from_millis(1000)) {
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(Msg::Pause(p)) => {
                paused = p;
                publish(&status, &|s| {
                    if s.state != MonitorState::Error {
                        s.state = if p {
                            MonitorState::Paused
                        } else {
                            MonitorState::Watching
                        };
                    }
                });
                if !p {
                    let _ = tx.send(Msg::Rescan);
                }
            }
            Ok(Msg::Config(c)) => {
                config = c;
                next_scan = schedule_next(&config);
            }
            Ok(msg @ (Msg::Rescan | Msg::ForceRescan)) => {
                if matches!(msg, Msg::ForceRescan) {
                    reported.clear();
                }
                let mut present = std::collections::HashSet::new();
                if let Ok(rd) = std::fs::read_dir(&root) {
                    for e in rd.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if let Ok(md) = e.metadata() {
                            if md.is_file() {
                                present.insert(name.clone());
                                nominate(&mut pending, name, Duration::ZERO);
                            }
                        }
                    }
                }
                reported.retain(|n, _| present.contains(n));
                next_scan = schedule_next(&config);
                let next_iso = next_scan.map(|n| {
                    let d = n.saturating_duration_since(Instant::now());
                    (chrono::Utc::now() + chrono::Duration::from_std(d).unwrap_or_default())
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                });
                publish(&status, &|s| {
                    s.last_scan_at = Some(now_iso());
                    s.next_scan_at = next_iso.clone();
                });
            }
            Ok(Msg::Fs(Ok(ev))) => {
                if ev.need_rescan() {
                    let _ = tx.send(Msg::Rescan);
                }
                let relevant = matches!(
                    ev.kind,
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
                );
                use notify::event::{ModifyKind, RenameMode};
                // A file renamed *away* (e.g. moved into a category folder) is
                // not a new arrival; for a two-path rename only the target is.
                let paths: &[PathBuf] = match ev.kind {
                    EventKind::Modify(ModifyKind::Name(RenameMode::From)) => &[],
                    EventKind::Modify(ModifyKind::Name(RenameMode::Both))
                        if ev.paths.len() == 2 =>
                    {
                        &ev.paths[1..]
                    }
                    _ => &ev.paths,
                };
                if relevant {
                    for p in paths {
                        let in_root = p
                            .parent()
                            .map(|d| compare_key(d) == root_key)
                            .unwrap_or(false);
                        if let (true, Some(name)) = (in_root, p.file_name()) {
                            // Re-arm the settle timer on every event for this file.
                            nominate(
                                &mut pending,
                                name.to_string_lossy().into_owned(),
                                Duration::from_secs(config.settle_secs),
                            );
                        }
                    }
                    publish(&status, &|s| s.last_event_at = Some(now_iso()));
                }
            }
            Ok(Msg::Fs(Err(e))) => {
                // Overflow or watch failure: rebuild the watch and rescan.
                log::warn!("monitor: watch error: {e}");
                watcher = None;
                next_health = Instant::now() + Duration::from_secs(2);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }

        let now = Instant::now();
        // A long gap between ticks means the machine slept: rescan.
        if now.duration_since(last_tick) > Duration::from_secs(30) {
            log::info!("monitor: resumed after a pause in execution; rescanning");
            let _ = tx.send(Msg::Rescan);
        }
        last_tick = now;
        if next_scan.is_some_and(|n| now >= n) {
            let _ = tx.send(Msg::Rescan);
            next_scan = None;
        }
        if watcher.is_some() && now >= next_health {
            next_health = now + Duration::from_secs(30);
            let healthy = query_entry(&root)
                .map(|i| i.is_dir && !i.is_unsafe_link)
                .unwrap_or(false);
            if !healthy {
                watcher = None;
                publish(&status, &|s| {
                    s.state = MonitorState::Error;
                    s.last_error = Some("The authorized folder is not available.".into());
                });
            }
        }

        if paused || watcher.is_none() {
            let n = pending.len();
            publish(&status, &|s| s.pending = n);
            continue;
        }

        let ready = check_pending(&root, &mut pending, &mut reported, &config, now);
        let n = pending.len();
        if status.lock().pending != n {
            publish(&status, &|s| s.pending = n);
        }
        if !ready.is_empty() {
            on_ready(ready);
        }
    }
    publish(&status, &|s| {
        s.state = MonitorState::Off;
        s.pending = 0;
    });
}

fn nominate(pending: &mut HashMap<String, Pending>, name: String, delay: Duration) {
    if is_temp_download(&name) || crate::fsops::is_staging_name(&name) {
        return;
    }
    let now = Instant::now();
    pending
        .entry(name)
        .and_modify(|p| p.next_check = p.next_check.max(now + delay))
        .or_insert(Pending {
            first_seen: now,
            next_check: now + delay,
            size: u64::MAX,
            modified_ms: 0,
            attempts: 0,
        });
}

/// Returns names that are ready; reschedules the rest with backoff.
fn check_pending(
    root: &std::path::Path,
    pending: &mut HashMap<String, Pending>,
    reported: &mut HashMap<String, (u64, i64)>,
    config: &MonitorConfig,
    now: Instant,
) -> Vec<String> {
    let mut ready = Vec::new();
    let mut drop_names = Vec::new();
    let now_wall = now_ms();
    for (name, p) in pending.iter_mut() {
        if now < p.next_check {
            continue;
        }
        let path = root.join(name);
        let Ok(info) = query_entry(&path) else {
            drop_names.push(name.clone());
            continue;
        };
        if info.is_dir || info.is_unsafe_link || info.hidden_or_system {
            drop_names.push(name.clone());
            continue;
        }
        p.attempts += 1;
        let backoff = Duration::from_secs(
            (config.settle_secs).max(5) * (1 + (p.attempts as u64 / 10)).min(6),
        );
        let changed = info.size != p.size || info.modified_ms != p.modified_ms;
        p.size = info.size;
        p.modified_ms = info.modified_ms;
        let quiet_ms = now_wall - info.modified_ms;
        let settle_ms = if info.size == 0 {
            config.settle_secs.max(60) * 1000
        } else {
            config.settle_secs * 1000
        } as i64;
        if changed && p.attempts > 1 || quiet_ms < settle_ms {
            p.next_check = now + Duration::from_millis((settle_ms - quiet_ms).max(1000) as u64);
        } else if has_partial_sibling(root, name) || is_locked(&path) {
            p.next_check = now + backoff;
        } else {
            let stamp = (info.size, info.modified_ms);
            if reported.get(name) != Some(&stamp) {
                reported.insert(name.clone(), stamp);
                ready.push(name.clone());
            }
            drop_names.push(name.clone());
            continue;
        }
        if p.attempts >= MAX_ATTEMPTS {
            log::info!(
                "monitor: giving up on a file after {} checks ({}s)",
                p.attempts,
                p.first_seen.elapsed().as_secs()
            );
            drop_names.push(name.clone());
        }
    }
    for n in drop_names {
        pending.remove(&n);
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounces_and_ignores_temp_files() {
        let mut pending = HashMap::new();
        nominate(&mut pending, "a.pdf".into(), Duration::from_secs(15));
        nominate(&mut pending, "a.pdf".into(), Duration::from_secs(15));
        nominate(&mut pending, "b.crdownload".into(), Duration::from_secs(15));
        nominate(
            &mut pending,
            ".tucklane-staging-x.tmp".into(),
            Duration::from_secs(15),
        );
        assert_eq!(pending.len(), 1);
    }
}
