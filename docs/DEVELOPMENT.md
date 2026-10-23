# Development

## Prerequisites (Windows 10/11 x64)

* [Rust](https://rustup.rs) stable (MSVC toolchain) — tested with 1.99
* Visual Studio 2022 Build Tools with "Desktop development with C++"
* Node.js 22 LTS and npm
* WebView2 Runtime (preinstalled on Windows 11)

See the official [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Commands

```bash
npm ci                 # install exact frontend dependencies
npm run tauri dev      # run the app with hot reload
npm test               # Vitest + React Testing Library
npm run lint           # ESLint
npm run typecheck      # TypeScript
npm run format:check   # Prettier
cd src-tauri && cargo test          # Rust unit + filesystem acceptance tests
cd src-tauri && cargo clippy --all-targets -- -D warnings
cd src-tauri && cargo fmt --check
npm run tauri build    # release exe + NSIS and MSI installers
npm run icons          # regenerate icons from assets/brand/tucklane-icon.svg
```

Build outputs:

* `src-tauri/target/release/Tucklane.exe`
* `src-tauri/target/release/bundle/nsis/Tucklane_<version>_x64-setup.exe`
* `src-tauri/target/release/bundle/msi/Tucklane_<version>_x64_en-US.msi`

## Testing rules

* **Never** point development builds or tests at your real Downloads folder.
  Rust tests create sandboxes under `src-tauri/target/test-sandbox/`
  (`testutil::Fixture`). When trying the dev app, create a scratch folder (for
  example `D:\TucklaneSandbox\Downloads`) and authorize that.
* The dev app uses the same app-data folder as an installed copy
  (`%APPDATA%\io.github.tucklane.desktop`). Delete it to re-run onboarding.
* Symlink tests need Windows Developer Mode; they skip themselves otherwise.
  Junction tests use `mklink /J`, which needs no special rights.
* The duplicate test sends one sandbox file to the Recycle Bin and immediately
  purges that same item.

## Project layout

See [ARCHITECTURE.md](ARCHITECTURE.md). Key rule: **filesystem and security
logic lives in Rust**. The frontend never receives filesystem permissions.

## Adding a Tauri command

1. Implement it in `src-tauri/src/commands/<area>.rs` using `commands::run`
   for blocking work. Accept only relative paths; resolve them with the guard.
2. Register it in `lib.rs` (`generate_handler!`).
3. Add the TypeScript type to `src/types/api.ts` and a wrapper in `src/lib/api.ts`.

## Adding a database migration

Add `src-tauri/migrations/000N_description.sql` and append it to `MIGRATIONS`
in `database.rs`. Never edit a released migration.
