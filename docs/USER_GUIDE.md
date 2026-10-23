# User guide

## First run

1. **Welcome** — *Get Started*.
2. **Choose your folder** — Tucklane detects your Windows Downloads folder
   (including a redirected one). You can pick another local folder. Drives,
   system folders, your whole user profile and network locations are refused.
3. **Organization style** — *Simple*, *Balanced* (recommended) or *Advanced*.
4. **Behavior** — preview first (on), automatic organization (off), start with
   Windows (off), tray icon (on), notifications (on), duplicate detection (on).
5. **Analysis** — Tucklane scans the folder without moving anything. Choose
   *Review Plan*, *Organize Now*, or finish without organizing.

## What gets organized

Only **loose files at the top level** of your folder. Folders you created —
and everything inside them — are left alone. Hidden and system files (such as
`desktop.ini`), links/junctions and unfinished downloads are skipped.

Default *Balanced* layout:

```text
Downloads/
├── Documents/  PDF, Word, Excel, Presentations, Reports
├── Media/      Images, Videos, Audio, Subtitles
├── Software/   Installers, Packages
├── Archives/
├── Developer/  Code, Data
└── Other/      Torrents (and unknown files, if you choose)
```

## Previewing and organizing

The **Organize** screen lists every proposed move with its destination and the
reason (rule, extension, signature or filename hint). Untick anything you want
to keep in place. Tabs show files that are still downloading or in use, files
that need your review, and files left alone.

*Organize Selected* moves files one at a time and shows progress. *Cancel* stops
after the current file. The final report has **Undo This Session**.

*Save Plan* keeps your selection; loading it later re-checks every file first.

## Name collisions

Tucklane never overwrites a file. In Settings › Rules choose:

* **Rename automatically** (default): `invoice.pdf` becomes `invoice (1).pdf`;
  `invoice (1).pdf` becomes `invoice (2).pdf`, not `invoice (1) (1).pdf`.
* **Skip** the file.
* **Ask** — conflicting files are unselected in the preview until you choose them.

## Undo

Undo a single move from the Dashboard or Activity, several selected moves, or a
whole session (Activity › Sessions). Before restoring, Tucklane checks the file
is still where it put it and unchanged. If something else now has the original
name, it asks before restoring under a new name. Restored files are excluded
from automatic organization so they stay where you put them (re-include them in
Settings › Rules › Excluded files).

## Automatic organization

Turn on in Settings › Automation. While Tucklane runs (window open or in the
tray), finished downloads are sorted after the settling time (15 s by default).
A file counts as finished only when it is not a browser temp file, has stopped
changing, and is not open in another program. Notifications are grouped and
never show file names. Pause from the sidebar, the tray menu or Settings.

**Closing the window** keeps Tucklane in the tray by default. Use **Quit** in
the tray menu to exit; monitoring stops then.

## Rules

Rules run before the default style, top to bottom; the first match wins.
Conditions: extension, name contains/starts/ends with, wildcard pattern
(`Invoice-*.pdf`), size, modified/created age, file type. Actions: move to a
folder inside your Downloads, leave in place, flag for review, or move only
after confirmation. *Test against my folder* shows which files a rule would
match and whether a higher rule overrides it. Rules can be exported (copied as
JSON) and imported.

## Duplicates

*Find duplicates* groups files by size, then confirms identical content with
SHA-256. Nothing is selected for you. Pick copies to remove; they go to the
**Recycle Bin** after you confirm. Tucklane always keeps at least one copy.

## Activity

Every file operation (moves, undos, Recycle Bin actions) with filters for date,
type and result, session grouping, details, undo and retry. The *Event log* tab
records scans, sessions and errors. *Export CSV* writes the filtered list into
your Downloads folder.

## Uninstalling

Use *Apps › Installed apps*. Your downloaded files are never touched. The
uninstaller removes the "start with Windows" entry. Tucklane's settings and
history (`%APPDATA%\io.github.tucklane.desktop`) are kept unless you tick
*Delete the application data* in the uninstaller.
