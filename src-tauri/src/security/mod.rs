//! Folder authorization. Every filesystem path Tucklane touches on behalf of
//! the user flows through [`AuthorizedRootGuard`]. The frontend only ever
//! sends paths *relative* to the authorized root; the guard rejects anything
//! that could escape it (absolute paths, `..`, alternate data streams, links,
//! junctions and other unsafe reparse points).

pub mod policy;
#[cfg(windows)]
pub mod winapi;

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
pub use policy::RootPolicy;

/// Platform-neutral facts about a path, obtained from an open handle that did
/// not follow links.
#[derive(Debug, Clone)]
pub struct EntryInfo {
    pub final_path: PathBuf,
    pub is_dir: bool,
    /// The entry is a symbolic link, junction, mount point or other reparse
    /// point that Tucklane refuses to follow or move.
    pub is_unsafe_link: bool,
    pub hidden_or_system: bool,
    pub size: u64,
    pub modified_ms: i64,
    pub file_id: String,
}

/// Reparse tags that are storage implementation details rather than links:
/// OneDrive/cloud placeholders, data deduplication and WOF compression.
pub fn is_benign_reparse_tag(tag: u32) -> bool {
    const NAME_SURROGATE: u32 = 0x2000_0000;
    if tag & NAME_SURROGATE != 0 {
        return false;
    }
    (tag & 0xFFFF_0FFF) == 0x9000_001A // IO_REPARSE_TAG_CLOUD_*
        || tag == 0x8000_0013 // IO_REPARSE_TAG_DEDUP
        || tag == 0x8000_0017 // IO_REPARSE_TAG_WOF
}

#[cfg(windows)]
pub fn query_entry(path: &Path) -> std::io::Result<EntryInfo> {
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    let f = winapi::open_for_query(path)?;
    let info = winapi::handle_info(&f)?;
    Ok(EntryInfo {
        is_unsafe_link: info.reparse_tag.is_some_and(|t| !is_benign_reparse_tag(t)),
        hidden_or_system: info.attributes & (HIDDEN | SYSTEM) != 0,
        final_path: info.final_path,
        is_dir: info.is_dir,
        size: info.size,
        modified_ms: info.modified_ms,
        file_id: info.file_id,
    })
}

#[cfg(not(windows))]
pub fn query_entry(path: &Path) -> std::io::Result<EntryInfo> {
    let md = std::fs::symlink_metadata(path)?;
    let is_link = md.file_type().is_symlink();
    let final_path = if is_link {
        path.to_path_buf()
    } else {
        std::fs::canonicalize(path)?
    };
    let modified_ms = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    #[cfg(unix)]
    let file_id = {
        use std::os::unix::fs::MetadataExt;
        format!("{:x}-{:x}", md.dev(), md.ino())
    };
    #[cfg(not(unix))]
    let file_id = String::new();
    let hidden = path
        .file_name()
        .map(|n| n.to_string_lossy().starts_with('.'))
        .unwrap_or(false);
    Ok(EntryInfo {
        final_path,
        is_dir: md.is_dir(),
        is_unsafe_link: is_link,
        hidden_or_system: hidden,
        size: md.len(),
        modified_ms,
        file_id,
    })
}

/// Normalizes a path for case-insensitive, prefix-insensitive comparison.
pub fn compare_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s
    };
    upper_simple(s.trim_end_matches('\\'))
}

/// Per-character uppercase that never changes length (unlike
/// `str::to_uppercase`, which maps `ß` to `SS`). Closer to NTFS's
/// one-to-one upcase table, so distinct names never compare equal.
pub fn upper_simple(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        })
        .collect()
}

/// True if `child` is `parent` or lies beneath it.
pub fn is_within(parent: &Path, child: &Path) -> bool {
    let p = compare_key(parent);
    let c = compare_key(child);
    c == p || (c.starts_with(&p) && c.as_bytes().get(p.len()) == Some(&b'\\'))
}

/// Strips the `\\?\` prefix for display.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        s.into_owned()
    }
}

const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "COM¹", "COM²", "COM³", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9", "LPT¹", "LPT²", "LPT³", "CONIN$", "CONOUT$",
];

/// True for DOS device names such as `CON` or `com1.txt`.
pub fn is_reserved_name(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    RESERVED_NAMES.contains(&stem.as_str())
}

/// How strictly to validate a relative path component.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NameRule {
    /// The name refers to an entry that already exists on disk.
    Existing,
    /// Tucklane is about to create this name; it must be portable and safe.
    Create,
}

/// Validates a single path component (file or folder name).
pub fn validate_component(c: &str, rule: NameRule) -> AppResult<()> {
    if c.is_empty() || c == "." || c == ".." {
        return Err(AppError::unauthorized("Path traversal is not allowed."));
    }
    if c.len() > 255 * 4 || c.chars().count() > 255 {
        return Err(AppError::invalid("A file or folder name is too long."));
    }
    if c.chars()
        .any(|ch| (ch as u32) < 0x20 || "<>:\"|?*/\\".contains(ch))
    {
        return Err(AppError::invalid(format!(
            "\"{c}\" contains characters Windows does not allow."
        )));
    }
    if rule == NameRule::Create {
        if c.ends_with('.') || c.ends_with(' ') || c.starts_with(' ') {
            return Err(AppError::invalid(format!(
                "\"{c}\" cannot start or end with a space or period."
            )));
        }
        if is_reserved_name(c) {
            return Err(AppError::invalid(format!(
                "\"{c}\" is a reserved Windows name."
            )));
        }
    }
    Ok(())
}

/// Splits and validates an untrusted relative path. Accepts `/` or `\`.
pub fn split_rel(rel: &str, rule: NameRule) -> AppResult<Vec<String>> {
    if rel.len() > 8192 {
        return Err(AppError::invalid("Path is too long."));
    }
    if rel.starts_with('/') || rel.starts_with('\\') || rel.contains(':') || rel.contains('\0') {
        return Err(AppError::unauthorized(
            "Only paths inside the authorized folder are allowed.",
        ));
    }
    let parts: Vec<String> = rel.split(['/', '\\']).map(str::to_string).collect();
    if parts.is_empty() || (parts.len() == 1 && parts[0].is_empty()) {
        return Err(AppError::invalid("Path is empty."));
    }
    for p in &parts {
        validate_component(p, rule)?;
    }
    Ok(parts)
}

/// The single source of truth for "is Tucklane allowed to touch this path?".
#[derive(Debug, Clone)]
pub struct AuthorizedRootGuard {
    id: String,
    root: PathBuf,
    display: String,
    root_file_id: String,
}

impl AuthorizedRootGuard {
    /// Validates `path` against `policy` and returns a guard for it.
    pub fn open(path: &Path, id: impl Into<String>, policy: &RootPolicy) -> AppResult<Self> {
        let canonical = policy.validate(path)?;
        let info = query_entry(&canonical).map_err(|e| AppError::InvalidRoot(e.to_string()))?;
        Ok(Self {
            id: id.into(),
            display: display_path(&canonical),
            root: canonical,
            root_file_id: info.file_id,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Canonical root in `\\?\` form (long-path safe).
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn display_root(&self) -> &str {
        &self.display
    }

    /// Confirms the root still exists, is the same directory that was
    /// authorized, and has not been swapped for a link.
    pub fn verify_root(&self) -> AppResult<()> {
        let info = query_entry(&self.root).map_err(|_| {
            AppError::InvalidRoot("The authorized folder is no longer available.".into())
        })?;
        if !info.is_dir || info.is_unsafe_link {
            return Err(AppError::InvalidRoot(
                "The authorized folder is no longer a normal folder.".into(),
            ));
        }
        if !self.root_file_id.is_empty() && info.file_id != self.root_file_id {
            return Err(AppError::InvalidRoot(
                "The authorized folder was replaced. Please re-authorize it in Settings.".into(),
            ));
        }
        if compare_key(&info.final_path) != compare_key(&self.root) {
            return Err(AppError::InvalidRoot("The authorized folder moved.".into()));
        }
        Ok(())
    }

    /// Lexically joins a validated relative path onto the root. Does not touch disk.
    pub fn join_rel(&self, rel: &str, rule: NameRule) -> AppResult<PathBuf> {
        let parts = split_rel(rel, rule)?;
        let mut p = self.root.clone();
        for c in parts {
            p.push(c);
        }
        Ok(p)
    }

    /// Ensures an absolute path (e.g. from a directory listing) is inside the
    /// root, and returns it in canonical form.
    pub fn check_contained(&self, abs: &Path) -> AppResult<()> {
        if is_within(&self.root, abs) {
            Ok(())
        } else {
            Err(AppError::unauthorized(
                "That location is outside the authorized folder.",
            ))
        }
    }

    /// Converts an absolute path inside the root to a `/`-separated relative path.
    pub fn rel_of(&self, abs: &Path) -> AppResult<String> {
        self.check_contained(abs)?;
        let root_key = display_path(&self.root);
        let abs_s = display_path(abs);
        let rest = abs_s
            .get(root_key.len()..)
            .unwrap_or("")
            .trim_start_matches('\\');
        if rest.is_empty() {
            return Err(AppError::unauthorized(
                "The authorized folder itself cannot be used here.",
            ));
        }
        Ok(rest.replace('\\', "/"))
    }

    /// Resolves a relative path to an existing entry. Every component between
    /// the root and the target is checked so that no link or junction is
    /// traversed, and the handle-reported final path must remain inside the root.
    pub fn resolve_existing(&self, rel: &str) -> AppResult<(PathBuf, EntryInfo)> {
        let parts = split_rel(rel, NameRule::Existing)?;
        let mut p = self.root.clone();
        let mut last = None;
        for (i, c) in parts.iter().enumerate() {
            p.push(c);
            let info = query_entry(&p).map_err(AppError::from)?;
            if info.is_unsafe_link {
                return Err(AppError::unauthorized(format!(
                    "\"{c}\" is a link or junction. Tucklane does not follow links."
                )));
            }
            if i + 1 < parts.len() && !info.is_dir {
                return Err(AppError::NotFound("The folder no longer exists.".into()));
            }
            last = Some(info);
        }
        let info = last.ok_or_else(|| AppError::invalid("Path is empty."))?;
        if !is_within(&self.root, &info.final_path)
            || compare_key(&info.final_path) == compare_key(&self.root)
        {
            return Err(AppError::unauthorized(
                "That location is outside the authorized folder.",
            ));
        }
        Ok((p, info))
    }

    /// Checks a destination directory without creating it. Returns the path
    /// if it exists as a safe directory, `None` if (part of) it does not exist
    /// yet, or an error if a link or file is in the way.
    pub fn probe_dir(&self, rel_dir: &str) -> AppResult<Option<PathBuf>> {
        if rel_dir.is_empty() {
            return Ok(Some(self.root.clone()));
        }
        let parts = split_rel(rel_dir, NameRule::Existing)?;
        let mut p = self.root.clone();
        for c in &parts {
            p.push(c);
            match query_entry(&p) {
                Ok(info) if info.is_unsafe_link => {
                    return Err(AppError::unauthorized(format!(
                        "\"{c}\" is a link or junction. Tucklane does not follow links."
                    )))
                }
                Ok(info) if !info.is_dir => {
                    return Err(AppError::Conflict(format!(
                        "A file named \"{c}\" is in the way of the destination folder."
                    )))
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    validate_component(c, NameRule::Create)?;
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(p))
    }

    /// Creates (if needed) a directory inside the root, one component at a
    /// time, refusing to traverse links.
    pub fn ensure_dir(&self, rel_dir: &str) -> AppResult<PathBuf> {
        let parts = split_rel(rel_dir, NameRule::Existing)?;
        let mut p = self.root.clone();
        for c in &parts {
            p.push(c);
            match query_entry(&p) {
                Ok(info) => {
                    if info.is_unsafe_link {
                        return Err(AppError::unauthorized(format!(
                            "\"{c}\" is a link or junction. Tucklane does not follow links."
                        )));
                    }
                    if !info.is_dir {
                        return Err(AppError::Conflict(format!(
                            "A file named \"{c}\" is in the way of the destination folder."
                        )));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    validate_component(c, NameRule::Create)?;
                    match std::fs::create_dir(&p) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(e) => return Err(e.into()),
                    }
                    let info = query_entry(&p)?;
                    if info.is_unsafe_link || !info.is_dir {
                        return Err(AppError::unauthorized(
                            "Destination folder is not a normal folder.",
                        ));
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        let info = query_entry(&p)?;
        if !is_within(&self.root, &info.final_path) {
            return Err(AppError::unauthorized(
                "That location is outside the authorized folder.",
            ));
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_absolute() {
        for bad in [
            "..",
            "../x",
            "a/../../b",
            "C:\\Windows",
            "\\\\server\\share",
            "/etc",
            "file.txt:stream",
            "a//b",
            "",
            "a/./b",
            "\\\\?\\C:\\x",
        ] {
            assert!(
                split_rel(bad, NameRule::Existing).is_err(),
                "should reject {bad:?}"
            );
        }
        assert!(split_rel("Documents/PDF/report.pdf", NameRule::Existing).is_ok());
        assert!(split_rel("Media\\Images\\ٹیسٹ.png", NameRule::Existing).is_ok());
    }

    #[test]
    fn reserved_names() {
        assert!(is_reserved_name("CON"));
        assert!(is_reserved_name("con.txt"));
        assert!(is_reserved_name("LPT1.tar.gz"));
        assert!(!is_reserved_name("console.txt"));
        assert!(validate_component("NUL", NameRule::Create).is_err());
        assert!(validate_component("NUL", NameRule::Existing).is_ok());
        assert!(validate_component("trailing.", NameRule::Create).is_err());
    }

    #[test]
    fn containment_is_case_insensitive_and_boundary_safe() {
        let root = Path::new(r"\\?\C:\Users\A\Downloads");
        assert!(is_within(root, Path::new(r"C:\users\a\downloads\x.pdf")));
        assert!(is_within(root, Path::new(r"\\?\C:\Users\A\Downloads")));
        assert!(!is_within(root, Path::new(r"C:\Users\A\Downloads2\x.pdf")));
        assert!(!is_within(root, Path::new(r"C:\Users\A\Desktop\x.pdf")));
        // Length-changing case mappings must not merge distinct names.
        let strasse = Path::new(r"D:\Straße");
        assert!(!is_within(strasse, Path::new(r"D:\Strasse\x.pdf")));
        assert!(is_within(strasse, Path::new(r"d:\straße\x.pdf")));
    }

    #[test]
    fn benign_tags() {
        assert!(is_benign_reparse_tag(0x9000_001A));
        assert!(is_benign_reparse_tag(0x9000_101A));
        assert!(!is_benign_reparse_tag(0xA000_000C)); // symlink
        assert!(!is_benign_reparse_tag(0xA000_0003)); // junction
        assert!(!is_benign_reparse_tag(0x8000_001B)); // app exec link
    }
}
