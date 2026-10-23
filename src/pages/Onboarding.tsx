import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  FolderOpen,
  Loader2,
  ShieldCheck,
  Undo2,
  Wand2,
  Eye,
  AlertTriangle,
  Folder,
} from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount } from "@/lib/format";
import { cn } from "@/lib/cn";
import { useApp } from "@/stores/app";
import type { FolderValidation, Preset, PresetInfo, ScanSummary, Settings } from "@/types/api";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/primitives";
import { LogoMark } from "@/components/common/Logo";
import { HelpDialog } from "@/components/layout/HelpDialog";
import { toast } from "@/components/ui/toast";

const STEPS = ["Welcome", "Folder", "Style", "Behavior", "Analysis"];

const PRESET_TEXT: Record<Preset, { title: string; body: string }> = {
  simple: {
    title: "Simple",
    body: "Documents, Images, Videos, Audio, Software, Archives and Other.",
  },
  balanced: {
    title: "Balanced",
    body: "Documents with subfolders, Media, Software, Archives, Developer and Other.",
  },
  advanced: {
    title: "Advanced",
    body: "Detailed classification with more nested folders for specific file types.",
  },
};

function Steps({ step }: { step: number }) {
  return (
    <ol className="flex items-center gap-2" aria-label="Setup progress">
      {STEPS.map((s, i) => (
        <li key={s} className="flex items-center gap-2">
          <span
            className={cn(
              "flex size-6 items-center justify-center rounded-full text-[12px] font-semibold",
              i < step
                ? "bg-accent-strong text-white"
                : i === step
                  ? "bg-brand text-brand-fg"
                  : "bg-surface-3 text-subtle",
            )}
            aria-current={i === step ? "step" : undefined}
          >
            {i < step ? <CheckCircle2 className="size-3.5" /> : i + 1}
          </span>
          <span
            className={cn("text-[12.5px]", i === step ? "font-semibold text-fg" : "text-subtle")}
          >
            {s}
          </span>
          {i < STEPS.length - 1 && <span className="mx-1 h-px w-6 bg-border-strong" aria-hidden />}
        </li>
      ))}
    </ol>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  const id = label.replace(/\W+/g, "-").toLowerCase();
  return (
    <div className="flex items-start justify-between gap-6 rounded-xl border border-border bg-surface px-4 py-3">
      <div>
        <label htmlFor={id} className="text-sm font-medium">
          {label}
        </label>
        <p className="mt-0.5 text-[13px] text-muted">{hint}</p>
      </div>
      <Switch id={id} checked={checked} onCheckedChange={onChange} label={label} />
    </div>
  );
}

export function Onboarding() {
  const status = useApp((s) => s.status)!;
  const refresh = useApp((s) => s.refresh);
  const setPage = useApp((s) => s.setPage);
  const [step, setStep] = useState(0);
  const [help, setHelp] = useState(false);
  const [folder, setFolder] = useState<FolderValidation | null>(null);
  const [checking, setChecking] = useState(false);
  const [presets, setPresets] = useState<PresetInfo[]>([]);
  const [s, setS] = useState<Settings>(status.settings);
  const [scan, setScan] = useState<ScanSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);

  useEffect(() => {
    void api.presets().then(setPresets);
    void (async () => {
      setChecking(true);
      try {
        const existing = status.root?.path ?? (await api.defaultDownloads());
        if (existing) setFolder(await api.validateRoot(existing));
      } finally {
        setChecking(false);
      }
    })();
  }, [status.root?.path]);

  const choose = async () => {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose the folder Tucklane may organize",
      defaultPath: folder?.path,
    });
    if (typeof picked === "string") {
      setChecking(true);
      try {
        setFolder(await api.validateRoot(picked));
      } finally {
        setChecking(false);
      }
    }
  };

  const confirmFolder = async () => {
    if (!folder?.ok) return;
    setBusy(true);
    try {
      await api.selectRoot(folder.path);
      await refresh();
      setStep(2);
    } catch (e) {
      toast.error("That folder can't be used", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const analyze = async () => {
    setStep(4);
    setScan(null);
    setScanError(null);
    try {
      await api.updateSettings({ ...s, onboardingComplete: false });
      setScan(await api.scan());
    } catch (e) {
      setScanError(errorMessage(e));
    }
  };

  const finish = async (then: "review" | "organize" | "dashboard") => {
    setBusy(true);
    try {
      await api.updateSettings({ ...s, onboardingComplete: true });
      if (then === "organize") {
        const plan = await api.preview();
        const ids = plan.items.filter((i) => i.selected).map((i) => i.id);
        if (ids.length > 0) {
          const rep = await api.execute(plan.id, ids);
          toast.success(
            `${rep.session.completed} files organized`,
            "Every move can be undone from Activity.",
            {
              label: "Undo this session",
              run: () => void api.undoSession(rep.session.id).then(() => useApp.getState().bump()),
            },
          );
        }
      }
      await refresh();
      setPage(then === "review" ? "organize" : "dashboard");
    } catch (e) {
      toast.error("Something went wrong", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="flex items-center justify-between px-10 pt-7">
        <div className="flex items-center gap-2.5">
          <LogoMark size={28} />
          <span className="font-display text-[16px] font-semibold">Tucklane</span>
        </div>
        <Steps step={step} />
      </div>

      {/* m-auto centers when there is room and scrolls from the top when not. */}
      <div className="flex min-h-0 flex-1 overflow-auto px-10 py-8">
        <div className="tl-enter m-auto w-full max-w-[680px]" key={step}>
          {step === 0 && (
            <div className="text-center">
              <div className="mx-auto mb-5 w-fit">
                <LogoMark size={72} />
              </div>
              <h1 className="font-display text-[34px] leading-tight font-semibold tracking-tight">
                Meet Tucklane.
                <br />
                Your downloads, finally in order.
              </h1>
              <p className="mx-auto mt-3 max-w-lg text-[15px] text-muted">
                Organize downloads automatically, find what matters, and undo any change whenever
                you want.
              </p>
              <div className="mt-8 flex justify-center gap-3">
                <Button variant="primary" size="lg" onClick={() => setStep(1)}>
                  Get Started <ArrowRight className="size-4" />
                </Button>
                <Button variant="outline" size="lg" onClick={() => setHelp(true)}>
                  Learn How It Works
                </Button>
              </div>
              <div className="mx-auto mt-8 grid max-w-xl grid-cols-3 gap-3 text-left">
                {[
                  {
                    icon: ShieldCheck,
                    t: "Only your folder",
                    b: "Works inside the one folder you approve.",
                  },
                  { icon: Eye, t: "Preview first", b: "See every move before anything happens." },
                  {
                    icon: Undo2,
                    t: "Always undoable",
                    b: "Every change is recorded and reversible.",
                  },
                ].map((f) => (
                  <div key={f.t} className="rounded-xl border border-border bg-surface p-3.5">
                    <f.icon className="mb-2 size-4 text-accent-strong" />
                    <div className="text-[13px] font-semibold">{f.t}</div>
                    <div className="mt-0.5 text-[12.5px] text-muted">{f.b}</div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {step === 1 && (
            <div>
              <h1 className="font-display text-[26px] font-semibold tracking-tight">
                Choose your folder
              </h1>
              <p className="mt-1.5 text-[14px] text-muted">
                Tucklane will only organize files inside the folder you approve.
              </p>
              <div className="mt-6 rounded-2xl border border-border bg-surface p-5">
                <div className="flex items-center gap-3">
                  <div className="flex size-10 items-center justify-center rounded-xl bg-accent-soft text-accent-strong">
                    <Folder className="size-5" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-[12px] tracking-wide text-subtle uppercase">
                      Folder to organize
                    </div>
                    <div className="truncate font-mono text-[13.5px]" data-selectable>
                      {checking ? "Detecting…" : (folder?.path ?? "No Downloads folder detected")}
                    </div>
                  </div>
                  <Button variant="outline" onClick={choose}>
                    <FolderOpen className="size-4" /> Choose another…
                  </Button>
                </div>
                {folder && !folder.ok && (
                  <p
                    className="mt-3 flex items-start gap-2 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger"
                    role="alert"
                  >
                    <AlertTriangle className="mt-0.5 size-4 shrink-0" /> {folder.message}
                  </p>
                )}
                {folder?.ok && (
                  <p className="mt-3 flex items-center gap-2 text-[13px] text-success">
                    <CheckCircle2 className="size-4" /> This folder can be managed safely.
                  </p>
                )}
              </div>
              <ul className="mt-4 space-y-1.5 text-[13px] text-muted">
                <li>
                  • Tucklane never reads or moves anything outside this folder, and never follows
                  links or junctions out of it.
                </li>
                <li>• Folders you created inside it are left exactly as they are.</li>
                <li>
                  • You can change the folder later in Settings. Access is never expanded silently.
                </li>
              </ul>
            </div>
          )}

          {step === 2 && (
            <div>
              <h1 className="font-display text-[26px] font-semibold tracking-tight">
                Choose an organization style
              </h1>
              <p className="mt-1.5 text-[14px] text-muted">
                You can switch styles or add your own rules at any time.
              </p>
              <div className="mt-6 grid gap-3" role="radiogroup" aria-label="Organization style">
                {(["simple", "balanced", "advanced"] as Preset[]).map((p) => {
                  const info = presets.find((x) => x.id === p);
                  const active = s.preset === p;
                  return (
                    <button
                      key={p}
                      role="radio"
                      aria-checked={active}
                      onClick={() => setS({ ...s, preset: p })}
                      className={cn(
                        "rounded-2xl border bg-surface p-4 text-left transition-colors",
                        active
                          ? "border-accent-strong ring-2 ring-accent/40"
                          : "border-border hover:border-border-strong",
                      )}
                    >
                      <div className="flex items-center gap-2">
                        <span className="text-[15px] font-semibold">{PRESET_TEXT[p].title}</span>
                        {p === "balanced" && (
                          <span className="rounded-full bg-accent-soft px-2 py-0.5 text-[11.5px] font-semibold text-accent-strong">
                            Recommended
                          </span>
                        )}
                      </div>
                      <p className="mt-0.5 text-[13px] text-muted">{PRESET_TEXT[p].body}</p>
                      {active && info && (
                        <div className="mt-3 flex flex-wrap gap-1.5">
                          {info.folders.map((f) => (
                            <span
                              key={f}
                              className="rounded-md bg-surface-2 px-2 py-0.5 font-mono text-[11.5px] text-muted"
                            >
                              {f.replace(/\//g, " › ")}
                            </span>
                          ))}
                        </div>
                      )}
                    </button>
                  );
                })}
              </div>
            </div>
          )}

          {step === 3 && (
            <div>
              <h1 className="font-display text-[26px] font-semibold tracking-tight">
                How should Tucklane behave?
              </h1>
              <p className="mt-1.5 text-[14px] text-muted">
                Safe defaults are selected. Nothing moves without your say-so.
              </p>
              <div className="mt-6 grid gap-2.5">
                <Toggle
                  label="Preview changes before organizing"
                  hint="Review every proposed move first."
                  checked={s.previewBeforeOrganize}
                  onChange={(v) => setS({ ...s, previewBeforeOrganize: v })}
                />
                <Toggle
                  label="Organize new downloads automatically"
                  hint="Finished downloads are sorted while Tucklane runs. Off until you turn it on."
                  checked={s.autoOrganize}
                  onChange={(v) => setS({ ...s, autoOrganize: v })}
                />
                <Toggle
                  label="Start with Windows"
                  hint="Launch quietly in the notification area when you sign in."
                  checked={s.launchAtStartup}
                  onChange={(v) => setS({ ...s, launchAtStartup: v })}
                />
                <Toggle
                  label="Show system tray icon"
                  hint="Keep Tucklane running in the notification area when the window is closed."
                  checked={s.showTrayIcon}
                  onChange={(v) => setS({ ...s, showTrayIcon: v, closeToTray: v })}
                />
                <Toggle
                  label="Show desktop notifications"
                  hint="Grouped summaries only — never file names."
                  checked={s.notifications}
                  onChange={(v) => setS({ ...s, notifications: v })}
                />
                <Toggle
                  label="Enable duplicate detection"
                  hint="Find identical files on request. Nothing is ever deleted automatically."
                  checked={s.duplicateDetection}
                  onChange={(v) => setS({ ...s, duplicateDetection: v })}
                />
              </div>
            </div>
          )}

          {step === 4 && (
            <div>
              <h1 className="font-display text-[26px] font-semibold tracking-tight">
                Here’s what we found
              </h1>
              <p className="mt-1.5 text-[14px] text-muted">
                Nothing has been moved. Review the plan or organize now — you can undo either way.
              </p>
              {!scan && !scanError && (
                <div className="mt-10 flex flex-col items-center gap-3 text-muted">
                  <Loader2 className="size-6 animate-spin text-accent-strong" />
                  Analyzing {status.root?.path}…
                </div>
              )}
              {scanError && (
                <p className="mt-6 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger">
                  {scanError}
                </p>
              )}
              {scan && (
                <>
                  <div className="mt-6 grid grid-cols-4 gap-3">
                    {[
                      ["Total files", formatCount(scan.totalFiles)],
                      ["Total size", formatBytes(scan.totalBytes)],
                      ["Categories", formatCount(scan.categories)],
                      ["Suggested moves", formatCount(scan.suggestedChanges)],
                      ["Unclassified", formatCount(scan.unclassified)],
                      ["In use now", formatCount(scan.inUse)],
                      ["Still downloading", formatCount(scan.pending)],
                      ["Possible duplicates", formatCount(scan.possibleDuplicateGroups)],
                    ].map(([k, v]) => (
                      <div key={k} className="rounded-xl border border-border bg-surface p-3.5">
                        <div className="tl-num font-display text-[22px] font-semibold">{v}</div>
                        <div className="text-[12.5px] text-muted">{k}</div>
                      </div>
                    ))}
                  </div>
                  <p className="mt-2 text-[12px] text-subtle">
                    “Possible duplicates” are same-size files; the Duplicates screen verifies
                    content before calling anything a duplicate.
                  </p>
                  {scan.byFolder.length > 0 && (
                    <div className="mt-4 rounded-xl border border-border bg-surface p-4">
                      <div className="mb-2 text-[13px] font-semibold">Where files would go</div>
                      <div className="grid grid-cols-2 gap-x-6 gap-y-1">
                        {scan.byFolder.map(([f, n]) => (
                          <div key={f} className="flex justify-between text-[13px]">
                            <span className="font-mono text-muted">{f.replace(/\//g, " › ")}</span>
                            <span className="tl-num">{n}</span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between border-t border-border bg-surface px-10 py-4">
        <Button
          variant="ghost"
          onClick={() => setStep(Math.max(0, step - 1))}
          disabled={step === 0 || busy}
        >
          <ArrowLeft className="size-4" /> Back
        </Button>
        <div className="flex gap-2">
          {step === 1 && (
            <Button variant="primary" onClick={confirmFolder} disabled={!folder?.ok} loading={busy}>
              Authorize this folder <ArrowRight className="size-4" />
            </Button>
          )}
          {step === 2 && (
            <Button variant="primary" onClick={() => setStep(3)}>
              Continue <ArrowRight className="size-4" />
            </Button>
          )}
          {step === 3 && (
            <Button variant="primary" onClick={analyze}>
              Analyze my folder <ArrowRight className="size-4" />
            </Button>
          )}
          {step === 4 && (
            <>
              <Button
                variant="ghost"
                onClick={() => finish("dashboard")}
                disabled={!scan}
                loading={busy}
              >
                Finish without organizing
              </Button>
              <Button variant="outline" onClick={() => finish("review")} disabled={!scan || busy}>
                <Eye className="size-4" /> Review Plan
              </Button>
              <Button
                variant="primary"
                onClick={() => finish("organize")}
                disabled={!scan || scan.suggestedChanges === 0 || busy}
              >
                <Wand2 className="size-4" /> Organize Now
              </Button>
            </>
          )}
        </div>
      </div>
      <HelpDialog open={help} onOpenChange={setHelp} />
    </div>
  );
}
