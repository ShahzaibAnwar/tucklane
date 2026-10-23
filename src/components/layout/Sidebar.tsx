import { useState } from "react";
import { CircleHelp, Pause, Play, Sparkles } from "lucide-react";
import { useApp } from "@/stores/app";
import { api, errorMessage } from "@/lib/api";
import { cn } from "@/lib/cn";
import { LogoMark } from "@/components/common/Logo";
import { Tooltip } from "@/components/ui/primitives";
import { toast } from "@/components/ui/toast";
import { HelpDialog } from "@/components/layout/HelpDialog";
import { NAV } from "@/components/layout/nav";

function MonitorPill() {
  const status = useApp((s) => s.status);
  const refresh = useApp((s) => s.refresh);
  const [busy, setBusy] = useState(false);
  if (!status) return null;
  const m = status.monitor;
  const auto = status.settings.autoOrganize;
  const label =
    m.state === "watching"
      ? auto
        ? "Auto-organizing"
        : "Watching"
      : m.state === "paused"
        ? "Paused"
        : m.state === "error"
          ? "Folder unavailable"
          : "Monitoring off";
  const dot =
    m.state === "watching"
      ? "bg-success"
      : m.state === "paused"
        ? "bg-warning"
        : m.state === "error"
          ? "bg-danger"
          : "bg-subtle";
  const paused = m.state === "paused";
  const toggle = async () => {
    setBusy(true);
    try {
      if (paused) await api.resumeMonitoring();
      else await api.pauseMonitoring();
      await refresh();
    } catch (e) {
      toast.error("Could not change monitoring", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex items-center justify-between gap-2 rounded-lg bg-surface-2 px-2.5 py-2">
      <div className="min-w-0">
        <div className="flex items-center gap-2 text-[13px] font-medium">
          <span
            className={cn(
              "size-2 shrink-0 rounded-full",
              dot,
              m.state === "watching" && "animate-pulse",
            )}
            aria-hidden
          />
          <span className="truncate">{label}</span>
        </div>
        {m.pending > 0 && (
          <p className="mt-0.5 pl-4 text-[12px] text-muted">{m.pending} settling</p>
        )}
        {m.state === "error" && m.lastError && (
          <p className="mt-0.5 pl-4 text-[12px] text-danger">{m.lastError}</p>
        )}
      </div>
      {(m.state === "watching" || m.state === "paused") && (
        <Tooltip content={paused ? "Resume monitoring" : "Pause monitoring"}>
          <button
            onClick={toggle}
            disabled={busy}
            className="rounded-md p-1.5 text-muted hover:bg-surface-3 hover:text-fg disabled:opacity-50"
            aria-label={paused ? "Resume monitoring" : "Pause monitoring"}
          >
            {paused ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
          </button>
        </Tooltip>
      )}
    </div>
  );
}

export function Sidebar() {
  const page = useApp((s) => s.page);
  const setPage = useApp((s) => s.setPage);
  const status = useApp((s) => s.status);
  const [help, setHelp] = useState(false);
  return (
    <aside
      className="flex w-[232px] shrink-0 flex-col border-r border-border bg-surface/60"
      aria-label="Main navigation"
    >
      <div className="flex items-center gap-2.5 px-4 pt-5 pb-4">
        <LogoMark size={30} />
        <div>
          <div className="font-display text-[16px] leading-tight font-semibold tracking-tight">
            Tucklane
          </div>
          <div className="text-[11.5px] text-muted">A quieter place for your downloads</div>
        </div>
      </div>
      <nav className="flex flex-col gap-0.5 px-2.5">
        {NAV.map((n, i) => {
          const active = page === n.page;
          return (
            <button
              key={n.page}
              onClick={() => setPage(n.page)}
              aria-current={active ? "page" : undefined}
              title={`${n.label} (Ctrl+${i + 1})`}
              className={cn(
                "group flex h-9 items-center gap-3 rounded-lg px-3 text-[13.5px] font-medium transition-colors",
                active
                  ? "bg-accent-soft text-accent-strong"
                  : "text-muted hover:bg-surface-2 hover:text-fg",
              )}
            >
              <n.icon className="size-[17px]" aria-hidden />
              {n.label}
            </button>
          );
        })}
      </nav>
      <div className="mt-auto flex flex-col gap-2 p-3">
        {status?.root && (
          <Tooltip content={status.root.path} side="right">
            <button
              onClick={() =>
                api.openRoot().catch((e) => toast.error("Could not open folder", errorMessage(e)))
              }
              className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left hover:bg-surface-2"
            >
              <Sparkles className="size-3.5 shrink-0 text-accent-strong" aria-hidden />
              <div className="min-w-0">
                <div className="text-[11px] tracking-wide text-subtle uppercase">Managing</div>
                <div className="truncate text-[12.5px] font-medium" dir="ltr">
                  {status.root.path}
                </div>
              </div>
            </button>
          </Tooltip>
        )}
        <MonitorPill />
        <div className="flex items-center justify-between px-1 pt-1 text-[12px] text-subtle">
          <button
            onClick={() => setHelp(true)}
            className="flex items-center gap-1.5 rounded px-1 py-0.5 hover:text-fg"
          >
            <CircleHelp className="size-3.5" /> Help
          </button>
          <span>v{status?.version ?? "…"}</span>
        </div>
      </div>
      <HelpDialog open={help} onOpenChange={setHelp} />
    </aside>
  );
}
