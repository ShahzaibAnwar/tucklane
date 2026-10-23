import { useEffect, useState } from "react";
import { Tooltip as RTooltip } from "radix-ui";
import { AlertTriangle, Loader2, X } from "lucide-react";
import { EVENTS } from "@/lib/api";
import { useBackendEvent } from "@/hooks/useData";
import { useApp, type Page } from "@/stores/app";
import type { MonitorStatus } from "@/types/api";
import { Sidebar } from "@/components/layout/Sidebar";
import { NAV } from "@/components/layout/nav";
import { Toaster } from "@/components/ui/toast";
import { UndoDialogHost } from "@/components/common/undo";
import { useTheme } from "@/hooks/useTheme";
import { Onboarding } from "@/pages/Onboarding";
import { Dashboard } from "@/pages/Dashboard";
import { Explorer } from "@/pages/Explorer";
import { Organize } from "@/pages/Organize";
import { Rules } from "@/pages/Rules";
import { Duplicates } from "@/pages/Duplicates";
import { ActivityPage } from "@/pages/Activity";
import { SettingsPage } from "@/pages/Settings";

const PAGES: Record<Page, () => React.ReactElement> = {
  dashboard: Dashboard,
  explorer: Explorer,
  organize: Organize,
  rules: Rules,
  duplicates: Duplicates,
  activity: ActivityPage,
  settings: SettingsPage,
};

function Notices() {
  const status = useApp((s) => s.status);
  const setPage = useApp((s) => s.setPage);
  const [hidden, setHidden] = useState(false);
  if (!status || hidden) return null;
  const notes: string[] = [...status.startupNotes];
  const r = status.recovery;
  if (r && r.needsAttention > 0)
    notes.push(
      `Tucklane was interrupted last time. ${r.needsAttention} file operation${r.needsAttention === 1 ? "" : "s"} need${r.needsAttention === 1 ? "s" : ""} your attention in Activity.`,
    );
  if (status.root && !status.rootAvailable)
    notes.push(
      status.rootError ??
        "The authorized folder is not available. Reconnect the drive or choose it again in Settings.",
    );
  if (!status.root && status.rootError) notes.push(status.rootError);
  if (notes.length === 0) return null;
  return (
    <div
      className="flex items-start gap-3 border-b border-border bg-warning-soft px-6 py-2.5 text-[13px] text-warning"
      role="alert"
    >
      <AlertTriangle className="mt-0.5 size-4 shrink-0" />
      <div className="flex-1 space-y-0.5">
        {notes.map((n) => (
          <p key={n}>{n}</p>
        ))}
        {r && r.needsAttention > 0 && (
          <button className="font-semibold underline" onClick={() => setPage("activity")}>
            Review in Activity
          </button>
        )}
      </div>
      <button
        onClick={() => setHidden(true)}
        aria-label="Dismiss"
        className="rounded p-0.5 hover:bg-black/5"
      >
        <X className="size-3.5" />
      </button>
    </div>
  );
}

export function App() {
  const { status, loadError, page, refresh, bump, setMonitor, setPage } = useApp();
  useTheme(status?.settings.theme ?? "system");

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useBackendEvent<string>(EVENTS.changed, () => bump());
  useBackendEvent<MonitorStatus>(EVENTS.monitor, (m) => setMonitor(m));
  useBackendEvent<string>(EVENTS.navigate, (p) => {
    if (p in PAGES) setPage(p as Page);
  });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.altKey && !e.shiftKey) {
        const n = Number(e.key);
        if (n >= 1 && n <= NAV.length) {
          e.preventDefault();
          setPage(NAV[n - 1].page);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setPage]);

  if (loadError && !status) {
    return (
      <div className="flex h-full items-center justify-center p-10 text-center">
        <div>
          <AlertTriangle className="mx-auto mb-3 size-8 text-danger" />
          <h1 className="text-lg font-semibold">Tucklane could not load</h1>
          <p className="mt-1 max-w-md text-sm text-muted">{loadError}</p>
          <button
            className="mt-4 text-sm font-semibold text-accent-strong"
            onClick={() => void refresh()}
          >
            Try again
          </button>
        </div>
      </div>
    );
  }
  if (!status) {
    return (
      <div className="flex h-full items-center justify-center text-muted" aria-busy>
        <Loader2 className="size-5 animate-spin" />
      </div>
    );
  }

  const Page = PAGES[page];
  return (
    <RTooltip.Provider>
      {!status.settings.onboardingComplete ? (
        <Onboarding />
      ) : (
        <div className="flex h-full">
          <Sidebar />
          <main className="flex min-w-0 flex-1 flex-col">
            <Notices />
            <div className="min-h-0 flex-1 overflow-auto" key={page}>
              <Page />
            </div>
          </main>
        </div>
      )}
      <UndoDialogHost />
      <Toaster />
    </RTooltip.Provider>
  );
}
