import { useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ContextMenu } from "radix-ui";
import {
  ChevronRight,
  ClipboardCopy,
  EyeOff,
  Eye,
  ExternalLink,
  FolderOpen,
  History,
  Home,
  LayoutGrid,
  List,
  Loader2,
  Search,
  Wand2,
  X,
  FolderSearch,
} from "lucide-react";
import { api, errorCode, errorMessage } from "@/lib/api";
import { formatBytes, formatDateTime, timeAgo, winPath } from "@/lib/format";
import { cn } from "@/lib/cn";
import { useApp } from "@/stores/app";
import { useData } from "@/hooks/useData";
import type { ExplorerEntry, FileKind, OperationRecord } from "@/types/api";
import { KIND_LABELS } from "@/lib/kinds";
import { PageHeader } from "@/components/layout/PageHeader";
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
import { ConfirmDialog, Dialog } from "@/components/ui/dialog";
import { FileIcon } from "@/components/common/FileIcon";
import { OpBadge } from "@/components/common/OpBadge";
import { toast } from "@/components/ui/toast";

type SortKey = "name" | "size" | "modified" | "type";

function Thumb({ entry }: { entry: ExplorerEntry }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    if (entry.kind === "image" && entry.size < 6 * 1024 * 1024) {
      api
        .thumbnail(entry.rel)
        .then((s) => live && setSrc(s))
        .catch(() => {});
    }
    return () => {
      live = false;
    };
  }, [entry.rel, entry.kind, entry.size]);
  if (src)
    return (
      <img
        src={src}
        alt=""
        className="h-full w-full object-cover"
        loading="lazy"
        decoding="async"
      />
    );
  return (
    <FileIcon kind={entry.kind} isDir={entry.isDir} isLink={entry.isLink} className="size-9" />
  );
}

function HistoryDialog({ rel, onClose }: { rel: string | null; onClose: () => void }) {
  const [ops, setOps] = useState<OperationRecord[] | null>(null);
  useEffect(() => {
    setOps(null);
    if (rel)
      api
        .fileHistory(rel)
        .then(setOps)
        .catch((e) => toast.error("Could not load history", errorMessage(e)));
  }, [rel]);
  return (
    <Dialog
      open={!!rel}
      onOpenChange={(v) => !v && onClose()}
      title="File history"
      description={winPath(rel)}
      wide
    >
      {!ops && <Loader2 className="size-4 animate-spin" />}
      {ops && ops.length === 0 && (
        <p className="text-[13px] text-muted">Tucklane has not moved this file.</p>
      )}
      <ul className="divide-y divide-border">
        {ops?.map((o) => (
          <li key={o.id} className="flex items-center gap-3 py-2 text-[13px]">
            <OpBadge state={o.state} kind={o.kind} />
            <div className="min-w-0 flex-1">
              <div className="truncate font-mono text-[12px]">
                {winPath(o.sourceRel)} → {winPath(o.destRel ?? o.destDirRel)}
              </div>
              <div className="text-[12px] text-muted">{o.error ?? o.reason}</div>
            </div>
            <span className="text-[12px] text-subtle">{formatDateTime(o.createdAt)}</span>
          </li>
        ))}
      </ul>
    </Dialog>
  );
}

export function Explorer() {
  const organizeSelection = useApp((s) => s.organizeSelection);
  const [rel, setRel] = useState("");
  const [view, setView] = useState<"list" | "grid">("list");
  const [sort, setSort] = useState<SortKey>("name");
  const [desc, setDesc] = useState(false);
  const [kind, setKind] = useState<"" | FileKind | "folders">("");
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<ExplorerEntry[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [historyRel, setHistoryRel] = useState<string | null>(null);
  const [execConfirm, setExecConfirm] = useState<string | null>(null);
  const { data, error, loading, reload } = useData(() => api.listDir(rel), [rel]);

  useEffect(() => setSelected(new Set()), [rel]);

  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setSearchResults(null);
      return;
    }
    const t = setTimeout(async () => {
      setSearching(true);
      try {
        setSearchResults(await api.search(q));
      } catch (e) {
        toast.error("Search failed", errorMessage(e));
      } finally {
        setSearching(false);
      }
    }, 250);
    return () => clearTimeout(t);
  }, [query]);

  const entries = useMemo(() => {
    let list = searchResults ?? data?.entries ?? [];
    if (kind === "folders") list = list.filter((e) => e.isDir);
    else if (kind) list = list.filter((e) => e.kind === kind);
    const dir = desc ? -1 : 1;
    return [...list].sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      switch (sort) {
        case "size":
          return (a.size - b.size) * dir;
        case "modified":
          return (a.modifiedMs - b.modifiedMs) * dir;
        case "type":
          return (
            ((a.kindLabel ?? a.ext ?? "").localeCompare(b.kindLabel ?? b.ext ?? "") ||
              a.name.localeCompare(b.name)) * dir
          );
        default:
          return (
            a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * dir
          );
      }
    });
  }, [data, searchResults, kind, sort, desc]);

  const crumbs = rel ? rel.split("/") : [];
  const selectedEntries = entries.filter((e) => selected.has(e.rel));
  const selectedFiles = selectedEntries.filter((e) => !e.isDir && !e.isLink);
  const topLevelSelected = selectedFiles.filter((e) => !e.rel.includes("/"));

  const openEntry = async (e: ExplorerEntry, confirmed = false) => {
    if (e.isLink) {
      toast.warning(
        "Links are not followed",
        "Tucklane doesn't open links or junctions inside the managed folder.",
      );
      return;
    }
    if (e.isDir) {
      setQuery("");
      setRel(e.rel);
      return;
    }
    try {
      await api.openFile(e.rel, confirmed);
    } catch (err) {
      // The backend decides which files need confirmation (programs,
      // scripts, shortcuts, unknown types…); the UI just asks.
      if (errorCode(err) === "confirm_required") setExecConfirm(e.rel);
      else toast.error("Could not open file", errorMessage(err));
    }
  };

  const copyPath = async (r: string) => {
    try {
      await navigator.clipboard.writeText(await api.absolutePath(r));
      toast.success("Path copied");
    } catch (e) {
      toast.error("Could not copy path", errorMessage(e));
    }
  };

  const setExcluded = async (rels: string[], exclude: boolean) => {
    try {
      if (exclude) await api.exclude(rels);
      else await api.include(rels);
      toast.success(
        exclude ? `${rels.length} excluded from organization` : `${rels.length} included again`,
      );
      void reload();
    } catch (e) {
      toast.error("Could not update", errorMessage(e));
    }
  };

  const parentRef = useRef<HTMLDivElement>(null);
  const cols = 6;
  const rows = view === "grid" ? Math.ceil(entries.length / cols) : entries.length;
  const virt = useVirtualizer({
    count: rows,
    getScrollElement: () => parentRef.current,
    estimateSize: () => (view === "grid" ? 156 : 44),
    overscan: 10,
    initialRect: { width: 1000, height: 600 },
  });

  const toggle = (r: string, multi: boolean, range: boolean, idx: number) => {
    const s = new Set(multi || range ? selected : []);
    if (range && lastIdx.current != null) {
      const [a, b] = [Math.min(lastIdx.current, idx), Math.max(lastIdx.current, idx)];
      for (let i = a; i <= b; i++) s.add(entries[i].rel);
    } else if (s.has(r) && multi) s.delete(r);
    else s.add(r);
    lastIdx.current = idx;
    setSelected(s);
  };
  const lastIdx = useRef<number | null>(null);

  const menu = (e: ExplorerEntry) => (
    <ContextMenu.Portal>
      <ContextMenu.Content className="z-50 min-w-[210px] rounded-lg border border-border bg-surface p-1 text-[13px] shadow-xl">
        {[
          {
            icon: e.isDir ? FolderOpen : ExternalLink,
            label: e.isDir ? "Open folder" : "Open",
            run: () => openEntry(e),
            hide: e.isLink,
          },
          {
            icon: FolderSearch,
            label: "Show in Windows Explorer",
            run: () =>
              api.reveal(e.rel).catch((x) => toast.error("Could not reveal", errorMessage(x))),
          },
          { icon: ClipboardCopy, label: "Copy path", run: () => copyPath(e.rel) },
          {
            icon: Wand2,
            label: "Preview classification",
            run: () => organizeSelection([e.rel]),
            hide: e.isDir || e.isLink || e.rel.includes("/"),
          },
          {
            icon: e.excluded ? Eye : EyeOff,
            label: e.excluded ? "Include in organization" : "Exclude from organization",
            run: () => setExcluded([e.rel], !e.excluded),
            hide: e.isDir || e.isLink,
          },
          {
            icon: History,
            label: "View operation history",
            run: () => setHistoryRel(e.rel),
            hide: e.isDir,
          },
        ]
          .filter((i) => !i.hide)
          .map((i) => (
            <ContextMenu.Item
              key={i.label}
              onSelect={() => void i.run()}
              className="flex cursor-default items-center gap-2 rounded-md px-2.5 py-1.5 outline-none data-[highlighted]:bg-surface-2"
            >
              <i.icon className="size-4 text-muted" /> {i.label}
            </ContextMenu.Item>
          ))}
      </ContextMenu.Content>
    </ContextMenu.Portal>
  );

  return (
    <div className="flex h-full flex-col">
      <PageHeader
        title="File Explorer"
        subtitle="Browse everything inside your authorized folder. Links and junctions are shown but never followed."
        actions={
          <div
            className="flex rounded-lg border border-border-strong p-0.5"
            role="group"
            aria-label="View"
          >
            <Tooltip content="List view">
              <button
                aria-pressed={view === "list"}
                onClick={() => setView("list")}
                className={cn("rounded-md p-1.5", view === "list" ? "bg-surface-3" : "text-muted")}
                aria-label="List view"
              >
                <List className="size-4" />
              </button>
            </Tooltip>
            <Tooltip content="Grid view">
              <button
                aria-pressed={view === "grid"}
                onClick={() => setView("grid")}
                className={cn("rounded-md p-1.5", view === "grid" ? "bg-surface-3" : "text-muted")}
                aria-label="Grid view"
              >
                <LayoutGrid className="size-4" />
              </button>
            </Tooltip>
          </div>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col gap-3 px-8 pb-6">
        <div className="flex flex-wrap items-center gap-2">
          <nav aria-label="Location" className="flex min-w-0 flex-1 items-center gap-1 text-[13px]">
            <button
              onClick={() => {
                setRel("");
                setQuery("");
              }}
              className="flex items-center gap-1 rounded-md px-2 py-1 font-medium hover:bg-surface-2"
            >
              <Home className="size-3.5" /> Downloads
            </button>
            {crumbs.map((c, i) => (
              <span key={i} className="flex min-w-0 items-center gap-1">
                <ChevronRight className="size-3.5 shrink-0 text-subtle" />
                <button
                  onClick={() => setRel(crumbs.slice(0, i + 1).join("/"))}
                  className="truncate rounded-md px-2 py-1 hover:bg-surface-2"
                >
                  {c}
                </button>
              </span>
            ))}
          </nav>
          <div className="relative w-64">
            <Search className="pointer-events-none absolute top-2.5 left-2.5 size-4 text-subtle" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search all files"
              className="pr-8 pl-8"
              aria-label="Search all files"
            />
            {searching && (
              <Loader2 className="absolute top-2.5 right-2.5 size-4 animate-spin text-subtle" />
            )}
            {query && !searching && (
              <button
                onClick={() => setQuery("")}
                className="absolute top-2.5 right-2.5 text-subtle hover:text-fg"
                aria-label="Clear search"
              >
                <X className="size-4" />
              </button>
            )}
          </div>
          <Select
            value={kind}
            onChange={(e) => setKind(e.target.value as typeof kind)}
            aria-label="Filter by type"
          >
            <option value="">All types</option>
            <option value="folders">Folders</option>
            {(Object.keys(KIND_LABELS) as FileKind[]).map((k) => (
              <option key={k} value={k}>
                {KIND_LABELS[k]}
              </option>
            ))}
          </Select>
          <Select
            value={`${sort}:${desc ? "d" : "a"}`}
            onChange={(e) => {
              const [k, d] = e.target.value.split(":");
              setSort(k as SortKey);
              setDesc(d === "d");
            }}
            aria-label="Sort"
          >
            <option value="name:a">Name A–Z</option>
            <option value="name:d">Name Z–A</option>
            <option value="modified:d">Newest first</option>
            <option value="modified:a">Oldest first</option>
            <option value="size:d">Largest first</option>
            <option value="size:a">Smallest first</option>
            <option value="type:a">Type</option>
          </Select>
        </div>

        {selected.size > 0 && (
          <div className="tl-enter flex flex-wrap items-center gap-2 rounded-lg border border-accent/40 bg-accent-soft px-3 py-2 text-[13px]">
            <span className="font-semibold">{selected.size} selected</span>
            <span className="text-muted">
              · {formatBytes(selectedFiles.reduce((a, e) => a + e.size, 0))}
            </span>
            <div className="ml-auto flex gap-1.5">
              <Button
                size="sm"
                variant="primary"
                disabled={topLevelSelected.length === 0}
                onClick={() => organizeSelection(topLevelSelected.map((e) => e.rel))}
                title="Only loose files at the top of the folder are organized"
              >
                <Wand2 className="size-3.5" /> Organize selection
              </Button>
              <Button
                size="sm"
                disabled={selectedFiles.length === 0}
                onClick={() =>
                  setExcluded(
                    selectedFiles.map((e) => e.rel),
                    true,
                  )
                }
              >
                <EyeOff className="size-3.5" /> Exclude
              </Button>
              <Button
                size="sm"
                disabled={selectedFiles.length === 0}
                onClick={() =>
                  setExcluded(
                    selectedFiles.map((e) => e.rel),
                    false,
                  )
                }
              >
                <Eye className="size-3.5" /> Include
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                Clear
              </Button>
            </div>
          </div>
        )}

        <Card className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {view === "list" && (
            <div className="grid grid-cols-[28px_minmax(0,2.4fr)_minmax(0,1fr)_90px_150px_minmax(0,1.2fr)] gap-3 border-b border-border bg-surface-2/60 px-4 py-2 text-[12px] font-semibold tracking-wide text-muted uppercase">
              <Checkbox
                checked={
                  entries.length > 0 && selected.size === entries.length
                    ? true
                    : selected.size
                      ? "indeterminate"
                      : false
                }
                onCheckedChange={(v) =>
                  setSelected(v ? new Set(entries.map((e) => e.rel)) : new Set())
                }
                label="Select all"
              />
              <span>Name</span>
              <span>Type</span>
              <span className="text-right">Size</span>
              <span>Modified</span>
              <span>{searchResults ? "Location" : "Category"}</span>
            </div>
          )}
          {loading && !data && (
            <div className="flex flex-1 items-center justify-center gap-2 py-16 text-muted">
              <Loader2 className="size-4 animate-spin" /> Loading…
            </div>
          )}
          {error && (
            <p className="m-4 rounded-lg bg-danger-soft px-3 py-2 text-[13px] text-danger">
              {error}
            </p>
          )}
          {!loading && entries.length === 0 && !error && (
            <EmptyState
              icon={<FolderOpen className="size-5" />}
              title={query ? "No matching files" : "This folder is empty"}
            />
          )}
          <div
            ref={parentRef}
            className="min-h-0 flex-1 overflow-auto"
            role="grid"
            aria-label="Files"
            aria-multiselectable
          >
            <div style={{ height: virt.getTotalSize(), position: "relative" }}>
              {virt.getVirtualItems().map((vr) => {
                const style = {
                  position: "absolute" as const,
                  top: 0,
                  left: 0,
                  right: 0,
                  height: vr.size,
                  transform: `translateY(${vr.start}px)`,
                };
                if (view === "grid") {
                  const slice = entries.slice(vr.index * cols, vr.index * cols + cols);
                  return (
                    <div key={vr.key} style={style} className="grid grid-cols-6 gap-3 px-4 py-2">
                      {slice.map((e, j) => {
                        const idx = vr.index * cols + j;
                        return (
                          <ContextMenu.Root key={e.rel}>
                            <ContextMenu.Trigger asChild>
                              <button
                                role="gridcell"
                                aria-selected={selected.has(e.rel)}
                                onClick={(ev) => toggle(e.rel, ev.ctrlKey, ev.shiftKey, idx)}
                                onDoubleClick={() => void openEntry(e)}
                                onKeyDown={(ev) => ev.key === "Enter" && void openEntry(e)}
                                className={cn(
                                  "flex flex-col items-center gap-2 rounded-xl border p-2 text-center",
                                  selected.has(e.rel)
                                    ? "border-accent bg-accent-soft"
                                    : "border-transparent hover:bg-surface-2",
                                )}
                              >
                                <div className="flex h-[92px] w-full items-center justify-center overflow-hidden rounded-lg bg-surface-2">
                                  <Thumb entry={e} />
                                </div>
                                <span className="line-clamp-2 w-full text-[12px] leading-tight break-all">
                                  {e.name}
                                </span>
                              </button>
                            </ContextMenu.Trigger>
                            {menu(e)}
                          </ContextMenu.Root>
                        );
                      })}
                    </div>
                  );
                }
                const e = entries[vr.index];
                return (
                  <ContextMenu.Root key={e.rel}>
                    <ContextMenu.Trigger asChild>
                      <div
                        style={style}
                        role="row"
                        tabIndex={0}
                        aria-selected={selected.has(e.rel)}
                        onClick={(ev) => toggle(e.rel, ev.ctrlKey, ev.shiftKey, vr.index)}
                        onDoubleClick={() => void openEntry(e)}
                        onKeyDown={(ev) => {
                          if (ev.key === "Enter") void openEntry(e);
                          if (ev.key === " ") {
                            ev.preventDefault();
                            toggle(e.rel, true, false, vr.index);
                          }
                        }}
                        className={cn(
                          "grid cursor-default grid-cols-[28px_minmax(0,2.4fr)_minmax(0,1fr)_90px_150px_minmax(0,1.2fr)] items-center gap-3 border-b border-border px-4 text-[13px]",
                          selected.has(e.rel) ? "bg-accent-soft/60" : "hover:bg-surface-2/70",
                        )}
                      >
                        <Checkbox
                          checked={selected.has(e.rel)}
                          onCheckedChange={() => toggle(e.rel, true, false, vr.index)}
                          label={`Select ${e.name}`}
                        />
                        <div className="flex min-w-0 items-center gap-2.5">
                          <FileIcon kind={e.kind} isDir={e.isDir} isLink={e.isLink} />
                          <span className="truncate" title={e.name}>
                            {e.name}
                          </span>
                          {e.excluded && <Badge>Excluded</Badge>}
                          {e.isLink && <Badge tone="warning">Link</Badge>}
                        </div>
                        <span className="truncate text-muted">
                          {e.isDir
                            ? "Folder"
                            : (e.kindLabel ?? (e.ext ? `.${e.ext} file` : "File"))}
                        </span>
                        <span className="tl-num text-right text-muted">
                          {e.isDir ? "" : formatBytes(e.size)}
                        </span>
                        <span className="text-muted" title={formatDateTime(e.modifiedMs)}>
                          {timeAgo(e.modifiedMs)}
                        </span>
                        <span className="truncate font-mono text-[12px] text-muted">
                          {searchResults
                            ? winPath(e.rel.split("/").slice(0, -1).join("/")) || "Downloads"
                            : e.suggestedFolder
                              ? `→ ${winPath(e.suggestedFolder)}`
                              : ""}
                        </span>
                      </div>
                    </ContextMenu.Trigger>
                    {menu(e)}
                  </ContextMenu.Root>
                );
              })}
            </div>
          </div>
          <div className="border-t border-border px-4 py-2 text-[12px] text-subtle">
            {entries.length} item{entries.length === 1 ? "" : "s"}
            {searchResults && searchResults.length >= 500 && " · showing the first 500 matches"} ·
            Double-click to open · Right-click for more
          </div>
        </Card>
      </div>
      <HistoryDialog rel={historyRel} onClose={() => setHistoryRel(null)} />
      <ConfirmDialog
        open={!!execConfirm}
        onOpenChange={(v) => !v && setExecConfirm(null)}
        title="Open this file?"
        description="It may run a program or contain active content (for example a script, shortcut, installer or disk image). Only open it if you trust where it came from."
        confirmLabel="Open anyway"
        danger
        onConfirm={() => {
          const r = execConfirm!;
          setExecConfirm(null);
          void api
            .openFile(r, true)
            .catch((e) => toast.error("Could not open file", errorMessage(e)));
        }}
      />
    </div>
  );
}
