# Architecture

Tucklane is a [Tauri 2](https://v2.tauri.app) desktop app: a Rust backend that
owns all filesystem work, and a React + TypeScript frontend that renders state
and sends typed commands.

```text
┌──────────────────────── WebView2 (untrusted UI) ────────────────────────┐
│ React pages ─ zustand store ─ lib/api.ts (typed invoke wrappers)        │
└───────────────▲───────────────────────────────────┬────────────────────┘
       events   │                                   │ invoke(command, args)
┌───────────────┴───────────────────────────────────▼────────────────────┐
│ commands/*        typed Tauri commands, argument validation              │
│ state.rs          AppState, monitor lifecycle, auto-organize glue         │
│ ─────────────────────────────── core (no Tauri types) ──────────────── │
│ security/         AuthorizedRootGuard, RootPolicy, Win32 handle helpers  │
│ scanner.rs        directory listing / walking (never follows links)      │
│ classifier/       extension kinds, presets, signatures, filename hints   │
│ rules.rs          user rules: conditions, actions, validation            │
│ organizer/        plan (pure) → execute (transactional)                  │
│ fsops/            handle-based no-replace move, copy-verify, names        │
│ history.rs        undo, retry, crash recovery                            │
│ duplicates.rs     size → fingerprint → SHA-256, Recycle Bin removal       │
│ watcher.rs        notify-based monitor, debouncing, completion checks     │
│ database.rs       SQLite (rusqlite, bundled), migrations, queries        │
│ tray.rs / notify.rs  system tray, native notifications                    │
└──────────────────────────────────────────────────────────────────────────┘
```

## Core flows

### Organize (manual)

1. `preview_organization` → `organizer::plan::build_plan` lists the root's top
   level, classifies each loose file and resolves collisions **without touching
   disk**. Plans are cached in memory (last 4) and referenced by ID.
2. `execute_organization(planId, itemIds)` → `organizer::execute`:
   * creates a `sessions` row and one `operations` row per item (`planned`);
   * per item: re-checks size/mtime, records the file identity (`in_progress`),
     calls `fsops::move_within_root`, then records `completed` with the final
     path — or `skipped`/`failed` with a reason;
   * emits throttled `tucklane://organize-progress` events;
   * honours cancellation between files.

### Automatic organization

`watcher::Monitor` runs on its own thread. `notify` (ReadDirectoryChangesW)
events and periodic/wake rescans *nominate* top-level files. A nominated file
is handed to `state::on_files_ready` only when it is not a temporary download
(`.crdownload`, `.part`, …), has no partial sibling (Firefox), has been quiet
for the settling time and can be opened exclusively. If auto-organize is on,
the files are planned in `PlanMode::Auto` (high-confidence decisions only) and
executed as an `auto` session.

The watch is non-recursive, so moving files into category folders never
re-triggers it. Files restored by undo are added to `exclusions`, so they are
not moved again.

### Classification order

1. exact-file exclusions, protected patterns, ignored extensions
2. user rules (priority order; other matches are reported as `alsoMatched`)
3. opt-in file signature for missing/unknown/ambiguous extensions
4. opt-in filename hints (`classifier::smart::KeywordProvider`, behind the
   `ClassificationProvider` trait for future local models)
5. extension → `FileKind` → preset folder
6. unknown → review (or `Other`, if the user chose that)

### Undo and recovery

* `history::undo_operations` verifies identity (file ID, or SHA-256 for
  copy-verified moves), detects modification, refuses to overwrite the original
  location unless the user chooses "restore with a new name", and records the
  undo as its own operation.
* `history::recover` runs at startup and reconciles `in_progress` operations with
  disk (still at source → failed/untouched; found at destination by identity →
  completed; neither → `recovery_required`), removes Tucklane's own partial
  staging files, and closes interrupted sessions.

## Data

SQLite database in `%APPDATA%\io.github.tucklane.desktop\tucklane.db` (WAL).
Schema: `src-tauri/migrations/`. Migrations are applied in a transaction and
tracked with `PRAGMA user_version`. A corrupt database is renamed aside and
recreated (the UI tells the user); a database from a newer version is refused.

Tables: `settings`, `authorized_roots`, `rules`, `sessions`, `operations`,
`file_index`, `duplicate_groups`, `app_events`, `exclusions`, `saved_plans`.

Logs: `%LOCALAPPDATA%\io.github.tucklane.desktop\logs` (via `tauri-plugin-log`).

## Frontend

* `src/lib/api.ts` — the only place that calls `invoke`.
* `src/types/api.ts` — mirrors the Rust serde types.
* `src/stores/app.ts` — app status, current page, change ticks.
* `src/hooks/useData.ts` — loads data and reloads on `tucklane://data-changed`.
* Pages: Dashboard, File Explorer, Organize, Rules, Duplicates, Activity, Settings,
  plus first-run Onboarding.
* UI primitives built on Radix (`radix-ui`), Tailwind CSS v4 tokens in
  `src/styles/index.css`, Lucide icons. Large lists use `@tanstack/react-virtual`.

## Concurrency

* Every file-changing job (organize, auto-organize, undo, retry, recycle) takes
  `AppState::file_job`, so at most one runs at a time.
* All commands are `async` and run blocking work via `spawn_blocking`, keeping
  the UI thread free.
* Duplicate hashing uses at most two threads to stay gentle on HDDs; files are
  streamed with a 1 MiB buffer.
