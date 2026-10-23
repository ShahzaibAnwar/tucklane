import { create } from "zustand";
import { api } from "@/lib/api";
import type { AppStatus, MonitorStatus, Settings } from "@/types/api";

export type Page =
  | "dashboard"
  | "explorer"
  | "organize"
  | "rules"
  | "duplicates"
  | "activity"
  | "settings";

interface AppStore {
  status: AppStatus | null;
  loadError: string | null;
  page: Page;
  /** Increments whenever the backend reports changed data. */
  tick: number;
  /** Files handed from the explorer to the organize screen. */
  organizeOnly: string[] | null;
  setPage: (p: Page) => void;
  organizeSelection: (rels: string[]) => void;
  clearOrganizeOnly: () => void;
  refresh: () => Promise<void>;
  bump: () => void;
  setMonitor: (m: MonitorStatus) => void;
  setSettings: (s: Settings) => void;
}

export const useApp = create<AppStore>((set, get) => ({
  status: null,
  loadError: null,
  page: "dashboard",
  tick: 0,
  organizeOnly: null,
  setPage: (page) => set({ page }),
  organizeSelection: (rels) => set({ organizeOnly: rels, page: "organize" }),
  clearOrganizeOnly: () => set({ organizeOnly: null }),
  refresh: async () => {
    try {
      const status = await api.appStatus();
      set({ status, loadError: null });
    } catch (e) {
      set({ loadError: String((e as { message?: string })?.message ?? e) });
    }
  },
  bump: () => {
    set({ tick: get().tick + 1 });
    void get().refresh();
  },
  setMonitor: (monitor) => {
    const s = get().status;
    if (s) set({ status: { ...s, monitor } });
  },
  setSettings: (settings) => {
    const s = get().status;
    if (s) set({ status: { ...s, settings } });
  },
}));

export function useSettings(): Settings | null {
  return useApp((s) => s.status?.settings ?? null);
}
