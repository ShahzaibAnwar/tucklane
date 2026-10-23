import {
  Archive,
  AudioLines,
  Braces,
  Captions,
  Code2,
  Disc3,
  File,
  FileSpreadsheet,
  FileText,
  FileType2,
  Folder,
  Image,
  Link2,
  Magnet,
  Package,
  Presentation,
  ScrollText,
  BookOpen,
  NotebookPen,
  TerminalSquare,
  Video,
  BarChart3,
} from "lucide-react";
import type { FileKind } from "@/types/api";
import { cn } from "@/lib/cn";
import { extensionKind } from "@/lib/kinds";

const map: Record<FileKind, { icon: typeof File; color: string }> = {
  pdf: { icon: FileText, color: "text-[#c2412d]" },
  word: { icon: FileType2, color: "text-[#2b5fb8]" },
  text: { icon: ScrollText, color: "text-muted" },
  ebook: { icon: BookOpen, color: "text-[#7a5aa8]" },
  spreadsheet: { icon: FileSpreadsheet, color: "text-[#1f7a4d]" },
  presentation: { icon: Presentation, color: "text-[#c25a1d]" },
  report: { icon: BarChart3, color: "text-[#2d6a8f]" },
  image: { icon: Image, color: "text-[#a8467a]" },
  video: { icon: Video, color: "text-[#6a4fc2]" },
  audio: { icon: AudioLines, color: "text-[#c2417a]" },
  subtitle: { icon: Captions, color: "text-muted" },
  installer: { icon: Package, color: "text-[#2d6a8f]" },
  package: { icon: Package, color: "text-[#3f7f8f]" },
  archive: { icon: Archive, color: "text-[#a8670f]" },
  code: { icon: Code2, color: "text-[#23866a]" },
  data: { icon: Braces, color: "text-[#23866a]" },
  script: { icon: TerminalSquare, color: "text-[#23866a]" },
  notebook: { icon: NotebookPen, color: "text-[#c25a1d]" },
  torrent: { icon: Magnet, color: "text-muted" },
  font: { icon: FileType2, color: "text-muted" },
  disk_image: { icon: Disc3, color: "text-[#3f7f8f]" },
};

export function FileIcon({
  kind,
  name,
  isDir,
  isLink,
  className,
}: {
  kind?: FileKind | null;
  name?: string;
  isDir?: boolean;
  isLink?: boolean;
  className?: string;
}) {
  if (isLink) return <Link2 className={cn("size-4 text-warning", className)} aria-label="Link" />;
  if (isDir)
    return <Folder className={cn("size-4 text-accent-strong", className)} aria-label="Folder" />;
  const k = kind ?? (name ? extensionKind(name) : null);
  const m = k ? map[k] : null;
  const Icon = m?.icon ?? File;
  return <Icon className={cn("size-4", m?.color ?? "text-subtle", className)} aria-hidden />;
}
