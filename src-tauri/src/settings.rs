//! Persistent user settings. Stored one key per row in the `settings` table
//! and merged onto defaults, so new settings get sensible values on upgrade.

use serde::{Deserialize, Serialize};

use crate::classifier::kinds::Preset;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CollisionPolicy {
    #[default]
    Rename,
    Skip,
    Ask,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum UnknownFiles {
    /// Leave unrecognized files in place for manual review.
    #[default]
    Leave,
    /// Move unrecognized files into `Other`.
    MoveToOther,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub onboarding_complete: bool,
    pub theme: Theme,
    pub language: String,
    pub launch_at_startup: bool,
    /// Closing the window keeps Tucklane running in the tray.
    pub close_to_tray: bool,
    pub show_tray_icon: bool,
    pub notifications: bool,
    /// When launched by Windows at sign-in, stay in the tray.
    pub start_minimized: bool,
    pub preset: Preset,
    pub preview_before_organize: bool,
    pub auto_organize: bool,
    pub monitoring_paused: bool,
    pub settle_seconds: u32,
    /// Periodic safety rescan, in minutes. 0 disables it.
    pub scan_interval_minutes: u32,
    pub unknown_files: UnknownFiles,
    pub collision: CollisionPolicy,
    pub ignored_extensions: Vec<String>,
    /// Wildcard patterns for filenames Tucklane must never move.
    pub protected_patterns: Vec<String>,
    pub duplicate_detection: bool,
    pub confirm_destructive: bool,
    pub history_retention_days: u32,
    /// Local filename keyword hints such as "invoice" → Documents/Invoices.
    pub smart_filename_hints: bool,
    /// Read the first bytes of files with missing/unknown extensions to
    /// identify their type. Opt-in: it touches file contents.
    pub content_signatures: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            theme: Theme::System,
            language: "en".into(),
            launch_at_startup: false,
            close_to_tray: true,
            show_tray_icon: true,
            notifications: true,
            start_minimized: true,
            preset: Preset::Balanced,
            preview_before_organize: true,
            auto_organize: false,
            monitoring_paused: false,
            settle_seconds: 15,
            scan_interval_minutes: 30,
            unknown_files: UnknownFiles::Leave,
            collision: CollisionPolicy::Rename,
            ignored_extensions: vec![],
            protected_patterns: vec!["desktop.ini".into(), "*.lnk".into()],
            duplicate_detection: true,
            confirm_destructive: true,
            history_retention_days: 180,
            smart_filename_hints: false,
            content_signatures: false,
        }
    }
}

impl Settings {
    /// Clamps values into safe ranges and normalizes lists.
    pub fn validate(mut self) -> AppResult<Self> {
        if !(3..=3600).contains(&self.settle_seconds) {
            return Err(AppError::invalid(
                "Settling time must be between 3 and 3600 seconds.",
            ));
        }
        if self.scan_interval_minutes > 1440 {
            return Err(AppError::invalid("Scan interval must be at most 24 hours."));
        }
        if !(7..=3650).contains(&self.history_retention_days) {
            return Err(AppError::invalid(
                "History retention must be between 7 and 3650 days.",
            ));
        }
        if self.ignored_extensions.len() > 200 || self.protected_patterns.len() > 200 {
            return Err(AppError::invalid("Too many entries."));
        }
        self.ignored_extensions = self
            .ignored_extensions
            .iter()
            .map(|e| e.trim().trim_start_matches('.').to_lowercase())
            .filter(|e| !e.is_empty() && e.len() <= 20)
            .collect();
        self.ignored_extensions.sort();
        self.ignored_extensions.dedup();
        self.protected_patterns = self
            .protected_patterns
            .iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty() && p.len() <= 200)
            .collect();
        if self.language.is_empty() || self.language.len() > 10 {
            self.language = "en".into();
        }
        Ok(self)
    }
}
