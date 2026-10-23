// Typed wrappers around Tauri commands. The frontend never touches the
// filesystem directly: every path here is relative to the authorized folder
// and is validated again by the Rust AuthorizedRootGuard.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "@/types/api";

export const EVENTS = {
  organizeProgress: "tucklane://organize-progress",
  duplicatesProgress: "tucklane://duplicates-progress",
  monitor: "tucklane://monitor-status",
  changed: "tucklane://data-changed",
  navigate: "tucklane://navigate",
} as const;

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as T.ApiError).message);
  return String(e);
}

export function errorCode(e: unknown): string | undefined {
  if (e && typeof e === "object" && "code" in e) return String((e as T.ApiError).code);
  return undefined;
}

export function on<P>(event: string, cb: (payload: P) => void): Promise<UnlistenFn> {
  return listen<P>(event, (e) => cb(e.payload));
}

export const api = {
  // App & dashboard
  appStatus: () => invoke<T.AppStatus>("get_app_status"),
  dashboard: () => invoke<T.DashboardStats>("get_dashboard_stats"),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),

  // Folder authorization
  defaultDownloads: () => invoke<string | null>("get_default_downloads_folder"),
  validateRoot: (path: string) => invoke<T.FolderValidation>("validate_authorized_root", { path }),
  selectRoot: (path: string) => invoke<T.RootInfo>("select_authorized_root", { path }),

  // Organize
  scan: () => invoke<T.ScanSummary>("scan_downloads"),
  preview: (only?: string[]) => invoke<T.Plan>("preview_organization", { only: only ?? null }),
  execute: (planId: string, itemIds: string[]) =>
    invoke<T.ExecReport>("execute_organization", { planId, itemIds }),
  organizeNow: () => invoke<T.ExecReport>("organize_now"),
  cancelOrganize: () => invoke<void>("cancel_organization"),
  savePlan: (planId: string, itemIds: string[]) =>
    invoke<T.SavedPlanInfo>("save_plan", { planId, itemIds }),
  savedPlans: () => invoke<T.SavedPlanInfo[]>("list_saved_plans"),
  loadSavedPlan: (id: string) => invoke<T.Plan>("load_saved_plan", { id }),
  deleteSavedPlan: (id: string) => invoke<void>("delete_saved_plan", { id }),

  // History
  history: (query: T.HistoryQuery) => invoke<T.HistoryPage>("get_operation_history", { query }),
  sessions: (limit = 50, offset = 0) =>
    invoke<T.SessionRecord[]>("get_sessions", { limit, offset }),
  sessionOps: (sessionId: string) =>
    invoke<T.OperationRecord[]>("get_session_operations", { sessionId }),
  fileHistory: (rel: string) => invoke<T.OperationRecord[]>("get_file_history", { rel }),
  undo: (operationIds: string[], options?: T.UndoOptions) =>
    invoke<T.UndoReport>("undo_operations", { operationIds, options: options ?? null }),
  undoSession: (sessionId: string, options?: T.UndoOptions) =>
    invoke<T.UndoReport>("undo_session", { sessionId, options: options ?? null }),
  retry: (operationIds: string[]) => invoke<T.ExecReport>("retry_operations", { operationIds }),
  events: (p: { limit?: number; offset?: number; search?: string; severity?: string }) =>
    invoke<T.EventRecord[]>("get_activity_events", {
      limit: p.limit ?? 100,
      offset: p.offset ?? 0,
      search: p.search || null,
      severity: p.severity || null,
    }),
  exportCsv: (query: T.HistoryQuery) => invoke<string>("export_activity_csv", { query }),

  // Rules
  rules: () => invoke<T.Rule[]>("get_rules"),
  createRule: (draft: T.RuleDraft) => invoke<T.Rule>("create_rule", { draft }),
  updateRule: (id: string, draft: T.RuleDraft) => invoke<T.Rule>("update_rule", { id, draft }),
  deleteRule: (id: string) => invoke<void>("delete_rule", { id }),
  reorderRules: (ids: string[]) => invoke<T.Rule[]>("reorder_rules", { ids }),
  testRule: (draft: T.RuleDraft, ruleId?: string) =>
    invoke<T.RuleTestMatch[]>("test_rule", { draft, ruleId: ruleId ?? null }),
  exportRules: () => invoke<string>("export_rules"),
  importRules: (json: string) => invoke<number>("import_rules", { json }),

  // Duplicates
  findDuplicates: () => invoke<T.DuplicateReport>("find_duplicates"),
  cancelDuplicates: () => invoke<void>("cancel_duplicate_scan"),
  duplicates: () => invoke<T.DuplicateReport>("get_duplicate_groups"),
  recycle: (rels: string[]) =>
    invoke<T.RecycleResult[]>("recycle_duplicates", { rels, confirmed: true }),

  // Files
  listDir: (rel: string) => invoke<T.DirListing>("list_directory", { rel }),
  search: (query: string, limit = 500) =>
    invoke<T.ExplorerEntry[]>("search_files", { query, limit }),
  openFile: (rel: string, confirmedExecutable = false) =>
    invoke<void>("open_file", { rel, confirmedExecutable }),
  reveal: (rel: string) => invoke<void>("reveal_in_explorer", { rel }),
  openRoot: () => invoke<void>("open_root_folder"),
  openProjectPage: () => invoke<void>("open_project_page"),
  absolutePath: (rel: string) => invoke<string>("get_absolute_path", { rel }),
  thumbnail: (rel: string) => invoke<string | null>("get_thumbnail", { rel }),
  exclude: (rels: string[]) => invoke<void>("exclude_files", { rels }),
  include: (rels: string[]) => invoke<void>("include_files", { rels }),
  exclusions: () => invoke<T.ExclusionRecord[]>("get_exclusions"),

  // Settings
  settings: () => invoke<T.Settings>("get_settings"),
  updateSettings: (settings: T.Settings) => invoke<T.Settings>("update_settings", { settings }),
  presets: () => invoke<T.PresetInfo[]>("get_presets"),
  quit: () => invoke<void>("quit_app"),

  // Monitoring
  monitorStatus: () => invoke<T.MonitorStatus>("get_monitoring_status"),
  pauseMonitoring: () => invoke<T.MonitorStatus>("pause_monitoring"),
  resumeMonitoring: () => invoke<T.MonitorStatus>("resume_monitoring"),
  rescan: () => invoke<void>("rescan_now"),
};
