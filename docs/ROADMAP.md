# Roadmap

Ideas for future releases. Nothing here is implemented yet.

## Next (0.2)

* Native Windows shell thumbnails (`IShellItemImageFactory`) for videos and PDFs
  in the explorer grid (today: small raster images only).
* Per-category destination overrides in Settings (today: via rules).
* Translations (the UI is structured for it; English only today).
* Code signing for releases, once a certificate is available.
* Scheduled "tidy" sessions with a summary notification.
* Rule import from a file picker (today: paste JSON).

## Later

* Optional local classification model behind the existing
  `ClassificationProvider` trait — local only, opt-in, never moving
  low-confidence files automatically.
* Opt-in text extraction from selected document formats for smarter rules.
* Multiple authorized folders.
* Age-based archiving rules (e.g. move installers older than 90 days into an
  archive folder) with the same preview/undo guarantees.
* Playwright end-to-end tests against the real WebView2 via `tauri-driver`.
