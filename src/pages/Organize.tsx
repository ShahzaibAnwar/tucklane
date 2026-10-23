import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock,
  Loader2,
  RefreshCw,
  Save,
  Search,
  Undo2,
  Wand2,
  XCircle,
  History,
} from "lucide-react";
import { api, errorMessage, EVENTS } from "@/lib/api";
import { formatBytes, formatCount, formatDateTime, plural, winPath } from "@/lib/format";
import { cn } from "@/lib/cn";
import { useApp } from "@/stores/app";
import { useBackendEvent } from "@/hooks/useData";
import type { ExecReport, Plan, PlanItem, PlanStatus, Progress, SavedPlanInfo } from "@/types/api";
import { PageBody, PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Badge,
  Card,
  Checkbox,
  EmptyState,
  Input,
  ProgressBar,
  Tooltip,
} from "@/components/ui/primitives";
import { FileIcon } from "@/components/common/FileIcon";
import { undoSession } from "@/components/common/undo";
import { toast } from "@/components/ui/toast";

type Tab = "proposed" | "waiting" | "review" | "left" | "all";
const TABS: { id: Tab; label: string; statuses: PlanStatus[] }[] = [
  {
    id: "proposed",
    label: "Proposed",
    statuses: ["ready", "renamed", "needs_decision", "confirm"],
  },
  { id: "waiting", label: "Waiting", statuses: ["pending", "locked"] },
  { id: "review", label: "Needs review", statuses: ["review"] },
  { id: "left", label: "Left alone", statuses: ["ignored", "skipped"] },
  { id: "all", label: "All", statuses: [] },
];

const MOVABLE: PlanStatus[] = ["ready", "renamed", "needs_decision", "confirm"];

const STATUS_BADGE: Record<
  PlanStatus,
  { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" | "info" }
> = {
  ready: { label: "Ready", tone: "success" },
  renamed: { label: "Will rename", tone: "info" },
  needs_decision: { label: "Name taken", tone: "warning" },
  confirm: { label: "Confirm", tone: "warning" },
  pending: { label: "Downloading", tone: "info" },
  locked: { label: "In use", tone: "warning" },
  review: { label: "Review", tone: "neutral" },
  ignored: { label: "Left alone", tone: "neutral" },
  skipped: { label: "Skipped", tone: "danger" },
};

const SOURCE_LABEL: Record<string, string> = {
  rule: "Rule",
  extension: "Extension",
  signature: "File signature",
  filename_hint: "Filename hint",
  exclusion: "Excluded",
  protected: "Protected",
  ignored_extension: "Ignored type",
  unknown: "Unrecognized",
};

function Row({
  item,
  checked,
  onToggle,
}: {
  item: PlanItem;
  checked: boolean;
  onToggle: (v: boolean) => void;
}) {
  const c = item.classification;
  const movable = MOVABLE.includes(item.status);
  const b = STATUS_BADGE[item.status];
  return (
    <div
      className={cn(
        "grid h-full grid-cols-[32px_minmax(0,1.6fr)_minmax(0,1.2fr)_minmax(0,1.8fr)_110px] items-center gap-3 border-b border-border px-4 text-[13px]",
        checked && "bg-accent-soft/40",
      )}
    >
      <Checkbox
        checked={checked}
        disabled={!movable}
        onCheckedChange={onToggle}
        label={`Include ${item.name}`}
      />
      <div className="flex min-w-0 items-center gap-2.5">
        <FileIcon kind={c.kind} name={item.name} />
        <div className="min-w-0">
          <div className="truncate font-medium" title={item.name}>
            {item.name}
          </div>
          <div className="text-[11.5px] text-subtle">{formatBytes(item.size)}</div>
        </div>
      </div>
      <div className="min-w-0 truncate font-mono text-[12px] text-muted" title={item.destDir ?? ""}>
        {item.destDir ? (
          <>
            <ArrowRight className="mr-1 inline size-3 text-subtle" />
            {winPath(item.destDir)}
            {item.destName && item.destName !== item.name && (
              <span className="text-info"> as {item.destName}</span>
            )}
          </>
        ) : (
          <span className="text-subtle">Stays in place</span>
        )}
      </div>
      <div className="min-w-0">
        <div className="truncate" title={c.reason}>
          {c.reason}
        </div>
        <div className="mt-0.5 flex items-center gap-1.5 text-[11.5px] text-subtle">
          <span>{SOURCE_LABEL[c.source] ?? c.source}</span>
          {c.confidence < 1 && <span>· {Math.round(c.confidence * 100)}% confidence</span>}
          {c.alsoMatched.length > 0 && (
            <Tooltip content={`Also matched: ${c.alsoMatched.join(", ")} (lower priority)`}>
              <span className="underline decoration-dotted">+{c.alsoMatched.length} rule</span>
            </Tooltip>
          )}
          {item.note && (
            <Tooltip content={item.note}>
              <span className="truncate text-warning">· {item.note}</span>
            </Tooltip>
          )}
        </div>
      </div>
      <div>
        <Badge tone={b.tone}>{b.label}</Badge>
      </div>
    </div>
  );
}

function PreviewTable({
  items,
  selected,
  setSelected,
}: {
  items: PlanItem[];
  selected: Set<string>;
  setSelected: (s: Set<string>) => void;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 52,
    overscan: 12,
    initialRect: { width: 1000, height: 600 },
  });
  const movable = items.filter((i) => MOVABLE.includes(i.status));
  const allChecked = movable.length > 0 && movable.every((i) => selected.has(i.id));
  const some = movable.some((i) => selected.has(i.id));
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="grid grid-cols-[32px_minmax(0,1.6fr)_minmax(0,1.2fr)_minmax(0,1.8fr)_110px] items-center gap-3 border-b border-border bg-surface-2/60 px-4 py-2 text-[12px] font-semibold tracking-wide text-muted uppercase">
        <Checkbox
          checked={allChecked ? true : some ? "indeterminate" : false}
          disabled={movable.length === 0}
          label="Select all movable files"
          onCheckedChange={(on) => {
            const s = new Set(selected);
            movable.forEach((i) => (on ? s.add(i.id) : s.delete(i.id)));
            setSelected(s);
          }}
        />
        <span>Filename</span>
        <span>Destination</span>
        <span>Reason</span>
        <span>Status</span>
      </div>
      <div
        ref={parentRef}
        className="min-h-[240px] flex-1 overflow-auto"
        role="table"
        aria-rowcount={items.length}
      >
        <div style={{ height: v.getTotalSize(), position: "relative" }}>
          {v.getVirtualItems().map((r) => {
            const it = items[r.index];
            return (
              <div
                key={it.id}
                role="row"
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  right: 0,
                  height: r.size,
                  transform: `translateY(${r.start}px)`,
                }}
              >
                <Row
                  item={it}
                  checked={selected.has(it.id)}
                  onToggle={(on) => {
                    const s = new Set(selected);
                    if (on) s.add(it.id);
                    else s.delete(it.id);
                    setSelected(s);
                  }}
                />
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

function Execution({
  progress,
  report,
  onCancel,
  onDone,
  cancelling,
}: {
  progress: Progress | null;
  report: ExecReport | null;
  onCancel: () => void;
  onDone: () => void;
  cancelling: boolean;
}) {
  const setPage = useApp((s) => s.setPage);
  const p = progress;
  if (report) {
    const s = report.session;
    return (
      <Card className="tl-enter mx-auto w-full max-w-2xl p-8 text-center">
        <div
          className={cn(
            "mx-auto mb-4 flex size-14 items-center justify-center rounded-2xl",
            s.failed ? "bg-warning-soft text-warning" : "bg-success-soft text-success",
          )}
        >
          {s.failed ? <AlertTriangle className="size-7" /> : <CheckCircle2 className="size-7" />}
        </div>
        <h2 className="font-display text-[22px] font-semibold">
          {report.cancelled ? "Organization cancelled" : "Organization complete"}
        </h2>
        <p className="mt-1 text-[14px] text-muted">
          {plural(s.completed, "file")} moved · {formatCount(s.skipped)} skipped ·{" "}
          {formatCount(s.failed)} failed
        </p>
        {report.errors.length > 0 && (
          <ul className="mx-auto mt-4 max-h-36 max-w-lg overflow-auto rounded-lg bg-danger-soft px-3 py-2 text-left text-[12.5px] text-danger">
            {report.errors.map((e) => (
              <li key={e} className="truncate">
                {e}
              </li>
            ))}
          </ul>
        )}
        <div className="mt-6 flex justify-center gap-2">
          {s.completed > 0 && (
            <Button variant="primary" size="lg" onClick={() => void undoSession(s.id).then(onDone)}>
              <Undo2 className="size-4" /> Undo This Session
            </Button>
          )}
          <Button variant="outline" size="lg" onClick={() => setPage("activity")}>
            View details
          </Button>
          <Button variant="ghost" size="lg" onClick={onDone}>
            Done
          </Button>
        </div>
      </Card>
    );
  }
  return (
    <Card className="mx-auto w-full max-w-2xl p-8" aria-live="polite">
      <div className="flex items-center gap-3">
        <Loader2 className="size-5 animate-spin text-accent-strong" />
        <h2 className="font-display text-[20px] font-semibold">Organizing…</h2>
      </div>
      <div className="mt-5">
        <ProgressBar value={p?.done ?? 0} max={p?.total ?? 1} label="Organization progress" />
      </div>
      <div className="mt-2 flex justify-between text-[13px] text-muted">
        <span className="truncate">{p?.current ? `Moving ${p.current}` : "Preparing…"}</span>
        <span className="tl-num">
          {p?.done ?? 0} / {p?.total ?? "…"}
        </span>
      </div>
      <div className="mt-5 grid grid-cols-3 gap-3 text-center">
        {[
          ["Completed", p?.completed ?? 0, "text-success"],
          ["Skipped", p?.skipped ?? 0, "text-warning"],
          ["Failed", p?.failed ?? 0, "text-danger"],
        ].map(([l, n, c]) => (
          <div key={l as string} className="rounded-xl bg-surface-2 p-3">
            <div className={cn("tl-num font-display text-[22px] font-semibold", c as string)}>
              {n as number}
            </div>
            <div className="text-[12.5px] text-muted">{l as string}</div>
          </div>
        ))}
      </div>
      <div className="mt-6 flex justify-end">
        <Button variant="outline" onClick={onCancel} loading={cancelling}>
          <XCircle className="size-4" /> Cancel
        </Button>
      </div>
      <p className="mt-2 text-right text-[12px] text-subtle">
        Cancelling stops after the current file finishes moving safely.
      </p>
    </Card>
  );
}

export function Organize() {
  const only = useApp((s) => s.organizeOnly);
  const clearOnly = useApp((s) => s.clearOrganizeOnly);
  const setPage = useApp((s) => s.setPage);
  const bump = useApp((s) => s.bump);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [tab, setTab] = useState<Tab>("proposed");
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"preview" | "running">("preview");
  const [progress, setProgress] = useState<Progress | null>(null);
  const [report, setReport] = useState<ExecReport | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [saved, setSaved] = useState<SavedPlanInfo[]>([]);
  const [scopeOnly, setScopeOnly] = useState<string[] | null>(only);

  const load = async (from?: Promise<Plan>) => {
    setLoading(true);
    setError(null);
    try {
      const p = await (from ?? api.preview(scopeOnly ?? undefined));
      setPlan(p);
      setSelected(new Set(p.items.filter((i) => i.selected).map((i) => i.id)));
      if (p.summary.proposed === 0 && p.items.length > 0) setTab("all");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (only) clearOnly();
    void load();
    void api
      .savedPlans()
      .then(setSaved)
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scopeOnly]);

  useBackendEvent<Progress>(EVENTS.organizeProgress, (p) => {
    if (mode === "running") setProgress(p);
  });

  const items = useMemo(() => {
    if (!plan) return [];
    const t = TABS.find((x) => x.id === tab)!;
    const q = query.trim().toLowerCase();
    return plan.items.filter(
      (i) =>
        (t.statuses.length === 0 || t.statuses.includes(i.status)) &&
        (!q || i.name.toLowerCase().includes(q)),
    );
  }, [plan, tab, query]);

  const count = (t: Tab) => {
    if (!plan) return 0;
    const s = TABS.find((x) => x.id === t)!.statuses;
    return s.length === 0
      ? plan.items.length
      : plan.items.filter((i) => s.includes(i.status)).length;
  };

  const run = async () => {
    if (!plan || selected.size === 0) return;
    setMode("running");
    setProgress(null);
    setReport(null);
    try {
      const rep = await api.execute(plan.id, [...selected]);
      setReport(rep);
    } catch (e) {
      toast.error("Organization failed", errorMessage(e));
      setMode("preview");
    } finally {
      setCancelling(false);
    }
  };

  const savePlan = async () => {
    if (!plan) return;
    try {
      await api.savePlan(plan.id, [...selected]);
      setSaved(await api.savedPlans());
      toast.success(
        "Plan saved",
        "Load it later from Saved plans. It will be re-checked against your folder first.",
      );
    } catch (e) {
      toast.error("Could not save plan", errorMessage(e));
    }
  };

  if (mode === "running") {
    return (
      <>
        <PageHeader
          title="Organize"
          subtitle="Files are moved one at a time; each move is verified before it is recorded."
        />
        <PageBody>
          <Execution
            progress={progress}
            report={report}
            cancelling={cancelling}
            onCancel={async () => {
              setCancelling(true);
              await api.cancelOrganize();
            }}
            onDone={() => {
              setMode("preview");
              setReport(null);
              bump();
              void load();
            }}
          />
        </PageBody>
      </>
    );
  }

  const s = plan?.summary;
  const selectedBytes = plan
    ? plan.items.filter((i) => selected.has(i.id)).reduce((a, i) => a + i.size, 0)
    : 0;
  const rules = s ? Object.entries(s.byRule) : [];

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="Organize"
        subtitle={
          scopeOnly
            ? `Previewing ${plural(scopeOnly.length, "selected file")} from the explorer.`
            : "Preview what will move where. Nothing changes until you choose Organize Selected."
        }
        actions={
          <>
            {scopeOnly && (
              <Button variant="ghost" onClick={() => setScopeOnly(null)}>
                Preview whole folder
              </Button>
            )}
            {saved.length > 0 && (
              <select
                className="h-9 rounded-lg border border-border-strong bg-surface px-2.5 text-sm"
                aria-label="Load a saved plan"
                value=""
                onChange={(e) => e.target.value && void load(api.loadSavedPlan(e.target.value))}
              >
                <option value="">Saved plans…</option>
                {saved.map((p) => (
                  <option key={p.id} value={p.id}>
                    {formatDateTime(p.createdAt)} — {p.itemCount} files
                  </option>
                ))}
              </select>
            )}
            <Button onClick={() => void load()} loading={loading}>
              {!loading && <RefreshCw className="size-4" />} Refresh
            </Button>
          </>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col gap-4 px-8 pb-6">
        {s && (
          <div className="grid grid-cols-3 gap-3 md:grid-cols-6">
            {[
              ["Proposed changes", s.proposed, null],
              ["Skipped", s.skipped + s.ignored, null],
              ["Name conflicts", s.conflicts, s.conflicts ? "text-info" : null],
              [
                "Awaiting completion",
                s.pending + s.locked,
                s.pending + s.locked ? "text-warning" : null,
              ],
              ["Needs review", s.review, null],
              ["Rules responsible", rules.length, null],
            ].map(([l, n, c]) => (
              <Card key={l as string} className="px-3.5 py-2.5">
                <div className={cn("tl-num font-display text-[20px] font-semibold", c as string)}>
                  {formatCount(n as number)}
                </div>
                <div className="text-[12px] text-muted">{l as string}</div>
              </Card>
            ))}
          </div>
        )}

        <Card className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-border px-4 py-2.5">
            <div className="flex gap-1" role="tablist">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  role="tab"
                  aria-selected={tab === t.id}
                  onClick={() => setTab(t.id)}
                  className={cn(
                    "rounded-md px-2.5 py-1 text-[13px] font-medium",
                    tab === t.id
                      ? "bg-surface-3 text-fg"
                      : "text-muted hover:bg-surface-2 hover:text-fg",
                  )}
                >
                  {t.label} <span className="tl-num text-subtle">{count(t.id)}</span>
                </button>
              ))}
            </div>
            <div className="relative w-64">
              <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-subtle" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter by name"
                className="pl-8"
                aria-label="Filter by name"
              />
            </div>
          </div>
          {loading && !plan && (
            <div className="flex flex-1 items-center justify-center gap-2 py-16 text-muted">
              <Loader2 className="size-4 animate-spin" /> Analyzing your folder…
            </div>
          )}
          {error && (
            <p className="m-4 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger">
              {error}
            </p>
          )}
          {plan && plan.items.length === 0 && (
            <EmptyState icon={<CheckCircle2 className="size-5" />} title="Nothing to organize">
              There are no loose files at the top of your folder. Folders you created are always
              left alone.
            </EmptyState>
          )}
          {plan && plan.items.length > 0 && items.length === 0 && (
            <EmptyState icon={<Clock className="size-5" />} title="No files in this view" />
          )}
          {plan && items.length > 0 && (
            <PreviewTable items={items} selected={selected} setSelected={setSelected} />
          )}
        </Card>

        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-[13px] text-muted">
            {selected.size > 0 ? (
              <>
                <span className="font-semibold text-fg">{plural(selected.size, "file")}</span>{" "}
                selected · {formatBytes(selectedBytes)}
                {rules.length > 0 && (
                  <> · rules: {rules.map(([r, n]) => `${r} (${n})`).join(", ")}</>
                )}
              </>
            ) : (
              "Select files to organize."
            )}
            {s && s.foldersLeft > 0 && <> · {plural(s.foldersLeft, "folder")} left as is</>}
          </p>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={() => setPage("dashboard")}>
              Cancel
            </Button>
            <Button onClick={savePlan} disabled={!plan || selected.size === 0}>
              <Save className="size-4" /> Save Plan
            </Button>
            <Button variant="primary" onClick={run} disabled={selected.size === 0}>
              <Wand2 className="size-4" /> Organize Selected ({formatCount(selected.size)})
            </Button>
          </div>
        </div>
        {plan && (
          <p className="-mt-2 flex items-center gap-1.5 text-[12px] text-subtle">
            <History className="size-3.5" /> Preview created {formatDateTime(plan.createdAt)}. Files
            are re-checked right before they move.
          </p>
        )}
      </div>
    </div>
  );
}
