import { describe, expect, it } from "vitest";
import { folderOf, formatBytes, plural, timeAgo, winPath } from "./format";
import { extensionKind } from "./kinds";

describe("format", () => {
  it("formats bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 ** 3)).toBe("5.0 GB");
    expect(formatBytes(null)).toBe("—");
  });

  it("pluralizes", () => {
    expect(plural(1, "file")).toBe("1 file");
    expect(plural(3, "file")).toBe("3 files");
  });

  it("formats relative time", () => {
    const now = Date.parse("2026-10-23T12:00:00Z");
    expect(timeAgo(now - 10_000, now)).toBe("just now");
    expect(timeAgo(null, now)).toBe("never");
    expect(timeAgo(now - 3 * 3600_000, now)).toMatch(/3 hours ago/);
  });

  it("displays Windows paths", () => {
    expect(winPath("Documents/PDF/a.pdf")).toBe("Documents\\PDF\\a.pdf");
    expect(folderOf("Documents/PDF/a.pdf")).toBe("Documents/PDF");
    expect(folderOf("a.pdf")).toBe("");
  });

  it("maps extensions for icons", () => {
    expect(extensionKind("Report.PDF")).toBe("pdf");
    expect(extensionKind("archive.tar.gz")).toBe("archive");
    expect(extensionKind("README")).toBeNull();
    expect(extensionKind(".gitignore")).toBeNull();
  });
});
