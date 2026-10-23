import { create } from "zustand";
import { CheckCircle2, AlertTriangle, XCircle, Info, X } from "lucide-react";
import { cn } from "@/lib/cn";

type Kind = "success" | "error" | "info" | "warning";
interface Toast {
  id: number;
  kind: Kind;
  title: string;
  body?: string;
  action?: { label: string; run: () => void };
}

interface ToastStore {
  toasts: Toast[];
  push: (t: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;
}

let next = 1;
export const useToasts = create<ToastStore>((set, get) => ({
  toasts: [],
  push: (t) => {
    const id = next++;
    set({ toasts: [...get().toasts.slice(-3), { ...t, id }] });
    setTimeout(() => get().dismiss(id), t.action ? 9000 : 5000);
  },
  dismiss: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));

export const toast = {
  success: (title: string, body?: string, action?: Toast["action"]) =>
    useToasts.getState().push({ kind: "success", title, body, action }),
  error: (title: string, body?: string) =>
    useToasts.getState().push({ kind: "error", title, body }),
  info: (title: string, body?: string, action?: Toast["action"]) =>
    useToasts.getState().push({ kind: "info", title, body, action }),
  warning: (title: string, body?: string) =>
    useToasts.getState().push({ kind: "warning", title, body }),
};

const icons = {
  success: <CheckCircle2 className="size-4 text-success" />,
  error: <XCircle className="size-4 text-danger" />,
  info: <Info className="size-4 text-info" />,
  warning: <AlertTriangle className="size-4 text-warning" />,
};

export function Toaster() {
  const { toasts, dismiss } = useToasts();
  return (
    <div
      className="pointer-events-none fixed right-5 bottom-5 z-[60] flex w-[360px] flex-col gap-2"
      role="region"
      aria-label="Notifications"
      aria-live="polite"
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          className={cn("tl-card tl-enter pointer-events-auto flex gap-3 p-3.5")}
          role="status"
        >
          <div className="pt-0.5">{icons[t.kind]}</div>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-medium">{t.title}</p>
            {t.body && <p className="mt-0.5 text-[13px] break-words text-muted">{t.body}</p>}
            {t.action && (
              <button
                className="mt-2 text-[13px] font-semibold text-accent-strong hover:underline"
                onClick={() => {
                  t.action?.run();
                  dismiss(t.id);
                }}
              >
                {t.action.label}
              </button>
            )}
          </div>
          <button
            className="self-start rounded p-0.5 text-subtle hover:text-fg"
            onClick={() => dismiss(t.id)}
            aria-label="Dismiss"
          >
            <X className="size-3.5" />
          </button>
        </div>
      ))}
    </div>
  );
}
