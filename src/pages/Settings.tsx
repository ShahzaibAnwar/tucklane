import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  ExternalLink,
  FileDown,
  FolderOpen,
  Lock,
  LogOut,
  ShieldCheck,
  X,
} from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/stores/app";
import { useData } from "@/hooks/useData";
import type { Settings } from "@/types/api";
import { PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Badge, Card, Field, Input, Select, Switch } from "@/components/ui/primitives";
import { ConfirmDialog } from "@/components/ui/dialog";
import { toast } from "@/components/ui/toast";
import { winPath, timeAgo } from "@/lib/format";
import { cn } from "@/lib/cn";

const SECTIONS = [
  "General",
  "Folder",
  "Automation",
  "Rules",
  "Safety",
  "Privacy",
  "About",
] as const;
type Section = (typeof SECTIONS)[number];

function ListEditor({
  values,
  onChange,
  placeholder,
  label,
}: {
  values: string[];
  onChange: (v: string[]) => void;
  placeholder: string;
  label: string;
}) {
  const [text, setText] = useState("");
  const add = () => {
    const v = text.trim();
    if (v && !values.includes(v)) onChange([...values, v]);
    setText("");
  };
  return (
    <div className="w-80">
      <div className="flex gap-2">
        <Input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          placeholder={placeholder}
          aria-label={label}
        />
        <Button onClick={add} disabled={!text.trim()}>
          Add
        </Button>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {values.map((v) => (
          <span
            key={v}
            className="flex items-center gap-1 rounded-md bg-surface-2 px-2 py-0.5 font-mono text-[12px]"
          >
            {v}
            <button
              onClick={() => onChange(values.filter((x) => x !== v))}
              aria-label={`Remove ${v}`}
              className="text-subtle hover:text-fg"
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
        {values.length === 0 && <span className="text-[12px] text-subtle">None</span>}
      </div>
    </div>
  );
}

function NumberField({
  value,
  min,
  max,
  onCommit,
  label,
  suffix,
}: {
  value: number;
  min: number;
  max: number;
  onCommit: (n: number) => void;
  label: string;
  suffix: string;
}) {
  const [v, setV] = useState(String(value));
  useEffect(() => setV(String(value)), [value]);
  const commit = () => {
    const n = Math.round(Number(v));
    if (Number.isFinite(n) && n >= min && n <= max) onCommit(n);
    else setV(String(value));
  };
  return (
    <div className="flex items-center gap-2">
      <Input
        type="number"
        min={min}
        max={max}
        value={v}
        onChange={(e) => setV(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => e.key === "Enter" && commit()}
        className="w-24"
        aria-label={label}
      />
      <span className="text-[13px] text-muted">{suffix}</span>
    </div>
  );
}

export function SettingsPage() {
  const status = useApp((s) => s.status)!;
  const refresh = useApp((s) => s.refresh);
  const setSettingsLocal = useApp((s) => s.setSettings);
  const s = status.settings;
  const [section, setSection] = useState<Section>("General");
  const [changeFolder, setChangeFolder] = useState(false);
  const [validating, setValidating] = useState(false);
  const [folderCheck, setFolderCheck] = useState<string | null>(null);
  const { data: exclusions, reload: reloadExclusions } = useData(api.exclusions);
  const refs = useRef<Record<string, HTMLElement | null>>({});

  const update = async (patch: Partial<Settings>) => {
    const next = { ...s, ...patch };
    setSettingsLocal(next);
    try {
      setSettingsLocal(await api.updateSettings(next));
    } catch (e) {
      toast.error("Setting not saved", errorMessage(e));
      await refresh();
    }
  };

  const pickFolder = async () => {
    setChangeFolder(false);
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose the folder Tucklane may organize",
      defaultPath: status.root?.path,
    });
    if (typeof picked !== "string") return;
    try {
      const v = await api.validateRoot(picked);
      if (!v.ok) {
        toast.error("That folder can't be used", v.message ?? undefined);
        return;
      }
      await api.selectRoot(v.path);
      await refresh();
      toast.success(
        "Folder changed",
        `Tucklane now manages ${v.path}. The previous folder is no longer monitored.`,
      );
    } catch (e) {
      toast.error("Could not change folder", errorMessage(e));
    }
  };

  const validate = async () => {
    if (!status.root) return;
    setValidating(true);
    try {
      const v = await api.validateRoot(status.root.path);
      setFolderCheck(
        v.ok ? "The folder is available and safe to manage." : (v.message ?? "Unavailable"),
      );
    } finally {
      setValidating(false);
    }
  };

  const go = (sec: Section) => {
    setSection(sec);
    refs.current[sec]?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const sectionRef = (name: Section) => (el: HTMLElement | null) => {
    refs.current[name] = el;
  };

  return (
    <>
      <PageHeader title="Settings" subtitle="Changes apply immediately." />
      <div className="flex gap-8 px-8 pb-10">
        <nav
          className="sticky top-4 hidden h-fit w-40 shrink-0 flex-col gap-0.5 md:flex"
          aria-label="Settings sections"
        >
          {SECTIONS.map((x) => (
            <button
              key={x}
              onClick={() => go(x)}
              className={cn(
                "rounded-md px-3 py-1.5 text-left text-[13.5px]",
                section === x ? "bg-surface-2 font-medium" : "text-muted hover:text-fg",
              )}
            >
              {x}
            </button>
          ))}
        </nav>
        <div className="tl-enter flex min-w-0 max-w-3xl flex-1 flex-col gap-5">
          <Card ref={sectionRef("General")} className="px-5 py-2">
            <h2 className="pt-3 text-[15px] font-semibold">General</h2>
            <div className="divide-y divide-border">
              <Field label="Theme" hint="System follows your Windows light/dark setting.">
                <Select
                  value={s.theme}
                  onChange={(e) => void update({ theme: e.target.value as Settings["theme"] })}
                  aria-label="Theme"
                >
                  <option value="system">System</option>
                  <option value="light">Light</option>
                  <option value="dark">Dark</option>
                </Select>
              </Field>
              <Field
                label="Language"
                hint="English is the only language in this release; the interface is structured for translations."
              >
                <Select value={s.language} disabled aria-label="Language">
                  <option value="en">English</option>
                </Select>
              </Field>
              <Field
                label="Launch with Windows"
                hint="Starts quietly in the notification area when you sign in."
              >
                <Switch
                  checked={s.launchAtStartup}
                  onCheckedChange={(v) => void update({ launchAtStartup: v })}
                  label="Launch with Windows"
                />
              </Field>
              <Field
                label="Start minimized"
                hint="When launched at sign-in, stay in the notification area instead of opening the window."
              >
                <Switch
                  checked={s.startMinimized}
                  onCheckedChange={(v) => void update({ startMinimized: v })}
                  label="Start minimized"
                />
              </Field>
              <Field
                label="Show system tray icon"
                hint="The tray menu lets you organize, pause monitoring or quit."
              >
                <Switch
                  checked={s.showTrayIcon}
                  onCheckedChange={(v) =>
                    void update({ showTrayIcon: v, closeToTray: v ? s.closeToTray : false })
                  }
                  label="Show system tray icon"
                />
              </Field>
              <Field
                label="Closing the window keeps Tucklane running"
                hint="Monitoring continues from the tray. Choose Quit in the tray menu to exit completely. When off, closing the window quits Tucklane and stops monitoring."
              >
                <Switch
                  checked={s.closeToTray}
                  disabled={!s.showTrayIcon}
                  onCheckedChange={(v) => void update({ closeToTray: v })}
                  label="Close to tray"
                />
              </Field>
              <Field
                label="Desktop notifications"
                hint="Grouped summaries such as “12 downloads organized”. File names are never shown."
              >
                <Switch
                  checked={s.notifications}
                  onCheckedChange={(v) => void update({ notifications: v })}
                  label="Desktop notifications"
                />
              </Field>
            </div>
          </Card>

          <Card ref={sectionRef("Folder")} className="px-5 py-2">
            <h2 className="pt-3 text-[15px] font-semibold">Folder</h2>
            <div className="divide-y divide-border">
              <Field
                label="Authorized folder"
                hint={
                  status.root
                    ? `Authorized ${timeAgo(status.root.selectedAt)}. Tucklane can only read and move files inside it.`
                    : "No folder authorized."
                }
              >
                <div className="flex max-w-sm items-center gap-2">
                  <span
                    className="truncate font-mono text-[12.5px]"
                    data-selectable
                    title={status.root?.path}
                  >
                    {status.root?.path ?? "—"}
                  </span>
                  {status.rootAvailable ? (
                    <Badge tone="success">Available</Badge>
                  ) : (
                    <Badge tone="danger">Unavailable</Badge>
                  )}
                </div>
              </Field>
              <Field
                label="Validate folder"
                hint={
                  folderCheck ??
                  "Checks that the folder still exists and is still a safe, local folder."
                }
              >
                <Button onClick={validate} loading={validating}>
                  <CheckCircle2 className="size-4" /> Validate
                </Button>
              </Field>
              <Field
                label="Change folder"
                hint="Monitoring of the current folder stops immediately. History for it is kept."
              >
                <Button onClick={() => setChangeFolder(true)}>
                  <FolderOpen className="size-4" /> Change…
                </Button>
              </Field>
            </div>
          </Card>

          <Card ref={sectionRef("Automation")} className="px-5 py-2">
            <h2 className="pt-3 text-[15px] font-semibold">Automation</h2>
            <div className="divide-y divide-border">
              <Field
                label="Organize new downloads automatically"
                hint="Works while Tucklane is running, including from the tray. Only deterministic decisions are applied automatically; hints and rules that ask for confirmation wait for you."
              >
                <Switch
                  checked={s.autoOrganize}
                  onCheckedChange={(v) => void update({ autoOrganize: v })}
                  label="Automatic organization"
                />
              </Field>
              <Field
                label="Pause monitoring"
                hint="Stops watching for new downloads until you resume."
              >
                <Switch
                  checked={s.monitoringPaused}
                  onCheckedChange={(v) =>
                    void (v ? api.pauseMonitoring() : api.resumeMonitoring()).then(refresh)
                  }
                  label="Pause monitoring"
                />
              </Field>
              <Field
                label="File settling time"
                hint="A new file must stay unchanged this long before it counts as a finished download."
              >
                <NumberField
                  value={s.settleSeconds}
                  min={3}
                  max={3600}
                  onCommit={(n) => void update({ settleSeconds: n })}
                  label="Settling time"
                  suffix="seconds"
                />
              </Field>
              <Field
                label="Safety rescan"
                hint="Periodically rescans in case a filesystem event was missed. 0 turns it off."
              >
                <NumberField
                  value={s.scanIntervalMinutes}
                  min={0}
                  max={1440}
                  onCommit={(n) => void update({ scanIntervalMinutes: n })}
                  label="Scan frequency"
                  suffix="minutes"
                />
              </Field>
              <Field label="Unknown file types" hint="Files Tucklane doesn't recognize.">
                <Select
                  value={s.unknownFiles}
                  onChange={(e) =>
                    void update({ unknownFiles: e.target.value as Settings["unknownFiles"] })
                  }
                  aria-label="Unknown files"
                >
                  <option value="leave">Leave for review</option>
                  <option value="move_to_other">Move to Other</option>
                </Select>
              </Field>
            </div>
          </Card>

          <Card ref={sectionRef("Rules")} className="px-5 py-2">
            <h2 className="pt-3 text-[15px] font-semibold">Rules</h2>
            <div className="divide-y divide-border">
              <Field
                label="Organization style"
                hint="Default folders used when no custom rule matches."
              >
                <Select
                  value={s.preset}
                  onChange={(e) => void update({ preset: e.target.value as Settings["preset"] })}
                  aria-label="Organization style"
                >
                  <option value="simple">Simple</option>
                  <option value="balanced">Balanced (recommended)</option>
                  <option value="advanced">Advanced</option>
                </Select>
              </Field>
              <Field
                label="When a file with the same name exists"
                hint="Tucklane never overwrites a file."
              >
                <Select
                  value={s.collision}
                  onChange={(e) =>
                    void update({ collision: e.target.value as Settings["collision"] })
                  }
                  aria-label="Collision behavior"
                >
                  <option value="rename">Rename automatically — invoice (1).pdf</option>
                  <option value="skip">Skip the file</option>
                  <option value="ask">Ask me in the preview</option>
                </Select>
              </Field>
              <Field
                label="Smart filename hints"
                hint="Uses words in the filename (invoice, receipt, resume, report) to suggest folders. Runs locally; never reads file contents."
              >
                <Switch
                  checked={s.smartFilenameHints}
                  onCheckedChange={(v) => void update({ smartFilenameHints: v })}
                  label="Smart filename hints"
                />
              </Field>
              <Field
                label="Check file signatures"
                hint="For files with no or unknown extension, reads the first 512 bytes to identify the type (e.g. a PDF without .pdf). Off by default because it touches file contents."
              >
                <Switch
                  checked={s.contentSignatures}
                  onCheckedChange={(v) => void update({ contentSignatures: v })}
                  label="Check file signatures"
                />
              </Field>
              <Field
                label="Ignored extensions"
                hint="Files with these extensions are always left in place."
              >
                <ListEditor
                  values={s.ignoredExtensions}
                  onChange={(v) => void update({ ignoredExtensions: v })}
                  placeholder="iso"
                  label="Ignored extension"
                />
              </Field>
              <Field
                label="Protected filename patterns"
                hint="Wildcards (* and ?). Matching files are never moved."
              >
                <ListEditor
                  values={s.protectedPatterns}
                  onChange={(v) => void update({ protectedPatterns: v })}
                  placeholder="tax-*.pdf"
                  label="Protected pattern"
                />
              </Field>
              <Field
                label="Excluded files"
                hint="Files you excluded from organization, and files restored by undo."
              >
                <div className="w-80 space-y-1">
                  {exclusions?.length === 0 && (
                    <span className="text-[12px] text-subtle">None</span>
                  )}
                  {exclusions?.slice(0, 50).map((x) => (
                    <div
                      key={x.relPath}
                      className="flex items-center justify-between gap-2 text-[12.5px]"
                    >
                      <span className="truncate font-mono" title={winPath(x.relPath)}>
                        {winPath(x.relPath)}
                      </span>
                      <span className="flex items-center gap-1.5">
                        <Badge>
                          {x.reason === "undo"
                            ? "Restored"
                            : x.reason === "export"
                              ? "Export"
                              : "You"}
                        </Badge>
                        <button
                          className="text-subtle hover:text-fg"
                          aria-label={`Include ${x.relPath} again`}
                          onClick={() => void api.include([x.relPath]).then(reloadExclusions)}
                        >
                          <X className="size-3.5" />
                        </button>
                      </span>
                    </div>
                  ))}
                </div>
              </Field>
            </div>
          </Card>

          <Card ref={sectionRef("Safety")} className="px-5 py-2">
            <h2 className="pt-3 text-[15px] font-semibold">Safety</h2>
            <div className="divide-y divide-border">
              <Field
                label="Preview before organizing"
                hint="Organize buttons open the preview instead of moving files right away."
              >
                <Switch
                  checked={s.previewBeforeOrganize}
                  onCheckedChange={(v) => void update({ previewBeforeOrganize: v })}
                  label="Preview before organizing"
                />
              </Field>
              <Field
                label="Duplicate detection"
                hint="Enables the Duplicates screen. Duplicates are never deleted automatically."
              >
                <Switch
                  checked={s.duplicateDetection}
                  onCheckedChange={(v) => void update({ duplicateDetection: v })}
                  label="Duplicate detection"
                />
              </Field>
              <Field
                label="Confirm destructive actions"
                hint="Moving duplicates to the Recycle Bin always asks first; this cannot be turned off in this release."
              >
                <Switch
                  checked
                  disabled
                  onCheckedChange={() => {}}
                  label="Confirm destructive actions"
                />
              </Field>
              <Field
                label="History retention"
                hint="Older history is removed. Moves that can still be undone are kept so undo keeps working."
              >
                <NumberField
                  value={s.historyRetentionDays}
                  min={7}
                  max={3650}
                  onCommit={(n) => void update({ historyRetentionDays: n })}
                  label="History retention"
                  suffix="days"
                />
              </Field>
              <Field
                label="Undo"
                hint="Every move can be undone from Activity or the Dashboard. Undo never replaces an existing file; if the original name is taken, you choose a new name."
              >
                <ShieldCheck className="size-5 text-success" />
              </Field>
            </div>
          </Card>

          <Card ref={sectionRef("Privacy")} className="px-5 py-4">
            <h2 className="text-[15px] font-semibold">Privacy</h2>
            <p className="mt-2 flex items-center gap-2 text-[14px] font-medium">
              <Lock className="size-4 text-accent-strong" /> No accounts. No cloud uploads. No
              tracking. Your files stay on your computer.
            </p>
            <ul className="mt-2 space-y-1 text-[13px] text-muted">
              <li>
                • Tucklane makes no network requests. Classification, hashing and history all happen
                locally.
              </li>
              <li>
                • History is stored in a local database in your Windows app-data folder. It contains
                file names and relative paths, never file contents.
              </li>
              <li>• Exports (CSV, diagnostics) are written only into your authorized folder.</li>
            </ul>
          </Card>

          <Card ref={sectionRef("About")} className="px-5 py-4">
            <h2 className="text-[15px] font-semibold">About</h2>
            <dl className="mt-2 grid grid-cols-[140px_1fr] gap-y-1.5 text-[13px]">
              <dt className="text-muted">Version</dt>
              <dd>{status.version}</dd>
              <dt className="text-muted">License</dt>
              <dd>MIT — free and open source</dd>
              <dt className="text-muted">Source code</dt>
              <dd>
                <button
                  className="inline-flex items-center gap-1 text-accent-strong hover:underline"
                  onClick={() =>
                    void api
                      .openProjectPage()
                      .catch((e) => toast.error("Could not open link", errorMessage(e)))
                  }
                >
                  github.com/tucklane/tucklane <ExternalLink className="size-3" />
                </button>
              </dd>
              <dt className="text-muted">Release notes</dt>
              <dd className="text-muted">
                0.1.0 — first public release: safe organizing with preview and undo, rules,
                monitoring, duplicate finder, tray.
              </dd>
              <dt className="text-muted">Credits</dt>
              <dd className="text-muted">
                Built with Tauri, Rust, React, Tailwind CSS, Radix UI, Lucide icons, SQLite and the
                notify crate.
              </dd>
            </dl>
            <div className="mt-4 flex gap-2">
              <Button
                onClick={async () => {
                  try {
                    const name = await api.exportDiagnostics();
                    toast.success(
                      "Diagnostics saved",
                      `${name} is in your Downloads folder. It contains no file names.`,
                    );
                  } catch (e) {
                    toast.error("Could not export diagnostics", errorMessage(e));
                  }
                }}
              >
                <FileDown className="size-4" /> Export diagnostics
              </Button>
              <Button variant="ghost" onClick={() => void api.quit()}>
                <LogOut className="size-4" /> Quit Tucklane
              </Button>
            </div>
          </Card>
        </div>
      </div>
      <ConfirmDialog
        open={changeFolder}
        onOpenChange={setChangeFolder}
        title="Change the authorized folder?"
        description="Tucklane will stop monitoring the current folder immediately and will only be able to organize the new one. Files already moved stay where they are."
        confirmLabel="Choose new folder…"
        onConfirm={pickFolder}
      />
    </>
  );
}
