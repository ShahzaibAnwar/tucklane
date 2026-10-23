//! Typed records shared between the database layer and Tauri commands.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Planned,
    InProgress,
    Completed,
    Skipped,
    Failed,
    Undone,
    RecoveryRequired,
}

impl OpState {
    pub fn as_str(self) -> &'static str {
        match self {
            OpState::Planned => "planned",
            OpState::InProgress => "in_progress",
            OpState::Completed => "completed",
            OpState::Skipped => "skipped",
            OpState::Failed => "failed",
            OpState::Undone => "undone",
            OpState::RecoveryRequired => "recovery_required",
        }
    }

    pub fn parse(s: &str) -> OpState {
        match s {
            "planned" => OpState::Planned,
            "in_progress" => OpState::InProgress,
            "completed" => OpState::Completed,
            "skipped" => OpState::Skipped,
            "undone" => OpState::Undone,
            "recovery_required" => OpState::RecoveryRequired,
            _ => OpState::Failed,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpKind {
    Move,
    Undo,
    Recycle,
}

impl OpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OpKind::Move => "move",
            OpKind::Undo => "undo",
            OpKind::Recycle => "recycle",
        }
    }
    pub fn parse(s: &str) -> OpKind {
        match s {
            "undo" => OpKind::Undo,
            "recycle" => OpKind::Recycle,
            _ => OpKind::Move,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Manual,
    Auto,
    Undo,
    Retry,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionKind::Manual => "manual",
            SessionKind::Auto => "auto",
            SessionKind::Undo => "undo",
            SessionKind::Retry => "retry",
        }
    }
    pub fn parse(s: &str) -> SessionKind {
        match s {
            "auto" => SessionKind::Auto,
            "undo" => SessionKind::Undo,
            "retry" => SessionKind::Retry,
            _ => SessionKind::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: String,
    pub session_id: String,
    pub kind: OpKind,
    pub state: OpState,
    pub source_rel: String,
    pub dest_dir_rel: String,
    pub dest_rel: Option<String>,
    pub original_name: String,
    pub final_name: Option<String>,
    pub size: Option<i64>,
    pub mtime_ms: Option<i64>,
    pub file_id: Option<String>,
    pub sha256: Option<String>,
    pub method: Option<String>,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub reason: Option<String>,
    pub error: Option<String>,
    pub undo_of: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub id: String,
    pub kind: SessionKind,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub total: i64,
    pub completed: i64,
    pub skipped: i64,
    pub failed: i64,
    pub undone_at: Option<String>,
    /// Completed moves in this session that can still be undone.
    pub undoable: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRecord {
    pub id: i64,
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub session_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateMember {
    pub rel: String,
    pub name: String,
    pub size: u64,
    pub modified_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub id: String,
    pub hash: String,
    pub size: u64,
    pub members: Vec<DuplicateMember>,
    pub scanned_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootInfo {
    pub id: String,
    pub path: String,
    pub selected_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExclusionRecord {
    pub rel_path: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryQuery {
    pub search: Option<String>,
    pub state: Option<String>,
    pub kind: Option<String>,
    pub session_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
