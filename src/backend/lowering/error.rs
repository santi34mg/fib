//! Errors at the backend boundary; no string-to-error compatibility fallback.

/// An unfinished phase is never reported as successful compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendError {
    Invalid { message: String },
    NotImplemented { operation: &'static str },
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { message } => write!(f, "backend: {message}"),
            Self::NotImplemented { operation } => write!(f, "{operation} is not implemented"),
        }
    }
}

impl std::error::Error for BackendError {}
