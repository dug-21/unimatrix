//! `EvalError` — structured errors for the eval subsystem (nan-007).

use std::fmt;
use std::path::PathBuf;

/// Structured errors for the eval subsystem (no panics, no raw serde errors).
///
/// All variants produce user-readable messages that name the invariant
/// violated and the relevant paths or values (SR-08, SR-09).
#[derive(Debug)]
pub enum EvalError {
    /// A model file referenced in `[inference]` section is missing or unreadable.
    ///
    /// Returned at `from_profile()` time, never at inference time (C-14, FR-23).
    ModelNotFound(PathBuf),

    /// A config invariant was violated (weight sum, TOML parse error, etc.).
    ///
    /// The string is a user-readable message naming the expected and actual
    /// values. Never a raw serde error (C-06, C-15, SR-08).
    ConfigInvariant(String),

    /// The supplied `--db` path (eval run) resolves to the active daemon DB.
    ///
    /// Both resolved paths are included in the error for diagnostics (C-13,
    /// FR-44, ADR-001).
    LiveDbPath {
        /// The path as supplied by the caller (before canonicalization).
        supplied: PathBuf,
        /// The canonicalized active daemon DB path.
        active: PathBuf,
    },

    /// I/O error (file open, canonicalize failure, permission denied).
    Io(std::io::Error),

    /// Store/SQLx error from pool construction or query execution.
    Store(Box<dyn std::error::Error + Send + Sync>),

    /// Two profile TOMLs in a single `eval run` share the same `[profile].name`.
    ///
    /// Detected by `run_eval` before any `from_profile()` call. Named here for
    /// structural completeness.
    ProfileNameCollision(String),

    /// The `--k` argument is 0; P@K is undefined for k = 0.
    InvalidK(usize),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::ModelNotFound(p) => {
                write!(f, "model not found: {}", p.display())
            }
            EvalError::ConfigInvariant(msg) => write!(f, "{msg}"),
            EvalError::LiveDbPath { supplied, active } => write!(
                f,
                "eval db path resolves to the active database\n  \
                 supplied (resolved): {}\n  \
                 active:              {}\n  \
                 use a snapshot, not the live database",
                supplied.display(),
                active.display()
            ),
            EvalError::Io(e) => write!(f, "I/O error: {e}"),
            EvalError::Store(e) => write!(f, "store error: {e}"),
            EvalError::ProfileNameCollision(name) => write!(
                f,
                "duplicate profile name \"{name}\" — two profile TOMLs share the same [profile].name"
            ),
            EvalError::InvalidK(k) => write!(f, "--k must be >= 1, got {k}"),
        }
    }
}

impl std::error::Error for EvalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EvalError::Io(e) => Some(e),
            EvalError::Store(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}
