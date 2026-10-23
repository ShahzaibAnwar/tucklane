//! Which folders may be authorized as the managed root.
//!
//! The rules are deliberately conservative: the root must be an existing,
//! local, non-link directory that is not a drive root, not a system or
//! program directory, not application data, and not an ancestor of the
//! user's profile.

use std::path::{Component, Path, PathBuf};

use super::{compare_key, display_path, is_within, query_entry};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default)]
pub struct RootPolicy {
    /// The root may not be one of these or lie inside one of them.
    forbidden_subtrees: Vec<PathBuf>,
    /// The root may not be one of these or be an ancestor of one of them.
    forbidden_ancestors_of: Vec<PathBuf>,
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl RootPolicy {
    /// Policy used by the application. `protected` lists Tucklane's own data,
    /// log and installation directories.
    pub fn system(protected: &[PathBuf]) -> Self {
        let mut subtrees: Vec<PathBuf> = [
            "SystemRoot",
            "windir",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "ProgramData",
            "APPDATA",
            "LOCALAPPDATA",
        ]
        .iter()
        .filter_map(|v| env_path(v))
        .collect();
        if let Some(local) = env_path("LOCALAPPDATA") {
            if let Some(parent) = local.parent() {
                subtrees.push(parent.join("LocalLow"));
            }
        }
        if let Some(drive) = env_path("SystemDrive") {
            let d = PathBuf::from(format!("{}\\", drive.display()));
            subtrees.push(d.join("$Recycle.Bin"));
            subtrees.push(d.join("System Volume Information"));
            subtrees.push(d.join("Recovery"));
        }
        subtrees.extend(protected.iter().cloned());

        let mut ancestors = subtrees.clone();
        if let Some(profile) = env_path("USERPROFILE") {
            ancestors.push(profile);
        }
        if let Some(public) = env_path("PUBLIC") {
            ancestors.push(public);
        }
        Self {
            forbidden_subtrees: subtrees,
            forbidden_ancestors_of: ancestors,
        }
    }

    /// Adds an extra protected directory (used in tests and for the app dirs).
    pub fn protect(mut self, p: PathBuf) -> Self {
        self.forbidden_subtrees.push(p.clone());
        self.forbidden_ancestors_of.push(p);
        self
    }

    /// Validates a candidate root and returns its canonical form.
    pub fn validate(&self, candidate: &Path) -> AppResult<PathBuf> {
        if !candidate.is_absolute() {
            return Err(AppError::InvalidRoot(
                "Please choose a full folder path.".into(),
            ));
        }
        let shown = display_path(candidate);
        if shown.starts_with(r"\\") {
            return Err(AppError::InvalidRoot(
                "Network locations cannot be managed by Tucklane. Choose a folder on this PC."
                    .into(),
            ));
        }
        let info = query_entry(candidate).map_err(|_| {
            AppError::InvalidRoot(format!("\"{shown}\" does not exist or cannot be opened."))
        })?;
        if !info.is_dir {
            return Err(AppError::InvalidRoot(
                "Please choose a folder, not a file.".into(),
            ));
        }
        if info.is_unsafe_link {
            return Err(AppError::InvalidRoot(
                "That folder is a link or junction. Choose the real folder it points to.".into(),
            ));
        }
        let canonical = info.final_path;
        let canon_shown = display_path(&canonical);
        if canon_shown.starts_with(r"\\") {
            return Err(AppError::InvalidRoot(
                "Network locations cannot be managed by Tucklane. Choose a folder on this PC."
                    .into(),
            ));
        }
        #[cfg(windows)]
        if super::winapi::is_remote_drive(&canonical) {
            return Err(AppError::InvalidRoot(
                "Network drives cannot be managed by Tucklane. Choose a folder on this PC.".into(),
            ));
        }
        let normal_components = Path::new(&canon_shown)
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count();
        if normal_components == 0 {
            return Err(AppError::InvalidRoot(
                "A whole drive cannot be managed. Choose a specific folder such as Downloads."
                    .into(),
            ));
        }
        for p in &self.forbidden_subtrees {
            if is_within(p, &canonical) {
                return Err(AppError::InvalidRoot(format!(
                    "\"{canon_shown}\" is a system or application folder and cannot be managed."
                )));
            }
        }
        for p in &self.forbidden_ancestors_of {
            if is_within(&canonical, p) {
                return Err(AppError::InvalidRoot(format!(
                    "\"{canon_shown}\" contains your user profile or system folders. Choose a more specific folder such as Downloads."
                )));
            }
        }
        // Defensive: the canonical form must agree with what we checked.
        debug_assert_eq!(
            compare_key(&canonical),
            compare_key(Path::new(&canon_shown))
        );
        Ok(canonical)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn rejects_dangerous_roots() {
        let policy = RootPolicy::system(&[]);
        for p in [
            r"C:\",
            r"C:\Windows",
            r"C:\Windows\System32",
            r"C:\Program Files",
            r"C:\Users",
        ] {
            assert!(policy.validate(Path::new(p)).is_err(), "should reject {p}");
        }
        if let Some(profile) = env_path("USERPROFILE") {
            assert!(policy.validate(&profile).is_err());
        }
        if let Some(local) = env_path("LOCALAPPDATA") {
            assert!(policy.validate(&local.join("Temp")).is_err());
        }
        assert!(policy.validate(Path::new("relative\\path")).is_err());
        assert!(policy
            .validate(Path::new(r"\\server\share\Downloads"))
            .is_err());
    }
}
