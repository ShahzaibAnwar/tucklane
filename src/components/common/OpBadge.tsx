import type { OpKind, OpState } from "@/types/api";
import { Badge } from "@/components/ui/primitives";

const STATE: Record<
  OpState,
  { label: string; tone: "neutral" | "accent" | "success" | "warning" | "danger" | "info" }
> = {
  planned: { label: "Planned", tone: "neutral" },
  in_progress: { label: "In progress", tone: "info" },
  completed: { label: "Moved", tone: "success" },
  skipped: { label: "Skipped", tone: "warning" },
  failed: { label: "Failed", tone: "danger" },
  undone: { label: "Undone", tone: "neutral" },
  recovery_required: { label: "Needs attention", tone: "danger" },
};

export function OpBadge({ state, kind }: { state: OpState; kind?: OpKind }) {
  const s = STATE[state];
  let label = s.label;
  if (state === "completed" && kind === "undo") label = "Restored";
  if (state === "completed" && kind === "recycle") label = "Recycled";
  return <Badge tone={s.tone}>{label}</Badge>;
}
