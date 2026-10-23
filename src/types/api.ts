// Mirrors the serde types returned by the Rust backend (src-tauri/src).

export type Theme = "system" | "light" | "dark";
export type Preset = "simple" | "balanced" | "advanced";
export type CollisionPolicy = "rename" | "skip" | "ask";
export type UnknownFiles = "leave" | "move_to_other";

export interface Settings {
  onboardingComplete: boolean;
  theme: Theme;
  language: string;
  launchAtStartup: boolean;
  closeToTray: boolean;
  showTrayIcon: boolean;
  notifications: boolean;
  startMinimized: boolean;
  preset: Preset;
  previewBeforeOrganize: boolean;
  autoOrganize: boolean;
  monitoringPaused: boolean;
  settleSeconds: number;
  scanIntervalMinutes: number;
  unknownFiles: UnknownFiles;
  collision: CollisionPolicy;
  ignoredExtensions: string[];
  protectedPatterns: string[];
  duplicateDetection: boolean;
  confirmDestructive: boolean;
  historyRetentionDays: number;
  smartFilenameHints: boolean;
  contentSignatures: boolean;
}

export type MonitorState = "off" | "watching" | "paused" | "error";

export interface MonitorStatus {
  state: MonitorState;
  pending: number;
  lastScanAt: string | null;
  nextScanAt: string | null;
  lastEventAt: string | null;
  lastError: string | null;
}

export interface RootInfo {
  id: string;
  path: string;
  selectedAt: string;
}

export interface RecoveryReport {
  sessionsInterrupted: number;
  recoveredCompleted: number;
  recoveredUntouched: number;
  needsAttention: number;
  stagingRemoved: number;
}

export interface AppStatus {
  version: string;
  root: RootInfo | null;
  rootAvailable: boolean;
  rootError: string | null;
  settings: Settings;
  monitor: MonitorStatus;
  startupNotes: string[];
  recovery: RecoveryReport | null;
}

export interface FolderValidation {
  ok: boolean;
  path: string;
  message: string | null;
}

export type FileKind =
  | "pdf"
  | "word"
  | "text"
  | "ebook"
  | "spreadsheet"
  | "presentation"
  | "report"
  | "image"
  | "video"
  | "audio"
  | "subtitle"
  | "installer"
  | "package"
  | "archive"
  | "code"
  | "data"
  | "script"
  | "notebook"
  | "torrent"
  | "font"
  | "disk_image";

export type ClassSource =
  | "exclusion"
  | "protected"
  | "ignored_extension"
  | "rule"
  | "signature"
  | "filename_hint"
  | "extension"
  | "unknown";

export type Decision =
  | { type: "move"; folder: string; confirm: boolean }
  | { type: "ignore" }
  | { type: "review" };

export interface Classification {
  decision: Decision;
  kind: FileKind | null;
  kindLabel: string | null;
  source: ClassSource;
  reason: string;
  confidence: number;
  ruleId: string | null;
  ruleName: string | null;
  alsoMatched: string[];
}

export type PlanStatus =
  | "ready"
  | "renamed"
  | "needs_decision"
  | "confirm"
  | "pending"
  | "locked"
  | "review"
  | "ignored"
  | "skipped";

export interface PlanItem {
  id: string;
  sourceRel: string;
  name: string;
  size: number;
  modifiedMs: number;
  destDir: string | null;
  destName: string | null;
  status: PlanStatus;
  selected: boolean;
  note: string | null;
  classification: Classification;
}

export interface PlanSummary {
  filesScanned: number;
  bytesScanned: number;
  foldersLeft: number;
  hiddenSkipped: number;
  linksSkipped: number;
  proposed: number;
  conflicts: number;
  pending: number;
  locked: number;
  review: number;
  ignored: number;
  skipped: number;
  categories: number;
  byFolder: Record<string, number>;
  byRule: Record<string, number>;
}

export interface Plan {
  id: string;
  createdAt: string;
  items: PlanItem[];
  summary: PlanSummary;
}

export interface ScanSummary {
  planId: string;
  totalFiles: number;
  totalBytes: number;
  categories: number;
  suggestedChanges: number;
  unclassified: number;
  inUse: number;
  pending: number;
  possibleDuplicateGroups: number;
  byFolder: [string, number][];
}

export interface Progress {
  sessionId: string;
  total: number;
  done: number;
  completed: number;
  skipped: number;
  failed: number;
  current: string | null;
  finished: boolean;
}

export type SessionKind = "manual" | "auto" | "undo" | "retry";

export interface SessionRecord {
  id: string;
  kind: SessionKind;
  status: "running" | "completed" | "cancelled" | "failed" | "interrupted";
  startedAt: string;
  endedAt: string | null;
  total: number;
  completed: number;
  skipped: number;
  failed: number;
  undoneAt: string | null;
  undoable: number;
}

export interface ExecReport {
  session: SessionRecord;
  cancelled: boolean;
  errors: string[];
}

export type OpKind = "move" | "undo" | "recycle";
export type OpState =
  | "planned"
  | "in_progress"
  | "completed"
  | "skipped"
  | "failed"
  | "undone"
  | "recovery_required";

export interface OperationRecord {
  id: string;
  sessionId: string;
  kind: OpKind;
  state: OpState;
  sourceRel: string;
  destDirRel: string;
  destRel: string | null;
  originalName: string;
  finalName: string | null;
  size: number | null;
  mtimeMs: number | null;
  fileId: string | null;
  sha256: string | null;
  method: string | null;
  ruleId: string | null;
  ruleName: string | null;
  reason: string | null;
  error: string | null;
  undoOf: string | null;
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
}

export interface HistoryQuery {
  search?: string;
  state?: string;
  kind?: string;
  sessionId?: string;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
}

export interface HistoryPage {
  items: OperationRecord[];
  total: number;
}

export type UndoStatus =
  | "restored"
  | "missing"
  | "modified"
  | "conflict"
  | "not_undoable"
  | "failed";

export interface UndoResult {
  operationId: string;
  name: string;
  status: UndoStatus;
  message: string;
  restoredRel: string | null;
}

export interface UndoReport {
  sessionId: string | null;
  results: UndoResult[];
}

export interface UndoOptions {
  onConflict?: "ask" | "rename";
  allowModified?: boolean;
}

export interface EventRecord {
  id: number;
  severity: "info" | "warning" | "error";
  kind: string;
  message: string;
  sessionId: string | null;
  createdAt: string;
}

export type Condition =
  | { type: "extension_is"; values: string[] }
  | { type: "name_contains"; value: string }
  | { type: "name_starts_with"; value: string }
  | { type: "name_ends_with"; value: string }
  | { type: "name_matches"; pattern: string }
  | { type: "size_greater_than"; bytes: number }
  | { type: "size_less_than"; bytes: number }
  | { type: "modified_within_days"; days: number }
  | { type: "modified_older_than_days"; days: number }
  | { type: "created_within_days"; days: number }
  | { type: "kind_is"; kind: FileKind };

export type RuleAction =
  | { type: "move_to"; folder: string; confirm: boolean }
  | { type: "ignore" }
  | { type: "review" };

export interface RuleDraft {
  name: string;
  description: string;
  enabled: boolean;
  matchMode: "all" | "any";
  conditions: Condition[];
  action: RuleAction;
}

export interface Rule extends RuleDraft {
  id: string;
  priority: number;
  createdAt: string;
  updatedAt: string;
  lastTriggeredAt: string | null;
  matchCount: number;
}

export interface RuleTestMatch {
  rel: string;
  name: string;
  size: number;
  decides: boolean;
  decidedBy: string | null;
  destination: string | null;
}

export interface DuplicateMember {
  rel: string;
  name: string;
  size: number;
  modifiedMs: number;
}

export interface DuplicateGroup {
  id: string;
  hash: string;
  size: number;
  members: DuplicateMember[];
  scannedAt: string;
}

export interface DupSummary {
  groups: number;
  duplicateFiles: number;
  recoverableBytes: number;
  scannedAt: string | null;
}

export interface DuplicateReport {
  summary: DupSummary;
  groups: DuplicateGroup[];
}

export interface DupProgress {
  phase: "scanning" | "fingerprinting" | "verifying" | "done";
  filesSeen: number;
  candidates: number;
  hashed: number;
  toHash: number;
  finished: boolean;
}

export interface RecycleResult {
  rel: string;
  ok: boolean;
  message: string;
}

export interface ExplorerEntry {
  rel: string;
  name: string;
  isDir: boolean;
  size: number;
  modifiedMs: number;
  createdMs: number;
  hidden: boolean;
  isLink: boolean;
  ext: string | null;
  kind: FileKind | null;
  kindLabel: string | null;
  suggestedFolder: string | null;
  excluded: boolean;
}

export interface DirListing {
  rel: string;
  entries: ExplorerEntry[];
  total: number;
}

export interface HealthFactor {
  label: string;
  count: number;
  penalty: number;
}

export interface DashboardStats {
  filesScanned: number;
  bytesReviewed: number;
  filesOrganized: number;
  readyToOrganize: number;
  awaitingReview: number;
  pendingDownloads: number;
  unresolvedFailures: number;
  duplicateGroups: number;
  duplicateBytes: number;
  lastScanAt: string | null;
  healthScore: number;
  healthFactors: HealthFactor[];
  recent: OperationRecord[];
}

export interface SavedPlanInfo {
  id: string;
  createdAt: string;
  itemCount: number;
}

export interface PresetInfo {
  id: Preset;
  folders: string[];
}

export interface ExclusionRecord {
  relPath: string;
  reason: string;
  createdAt: string;
}

export interface ApiError {
  code: string;
  message: string;
}
