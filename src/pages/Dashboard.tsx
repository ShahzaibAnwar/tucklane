import { useEffect, useRef, useState } from "react";
import {
  ArrowRight,
  Copy,
  Eye,
  FolderOpen,
  Inbox,
  ListPlus,
  Pause,
  Play,
  RefreshCw,
  Settings as SettingsIcon,
  Undo2,
  Wand2,
} from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatCount, timeAgo, winPath } from "@/lib/format";
import { useData } from "@/hooks/useData";
import { useApp } from "@/stores/app";
import { PageBody, PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import { Card, CardHeader, EmptyState, Tooltip } from "@/components/ui/primitives";
import { FileIcon } from "@/components/common/FileIcon";
import { OpBadge } from "@/components/common/OpBadge";
import { undoOperations, undoSession } from "@/components/common/undo";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";
import type { DashboardStats } from "@/types/api";

function subtitle(d: DashboardStats | null, monitoring: string): string {
  if (!d) return "Checking your folder…";
  if (d.filesScanned === 0 && !d.lastScanAt) return "Run a scan to see how your folder is doing.";
  const parts: string[] = [];
  if (d.readyToOrganize > 0)
    parts.push(
      `${formatCount(d.readyToOrganize)} loose file${d.readyToOrganize === 1 ? "" : "s"} ready to organize`,
    );
  if (d.awaitingReview > 0) parts.push(`${formatCount(d.awaitingReview)} waiting for your review`);
  if (parts.length === 0) parts.push("Everything at the top of your folder is in order");
  return `${parts.join(" · ")}. ${monitoring}`;
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <Card className="px-4 py-3.5">
      <div className="tl-num font-display text-[26px] leading-tight font-semibold tracking-tight">
        {value}
      </div>
      <div className="mt-0.5 text-[13px] text-muted">{label}</div>
      {hint && <div className="mt-1 text-[11.5px] text-subtle">{hint}</div>}
    </Card>
  );
}

function HealthRing({ score }: { score: number }) {
  const r = 42;
  const c = 2 * Math.PI * r;
  const color = score >= 80 ? "var(--success)" : score >= 50 ? "var(--warning)" : "var(--danger)";
  return (
    <svg
      width="108"
      height="108"
      viewBox="0 0 108 108"
      role="img"
      aria-label={`Organization score ${score} out of 100`}
    >
      <circle cx="54" cy="54" r={r} fill="none" stroke="var(--surface-3)" strokeWidth="10" />
      <circle
        cx="54"
        cy="54"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="10"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - score / 100)}
        transform="rotate(-90 54 54)"
        style={{ transition: "stroke-dashoffset 400ms ease" }}
      />
      <text
        x="54"
        y="58"
        textAnchor="middle"
        className="tl-num"
        fontSize="24"
        fontWeight="600"
        fill="var(--fg)"
      >
        {score}
      </text>
    </svg>
  );
}

export function Dashboard() {
  const status = useApp((s) => s.status)!;
  const setPage = useApp((s) => s.setPage);
  const refresh = useApp((s) => s.refresh);
  const { data, error, reload } = useData(api.dashboard);
  const [scanning, setScanning] = useState(false);
  const [organizing, setOrganizing] = useState(false);
  const settings = status.settings;
  const m = status.monitor;

  const scan = async () => {
    setScanning(true);
    try {
      await api.scan();
      await reload();
    } catch (e) {
      toast.error("Scan failed", errorMessage(e));
    } finally {
      setScanning(false);
    }
  };

  const autoScanned = useRef(false);
  useEffect(() => {
    // First visit with no index yet: scan once so the numbers are real.
    // (An empty folder never gets a last-scan time, so guard against looping.)
    if (data && !data.lastScanAt && !autoScanned.current) {
      autoScanned.current = true;
      void scan();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data?.lastScanAt]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F5") {
        e.preventDefault();
        void scan();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const organize = async () => {
    if (settings.previewBeforeOrganize) {
      setPage("organize");
      return;
    }
    setOrganizing(true);
    try {
      const rep = await api.organizeNow();
      toast.success(
        `${rep.session.completed} files organized`,
        rep.session.skipped ? `${rep.session.skipped} skipped.` : undefined,
        {
          label: "Undo this session",
          run: () => void undoSession(rep.session.id),
        },
      );
    } catch (e) {
      toast.info("Nothing organized", errorMessage(e));
    } finally {
      setOrganizing(false);
    }
  };

  const monitoringText =
    m.state === "watching"
      ? settings.autoOrganize
        ? "New downloads are organized automatically."
        : "Watching for new downloads."
      : m.state === "paused"
        ? "Monitoring is paused."
        : m.state === "error"
          ? "The folder is unavailable."
          : "Monitoring is off.";

  return (
    <>
      <PageHeader
        title="Your downloads, under control."
        subtitle={error ? error : subtitle(data, monitoringText)}
        actions={
          <>
            <Button onClick={scan} loading={scanning} title="Scan now (F5)">
              {!scanning && <RefreshCw className="size-4" />} Scan Now
            </Button>
            <Button variant="primary" onClick={organize} loading={organizing}>
              {!organizing && <Wand2 className="size-4" />} Organize
            </Button>
            <Tooltip content="Settings">
              <Button
                variant="ghost"
                size="icon"
                onClick={() => setPage("settings")}
                aria-label="Settings"
              >
                <SettingsIcon className="size-4" />
              </Button>
            </Tooltip>
          </>
        }
      />
      <PageBody>
        <div className="grid grid-cols-2 gap-3 lg:grid-cols-5">
          <Stat
            label="Files scanned"
            value={data ? formatCount(data.filesScanned) : "—"}
            hint={data?.lastScanAt ? `Last scan ${timeAgo(data.lastScanAt)}` : undefined}
          />
          <Stat
            label="Files organized"
            value={data ? formatCount(data.filesOrganized) : "—"}
            hint="All time, including undone"
          />
          <Stat
            label="Space reviewed"
            value={data ? formatBytes(data.bytesReviewed) : "—"}
            hint="Loose files at the top level"
          />
          <Stat
            label="Potential duplicate space"
            value={data ? (data.duplicateGroups ? formatBytes(data.duplicateBytes) : "—") : "—"}
            hint={
              data?.duplicateGroups
                ? `${data.duplicateGroups} verified groups`
                : "Run a duplicate scan"
            }
          />
          <Stat
            label="Awaiting review"
            value={data ? formatCount(data.awaitingReview + data.pendingDownloads) : "—"}
            hint={data ? `${data.pendingDownloads} still downloading or in use` : undefined}
          />
        </div>

        <div className="grid gap-5 xl:grid-cols-[1fr_340px]">
          <Card>
            <CardHeader
              title="Recent activity"
              description="The latest files Tucklane moved. Undo any of them."
              action={
                <Button variant="ghost" size="sm" onClick={() => setPage("activity")}>
                  View all <ArrowRight className="size-3.5" />
                </Button>
              }
            />
            {data && data.recent.length === 0 && (
              <EmptyState icon={<Inbox className="size-5" />} title="Nothing moved yet">
                When you organize files, each move appears here with an undo button.
              </EmptyState>
            )}
            <ul className="divide-y divide-border">
              {data?.recent.map((op) => (
                <li key={op.id} className="flex items-center gap-3 px-5 py-2.5">
                  <FileIcon name={op.originalName} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[13.5px] font-medium">
                      {op.finalName ?? op.originalName}
                    </div>
                    <div className="truncate text-[12px] text-muted">
                      {op.state === "completed" || op.state === "undone"
                        ? `→ ${winPath(op.destDirRel)}`
                        : (op.error ?? op.reason)}
                    </div>
                  </div>
                  <span className="text-[12px] whitespace-nowrap text-subtle">
                    {timeAgo(op.completedAt ?? op.updatedAt)}
                  </span>
                  <OpBadge state={op.state} kind={op.kind} />
                  <Tooltip content="Undo this move">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={`Undo move of ${op.originalName}`}
                      disabled={op.state !== "completed" || op.kind !== "move"}
                      onClick={() => void undoOperations([op.id])}
                    >
                      <Undo2 className="size-4" />
                    </Button>
                  </Tooltip>
                </li>
              ))}
            </ul>
          </Card>

          <div className="flex flex-col gap-5">
            <Card>
              <CardHeader
                title="Folder health"
                description="How tidy the top of your folder is — not a measure of your PC."
              />
              <div className="flex items-center gap-4 px-5 pb-4">
                <HealthRing score={data?.healthScore ?? 100} />
                <ul className="flex-1 space-y-1.5 text-[12.5px]">
                  {data?.healthFactors.map((f) => (
                    <li key={f.label} className="flex justify-between gap-2">
                      <span className={cn(f.penalty ? "text-fg" : "text-subtle")}>
                        {f.label} <span className="tl-num text-muted">({f.count})</span>
                      </span>
                      <span className={cn("tl-num", f.penalty ? "text-warning" : "text-subtle")}>
                        {f.penalty ? `−${f.penalty}` : "0"}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            </Card>

            <Card>
              <CardHeader title="Monitoring" />
              <dl className="grid grid-cols-2 gap-y-2 px-5 pb-3 text-[13px]">
                <dt className="text-muted">Status</dt>
                <dd className="text-right font-medium capitalize">
                  {m.state === "watching" ? "On" : m.state}
                </dd>
                <dt className="text-muted">Automatic organizing</dt>
                <dd className="text-right font-medium">{settings.autoOrganize ? "On" : "Off"}</dd>
                <dt className="text-muted">Files settling</dt>
                <dd className="tl-num text-right font-medium">{m.pending}</dd>
                <dt className="text-muted">Last scan</dt>
                <dd className="text-right font-medium">{timeAgo(m.lastScanAt)}</dd>
                <dt className="text-muted">Next scheduled scan</dt>
                <dd className="text-right font-medium">
                  {m.nextScanAt ? timeAgo(m.nextScanAt) : "Not scheduled"}
                </dd>
              </dl>
              {m.lastError && (
                <p className="mx-5 mb-3 rounded-md bg-danger-soft px-2.5 py-1.5 text-[12.5px] text-danger">
                  {m.lastError}
                </p>
              )}
              <div className="flex gap-2 border-t border-border px-5 py-3">
                {(m.state === "watching" || m.state === "paused") && (
                  <Button
                    size="sm"
                    onClick={async () => {
                      try {
                        if (m.state === "paused") await api.resumeMonitoring();
                        else await api.pauseMonitoring();
                        await refresh();
                      } catch (e) {
                        toast.error("Could not change monitoring", errorMessage(e));
                      }
                    }}
                  >
                    {m.state === "paused" ? (
                      <Play className="size-3.5" />
                    ) : (
                      <Pause className="size-3.5" />
                    )}
                    {m.state === "paused" ? "Resume" : "Pause"}
                  </Button>
                )}
                {!settings.autoOrganize && (
                  <Button size="sm" variant="ghost" onClick={() => setPage("settings")}>
                    Turn on auto-organize
                  </Button>
                )}
              </div>
            </Card>
          </div>
        </div>

        <Card>
          <CardHeader title="Quick actions" />
          <div className="grid grid-cols-2 gap-2 px-5 pb-5 md:grid-cols-5">
            {[
              { icon: Wand2, label: "Organize Now", run: organize },
              { icon: Eye, label: "Preview Changes", run: () => setPage("organize") },
              { icon: Copy, label: "Find Duplicates", run: () => setPage("duplicates") },
              { icon: ListPlus, label: "Create Rule", run: () => setPage("rules") },
              {
                icon: FolderOpen,
                label: "Open Downloads Folder",
                run: () =>
                  api
                    .openRoot()
                    .catch((e) => toast.error("Could not open folder", errorMessage(e))),
              },
            ].map((a) => (
              <button
                key={a.label}
                onClick={() => void a.run()}
                className="flex items-center gap-2.5 rounded-xl border border-border bg-surface-2/60 px-3.5 py-3 text-left text-[13px] font-medium transition-colors hover:border-accent hover:bg-accent-soft"
              >
                <a.icon className="size-4 text-accent-strong" /> {a.label}
              </button>
            ))}
          </div>
        </Card>
      </PageBody>
    </>
  );
}
