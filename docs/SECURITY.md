# Security model

Tucklane moves files on a user's disk, so its security model is about one
promise: **it only ever touches files inside the single folder the user
authorized, and it never destroys data.**

This document describes how that promise is enforced and how to audit it.
For reporting vulnerabilities, see [`/SECURITY.md`](../SECURITY.md).

## Trust boundaries

| Component | Trusted? | Notes |
|---|---|---|
| Rust backend (`src-tauri/src`) | Yes | Enforces every rule below. |
| Web frontend (`src/`) | **No** | Treated as potentially compromised. It can only call typed commands. |
| File names, file contents | No | Untrusted data. Never executed or interpreted. |
| Rule import files | No | Validated exactly like rules created in the editor. |

The frontend is granted only `core:default` and `dialog:allow-open`
(`src-tauri/capabilities/default.json`). There is **no** `fs`, `shell` or
`opener` permission for the webview. Opening files and folders happens in Rust,
after authorization checks.

## The authorized root

* Chosen by the user during onboarding (default: the Windows *Downloads*
  known folder, resolved with `SHGetKnownFolderPath` so redirected folders work).
* Stored as a canonical path (`GetFinalPathNameByHandleW`) plus the directory's
  volume serial and file index, so a folder swapped for another one with the same
  name is detected (`AuthorizedRootGuard::verify_root`).
* Exactly one active root (enforced by a unique partial index in SQLite).

### Rejected roots — `security/policy.rs`

* relative paths, UNC/network paths, mapped network drives (`GetDriveTypeW`)
* drive roots (`C:\`)
* links/junctions (choose the real target instead)
* anything inside `%SystemRoot%`, `Program Files`, `Program Files (x86)`,
  `ProgramData`, `%APPDATA%`, `%LOCALAPPDATA%`, `LocalLow`, `$Recycle.Bin`,
  `System Volume Information`, `Recovery`, Tucklane's own data/log/install dirs
* any folder that *contains* one of the above or the user profile (e.g.
  `C:\Users`, `C:\Users\<name>`)

## `AuthorizedRootGuard` — `security/mod.rs`

All filesystem access on behalf of the user goes through the guard:

* **Relative paths only.** The frontend sends `/`-separated paths relative to
  the root. `split_rel` rejects absolute paths, drive letters, `:` (which also
  blocks NTFS alternate data streams and `\\?\` prefixes), `..`, `.`, empty
  components, control characters and Windows-invalid characters.
* **Names Tucklane creates** (destination folders) additionally may not be DOS
  device names (`CON`, `NUL`, `COM1`…), or start/end with spaces or periods.
* **No link traversal.** `resolve_existing` checks *every* component between
  the root and the target. A symbolic link, junction, mount point or any other
  reparse point that is not a known storage implementation detail (OneDrive
  cloud placeholders, deduplication, WOF compression) is refused. The final
  handle-reported path must still lie inside the root.
* **Directory creation** (`ensure_dir`) creates one component at a time and
  re-checks each one, so a planted junction named like a category folder
  (e.g. `Documents` → `C:\Users\me\Desktop`) is detected and refused.
* The root itself can never be a source or target of an operation.

## Moving files — `fsops/mod.rs`

1. The source is opened **by handle**, without following reparse points, with
   `DELETE` access and a share mode that only permits readers. If another
   program is writing the file (an active download) or holds it open without
   delete sharing, the open fails with a sharing violation and the file is
   reported as *in use*.
2. Windows is asked what the handle really refers to; its identity (volume
   serial + file index), size and modification time must match what was scanned.
3. The destination directory is opened and **pinned** (no delete sharing), which
   prevents it — and therefore its ancestors — from being renamed or replaced by
   a link during the move. Its final path must be inside the root.
4. The file is renamed **through the same handle** with
   `SetFileInformationByHandle(FileRenameInfo)` and `ReplaceIfExists = FALSE`.
   Existing files are never overwritten; on a name clash a new name such as
   `invoice (1).pdf` is chosen (or the move is skipped, per settings).
5. If an atomic rename is impossible (different volume), copy-and-verify is
   used: stream to a staging file inside the destination folder, `fsync`,
   re-hash the staging file with SHA-256, publish it with a no-replace rename,
   and only then delete the source **by handle**.

These steps mitigate time-of-check/time-of-use races. They cannot defend
against an attacker who already runs code as the same user — such an attacker
can modify files directly — but they do prevent Tucklane from being *tricked*
into writing outside the authorized folder.

## Data safety

* No permanent deletion anywhere in the codebase. Duplicate removal uses the
  Windows Recycle Bin (`trash` crate) after explicit confirmation; the backend
  re-hashes the file first, refuses to remove the last verified copy of a group,
  and refuses entirely on non-fixed drives (removable media may have no Recycle
  Bin, where Windows would delete permanently). Very large files can still make
  Windows ask whether to delete permanently; declining keeps the file.
* Opening files: only passive document/media types open directly. Programs,
  scripts, shortcuts, disk images, macro-enabled documents, HTML/SVG and
  unrecognized types require an explicit confirmation (decided in Rust,
  `commands/files.rs::opens_without_confirmation`). Trailing dots/spaces are
  stripped before the check, as Windows does.
* History, undo and retry only act on operations recorded for the currently
  authorized folder.
* Intent is persisted before each move (`planned` → `in_progress` with the
  file's identity), results only after the filesystem confirms them. Startup
  recovery reconciles interrupted operations with what is on disk.
* Undo verifies the moved file's identity, refuses to overwrite anything at the
  original location, and asks the user when a file was modified.

## Privacy

* No network access is used by the application at runtime (other than
  opening the project page in the user's browser on request).
* The SQLite database stores relative paths, names, sizes and timestamps —
  never file contents. Optional content-signature sniffing reads at most 512
  bytes and is off by default.
* Notifications contain counts, never file names.

## Auditing checklist

* `rg "remove_file|remove_dir"` — only Tucklane's own staging files, its
  own database sidecar files (`-wal`/`-shm` after setting a corrupt database
  aside), and test code.
* `rg "fs::rename|std::fs::copy"` — not used for user files (std `rename`
  replaces existing files on Windows; Tucklane uses handle-based no-replace).
* Tests: `src-tauri/src/security`, `src-tauri/src/acceptance_tests.rs`
  (`rejects_paths_outside_root`, `junction_escape_is_blocked`,
  `symlink_to_outside_is_not_followed`, `root_replaced_during_operation_aborts`,
  `junction_as_root_and_dangerous_roots_rejected`).

## Known limitations

* The executable and installer are **not code-signed** unless a maintainer
  configures signing (see `docs/RELEASING.md`). Windows SmartScreen will warn.
* An attacker with the same user privileges can always modify the user's files
  directly; Tucklane does not try to defend against that.
* Monitoring only runs while Tucklane is running (window or tray).
* On FAT/exFAT volumes file IDs change when a file changes folders. Moves
  still work and are recorded correctly, but crash recovery cannot locate a
  file at its destination by identity there and marks it "needs attention".
* Tucklane's install directory is protected, so a portable copy run from
  inside Downloads cannot authorize Downloads. Use the installer.
