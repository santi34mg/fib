//! Typed lowering errors (audit 02 §1).
//!
//! Replaces the previous `Result<_, Box<dyn Error>>` + `format!.into()`
//! strings with a small structured enum so compiler bugs become actionable:
//! - [`LowerError::MissingBlock`] carries the lowering site (`what`), the
//!   enclosing function name when known, and an optional source line.
//! - [`LowerError::UnknownLayout`] folds in the ad-hoc `UnknownLayout: ...`
//!   strings from `types.rs` (layout of an unresolvable nominal type).
//! - [`LowerError::Unsupported`] is the fallback for the remaining
//!   `format!` sites (via `From<String>`/`From<&str>` so existing call sites
//!   keep compiling while the migration completes).
//! - [`LowerError::Llvm`] wraps `inkwell::builder::BuilderError` messages.

use std::fmt;

/// Structured backend lowering error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LowerError {
    /// No LLVM insert block when one was required (replaces
    /// `get_insert_block().unwrap()` and the `format!("lowering ...")` string).
    MissingBlock {
        what: String,
        fn_name: Option<String>,
        line: Option<usize>,
    },
    /// Layout (size/align) of a nominal type could not be resolved.
    /// `name` keeps the human-readable detail, e.g.
    /// `"type 'Foo' not found in scope"`.
    UnknownLayout { name: String },
    /// Any other lowering failure (fallback while the migration completes).
    Unsupported(String),
    /// An LLVM builder failure (`inkwell::builder::BuilderError` message).
    Llvm(String),
}

impl LowerError {
    /// `lowering '<what>': no insert block`, with optional function/line context.
    pub fn missing_block(
        what: impl Into<String>,
        fn_name: Option<String>,
        line: Option<usize>,
    ) -> Self {
        Self::MissingBlock {
            what: what.into(),
            fn_name,
            line,
        }
    }

    /// Layout failure detail, rendered as `UnknownLayout: <name>`.
    pub fn unknown_layout(name: impl Into<String>) -> Self {
        Self::UnknownLayout { name: name.into() }
    }

    /// Fallback for ad-hoc lowering failures.
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::Unsupported(msg.into())
    }

    /// Wrap an LLVM builder failure message.
    pub fn llvm(msg: impl Into<String>) -> Self {
        Self::Llvm(msg.into())
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingBlock {
                what,
                fn_name,
                line,
            } => match (fn_name, line) {
                (Some(func), Some(line)) => write!(
                    f,
                    "lowering '{}' in function '{}' at line {}: no insert block",
                    what, func, line
                ),
                (Some(func), None) => write!(
                    f,
                    "lowering '{}' in function '{}': no insert block",
                    what, func
                ),
                (None, Some(line)) => {
                    write!(f, "lowering '{}' at line {}: no insert block", what, line)
                }
                (None, None) => write!(f, "lowering '{}': no insert block", what),
            },
            // Keep the historical `UnknownLayout: ...` prefix: existing
            // diagnostics and tests match on it.
            Self::UnknownLayout { name } => write!(f, "UnknownLayout: {}", name),
            Self::Unsupported(msg) => write!(f, "{}", msg),
            Self::Llvm(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for LowerError {}

// Fallback so existing `format!(...).into()` / `ok_or_else(|| format!(...))?`
// sites keep compiling while returning `LowerError` instead of
// `Box<dyn Error>`. New code should prefer the typed constructors above.
impl From<String> for LowerError {
    fn from(value: String) -> Self {
        Self::Unsupported(value)
    }
}

impl From<&str> for LowerError {
    fn from(value: &str) -> Self {
        Self::Unsupported(value.to_string())
    }
}

#[cfg(feature = "llvm")]
impl From<inkwell::builder::BuilderError> for LowerError {
    fn from(value: inkwell::builder::BuilderError) -> Self {
        Self::Llvm(value.to_string())
    }
}
