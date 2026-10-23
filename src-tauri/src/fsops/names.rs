//! Filename utilities: extension splitting and collision-free naming.

use std::collections::HashSet;
use std::path::Path;

/// Multi-part extensions that should stay together when adding a suffix.
const COMPOUND_EXTS: &[&str] = &[
    "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.lz", "user.js",
];

/// Splits `name` into `(stem, extension_with_dot)`.
/// `report.final.pdf` -> (`report.final`, `.pdf`), `a.tar.gz` -> (`a`, `.tar.gz`),
/// `.gitignore` -> (`.gitignore`, ``), `README` -> (`README`, ``).
pub fn split_name(name: &str) -> (&str, &str) {
    let lower = name.to_lowercase();
    for ext in COMPOUND_EXTS {
        let suffix = format!(".{ext}");
        if lower.ends_with(&suffix) && lower.len() > suffix.len() {
            let cut = name.len() - suffix.len();
            if name.is_char_boundary(cut) {
                return (&name[..cut], &name[cut..]);
            }
        }
    }
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(i) if i == name.len() - 1 => (name, ""),
        Some(i) => (&name[..i], &name[i..]),
    }
}

/// Lowercase extension without the dot (`pdf`, `tar.gz`), or `None`.
pub fn extension_of(name: &str) -> Option<String> {
    let (_, ext) = split_name(name);
    if ext.is_empty() {
        None
    } else {
        Some(ext[1..].to_lowercase())
    }
}

/// `invoice (2)` -> (`invoice`, Some(2)). Avoids `invoice (1) (1).pdf`.
pub fn strip_counter(stem: &str) -> (&str, Option<u32>) {
    if let Some(open) = stem.rfind(" (") {
        let tail = &stem[open + 2..];
        if let Some(num) = tail.strip_suffix(')') {
            if !num.is_empty() && num.len() <= 6 && num.bytes().all(|b| b.is_ascii_digit()) {
                return (&stem[..open], num.parse().ok());
            }
        }
    }
    (stem, None)
}

/// Case-insensitive key matching NTFS name comparison closely enough for
/// collision detection.
pub fn name_key(name: &str) -> String {
    crate::security::upper_simple(name)
}

fn with_counter(base: &str, ext: &str, n: u32) -> String {
    let suffix = format!(" ({n}){ext}");
    // Keep the result within the 255-character component limit.
    let max_base = 255usize.saturating_sub(suffix.chars().count());
    let base: String = base.chars().take(max_base).collect();
    format!("{base}{suffix}")
}

/// Returns `desired` if free in `dir`, otherwise the first free `name (n).ext`.
/// `reserved` holds name keys already claimed by other planned moves. A
/// `dir` of `None` means the folder does not exist yet.
pub fn next_free_name(dir: Option<&Path>, desired: &str, reserved: &HashSet<String>) -> String {
    let free = |n: &str| {
        !reserved.contains(&name_key(n))
            && dir.is_none_or(|d| std::fs::symlink_metadata(d.join(n)).is_err())
    };
    if free(desired) {
        return desired.to_string();
    }
    let (stem, ext) = split_name(desired);
    let (base, start) = strip_counter(stem);
    let mut n = start.map(|s| s + 1).unwrap_or(1).max(1);
    loop {
        let candidate = with_counter(base, ext, n);
        if free(&candidate) {
            return candidate;
        }
        n += 1;
        if n > 100_000 {
            // Practically unreachable; fall back to a unique token.
            return with_counter(base, ext, uuid::Uuid::new_v4().as_u128() as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_names() {
        assert_eq!(split_name("report.pdf"), ("report", ".pdf"));
        assert_eq!(
            split_name("report.final.v2.pdf"),
            ("report.final.v2", ".pdf")
        );
        assert_eq!(split_name("backup.TAR.GZ"), ("backup", ".TAR.GZ"));
        assert_eq!(split_name(".gitignore"), (".gitignore", ""));
        assert_eq!(split_name("README"), ("README", ""));
        assert_eq!(split_name("weird."), ("weird.", ""));
        assert_eq!(split_name("Ø³Ù†Ø¯.pdf"), ("Ø³Ù†Ø¯", ".pdf"));
        assert_eq!(extension_of("Photo.JPG").as_deref(), Some("jpg"));
        assert_eq!(extension_of("noext"), None);
    }

    #[test]
    fn strips_counters() {
        assert_eq!(strip_counter("invoice (2)"), ("invoice", Some(2)));
        assert_eq!(strip_counter("invoice"), ("invoice", None));
        assert_eq!(strip_counter("invoice (draft)"), ("invoice (draft)", None));
        assert_eq!(strip_counter("a (1) (3)"), ("a (1)", Some(3)));
    }

    #[test]
    fn collision_names_in_memory() {
        let dir = None;
        let mut reserved = HashSet::new();
        assert_eq!(next_free_name(dir, "invoice.pdf", &reserved), "invoice.pdf");
        reserved.insert(name_key("invoice.pdf"));
        assert_eq!(
            next_free_name(dir, "INVOICE.pdf", &reserved),
            "INVOICE (1).pdf"
        );
        reserved.insert(name_key("invoice (1).pdf"));
        assert_eq!(
            next_free_name(dir, "invoice.pdf", &reserved),
            "invoice (2).pdf"
        );
        reserved.insert(name_key("invoice (2).pdf"));
        assert_eq!(
            next_free_name(dir, "invoice (1).pdf", &reserved),
            "invoice (3).pdf"
        );
        let long = format!("{}.pdf", "x".repeat(251));
        reserved.insert(name_key(&long));
        let next = next_free_name(dir, &long, &reserved);
        assert!(next.chars().count() <= 255 && next.ends_with(" (1).pdf"));
    }
}
