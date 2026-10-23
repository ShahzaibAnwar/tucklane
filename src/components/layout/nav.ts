import {
  Activity,
  Copy,
  FolderTree,
  LayoutDashboard,
  ListChecks,
  Settings,
  Wand2,
} from "lucide-react";
import type { Page } from "@/stores/app";

export const NAV: { page: Page; label: string; icon: typeof Activity }[] = [
  { page: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { page: "explorer", label: "File Explorer", icon: FolderTree },
  { page: "organize", label: "Organize", icon: Wand2 },
  { page: "rules", label: "Rules", icon: ListChecks },
  { page: "duplicates", label: "Duplicates", icon: Copy },
  { page: "activity", label: "Activity", icon: Activity },
  { page: "settings", label: "Settings", icon: Settings },
];
