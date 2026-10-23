import { create } from "zustand";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/stores/app";
import type { UndoReport, UndoResult } from "@/types/api";
import { ConfirmDialog } from "@/components/ui/dialog";
import { toast } from "@/components/ui/toast";

// Undo is a primary feature. When restoring is unsafe (name taken, file
// changed) the user decides; nothing is ever replaced silently.

interface Pending {
  kind: "conflict" | "modified";
  results: UndoResult[];
}

const usePending = create<{
  pending: Pending | null;
  set: (p: Pending | null) => void;
  busy: boolean;
  setBusy: (b: boolean) => void;
}>((set) => ({
  pending: null,
  set: (pending) => set({ pending }),
  busy: false,
  setBusy: (busy) => set({ busy }),
}));

function report(r: UndoReport) {
  const restored = r.results.filter((x) => x.status === "restored");
  const conflicts = r.results.filter((x) => x.status === "conflict");
  const modified = r.results.filter((x) => x.status === "modified");
  const others = r.results.filter((x) => !["restored", "conflict", "modified"].includes(x.status));
  if (restored.length) {
    const renamed = restored.filter((x) => x.message.startsWith("Restored as"));
    toast.success(
      `${restored.length} file${restored.length === 1 ? "" : "s"} restored`,
      renamed.length
        ? `${renamed.length} restored under a new name because the original name was taken.`
        : "Back where they were.",
    );
  }
  if (others.length) {
    toast.warning(
      `${others.length} file${others.length === 1 ? "" : "s"} could not be restored`,
      others.length === 1
        ? others[0].message
        : others
            .slice(0, 3)
            .map((o) => `${o.name}: ${o.message}`)
            .join(" "),
    );
  }
  if (conflicts.length) usePending.getState().set({ kind: "conflict", results: conflicts });
  else if (modified.length) usePending.getState().set({ kind: "modified", results: modified });
  useApp.getState().bump();
}

export async function undoOperations(ids: string[]) {
  try {
    report(await api.undo(ids));
  } catch (e) {
    toast.error("Undo failed", errorMessage(e));
  }
}

export async function undoSession(sessionId: string) {
  try {
    report(await api.undoSession(sessionId));
  } catch (e) {
    toast.error("Undo failed", errorMessage(e));
  }
}

export function UndoDialogHost() {
  const { pending, set, busy, setBusy } = usePending();
  if (!pending) return null;
  const n = pending.results.length;
  const first = pending.results[0];
  const isConflict = pending.kind === "conflict";
  const go = async () => {
    setBusy(true);
    try {
      const r = await api.undo(
        pending.results.map((x) => x.operationId),
        isConflict ? { onConflict: "rename" } : { allowModified: true, onConflict: "ask" },
      );
      set(null);
      report(r);
    } catch (e) {
      toast.error("Undo failed", errorMessage(e));
      set(null);
    } finally {
      setBusy(false);
    }
  };
  return (
    <ConfirmDialog
      open
      onOpenChange={(v) => !v && set(null)}
      title={isConflict ? "The original name is taken" : "The file changed after it was moved"}
      description={
        isConflict
          ? `${n === 1 ? `Another file named “${first.name}” is` : `${n} files have other files`} now in the original location. Tucklane won't replace ${n === 1 ? "it" : "them"}.`
          : `${n === 1 ? `“${first.name}” was` : `${n} files were`} edited after Tucklane moved ${n === 1 ? "it" : "them"}. Restoring moves the current version back.`
      }
      confirmLabel={isConflict ? "Restore with a new name" : "Restore anyway"}
      onConfirm={go}
      busy={busy}
    >
      {n > 1 && (
        <ul className="max-h-40 space-y-0.5 overflow-auto text-[13px] text-muted">
          {pending.results.map((r) => (
            <li key={r.operationId} className="truncate">
              {r.name}
            </li>
          ))}
        </ul>
      )}
    </ConfirmDialog>
  );
}
