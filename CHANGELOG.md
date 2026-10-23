# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.0] - 2026-10-23

First release.

### Added

* First-run onboarding: folder authorization (Windows Downloads known folder
  detected, redirected folders supported), organization style, behavior
  defaults, and a non-destructive initial analysis.
* Folder authorization enforced in the Rust backend (`AuthorizedRootGuard`):
  rejects paths outside the root, traversal, alternate data streams, links,
  junctions and unsafe reparse points; refuses system, program, app-data,
  network and drive-root folders.
* Rule-based classification with three presets (Simple, Balanced, Advanced),
  custom rules (conditions, priorities, testing, import/export), optional
  filename hints and optional file-signature detection.
* Preview of every proposed move with reasons; execution with progress,
  cancellation and a final report.
* Handle-based, no-overwrite moves; copy-and-verify fallback; collision naming
  (`name (1).ext`) with rename/skip/ask policies.
* Persistent history (SQLite), undo of single moves, selections or sessions,
  retry of failed moves, and crash recovery at startup.
* Background monitoring with download-completion detection, pause/resume,
  periodic safety rescans and automatic organization (off by default).
* Duplicate finder (size → fingerprint → SHA-256) with Recycle Bin removal that
  always keeps one copy.
* File Explorer scoped to the authorized folder: list/grid views, search,
  filters, sorting, multi-select, thumbnails for small images, context menu.
* Activity log with filters, session grouping, details and CSV export.
* System tray, grouped native notifications, launch at sign-in, light and dark
  themes.
* Windows NSIS (per-user) and MSI installers.

[Unreleased]: https://github.com/tucklane/tucklane/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tucklane/tucklane/releases/tag/v0.1.0
