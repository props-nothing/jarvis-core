//! Structured logging, correlation, and secret redaction for JARVIS.
//!
//! Owns the tracing setup, the redaction rules applied at every sink, and the
//! readers that back `jarvis logs`. It is an adapter crate: it depends on
//! `jarvis-core` for vocabulary and holds no business state.

mod logging;
mod redact;

pub use logging::{
    DEFAULT_TAIL_LINES, LOG_FILE_NAME, LogLine, LogStorageStats, Logging, LoggingError,
    MAX_LOG_FILE_BYTES, MAX_TAIL_BYTES, MAX_TAIL_LINES, RETAINED_LOG_FILES, count_lines,
    log_storage_stats, read_tail,
};
pub use redact::{MAX_LINE_BYTES, MAX_SECRETS, MIN_SECRET_BYTES, Redactor};
