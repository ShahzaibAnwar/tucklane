//! Application error type. Errors are serialized to the frontend as
//! `{ code, message }` so the UI can react to specific failure kinds without
//! parsing strings. Messages never include file contents.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("No folder has been authorized yet.")]
    NoRoot,
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    InvalidRoot(String),
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("The file is in use by another program.")]
    Locked,
    #[error("The file changed since it was scanned.")]
    Changed,
    #[error("There is not enough free disk space.")]
    DiskFull,
    #[error("Windows denied access to the file.")]
    PermissionDenied,
    #[error("{0}")]
    Busy(String),
    #[error("Cancelled.")]
    Cancelled,
    /// The action is allowed but the user must confirm it first.
    #[error("{0}")]
    ConfirmRequired(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("{0}")]
    Io(String),
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NoRoot => "no_root",
            AppError::Unauthorized(_) => "unauthorized",
            AppError::InvalidRoot(_) => "invalid_root",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::NotFound(_) => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Locked => "locked",
            AppError::Changed => "changed",
            AppError::DiskFull => "disk_full",
            AppError::PermissionDenied => "permission_denied",
            AppError::Busy(_) => "busy",
            AppError::Cancelled => "cancelled",
            AppError::ConfirmRequired(_) => "confirm_required",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
        }
    }

    pub fn unauthorized(msg: impl Into<String>) -> Self {
        AppError::Unauthorized(msg.into())
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        AppError::InvalidInput(msg.into())
    }
}

/// Maps raw OS errors to user-meaningful variants.
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        // Windows error codes: https://learn.microsoft.com/windows/win32/debug/system-error-codes
        match e.raw_os_error() {
            Some(32) | Some(33) => AppError::Locked, // SHARING_VIOLATION, LOCK_VIOLATION
            Some(5) => AppError::PermissionDenied,
            Some(39) | Some(112) => AppError::DiskFull,
            Some(80) | Some(183) => {
                AppError::Conflict("A file with that name already exists.".into())
            }
            Some(2) | Some(3) => AppError::NotFound("The file no longer exists.".into()),
            _ => match e.kind() {
                std::io::ErrorKind::NotFound => {
                    AppError::NotFound("The file no longer exists.".into())
                }
                std::io::ErrorKind::PermissionDenied => AppError::PermissionDenied,
                std::io::ErrorKind::AlreadyExists => {
                    AppError::Conflict("A file with that name already exists.".into())
                }
                _ => AppError::Io(e.to_string()),
            },
        }
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Database(format!("serialization: {e}"))
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}
