# Releasing

## Checklist

1. All CI checks green on `main` (format, lint, typecheck, Vitest, `cargo test`,
   clippy, Tauri build).
2. Bump the version in **all three** places:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
3. Update `CHANGELOG.md`.
4. Build locally and smoke-test the NSIS installer on a clean Windows 10 and 11
   machine (or VM): install, onboarding with a scratch folder, organize, undo,
   tray, quit, uninstall (check the Start Menu entry and startup entry are gone).
5. Verify the icon of `Tucklane.exe`, the installer, Start Menu and taskbar.
6. Tag: `git tag v0.1.0 && git push origin v0.1.0`.
7. The **Release** workflow builds the installers and creates a **draft**
   GitHub release with the artifacts attached. Review the notes, then publish.

## Publisher identity

The publisher shown in *Installed apps* is `bundle.publisher` in
`src-tauri/tauri.conf.json` (default: "Tucklane Contributors"). Change it if
you distribute your own build.

## Code signing

Releases are **unsigned** by default, and Windows SmartScreen will show
"Windows protected your PC" until the build gains reputation. To sign:

* obtain an Authenticode code-signing certificate (OV/EV, or Azure Trusted
  Signing),
* configure `bundle.windows.certificateThumbprint` / `signCommand` following
  <https://v2.tauri.app/distribute/sign/windows/>,
* provide secrets to the release workflow.

Do not claim a release is signed unless this has been done.

## Installer behavior

* NSIS installer, per-user (`installMode: currentUser`), no administrator rights.
* Start Menu shortcut under "Tucklane"; optional desktop shortcut.
* Uninstall removes the program and the startup (Run) registry entry
  (`src-tauri/windows/hooks.nsh`). App data is removed only if the user ticks
  "Delete the application data". Files in the managed folder are never touched.
* The MSI is provided for managed deployments. It does not run the NSIS hooks;
  turn off "Launch with Windows" before uninstalling an MSI install, or remove
  the `Tucklane` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
