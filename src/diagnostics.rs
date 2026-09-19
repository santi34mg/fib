//! Unified structured compiler diagnostics (audit 02 §2).
//!
//! Every stage — lexer (via `TokenKind::Error`), parser, analyzer, lowerer,
//! and driver — promotes into [`CompilerError`] so the user sees one
//! rendering shape:
//!
//! ```text
//! error[type]: assignment to immutable variable `x`
//!   --> samples/foo.fib:5:3
//!    |
//!  5 |   x = 2;
//!    |   ^
//!    = help: declare `x` as mutable
//! ```
//!
//! The panel is drawn from [`Span`] (start position) plus the source line
//! and filename attached at promote time, when the driver has the source in
//! hand.

use std::fmt;
use std::path::PathBuf;

/// 1-based start position of a token or node in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Broad category of a compiler error, used for the leading tag and as a
/// stable key for tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Filesystem / IO failures (reading sources, writing outputs).
    Io,
    /// A lexical error surfaced by the lexer.
    Lex,
    /// A parse-time grammar error.
    Parse,
    /// Name resolution failures (undefined identifiers, imports, modules).
    Name,
    /// Type checking / coercion failures.
    Type,
    /// Backend failures (IR lowering, LLVM).
    Backend,
    /// Toolchain / environment failures (missing `cc`, LLVM, bad flags).
    Toolchain,
}

impl ErrorKind {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Lex => "lex",
            Self::Parse => "parse",
            Self::Name => "name",
            Self::Type => "type",
            Self::Backend => "backend",
            Self::Toolchain => "toolchain",
        }
    }
}

/// A universal structured compiler error, described by a [`ErrorKind`], an
/// optional source [`Span`] and an optional remedy hint. `filename` and
/// `source_line` are render context attached when the error crosses into the
/// driver, which owns the source text.
#[derive(Debug, Clone)]
pub struct CompilerError {
    pub kind: ErrorKind,
    pub message: String,
    pub span: Option<Span>,
    pub hint: Option<String>,
    pub filename: Option<PathBuf>,
    pub source_line: Option<String>,
}

impl CompilerError {
    pub const fn new(kind: ErrorKind, message: String) -> Self {
        Self {
            kind,
            message,
            span: None,
            hint: None,
            filename: None,
            source_line: None,
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.span = Some(Span::new(line, column));
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach the render context the driver has at promote time: the file
    /// being compiled and the text of the offending source line.
    pub fn with_source(
        mut self,
        filename: impl Into<PathBuf>,
        source_line: impl Into<String>,
    ) -> Self {
        self.filename = Some(filename.into());
        self.source_line = Some(source_line.into());
        self
    }

    /// The leading line, e.g. `error[type]: assignment to immutable variable`.
    pub fn header(&self) -> String {
        format!("error[{}]: {}", self.kind.tag(), self.message)
    }

    /// Plain (span-less) rendering used when no source position is known:
    /// the header plus any hint.
    fn render_plain(&self) -> String {
        let mut out = self.header();
        if let Some(hint) = &self.hint {
            out.push_str(&format!("\n  = help: {}", hint));
        }
        out
    }

    /// Render a rustc-style diagnostic panel.
    pub fn render(&self) -> String {
        let Some(span) = self.span else {
            return self.render_plain();
        };
        let mut out = self.header();
        let file = self
            .filename
            .as_deref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        out.push_str(&format!("\n  --> {}:{}:{}", file, span.line, span.column));
        if let Some(src) = &self.source_line {
            out.push_str(&format!(
                "\n   |\n {:>3} | {}\n   | {}{}",
                span.line,
                src,
                " ".repeat(span.column.saturating_sub(1)),
                "^"
            ));
        }
        if let Some(hint) = &self.hint {
            out.push_str(&format!("\n   = help: {}", hint));
        }
        out
    }
}

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

impl std::error::Error for CompilerError {}

impl From<crate::frontend::parser::ParseError> for CompilerError {
    fn from(pe: crate::frontend::parser::ParseError) -> Self {
        CompilerError {
            kind: ErrorKind::Parse,
            message: pe.message,
            span: Some(Span::new(pe.line, pe.column)),
            hint: None,
            filename: Some(pe.filename.to_path_buf()),
            source_line: Some(pe.source_line),
        }
    }
}

impl From<crate::frontend::analyze::AnalysisError> for CompilerError {
    fn from(ae: crate::frontend::analyze::AnalysisError) -> Self {
        CompilerError {
            kind: ae.kind,
            message: ae.message,
            span: ae.span,
            hint: ae.hint,
            filename: None,
            source_line: None,
        }
    }
}

/// Extract a single 1-based source line (without its trailing newline) for
/// the diagnostic caret panel, when the driver still has the source text.
pub fn source_line_at(source: &str, line: usize) -> Option<String> {
    source.lines().nth(line.checked_sub(1)?).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::analyze::AnalysisError;
    use crate::frontend::parser::ParseError;
    use std::path::Path;

    fn parse_err() -> ParseError {
        ParseError {
            filename: Path::new("broken.fib").into(),
            message: "expected an atom".to_string(),
            line: 3,
            column: 9,
            source_line: "  x := * 1".to_string(),
        }
    }

    #[test]
    fn converts_parse_error_to_compiler_error() {
        let ce = CompilerError::from(parse_err());
        assert_eq!(ce.kind, ErrorKind::Parse);
        assert_eq!(ce.kind.tag(), "parse");
        assert_eq!(ce.span, Some(Span::new(3, 9)));
        assert_eq!(
            ce.filename.as_deref().unwrap().to_string_lossy(),
            "broken.fib"
        );
        assert_eq!(ce.source_line.as_deref(), Some("  x := * 1"));
    }

    #[test]
    fn converts_analysis_error_keeping_kind_and_hint() {
        let ae = AnalysisError::from("cannot assign to constant 'x'".to_string())
            .with_hint("declare the binding with `var` to make it mutable")
            .with_span(Span::new(7, 3));
        let ce = CompilerError::from(ae);
        assert_eq!(ce.kind, ErrorKind::Type);
        assert_eq!(ce.message, "cannot assign to constant 'x'");
        assert_eq!(ce.span, Some(Span::new(7, 3)));
        assert_eq!(
            ce.hint.as_deref(),
            Some("declare the binding with `var` to make it mutable")
        );
        assert!(ce.filename.is_none());
    }

    #[test]
    fn renders_rustc_style_panel_with_caret_and_help() {
        let ce = CompilerError::from(parse_err())
            .with_hint("add a primary expression after the operator");
        let text = ce.render();
        assert!(text.starts_with("error[parse]: expected an atom"));
        assert!(text.contains("--> broken.fib:3:9"));
        assert!(text.contains("3 |   x := * 1"));
        assert!(text.contains("^"));
        assert!(text.contains("= help: add a primary expression after the operator"));
    }

    #[test]
    fn renders_plain_when_no_span() {
        let ce = CompilerError::new(
            ErrorKind::Backend,
            "lowering failed: unhandled node".to_string(),
        )
        .with_hint("this construct is not supported by the IR backend yet");
        assert_eq!(
            ce.render(),
            "error[backend]: lowering failed: unhandled node\n  = help: this construct is not supported by the IR backend yet"
        );
    }

    #[test]
    fn extracts_one_based_source_lines() {
        let src = "one\n  two\n\tthree\n";
        assert_eq!(source_line_at(src, 1).as_deref(), Some("one"));
        assert_eq!(source_line_at(src, 2).as_deref(), Some("  two"));
        assert_eq!(source_line_at(src, 3).as_deref(), Some("\tthree"));
        assert_eq!(source_line_at(src, 0), None);
        assert_eq!(source_line_at(src, 4), None);
    }

    #[test]
    fn display_matches_render() {
        let ce = CompilerError::from(parse_err());
        assert_eq!(ce.to_string(), ce.render());
    }
}
