import { mockIPC } from "@tauri-apps/api/mocks";
import type { AppStatus, Plan, PlanItem, Settings } from "@/types/api";
import { useApp } from "@/stores/app";

export const defaultSettings: Settings = {
  onboardingComplete: true,
  theme: "system",
  language: "en",
  launchAtStartup: false,
  closeToTray: true,
  showTrayIcon: true,
  notifications: true,
  startMinimized: true,
  preset: "balanced",
  previewBeforeOrganize: true,
  autoOrganize: false,
  monitoringPaused: false,
  settleSeconds: 15,
  scanIntervalMinutes: 30,
  unknownFiles: "leave",
  collision: "rename",
  ignoredExtensions: [],
  protectedPatterns: ["desktop.ini", "*.lnk"],
  duplicateDetection: true,
  confirmDestructive: true,
  historyRetentionDays: 180,
  smartFilenameHints: false,
  contentSignatures: false,
};

export function appStatus(over: Partial<AppStatus> = {}): AppStatus {
  return {
    version: "0.1.0",
    root: { id: "r1", path: "C:\\Users\\Test\\Downloads", selectedAt: "2026-10-01T00:00:00Z" },
    rootAvailable: true,
    rootError: null,
    settings: defaultSettings,
    monitor: {
      state: "watching",
      pending: 0,
      lastScanAt: null,
      nextScanAt: null,
      lastEventAt: null,
      lastError: null,
    },
    startupNotes: [],
    recovery: null,
    ...over,
  };
}

export function planItem(over: Partial<PlanItem>): PlanItem {
  return {
    id: over.name ?? "x",
    sourceRel: over.name ?? "x",
    name: "x",
    size: 1024,
    modifiedMs: 0,
    destDir: "Documents/PDF",
    destName: over.name ?? "x",
    status: "ready",
    selected: true,
    note: null,
    classification: {
      decision: { type: "move", folder: "Documents/PDF", confirm: false },
      kind: "pdf",
      kindLabel: "PDF document",
      source: "extension",
      reason: "PDF document (.pdf extension).",
      confidence: 1,
      ruleId: null,
      ruleName: null,
      alsoMatched: [],
    },
    ...over,
  };
}

export function plan(items: PlanItem[]): Plan {
  return {
    id: "plan-1",
    createdAt: "2026-10-23T10:00:00Z",
    items,
    summary: {
      filesScanned: items.length,
      bytesScanned: items.reduce((a, i) => a + i.size, 0),
      foldersLeft: 0,
      hiddenSkipped: 0,
      linksSkipped: 0,
      proposed: items.filter((i) =>
        ["ready", "renamed", "needs_decision", "confirm"].includes(i.status),
      ).length,
      conflicts: items.filter((i) => i.status === "renamed").length,
      pending: items.filter((i) => i.status === "pending").length,
      locked: 0,
      review: items.filter((i) => i.status === "review").length,
      ignored: 0,
      skipped: 0,
      categories: 1,
      byFolder: {},
      byRule: {},
    },
  };
}

type Handler = (args: Record<string, unknown>) => unknown;

/** Installs an IPC mock; records every call for assertions. */
export function mockBackend(handlers: Record<string, Handler>) {
  const calls: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (cmd.startsWith("plugin:event|")) return 1;
    const h = handlers[cmd];
    if (!h) throw { code: "not_mocked", message: `No mock for ${cmd}` };
    return h(a);
  });
  return calls;
}

export function seedStore(status: AppStatus) {
  useApp.setState({ status, loadError: null, page: "dashboard", tick: 0, organizeOnly: null });
}
