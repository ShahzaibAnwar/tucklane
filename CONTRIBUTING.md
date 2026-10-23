# Contributing to Tucklane

Thanks for helping! Tucklane moves people's files, so the bar for correctness
and safety is high, and we're glad to help you meet it.

## Ground rules

* **Safety first.** Changes must not weaken the folder authorization model
  (`docs/SECURITY.md`). Filesystem logic belongs in Rust and goes through
  `AuthorizedRootGuard`. Never add a frontend filesystem permission.
* **No data loss.** Never overwrite, never permanently delete. Duplicate
  removal uses the Recycle Bin only.
* **No fake UI.** Every number and list on screen comes from the backend. No
  placeholder statistics, no buttons that do nothing.
* **Privacy.** No telemetry, no network calls, no uploading file names or contents.
* **Test in sandboxes.** Never run development builds against your real
  Downloads folder; use a scratch folder.

## Getting started

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for setup and commands.

## Pull requests

1. Open an issue first for larger changes so we can agree on the approach.
2. Keep PRs focused. Include tests: Rust tests for backend behavior (use
   `testutil::Fixture`), Vitest/RTL tests for UI logic.
3. Run before pushing:

   ```bash
   npm run lint && npm run typecheck && npm test
   cd src-tauri && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
   ```

4. Update docs and `CHANGELOG.md` (Unreleased section) when behavior changes.
5. Fill in the PR template, including how you tested on Windows.

## Reporting bugs

Use the bug report template. Please **never attach personal files** or
unsanitized screenshots of your folders. "Export diagnostics" in Settings ›
About produces a report without file names.

## Security issues

Please report privately — see [SECURITY.md](SECURITY.md).

## Code of conduct

This project follows our [Code of Conduct](CODE_OF_CONDUCT.md).
