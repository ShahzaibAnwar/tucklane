//! Filesystem acceptance tests. Each test runs in a throwaway sandbox under
//! `target/test-sandbox`; nothing touches a real Downloads folder.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use crate::database::Db;
use crate::error::AppError;
use crate::fsops::{move_within_root, Expectation, MoveMethod, MoveRequest, OnCollision};
use crate::history::{self, UndoConflict, UndoOptions, UndoStatus};
use crate::models::{now_ms, OpKind, OpState, SessionKind};
use crate::organizer::{
    build_plan, exec_items_from_plan, execute, Plan, PlanInputs, PlanMode, PlanStatus,
};
use crate::security::{AuthorizedRootGuard, NameRule, RootPolicy};
use crate::settings::{CollisionPolicy, Settings};
use crate::testutil::{age, Fixture};

fn plan_mode(fx: &Fixture, settings: &Settings, mode: PlanMode) -> Plan {
    let mut excluded = HashSet::new();
    let mut restored = HashSet::new();
    for e in fx.db.lock().list_exclusions(fx.guard.id()).unwrap() {
        if e.reason == "user" {
            excluded.insert(e.rel_path.to_uppercase());
        } else {
            restored.insert(e.rel_path.to_uppercase());
        }
    }
    let rules = fx.db.lock().list_rules().unwrap();
    build_plan(
        &fx.guard,
        &PlanInputs {
            settings,
            rules: &rules,
            excluded: &excluded,
            restored: &restored,
            now_ms: now_ms(),
            mode,
            only: None,
        },
    )
    .unwrap()
}

fn plan_with(fx: &Fixture, settings: &Settings) -> Plan {
    plan_mode(fx, settings, PlanMode::Manual)
}

fn plan(fx: &Fixture) -> Plan {
    plan_with(fx, &Settings::default())
}

fn organize_all(fx: &Fixture, p: &Plan) -> crate::organizer::ExecReport {
    let items = exec_items_from_plan(
        p.items.iter().filter(|i| i.selected),
        CollisionPolicy::Rename,
    );
    execute(
        &fx.guard,
        &fx.db,
        SessionKind::Manual,
        items,
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap()
}

fn item<'a>(p: &'a Plan, name: &str) -> &'a crate::organizer::PlanItem {
    p.items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("{name} not in plan"))
}

fn mklink_junction(link: &std::path::Path, target: &std::path::Path) -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ------------------------------------------------------------ basic organization

#[test]
fn organizes_common_types_and_leaves_folders_alone() {
    let fx = Fixture::new();
    fx.file("report.pdf", b"%PDF-1.4");
    fx.file("invoice-a.pdf", b"a");
    fx.file("invoice-b.pdf", b"b");
    fx.file("photo.jpg", b"jpg");
    fx.file("movie.mp4", b"mp4");
    fx.file("setup.exe", b"MZ");
    fx.file("mystery.qqq", b"?");
    fx.file("My Project/notes.txt", b"nested");
    let p = plan(&fx);
    assert_eq!(
        item(&p, "report.pdf").dest_dir.as_deref(),
        Some("Documents/PDF")
    );
    assert_eq!(item(&p, "mystery.qqq").status, PlanStatus::Review);
    assert_eq!(p.summary.folders_left, 1);
    let rep = organize_all(&fx, &p);
    assert_eq!(rep.session.completed, 6);
    for rel in [
        "Documents/PDF/report.pdf",
        "Documents/PDF/invoice-a.pdf",
        "Documents/PDF/invoice-b.pdf",
        "Media/Images/photo.jpg",
        "Media/Videos/movie.mp4",
        "Software/Installers/setup.exe",
    ] {
        assert!(fx.root.join(rel).exists(), "{rel} should exist");
    }
    assert!(
        fx.root.join("mystery.qqq").exists(),
        "unknown files stay put by default"
    );
    assert!(
        fx.root.join("My Project/notes.txt").exists(),
        "user folders are not reorganized"
    );
}

#[test]
fn unknown_files_go_to_other_only_when_chosen() {
    let fx = Fixture::new();
    fx.file("mystery.qqq", b"?");
    fx.file("README", b"no ext");
    let s = Settings {
        unknown_files: crate::settings::UnknownFiles::MoveToOther,
        ..Settings::default()
    };
    let p = plan_with(&fx, &s);
    assert_eq!(item(&p, "mystery.qqq").dest_dir.as_deref(), Some("Other"));
    assert_eq!(item(&p, "README").dest_dir.as_deref(), Some("Other"));
}

// --------------------------------------------------------------- filename handling

#[test]
fn identical_and_case_insensitive_names_never_overwrite() {
    let fx = Fixture::new();
    fx.file("Documents/PDF/invoice.pdf", b"existing");
    fx.file("invoice.pdf", b"new one");
    fx.file("Documents/PDF/Photo.PDF", b"existing2");
    fx.file("photo.pdf", b"new two");
    let p = plan(&fx);
    assert_eq!(item(&p, "invoice.pdf").status, PlanStatus::Renamed);
    assert_eq!(
        item(&p, "invoice.pdf").dest_name.as_deref(),
        Some("invoice (1).pdf")
    );
    assert_eq!(
        item(&p, "photo.pdf").dest_name.as_deref(),
        Some("photo (1).pdf")
    );
    organize_all(&fx, &p);
    assert_eq!(
        std::fs::read(fx.root.join("Documents/PDF/invoice.pdf")).unwrap(),
        b"existing"
    );
    assert_eq!(
        std::fs::read(fx.root.join("Documents/PDF/invoice (1).pdf")).unwrap(),
        b"new one"
    );
    assert_eq!(
        std::fs::read(fx.root.join("Documents/PDF/Photo.PDF")).unwrap(),
        b"existing2"
    );
}

#[test]
fn collision_skip_and_ask_policies() {
    let fx = Fixture::new();
    fx.file("Documents/PDF/a.pdf", b"old");
    fx.file("a.pdf", b"new");
    let skip = plan_with(
        &fx,
        &Settings {
            collision: CollisionPolicy::Skip,
            ..Settings::default()
        },
    );
    assert_eq!(item(&skip, "a.pdf").status, PlanStatus::Skipped);
    let ask = plan_with(
        &fx,
        &Settings {
            collision: CollisionPolicy::Ask,
            ..Settings::default()
        },
    );
    let a = item(&ask, "a.pdf");
    assert_eq!(a.status, PlanStatus::NeedsDecision);
    assert!(
        !a.selected,
        "conflicts awaiting a decision are not preselected"
    );
}

#[test]
fn unicode_urdu_long_multi_period_and_extensionless_names() {
    let fx = Fixture::new();
    let urdu = "رپورٹ سالانہ.pdf";
    let long_name = format!("{}.final.v2.pdf", "ل".repeat(120));
    fx.file(urdu, b"u");
    fx.file("résumé – 2026 ✓.docx", b"r");
    fx.file("archive.backup.tar.gz", b"t");
    fx.file(&long_name, b"l");
    fx.file("NOEXT", b"n");
    // Deep folder so the full destination path exceeds MAX_PATH (260).
    let deep = format!("{}/{}", "d".repeat(100), "e".repeat(100));
    let s = Settings::default();
    let mut rules = vec![];
    let draft = crate::rules::RuleDraft {
        name: "Deep".into(),
        description: String::new(),
        enabled: true,
        match_mode: crate::rules::MatchMode::All,
        conditions: vec![crate::rules::Condition::NameStartsWith { value: "ل".into() }],
        action: crate::rules::RuleAction::MoveTo {
            folder: deep.clone(),
            confirm: false,
        },
    }
    .validate()
    .unwrap();
    rules.push(fx.db.lock().insert_rule(&draft).unwrap());
    let p = plan_with(&fx, &s);
    assert_eq!(
        item(&p, "archive.backup.tar.gz").dest_dir.as_deref(),
        Some("Archives")
    );
    assert_eq!(item(&p, "NOEXT").status, PlanStatus::Review);
    let rep = organize_all(&fx, &p);
    assert_eq!(rep.session.failed, 0, "{:?}", rep.errors);
    assert!(fx.guard.root().join("Documents/PDF").join(urdu).exists());
    assert!(fx
        .guard
        .root()
        .join("Documents/Word/résumé – 2026 ✓.docx")
        .exists());
    let dest = fx
        .guard
        .root()
        .join(deep.replace('/', "\\"))
        .join(&long_name);
    assert!(dest.to_string_lossy().len() > 260);
    assert!(dest.exists(), "long path destination should work");
}

#[test]
fn reserved_device_names_are_handled_safely() {
    let fx = Fixture::new();
    // Created through the \\?\ namespace, as some tools do.
    let con = fx.guard.root().join("CON.txt");
    std::fs::write(&con, b"device-named").unwrap();
    age(&con);
    let p = plan(&fx);
    let it = item(&p, "CON.txt");
    assert_eq!(it.dest_dir.as_deref(), Some("Documents"));
    organize_all(&fx, &p);
    assert!(fx.guard.root().join("Documents").join("CON.txt").exists());
    // Tucklane never *creates* reserved names for folders.
    assert!(crate::rules::validate_folder("Stuff/NUL").is_err());
}

// ------------------------------------------------------------------ download safety

#[test]
fn incomplete_downloads_are_not_moved() {
    let fx = Fixture::new();
    fx.file("big.zip.crdownload", b"partial");
    fx.file("Unconfirmed 123.crdownload", b"partial");
    fx.file("video.mp4", b"");
    fx.file("video.mp4.part", b"firefox partial");
    std::fs::write(fx.root.join("fresh.pdf"), b"just written").unwrap(); // mtime = now
    let p = plan(&fx);
    assert_eq!(item(&p, "video.mp4").status, PlanStatus::Pending);
    assert_eq!(item(&p, "fresh.pdf").status, PlanStatus::Pending);
    for n in ["big.zip.crdownload", "Unconfirmed 123.crdownload"] {
        assert_eq!(item(&p, n).status, PlanStatus::Pending, "{n}");
    }
    assert!(p.items.iter().all(|i| !i.selected));
}

#[test]
fn locked_file_is_reported_and_skipped() {
    use std::os::windows::fs::OpenOptionsExt;
    let fx = Fixture::new();
    let p_locked = fx.file("locked.pdf", b"busy");
    let _hold = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&p_locked)
        .unwrap();
    let p = plan(&fx);
    assert_eq!(item(&p, "locked.pdf").status, PlanStatus::Locked);
    // Even if forced through, execution skips it without damage.
    let it = item(&p, "locked.pdf");
    let exec = crate::organizer::ExecItem {
        source_rel: it.source_rel.clone(),
        name: it.name.clone(),
        size: it.size,
        modified_ms: it.modified_ms,
        dest_dir: "Documents/PDF".into(),
        dest_name: it.name.clone(),
        on_collision: OnCollision::Rename,
        rule_id: None,
        rule_name: None,
        reason: None,
    };
    let rep = execute(
        &fx.guard,
        &fx.db,
        SessionKind::Manual,
        vec![exec],
        &AtomicBool::new(false),
        &|_| {},
    )
    .unwrap();
    assert_eq!(rep.session.skipped, 1);
    assert!(p_locked.exists());
}

#[test]
fn changed_deleted_or_renamed_between_scan_and_move() {
    let fx = Fixture::new();
    let grow = fx.file("grow.pdf", b"v1");
    let gone = fx.file("gone.pdf", b"x");
    let ren = fx.file("renamed.pdf", b"y");
    let p = plan(&fx);
    std::fs::write(&grow, b"v2 is longer").unwrap();
    age(&grow);
    std::fs::remove_file(&gone).unwrap();
    std::fs::rename(&ren, fx.root.join("renamed-later.pdf")).unwrap();
    let rep = organize_all(&fx, &p);
    assert_eq!(rep.session.completed, 0);
    assert_eq!(rep.session.skipped, 3);
    assert!(grow.exists());
    assert!(fx.root.join("renamed-later.pdf").exists());
    let ops = fx.db.lock().ops_for_session(&rep.session.id).unwrap();
    assert!(ops
        .iter()
        .all(|o| o.state == OpState::Skipped && o.error.is_some()));
}

// ------------------------------------------------------------------ folder security

#[test]
fn rejects_paths_outside_root() {
    let fx = Fixture::new();
    std::fs::write(fx.outside.join("secret.txt"), b"desktop").unwrap();
    for bad in [
        "../Desktop/secret.txt",
        "..\\Desktop\\secret.txt",
        "Documents/../../Desktop/secret.txt",
    ] {
        assert!(
            matches!(
                fx.guard.resolve_existing(bad),
                Err(AppError::Unauthorized(_))
            ),
            "{bad}"
        );
    }
    let abs = fx.outside.join("secret.txt").to_string_lossy().into_owned();
    assert!(fx.guard.resolve_existing(&abs).is_err());
    // Moving a file *to* outside the root is impossible through the API.
    fx.file("a.pdf", b"a");
    let req = MoveRequest {
        source_rel: "a.pdf",
        dest_dir_rel: "../Desktop",
        desired_name: "a.pdf",
        on_collision: OnCollision::Fail,
        expect: Expectation::default(),
        force_copy: false,
    };
    assert!(move_within_root(&fx.guard, &req).is_err());
    let req2 = MoveRequest {
        dest_dir_rel: "Docs",
        desired_name: "..\\..\\Desktop\\a.pdf",
        ..req.clone()
    };
    assert!(move_within_root(&fx.guard, &req2).is_err());
    assert!(fx.root.join("a.pdf").exists());
    assert!(!fx.outside.join("a.pdf").exists());
    // Moving or touching the root itself is impossible.
    assert!(fx.guard.resolve_existing("").is_err());
}

#[test]
fn junction_escape_is_blocked() {
    let fx = Fixture::new();
    // A junction named like a category folder, pointing at the Desktop.
    if !mklink_junction(&fx.root.join("Documents"), &fx.outside) {
        eprintln!("mklink /J unavailable; skipping");
        return;
    }
    fx.file("report.pdf", b"r");
    let p = plan(&fx);
    let it = item(&p, "report.pdf");
    assert_eq!(it.status, PlanStatus::Skipped, "{:?}", it.note);
    let rep = organize_all(&fx, &p);
    assert_eq!(rep.session.total, 0);
    let req = MoveRequest {
        source_rel: "report.pdf",
        dest_dir_rel: "Documents/PDF",
        desired_name: "report.pdf",
        on_collision: OnCollision::Fail,
        expect: Expectation::default(),
        force_copy: false,
    };
    assert!(matches!(
        move_within_root(&fx.guard, &req),
        Err(AppError::Unauthorized(_))
    ));
    assert!(
        std::fs::read_dir(&fx.outside).unwrap().next().is_none(),
        "nothing may land outside"
    );
    // Listing or resolving through the junction is refused too.
    assert!(fx.guard.resolve_existing("Documents/anything.txt").is_err());
    // The junction itself is never treated as a movable file.
    assert_eq!(p.summary.links_skipped, 1);
}

#[test]
fn symlink_to_outside_is_not_followed() {
    let fx = Fixture::new();
    let target = fx.outside.join("secret.pdf");
    std::fs::write(&target, b"s").unwrap();
    let link = fx.root.join("innocent.pdf");
    if std::os::windows::fs::symlink_file(&target, &link).is_err() {
        eprintln!("symlink creation needs Developer Mode; skipping");
        return;
    }
    let p = plan(&fx);
    assert!(p.items.iter().all(|i| i.name != "innocent.pdf"));
    assert_eq!(p.summary.links_skipped, 1);
    assert!(matches!(
        fx.guard.resolve_existing("innocent.pdf"),
        Err(AppError::Unauthorized(_))
    ));
}

#[test]
fn junction_as_root_and_dangerous_roots_rejected() {
    let fx = Fixture::new();
    let link = fx.dir.path().join("LinkRoot");
    if mklink_junction(&link, &fx.root) {
        assert!(AuthorizedRootGuard::open(&link, "x", &RootPolicy::system(&[])).is_err());
    }
    let protected = fx.dir.path().join("AppData");
    std::fs::create_dir_all(protected.join("inner")).unwrap();
    let pol = RootPolicy::system(&[]).protect(protected.clone());
    assert!(
        pol.validate(&protected.join("inner")).is_err(),
        "inside app data"
    );
    assert!(pol.validate(fx.dir.path()).is_err(), "ancestor of app data");
}

#[test]
fn root_replaced_during_operation_aborts() {
    let fx = Fixture::new();
    fx.file("a.pdf", b"a");
    fx.file("b.pdf", b"b");
    let p = plan(&fx);
    // Swap the root for a different folder with the same name.
    std::fs::rename(&fx.root, fx.dir.path().join("Downloads-old")).unwrap();
    std::fs::create_dir_all(&fx.root).unwrap();
    std::fs::write(fx.root.join("a.pdf"), b"a").unwrap();
    let items = exec_items_from_plan(
        p.items.iter().filter(|i| i.selected),
        CollisionPolicy::Rename,
    );
    let r = execute(
        &fx.guard,
        &fx.db,
        SessionKind::Manual,
        items,
        &AtomicBool::new(false),
        &|_| {},
    );
    assert!(matches!(r, Err(AppError::InvalidRoot(_))));
    assert!(fx.root.join("a.pdf").exists());
}

// ----------------------------------------------------------------- undo & recovery

#[test]
fn undo_restores_and_handles_conflicts_and_changes() {
    let fx = Fixture::new();
    fx.file("a.pdf", b"a");
    fx.file("b.pdf", b"b");
    fx.file("c.pdf", b"c");
    fx.file("d.pdf", b"d");
    let rep = organize_all(&fx, &plan(&fx));
    assert_eq!(rep.session.completed, 4);
    let ops = fx.db.lock().ops_for_session(&rep.session.id).unwrap();
    let op = |n: &str| {
        ops.iter()
            .find(|o| o.original_name == n)
            .unwrap()
            .id
            .clone()
    };

    // Plain undo.
    let r = history::undo_operations(&fx.guard, &fx.db, &[op("a.pdf")], &UndoOptions::default())
        .unwrap();
    assert_eq!(r.results[0].status, UndoStatus::Restored);
    assert!(fx.root.join("a.pdf").exists());
    // The restored file is excluded from automatic re-organization…
    let auto = plan_mode(&fx, &Settings::default(), PlanMode::Auto);
    assert_eq!(item(&auto, "a.pdf").status, PlanStatus::Ignored);
    // …but can still be chosen manually (unselected by default).
    let manual = plan(&fx);
    assert_eq!(item(&manual, "a.pdf").status, PlanStatus::Confirm);
    assert!(!item(&manual, "a.pdf").selected);

    // Conflict: something new occupies the original name.
    fx.file("b.pdf", b"a different b");
    let r = history::undo_operations(&fx.guard, &fx.db, &[op("b.pdf")], &UndoOptions::default())
        .unwrap();
    assert_eq!(r.results[0].status, UndoStatus::Conflict);
    assert_eq!(
        std::fs::read(fx.root.join("b.pdf")).unwrap(),
        b"a different b"
    );
    let r = history::undo_operations(
        &fx.guard,
        &fx.db,
        &[op("b.pdf")],
        &UndoOptions {
            on_conflict: UndoConflict::Rename,
            allow_modified: false,
        },
    )
    .unwrap();
    assert_eq!(r.results[0].status, UndoStatus::Restored);
    assert_eq!(std::fs::read(fx.root.join("b (1).pdf")).unwrap(), b"b");

    // Externally modified after the move.
    let moved_c = fx.root.join("Documents/PDF/c.pdf");
    std::fs::write(&moved_c, b"edited by user").unwrap();
    let r = history::undo_operations(&fx.guard, &fx.db, &[op("c.pdf")], &UndoOptions::default())
        .unwrap();
    assert_eq!(r.results[0].status, UndoStatus::Modified);
    assert!(moved_c.exists());

    // Missing destination.
    std::fs::remove_file(fx.root.join("Documents/PDF/d.pdf")).unwrap();
    let r = history::undo_operations(&fx.guard, &fx.db, &[op("d.pdf")], &UndoOptions::default())
        .unwrap();
    assert_eq!(r.results[0].status, UndoStatus::Missing);

    // Undo is itself recorded.
    let undo_ops: Vec<_> = fx
        .db
        .lock()
        .query_ops(
            fx.guard.id(),
            &crate::models::HistoryQuery {
                kind: Some("undo".into()),
                ..Default::default()
            },
        )
        .unwrap()
        .0;
    assert!(undo_ops.iter().any(|o| o.state == OpState::Completed));
    assert_eq!(
        fx.db.lock().get_op(&op("a.pdf")).unwrap().state,
        OpState::Undone
    );
}

#[test]
fn undo_whole_session() {
    let fx = Fixture::new();
    for n in ["1.png", "2.mp3", "3.zip"] {
        fx.file(n, n.as_bytes());
    }
    let rep = organize_all(&fx, &plan(&fx));
    let r =
        history::undo_session(&fx.guard, &fx.db, &rep.session.id, &UndoOptions::default()).unwrap();
    assert!(r.results.iter().all(|r| r.status == UndoStatus::Restored));
    for n in ["1.png", "2.mp3", "3.zip"] {
        assert!(fx.root.join(n).exists());
    }
    let s = fx.db.lock().get_session(&rep.session.id).unwrap();
    assert!(s.undone_at.is_some());
}

#[test]
fn copy_verify_fallback_moves_safely() {
    let fx = Fixture::new();
    let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    fx.file("big.bin", &data);
    let (_, info) = fx.guard.resolve_existing("big.bin").unwrap();
    let out = move_within_root(
        &fx.guard,
        &MoveRequest {
            source_rel: "big.bin",
            dest_dir_rel: "Other",
            desired_name: "big.bin",
            on_collision: OnCollision::Fail,
            expect: Expectation {
                size: Some(info.size),
                modified_ms: None,
                file_id: Some(info.file_id),
            },
            force_copy: true,
        },
    )
    .unwrap();
    assert_eq!(out.method, MoveMethod::CopyVerify);
    assert!(out.sha256.is_some());
    assert!(!fx.root.join("big.bin").exists());
    assert_eq!(std::fs::read(fx.root.join("Other/big.bin")).unwrap(), data);
    let leftovers: Vec<_> = std::fs::read_dir(fx.root.join("Other"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(leftovers.len(), 1, "no staging files left behind");
}

#[test]
fn never_overwrites_even_with_fail_policy() {
    let fx = Fixture::new();
    fx.file("Archives/x.zip", b"keep me");
    fx.file("x.zip", b"incoming");
    let r = move_within_root(
        &fx.guard,
        &MoveRequest {
            source_rel: "x.zip",
            dest_dir_rel: "Archives",
            desired_name: "x.zip",
            on_collision: OnCollision::Fail,
            expect: Expectation::default(),
            force_copy: false,
        },
    );
    assert!(matches!(r, Err(AppError::Conflict(_))));
    assert_eq!(
        std::fs::read(fx.root.join("Archives/x.zip")).unwrap(),
        b"keep me"
    );
    assert!(fx.root.join("x.zip").exists());
}

#[test]
fn crash_recovery_reconciles_with_disk() {
    let fx = Fixture::new();
    let db = &fx.db;
    fx.file("untouched.pdf", b"u");
    fx.file("Documents/PDF/moved.pdf", b"m");
    let sid = db
        .lock()
        .create_session(fx.guard.id(), SessionKind::Manual, 4)
        .unwrap();
    let mk = |src: &str, name: &str| {
        db.lock()
            .insert_planned_op(
                &sid,
                fx.guard.id(),
                OpKind::Move,
                src,
                "Documents/PDF",
                name,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap()
    };
    // 1. Crashed before the rename: the file is still at the source.
    let op1 = mk("untouched.pdf", "untouched.pdf");
    let (_, i1) = fx.guard.resolve_existing("untouched.pdf").unwrap();
    db.lock()
        .mark_op_in_progress(&op1, &i1.file_id, i1.size as i64, i1.modified_ms)
        .unwrap();
    // A partial staging copy from an interrupted copy-verify.
    std::fs::write(
        fx.root.join("Documents/PDF/.tucklane-staging-abc.tmp"),
        b"part",
    )
    .unwrap();
    // 2. Crashed after the rename but before recording success.
    let op2 = mk("moved.pdf", "moved.pdf");
    let (_, i2) = fx
        .guard
        .resolve_existing("Documents/PDF/moved.pdf")
        .unwrap();
    db.lock()
        .mark_op_in_progress(&op2, &i2.file_id, i2.size as i64, i2.modified_ms)
        .unwrap();
    // 3. File vanished entirely.
    let op3 = mk("ghost.pdf", "ghost.pdf");
    db.lock()
        .mark_op_in_progress(&op3, "ffffffff-0000000000000000", 1, 0)
        .unwrap();
    // 4. Never started.
    let op4 = mk("later.pdf", "later.pdf");

    let rep = history::recover(Some(&fx.guard), db).unwrap();
    let d = db.lock();
    assert_eq!(d.get_op(&op1).unwrap().state, OpState::Failed);
    let o2 = d.get_op(&op2).unwrap();
    assert_eq!(o2.state, OpState::Completed);
    assert_eq!(o2.dest_rel.as_deref(), Some("Documents/PDF/moved.pdf"));
    assert_eq!(d.get_op(&op3).unwrap().state, OpState::RecoveryRequired);
    assert_eq!(d.get_op(&op4).unwrap().state, OpState::Skipped);
    assert_eq!(d.get_session(&sid).unwrap().status, "interrupted");
    assert_eq!(rep.staging_removed, 1);
    assert!(fx.root.join("untouched.pdf").exists());
}

#[test]
fn cancellation_stops_after_current_file() {
    let fx = Fixture::new();
    for i in 0..5 {
        fx.file(&format!("f{i}.pdf"), b"x");
    }
    let p = plan(&fx);
    let cancel = Arc::new(AtomicBool::new(false));
    let c2 = cancel.clone();
    let items = exec_items_from_plan(
        p.items.iter().filter(|i| i.selected),
        CollisionPolicy::Rename,
    );
    let rep = execute(
        &fx.guard,
        &fx.db,
        SessionKind::Manual,
        items,
        &cancel,
        &move |pr| {
            if pr.completed == 2 {
                c2.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        },
    )
    .unwrap();
    assert!(rep.cancelled);
    assert_eq!(rep.session.completed, 2);
    assert_eq!(rep.session.skipped, 3);
    assert_eq!(rep.session.status, "cancelled");
}

#[test]
fn database_survives_restart_with_history() {
    let dir = crate::testutil::sandbox();
    let path = dir.path().join("t.db");
    {
        let db = Db::open(&path).unwrap().db;
        let sid = db.create_session("r", SessionKind::Manual, 0).unwrap();
        db.finish_session(&sid, "completed").unwrap();
    }
    let db = Db::open(&path).unwrap().db;
    assert_eq!(db.list_sessions("r", 10, 0).unwrap().len(), 1);
}

// --------------------------------------------------------------------- duplicates

#[test]
fn duplicate_finder_verifies_content_and_keeps_a_copy() {
    let fx = Fixture::new();
    fx.file("a.bin", b"same content here");
    fx.file("sub/b.bin", b"same content here");
    fx.file("c.bin", b"same content HERE"); // same size, different bytes
    fx.file("same-name.txt", b"one");
    fx.file("other/same-name.txt", b"two");
    let groups =
        crate::duplicates::find_duplicates(&fx.guard, &AtomicBool::new(false), &|_| {}).unwrap();
    assert_eq!(groups.len(), 1);
    let rels: HashSet<String> = groups[0].members.iter().map(|m| m.rel.clone()).collect();
    assert_eq!(
        rels,
        HashSet::from(["a.bin".to_string(), "sub/b.bin".to_string()])
    );
    fx.db
        .lock()
        .replace_duplicate_groups(fx.guard.id(), &groups)
        .unwrap();

    // Selecting every copy is refused: one must remain.
    let all: Vec<String> = rels.iter().cloned().collect();
    let r = crate::duplicates::recycle_duplicates(&fx.guard, &fx.db, &all).unwrap();
    assert!(r.iter().all(|x| !x.ok));
    assert!(fx.root.join("a.bin").exists() && fx.root.join("sub/b.bin").exists());

    // Recycling one copy goes to the Recycle Bin (then we purge our test item).
    let r = crate::duplicates::recycle_duplicates(&fx.guard, &fx.db, &["sub/b.bin".to_string()])
        .unwrap();
    assert!(r[0].ok, "{}", r[0].message);
    assert!(!fx.root.join("sub/b.bin").exists());
    assert!(fx.root.join("a.bin").exists());
    let ours: Vec<_> = trash::os_limited::list()
        .unwrap()
        .into_iter()
        .filter(|i| {
            i.original_parent.starts_with(fx.root.join("sub"))
                || i.original_parent
                    .to_string_lossy()
                    .contains(&*fx.dir.path().file_name().unwrap().to_string_lossy())
        })
        .collect();
    assert!(!ours.is_empty(), "item should be in the Recycle Bin");
    trash::os_limited::purge_all(ours).unwrap();
}

// ------------------------------------------------------------------------ watcher

#[test]
fn watcher_picks_up_completed_downloads_only() {
    use crate::watcher::{Monitor, MonitorConfig};
    let fx = Fixture::new();
    fx.file("already-here.pdf", b"old");
    let ready = Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
    let r2 = ready.clone();
    let m = Monitor::start(
        fx.guard.root().to_path_buf(),
        MonitorConfig {
            settle_secs: 3,
            scan_interval_mins: 0,
        },
        false,
        Arc::new(move |names| r2.lock().extend(names)),
        Arc::new(|_| {}),
    );
    std::thread::sleep(Duration::from_millis(500));
    // A browser download: temp name, then renamed when complete.
    std::fs::write(fx.root.join("new.zip.crdownload"), b"partial").unwrap();
    for i in 0..5 {
        std::fs::write(fx.root.join(format!("burst-{i}.png")), b"x").unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    std::fs::rename(fx.root.join("new.zip.crdownload"), fx.root.join("new.zip")).unwrap();
    // Pause: nothing is reported while paused.
    m.set_paused(true);
    std::thread::sleep(Duration::from_secs(5));
    let while_paused = ready.lock().len();
    m.set_paused(false);
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let got: HashSet<String> = ready.lock().iter().cloned().collect();
        if got.len() >= 7 || std::time::Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    m.stop();
    let got = ready.lock().clone();
    let set: HashSet<String> = got.iter().cloned().collect();
    assert!(
        set.contains("already-here.pdf"),
        "startup rescan finds existing files: {set:?}"
    );
    assert!(set.contains("new.zip"), "{set:?}");
    assert!(!set.iter().any(|n| n.ends_with(".crdownload")));
    for i in 0..5 {
        assert!(set.contains(&format!("burst-{i}.png")));
    }
    assert_eq!(got.len(), set.len(), "each file reported once");
    assert!(
        while_paused <= 1,
        "only the startup rescan may report before pausing"
    );
}

#[test]
fn guard_join_validates_new_names() {
    let fx = Fixture::new();
    assert!(fx.guard.join_rel("ok/name.pdf", NameRule::Create).is_ok());
    assert!(fx.guard.join_rel("bad./x", NameRule::Create).is_err());
    assert!(fx.guard.join_rel("a/CON", NameRule::Create).is_err());
}
