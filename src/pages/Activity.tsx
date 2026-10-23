import { useMemo, useState } from "react";
import {
  FileDown,
  Info,
  Layers,
  ListTree,
  RotateCcw,
  Search,
  Undo2,
  ScrollText,
} from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, formatDateTime, timeAgo, winPath, plural } from "@/lib/format";
import { useData } from "@/hooks/useData";
import { useApp } from "@/stores/app";
import type { HistoryQuery, OperationRecord, SessionRecord } from "@/types/api";
import { PageBody, PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Badge,
  Card,
  Checkbox,
  EmptyState,
  Input,
  Select,
  Tooltip,
} from "@/components/ui/primitives";
import { Dialog } from "@/components/ui/dialog";
import { FileIcon } from "@/components/common/FileIcon";
import { OpBadge } from "@/components/common/OpBadge";
import { undoOperations, undoSession } from "@/components/common/undo";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";

type Tab = "operations" | "sessions" | "events";
const PAGE = 100;

function DetailDialog({ op, onClose }: { op: OperationRecord | null; onClose: () => void }) {
  if (!op) return null;
  const rows: [string, string | null | undefined][] = [
    ["Operation", op.kind],
    ["Result", op.state.replace("_", " ")],
    ["Original path", winPath(op.sourceRel)],
    ["Destination", winPath(op.destRel ?? op.destDirRel)],
    ["Original name", op.originalName],
    ["Final name", op.finalName],
    ["Size", op.size != null ? formatBytes(op.size) : null],
    ["Modified", op.mtimeMs ? formatDateTime(op.mtimeMs) : null],
    ["Rule", op.ruleName],
    ["Reason", op.reason],
    [
      "Method",
      op.method === "copy_verify"
        ? "Copy and verify (SHA-256)"
        : op.method === "rename"
          ? "Atomic rename"
          : op.method,
    ],
    [
      "Verification",
      op.sha256 ? `SHA-256 ${op.sha256}` : op.fileId ? `File ID ${op.fileId}` : null,
    ],
    ["Error", op.error],
    ["Recorded", formatDateTime(op.createdAt)],
    ["Completed", op.completedAt ? formatDateTime(op.completedAt) : null],
    ["Session", op.sessionId],
  ];
  return (
    <Dialog open onOpenChange={(v) => !v && onClose()} title={op.finalName ?? op.originalName} wide>
      <dl className="grid grid-cols-[140px_1fr] gap-x-4 gap-y-1.5 text-[13px]" data-selectable>
        {rows
          .filter(([, v]) => v)
          .map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted">{k}</dt>
              <dd className="font-mono text-[12.5px] break-all">{v}</dd>
            </div>
          ))}
      </dl>
    </Dialog>
  );
}

function Operations({ initialSession }: { initialSession: string | null }) {
  const [search, setSearch] = useState("");
  const [state, setState] = useState("");
  const [kind, setKind] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [page, setPage] = useState(0);
  const [sessionId, setSessionId] = useState<string | null>(initialSession);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [detail, setDetail] = useState<OperationRecord | null>(null);
  const [group, setGroup] = useState(false);

  const query: HistoryQuery = useMemo(
    () => ({
      search: search || undefined,
      state: state || undefined,
      kind: kind || undefined,
      sessionId: sessionId || undefined,
      from: from ? new Date(from).toISOString() : undefined,
      to: to ? new Date(`${to}T23:59:59`).toISOString() : undefined,
      limit: PAGE,
      offset: page * PAGE,
    }),
    [search, state, kind, sessionId, from, to, page],
  );
  const { data, loading } = useData(() => api.history(query), [JSON.stringify(query)]);
  const items = useMemo(() => data?.items ?? [], [data]);
  const sel = items.filter((o) => selected.has(o.id));
  const undoable = sel.filter((o) => o.kind === "move" && o.state === "completed");
  const retryable = sel.filter(
    (o) => o.kind === "move" && (o.state === "failed" || o.state === "skipped"),
  );

  const exportCsv = async () => {
    try {
      const name = await api.exportCsv({ ...query, limit: undefined, offset: undefined });
      toast.success("Activity exported", `Saved as ${name} in your Downloads folder.`);
    } catch (e) {
      toast.error("Export failed", errorMessage(e));
    }
  };

  const retry = async () => {
    try {
      const r = await api.retry(retryable.map((o) => o.id));
      toast.success(
        `Retry: ${r.session.completed} moved`,
        r.session.failed || r.session.skipped
          ? `${r.session.skipped} skipped, ${r.session.failed} failed.`
          : undefined,
      );
      setSelected(new Set());
      useApp.getState().bump();
    } catch (e) {
      toast.error("Retry failed", errorMessage(e));
    }
  };

  const grouped = useMemo(() => {
    if (!group) return [{ key: "", ops: items }];
    const m = new Map<string, OperationRecord[]>();
    items.forEach((o) => m.set(o.sessionId, [...(m.get(o.sessionId) ?? []), o]));
    return [...m.entries()].map(([key, ops]) => ({ key, ops }));
  }, [items, group]);

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative w-64">
          <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-subtle" />
          <Input
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setPage(0);
            }}
            placeholder="Search filenames"
            className="pl-8"
            aria-label="Search filenames"
          />
        </div>
        <Select
          value={kind}
          onChange={(e) => {
            setKind(e.target.value);
            setPage(0);
          }}
          aria-label="Operation type"
        >
          <option value="">All operations</option>
          <option value="move">Moves</option>
          <option value="undo">Undos</option>
          <option value="recycle">Recycle Bin</option>
        </Select>
        <Select
          value={state}
          onChange={(e) => {
            setState(e.target.value);
            setPage(0);
          }}
          aria-label="Result"
        >
          <option value="">Any result</option>
          <option value="completed">Completed</option>
          <option value="skipped">Skipped</option>
          <option value="failed">Failed</option>
          <option value="undone">Undone</option>
          <option value="recovery_required">Needs attention</option>
        </Select>
        <label className="flex items-center gap-1.5 text-[13px] text-muted">
          From{" "}
          <Input
            type="date"
            value={from}
            onChange={(e) => {
              setFrom(e.target.value);
              setPage(0);
            }}
            className="w-36"
            aria-label="From date"
          />
        </label>
        <label className="flex items-center gap-1.5 text-[13px] text-muted">
          To{" "}
          <Input
            type="date"
            value={to}
            onChange={(e) => {
              setTo(e.target.value);
              setPage(0);
            }}
            className="w-36"
            aria-label="To date"
          />
        </label>
        <Button
          size="sm"
          variant={group ? "secondary" : "ghost"}
          onClick={() => setGroup(!group)}
          aria-pressed={group}
        >
          <Layers className="size-3.5" /> Group by session
        </Button>
        <div className="ml-auto">
          <Button size="sm" onClick={exportCsv}>
            <FileDown className="size-3.5" /> Export CSV
          </Button>
        </div>
      </div>
      {sessionId && (
        <div className="flex items-center gap-2 text-[13px]">
          <Badge tone="info">Filtered to one session</Badge>
          <button className="text-accent-strong hover:underline" onClick={() => setSessionId(null)}>
            Show all
          </button>
        </div>
      )}
      {sel.length > 0 && (
        <div className="flex items-center gap-2 rounded-lg border border-accent/40 bg-accent-soft px-3 py-2 text-[13px]">
          <span className="font-semibold">{sel.length} selected</span>
          <div className="ml-auto flex gap-1.5">
            <Button
              size="sm"
              variant="primary"
              disabled={!undoable.length}
              onClick={() =>
                void undoOperations(undoable.map((o) => o.id)).then(() => setSelected(new Set()))
              }
            >
              <Undo2 className="size-3.5" /> Undo {undoable.length || ""}
            </Button>
            <Button size="sm" disabled={!retryable.length} onClick={retry}>
              <RotateCcw className="size-3.5" /> Retry {retryable.length || ""}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
              Clear
            </Button>
          </div>
        </div>
      )}
      <Card>
        {!loading && items.length === 0 && (
          <EmptyState icon={<ListTree className="size-5" />} title="No file operations">
            Moves, undos and Recycle Bin actions appear here. Every move can be undone.
          </EmptyState>
        )}
        {grouped.map((g) => (
          <div key={g.key}>
            {g.key && (
              <div className="border-b border-border bg-surface-2/60 px-5 py-1.5 text-[12px] font-semibold text-muted">
                Session {g.key.slice(0, 8)} · {formatDateTime(g.ops[g.ops.length - 1].createdAt)}
              </div>
            )}
            <ul className="divide-y divide-border">
              {g.ops.map((o) => (
                <li
                  key={o.id}
                  className={cn(
                    "flex items-center gap-3 px-5 py-2.5 text-[13px]",
                    selected.has(o.id) && "bg-accent-soft/40",
                  )}
                >
                  <Checkbox
                    checked={selected.has(o.id)}
                    onCheckedChange={(v) => {
                      const s = new Set(selected);
                      if (v) s.add(o.id);
                      else s.delete(o.id);
                      setSelected(s);
                    }}
                    label={`Select ${o.originalName}`}
                  />
                  <FileIcon name={o.originalName} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{o.finalName ?? o.originalName}</div>
                    <div className="truncate text-[12px] text-muted">
                      {o.kind === "recycle"
                        ? "Moved to the Recycle Bin"
                        : `${winPath(o.sourceRel)} → ${winPath(o.destRel ?? o.destDirRel) || "Downloads"}`}
                      {o.error && <span className="text-warning"> · {o.error}</span>}
                    </div>
                  </div>
                  {o.ruleName && <Badge tone="accent">{o.ruleName}</Badge>}
                  <span
                    className="w-24 text-right text-[12px] text-subtle"
                    title={formatDateTime(o.createdAt)}
                  >
                    {timeAgo(o.createdAt)}
                  </span>
                  <OpBadge state={o.state} kind={o.kind} />
                  <Tooltip content="Details">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label="Details"
                      onClick={() => setDetail(o)}
                    >
                      <Info className="size-4" />
                    </Button>
                  </Tooltip>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </Card>
      {data && data.total > PAGE && (
        <div className="flex items-center justify-end gap-2 text-[13px] text-muted">
          {page * PAGE + 1}–{Math.min(data.total, (page + 1) * PAGE)} of {data.total}
          <Button size="sm" disabled={page === 0} onClick={() => setPage(page - 1)}>
            Previous
          </Button>
          <Button
            size="sm"
            disabled={(page + 1) * PAGE >= data.total}
            onClick={() => setPage(page + 1)}
          >
            Next
          </Button>
        </div>
      )}
      <DetailDialog op={detail} onClose={() => setDetail(null)} />
    </>
  );
}

const KIND_LABEL: Record<string, string> = {
  manual: "Organize",
  auto: "Automatic",
  undo: "Undo",
  retry: "Retry",
};

function Sessions({ onOpen }: { onOpen: (id: string) => void }) {
  const { data } = useData(() => api.sessions(100, 0));
  return (
    <Card>
      {data && data.length === 0 && (
        <EmptyState icon={<Layers className="size-5" />} title="No sessions yet" />
      )}
      <ul className="divide-y divide-border">
        {data?.map((s: SessionRecord) => (
          <li key={s.id} className="flex items-center gap-4 px-5 py-3 text-[13px]">
            <Badge tone={s.kind === "auto" ? "info" : s.kind === "undo" ? "neutral" : "accent"}>
              {KIND_LABEL[s.kind]}
            </Badge>
            <div className="min-w-0 flex-1">
              <div className="font-medium">{formatDateTime(s.startedAt)}</div>
              <div className="text-[12px] text-muted">
                {plural(s.completed, "file")} {s.kind === "undo" ? "restored" : "moved"} ·{" "}
                {s.skipped} skipped · {s.failed} failed
                {s.status !== "completed" && <span className="text-warning"> · {s.status}</span>}
                {s.undoneAt && <span> · undone {timeAgo(s.undoneAt)}</span>}
              </div>
            </div>
            <Button size="sm" variant="ghost" onClick={() => onOpen(s.id)}>
              View files
            </Button>
            {s.undoable > 0 && (s.kind === "manual" || s.kind === "auto" || s.kind === "retry") && (
              <Button size="sm" onClick={() => void undoSession(s.id)}>
                <Undo2 className="size-3.5" /> Undo session ({s.undoable})
              </Button>
            )}
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Events() {
  const [search, setSearch] = useState("");
  const [severity, setSeverity] = useState("");
  const { data } = useData(() => api.events({ limit: 300, search, severity }), [search, severity]);
  return (
    <>
      <div className="flex gap-2">
        <div className="relative w-64">
          <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-subtle" />
          <Input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search log"
            className="pl-8"
            aria-label="Search log"
          />
        </div>
        <Select
          value={severity}
          onChange={(e) => setSeverity(e.target.value)}
          aria-label="Severity"
        >
          <option value="">All levels</option>
          <option value="info">Info</option>
          <option value="warning">Warnings</option>
          <option value="error">Errors</option>
        </Select>
      </div>
      <p className="text-[12.5px] text-subtle">
        The log records what happened (scans, sessions, errors). It is separate from file
        operations, which are the reversible changes.
      </p>
      <Card>
        {data && data.length === 0 && (
          <EmptyState icon={<ScrollText className="size-5" />} title="The log is empty" />
        )}
        <ul className="divide-y divide-border">
          {data?.map((e) => (
            <li key={e.id} className="flex items-start gap-3 px-5 py-2.5 text-[13px]">
              <Badge
                tone={
                  e.severity === "error"
                    ? "danger"
                    : e.severity === "warning"
                      ? "warning"
                      : "neutral"
                }
              >
                {e.kind}
              </Badge>
              <span className="flex-1">{e.message}</span>
              <span
                className="text-[12px] whitespace-nowrap text-subtle"
                title={formatDateTime(e.createdAt)}
              >
                {timeAgo(e.createdAt)}
              </span>
            </li>
          ))}
        </ul>
      </Card>
    </>
  );
}

export function ActivityPage() {
  const [tab, setTab] = useState<Tab>("operations");
  const [session, setSession] = useState<string | null>(null);
  return (
    <>
      <PageHeader
        title="Activity"
        subtitle="A complete, searchable record of everything Tucklane has done. Undo or retry from here."
      />
      <PageBody>
        <div className="flex gap-1 border-b border-border" role="tablist">
          {(
            [
              ["operations", "File operations"],
              ["sessions", "Sessions"],
              ["events", "Event log"],
            ] as [Tab, string][]
          ).map(([id, label]) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={cn(
                "-mb-px border-b-2 px-3 py-2 text-[13.5px] font-medium",
                tab === id
                  ? "border-accent-strong text-fg"
                  : "border-transparent text-muted hover:text-fg",
              )}
            >
              {label}
            </button>
          ))}
        </div>
        {tab === "operations" && <Operations key={session ?? "all"} initialSession={session} />}
        {tab === "sessions" && (
          <Sessions
            onOpen={(id) => {
              setSession(id);
              setTab("operations");
            }}
          />
        )}
        {tab === "events" && <Events />}
      </PageBody>
    </>
  );
}
