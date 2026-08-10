use std::fmt;

/// A single error type spanning every store/pipeline module in this crate.
/// Kept deliberately flat (no per-module error enums) since every failure
/// mode here ultimately comes down to "disk I/O failed" or "sqlite failed" —
/// splitting further would just add `From` boilerplate without helping
/// callers, who mostly map this straight to `tonic::Status::internal`.
#[derive(Debug)]
pub enum CoreError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    /// A file extension has no registered extractor. Not fatal — callers
    /// treat this as "index as metadata-only" per `docs/ROADMAP.md` M5's
    /// exit criteria, not an error to surface to the RPC caller.
    UnsupportedFileType,
    Extract(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Io(e) => write!(f, "io error: {e}"),
            CoreError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            CoreError::UnsupportedFileType => write!(f, "unsupported file type"),
            CoreError::Extract(msg) => write!(f, "extraction failed: {msg}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::Io(e)
    }
}

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Sqlite(e)
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
