import type { FileKind } from "@/types/api";

// Display-only mirror of the backend extension table, used for icons when
// only a filename is known (e.g. history rows). Classification itself always
// happens in Rust.
const EXT: Record<string, FileKind> = {};
const add = (kind: FileKind, exts: string) => exts.split(" ").forEach((e) => (EXT[e] = kind));
add("pdf", "pdf");
add("word", "doc docx odt rtf dotx pages");
add("text", "txt md markdown log");
add("ebook", "epub mobi azw3 djvu");
add("spreadsheet", "xls xlsx xlsm xlsb csv tsv ods numbers");
add("presentation", "ppt pptx pps ppsx odp key");
add("report", "rpt rdl rdlc mrt jrxml pbix");
add("image", "jpg jpeg png gif webp bmp heic heif svg tif tiff avif ico raw cr2 nef arw dng psd");
add("video", "mp4 mkv avi mov webm wmv m4v mpg mpeg flv 3gp m2ts mts");
add("audio", "mp3 wav flac m4a aac ogg opus wma aiff mid midi");
add("subtitle", "srt ass ssa vtt sub sbv");
add("installer", "exe msi");
add(
  "package",
  "msix msixbundle appx appxbundle deb rpm apk dmg pkg appimage flatpak snap vsix nupkg",
);
add("archive", "zip rar 7z tar gz bz2 xz zst tgz cab lz lzma");
add("disk_image", "iso img vhd vhdx");
add(
  "code",
  "js mjs cjs jsx ts tsx py rs go java kt c h cpp hpp cc cs rb php swift html htm css scss vue svelte lua dart r scala sql",
);
add("data", "json xml yaml yml toml ini parquet sqlite db ndjson geojson");
add("script", "sh ps1 psm1 bat cmd vbs reg");
add("notebook", "ipynb");
add("torrent", "torrent");
add("font", "ttf otf woff woff2");

export function extensionKind(name: string): FileKind | null {
  const i = name.lastIndexOf(".");
  if (i <= 0) return null;
  return EXT[name.slice(i + 1).toLowerCase()] ?? null;
}

export const KIND_LABELS: Record<FileKind, string> = {
  pdf: "PDF document",
  word: "Word document",
  text: "Text document",
  ebook: "E-book",
  spreadsheet: "Spreadsheet",
  presentation: "Presentation",
  report: "Report file",
  image: "Image",
  video: "Video",
  audio: "Audio",
  subtitle: "Subtitles",
  installer: "Installer",
  package: "Software package",
  archive: "Archive",
  code: "Source code",
  data: "Data file",
  script: "Script",
  notebook: "Notebook",
  torrent: "Torrent metadata",
  font: "Font",
  disk_image: "Disk image",
};
