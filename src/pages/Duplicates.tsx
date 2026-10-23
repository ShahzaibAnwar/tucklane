import { useEffect, useState } from "react";
import {
  Copy,
  FolderSearch,
  Info,
  Loader2,
  ScanSearch,
  ShieldCheck,
  Trash2,
  XCircle,
} from "lucide-react";
import { api, errorMessage, EVENTS } from "@/lib/api";
import { formatBytes, formatCount, formatDateTime, timeAgo, winPath, folderOf } from "@/lib/format";
import { useBackendEvent, useData } from "@/hooks/useData";
import { useApp } from "@/stores/app";
import type { DupProgress, DuplicateGroup } from "@/types/api";
import { PageBody, PageHeader } from "@/components/layout/PageHeader";
import { Button } from "@/components/ui/button";
import {
  Badge,
  Card,
  Checkbox,
  EmptyState,
  ProgressBar,
  Tooltip,
} from "@/components/ui/primitives";
import { ConfirmDialog, Dialog } from "@/components/ui/dialog";
import { FileIcon } from "@/components/common/FileIcon";
import { toast } from "@/components/ui/toast";
import { cn } from "@/lib/cn";

function Group({
  g,
  selected,
  toggle,
  onCompare,
}: {
  g: DuplicateGroup;
  selected: Set<string>;
  toggle: (rel: string, v: boolean) => void;
  onCompare: () => void;
}) {
  const chosen = g.members.filter((m) => selected.has(m.rel));
  const allChosen = chosen.length === g.members.length;
  return (
    <Card className={cn(allChosen && "border-danger")}>
      <div className="flex items-center justify-between gap-3 border-b border-border px-5 py-3">
        <div className="flex min-w-0 items-center gap-3">
          <FileIcon name={g.members[0].name} className="size-5" />
          <div className="min-w-0">
            <div className="truncate text-[14px] font-semibold">{g.members[0].name}</div>
            <div className="text-[12px] text-muted">
              {g.members.length} identical copies · {formatBytes(g.size)} each ·{" "}
              <Tooltip content={`SHA-256 ${g.hash}`}>
                <span className="font-mono">sha256 {g.hash.slice(0, 10)}…</span>
              </Tooltip>
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone="success">
            <ShieldCheck className="size-3" /> Content verified
          </Badge>
          <Button size="sm" variant="ghost" onClick={onCompare}>
            Compare
          </Button>
        </div>
      </div>
      <ul className="divide-y divide-border">
        {g.members.map((m) => {
          const sel = selected.has(m.rel);
          return (
            <li
              key={m.rel}
              className={cn(
                "flex items-center gap-3 px-5 py-2.5 text-[13px]",
                sel && "bg-danger-soft/50",
              )}
            >
              <Checkbox
                checked={sel}
                onCheckedChange={(v) => toggle(m.rel, v)}
                label={`Select ${winPath(m.rel)} for removal`}
              />
              <div className="min-w-0 flex-1">
                <div className="truncate font-medium">{m.name}</div>
                <div className="truncate font-mono text-[12px] text-muted">
                  {winPath(folderOf(m.rel)) || "Downloads (top level)"}
                </div>
              </div>
              <span
                className="w-28 text-right text-[12px] text-subtle"
                title={formatDateTime(m.modifiedMs)}
              >
                {timeAgo(m.modifiedMs)}
              </span>
              {sel ? (
                <Badge tone="danger">To Recycle Bin</Badge>
              ) : (
                <Badge tone="success">Keep</Badge>
              )}
              <Tooltip content="Show in Windows Explorer">
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label="Show in Windows Explorer"
                  onClick={() =>
                    api.reveal(m.rel).catch((e) => toast.error("Could not reveal", errorMessage(e)))
                  }
                >
                  <FolderSearch className="size-4" />
                </Button>
              </Tooltip>
            </li>
          );
        })}
      </ul>
      {allChosen && (
        <p className="px-5 py-2 text-[12.5px] text-danger">
          Every copy is selected. Tucklane always keeps at least one copy — deselect the one to
          keep.
        </p>
      )}
    </Card>
  );
}

export function Duplicates() {
  const settings = useApp((s) => s.status?.settings);
  const setPage = useApp((s) => s.setPage);
  const { data, reload } = useData(api.duplicates);
  const [scanning, setScanning] = useState(false);
  const [progress, setProgress] = useState<DupProgress | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [compare, setCompare] = useState<DuplicateGroup | null>(null);

  useBackendEvent<DupProgress>(EVENTS.duplicatesProgress, setProgress);
  useEffect(() => setSelected(new Set()), [data]);

  const scan = async () => {
    setScanning(true);
    setProgress(null);
    try {
      await api.findDuplicates();
      await reload();
    } catch (e) {
      if (!errorMessage(e).startsWith("Cancelled"))
        toast.error("Duplicate scan failed", errorMessage(e));
    } finally {
      setScanning(false);
    }
  };

  const toggle = (rel: string, v: boolean) => {
    const s = new Set(selected);
    if (v) s.add(rel);
    else s.delete(rel);
    setSelected(s);
  };

  const groups = data?.groups ?? [];
  const blocked = groups.some((g) => g.members.every((m) => selected.has(m.rel)));
  const selectedBytes = groups.reduce(
    (a, g) => a + g.members.filter((m) => selected.has(m.rel)).length * g.size,
    0,
  );

  const recycle = async () => {
    setBusy(true);
    try {
      const res = await api.recycle([...selected]);
      const ok = res.filter((r) => r.ok).length;
      const failed = res.filter((r) => !r.ok);
      if (ok)
        toast.success(
          `${ok} cop${ok === 1 ? "y" : "ies"} moved to the Recycle Bin`,
          "Restore them from the Recycle Bin if you change your mind.",
        );
      if (failed.length) toast.warning(`${failed.length} kept`, failed[0].message);
      setConfirm(false);
      await reload();
    } catch (e) {
      toast.error("Could not move to Recycle Bin", errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (settings && !settings.duplicateDetection) {
    return (
      <>
        <PageHeader title="Duplicates" />
        <PageBody>
          <Card>
            <EmptyState
              icon={<Copy className="size-5" />}
              title="Duplicate detection is turned off"
              action={<Button onClick={() => setPage("settings")}>Open Settings</Button>}
            >
              Turn it on in Settings › Safety to find identical files. Scanning reads files locally
              to compare their content.
            </EmptyState>
          </Card>
        </PageBody>
      </>
    );
  }

  const phaseText: Record<string, string> = {
    scanning: "Listing files",
    fingerprinting: "Comparing quick fingerprints",
    verifying: "Verifying content with SHA-256",
    done: "Finishing",
  };

  return (
    <>
      <PageHeader
        title="Duplicates"
        subtitle="Files with identical content, verified by SHA-256. Nothing is deleted automatically, and matching names alone never count."
        actions={
          scanning ? (
            <Button onClick={() => void api.cancelDuplicates()}>
              <XCircle className="size-4" /> Cancel scan
            </Button>
          ) : (
            <Button variant="primary" onClick={scan}>
              <ScanSearch className="size-4" />{" "}
              {data?.summary.scannedAt ? "Scan again" : "Find duplicates"}
            </Button>
          )
        }
      />
      <PageBody>
        {scanning && (
          <Card className="p-5" aria-live="polite">
            <div className="mb-3 flex items-center gap-2 text-[14px] font-medium">
              <Loader2 className="size-4 animate-spin text-accent-strong" />{" "}
              {progress ? phaseText[progress.phase] : "Starting"}…
            </div>
            <ProgressBar
              value={progress?.phase === "verifying" ? progress.hashed : 0}
              max={progress?.toHash || 1}
              label="Duplicate scan progress"
            />
            <p className="mt-2 text-[12.5px] text-muted">
              {formatCount(progress?.filesSeen ?? 0)} files seen ·{" "}
              {formatCount(progress?.candidates ?? 0)} same-size candidates ·{" "}
              {formatCount(progress?.hashed ?? 0)} of {formatCount(progress?.toHash ?? 0)} verified
            </p>
          </Card>
        )}

        {data && (
          <div className="grid grid-cols-3 gap-3">
            {[
              ["Duplicate groups", formatCount(data.summary.groups)],
              ["Extra copies", formatCount(data.summary.duplicateFiles)],
              ["Potential recoverable space", formatBytes(data.summary.recoverableBytes)],
            ].map(([l, v]) => (
              <Card key={l} className="px-4 py-3.5">
                <div className="tl-num font-display text-[24px] font-semibold">{v}</div>
                <div className="text-[13px] text-muted">{l}</div>
              </Card>
            ))}
          </div>
        )}

        {data && !data.summary.scannedAt && !scanning && (
          <Card>
            <EmptyState
              icon={<Copy className="size-5" />}
              title="No scan yet"
              action={
                <Button variant="primary" onClick={scan}>
                  Find duplicates
                </Button>
              }
            >
              Tucklane groups files by size, then confirms matches by hashing their content. Files
              are read locally and never uploaded.
            </EmptyState>
          </Card>
        )}
        {data?.summary.scannedAt && groups.length === 0 && !scanning && (
          <Card>
            <EmptyState icon={<ShieldCheck className="size-5" />} title="No duplicates found">
              Scanned {timeAgo(data.summary.scannedAt)}.
            </EmptyState>
          </Card>
        )}

        {groups.length > 0 && (
          <>
            <div className="flex items-center gap-2 rounded-lg bg-info-soft px-3 py-2 text-[12.5px] text-info">
              <Info className="size-4 shrink-0" /> Copies are only ever moved to the Windows Recycle
              Bin, after you confirm. A duplicate may still matter — check its location first.
            </div>
            {groups.map((g) => (
              <Group
                key={g.id}
                g={g}
                selected={selected}
                toggle={toggle}
                onCompare={() => setCompare(g)}
              />
            ))}
          </>
        )}
      </PageBody>

      {selected.size > 0 && (
        <div className="sticky bottom-0 flex items-center justify-between border-t border-border bg-surface px-8 py-3">
          <span className="text-[13px]">
            <span className="font-semibold">{selected.size} selected</span> ·{" "}
            {formatBytes(selectedBytes)}
          </span>
          <div className="flex gap-2">
            <Button variant="ghost" onClick={() => setSelected(new Set())}>
              Clear selection
            </Button>
            <Button variant="danger" onClick={() => setConfirm(true)} disabled={blocked}>
              <Trash2 className="size-4" /> Move selected to Recycle Bin
            </Button>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title={`Move ${selected.size} cop${selected.size === 1 ? "y" : "ies"} to the Recycle Bin?`}
        description="Each file is re-verified first. At least one copy of every group is always kept. You can restore files from the Recycle Bin."
        confirmLabel="Move to Recycle Bin"
        danger
        busy={busy}
        onConfirm={recycle}
      >
        <ul className="max-h-48 space-y-0.5 overflow-auto font-mono text-[12px] text-muted">
          {[...selected].map((r) => (
            <li key={r} className="truncate">
              {winPath(r)}
            </li>
          ))}
        </ul>
      </ConfirmDialog>

      <Dialog
        open={!!compare}
        onOpenChange={(v) => !v && setCompare(null)}
        title="Compare copies"
        wide
      >
        {compare && (
          <table className="w-full text-left text-[12.5px]">
            <thead className="text-muted">
              <tr>
                <th className="py-1.5 font-medium">Location</th>
                <th className="py-1.5 font-medium">Size</th>
                <th className="py-1.5 font-medium">Modified</th>
                <th className="py-1.5 font-medium">SHA-256</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {compare.members.map((m) => (
                <tr key={m.rel}>
                  <td className="py-1.5 pr-3 font-mono">{winPath(m.rel)}</td>
                  <td className="tl-num py-1.5 pr-3">{formatBytes(m.size)}</td>
                  <td className="py-1.5 pr-3">{formatDateTime(m.modifiedMs)}</td>
                  <td className="py-1.5 font-mono text-subtle">{compare.hash.slice(0, 16)}…</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Dialog>
    </>
  );
}
