# Implementation progress

Status of every area in the product specification. "Tested" means covered by
automated tests that were run and passed; "Verified" means checked manually in
the built app. Anything not listed as done is not claimed as working.

Legend: ✅ done · 🟡 partial · ⛔ not implemented

## Foundation

| Item | Status | Notes |
|---|---|---|
| Tauri 2 + React + TypeScript + Vite + Tailwind v4 | ✅ | Tauri 2.12, React 19, Vite 7, Tailwind 4.1 |
| Original branding and Windows icons | ✅ | `assets/brand/tucklane-icon.svg` → `src-tauri/icons/*` (`.ico` 16–256 px) |
| SQLite with migrations | ✅ Tested | `database.rs`, `migrations/0001_initial.sql`; corrupt DB set aside; newer schema refused |
| Typed commands | ✅ | `src-tauri/src/commands/*`, `src/lib/api.ts`, `src/types/api.ts` |

## Security and core engine

| Item | Status | Notes |
|---|---|---|
| AuthorizedRootGuard (traversal, ADS, links, junctions, reparse points) | ✅ Tested | `security/`, acceptance tests |
| Dangerous-root policy (drives, system, program, app data, profile ancestors, network) | ✅ Tested | `security/policy.rs` |
| Handle-based no-replace moves; dest dir pinning | ✅ Tested | `fsops/mod.rs` |
| Copy-and-verify fallback (SHA-256, staging inside root) | ✅ Tested | forced in `copy_verify_fallback_moves_safely` |
| Classification pipeline and presets | ✅ Tested | `classifier/` |
| Collision handling (rename/skip/ask, case-insensitive, no `(1) (1)`) | ✅ Tested | |
| Unicode / Urdu / long paths / multi-dot / no extension / reserved names | ✅ Tested | |
| Operation history, explicit states, crash recovery | ✅ Tested | `history::recover` |
| Undo (single, selected, session; conflict/modified/missing handling) | ✅ Tested | |

## Interface

| Item | Status | Notes |
|---|---|---|
| Onboarding (5 steps) | ✅ Tested | Vitest + RTL |
| Dashboard (stats, health score, recent activity, quick actions, monitoring) | ✅ | All data from backend |
| File Explorer (list/grid, search, filters, sort, multi-select, context menu, thumbnails, history) | ✅ | Thumbnails: small raster images via backend only |
| Organize (preview table, tabs, save/load plan, execution progress, cancel, report, undo session) | ✅ Tested | |
| Rules (editor, conditions, priorities, test, import/export) | ✅ | |
| Duplicates | ✅ Tested (backend) | |
| Activity (operations, sessions, event log, filters, CSV export) | ✅ | |
| Settings (all sections) | ✅ | Language selector present but English only |
| Light/dark themes, keyboard shortcuts | ✅ | |

## Automation

| Item | Status | Notes |
|---|---|---|
| Watcher (notify, non-recursive), debounce, completion checks | ✅ Tested | `watcher_picks_up_completed_downloads_only` |
| Pause/resume, periodic rescan, wake detection, watch re-establishment | ✅ | Pause tested; sleep/wake detection is a tick-gap heuristic, not tested automatically |
| Tray menu and close-to-tray | ✅ | |
| Grouped notifications without file names | ✅ | |
| Launch at sign-in | ✅ | `tauri-plugin-autostart`, removed by NSIS uninstaller hook |

## Advanced

| Item | Status | Notes |
|---|---|---|
| Smart filename hints (local keywords) | ✅ Tested | Opt-in; never applied in automatic mode (confidence < 0.9) |
| File-signature detection | ✅ Tested | Opt-in; first 512 bytes |
| Local model provider | 🟡 | `ClassificationProvider` trait only; no model shipped |
| Content extraction from documents | ⛔ | Not implemented (documented on roadmap) |
| Native shell thumbnails for video/PDF | ⛔ | Roadmap |

## Release engineering

| Item | Status | Notes |
|---|---|---|
| GitHub Actions CI (format, lint, typecheck, tests, build) | ✅ | Not yet run on GitHub (no remote configured) |
| Release workflow (draft release) | ✅ | Not yet run |
| NSIS installer | ✅ Verified | Silent install/uninstall on Windows 11: per-user, no admin, Start Menu shortcut, uninstall entry (version + publisher), startup Run key removed on uninstall, app data kept, user files untouched |
| MSI installer | 🟡 | Builds; not install-tested. Does not run the NSIS uninstall hook (documented) |
| Exe/installer icon and version metadata | ✅ Verified | Icon extracted from installed exe and setup exe; FileVersion 0.1.0, CompanyName "Shahzaib Anwar". FileDescription is "Tucklane" (Tauri uses the product name) |
| Real-app smoke test | ✅ Verified | Onboarding with sandbox folder, analysis, preview, organize (22 files), undo session, dashboard, explorer, duplicates, activity, dark theme, close-to-tray, single instance, quit |
| Independent code review | ✅ | 6 findings (quit deadlock, FAT/exFAT recording, Recycle Bin on removable drives, open-confirmation gaps, cross-root history, `ß` comparison) — all fixed |
| Code signing | ⛔ | Not configured; SmartScreen limitation documented |
| Playwright E2E | ⛔ | UI covered by Vitest + RTL with mocked IPC; real-WebView E2E is on the roadmap |
