use fibc::parser::parser::ParseError;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Convert a `ParseError` (which carries a 1-based line/column) into an LSP `Diagnostic`.
pub fn parse_error_to_diagnostic(e: &ParseError) -> Diagnostic {
    let line = e.line.saturating_sub(1) as u32;
    let col = e.column.saturating_sub(1) as u32;
    Diagnostic {
        range: Range {
            start: Position { line, character: col },
            end: Position { line, character: col + 1 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: e.message.clone(),
        source: Some("fiber".into()),
        ..Default::default()
    }
}

/// Convert an unstructured analysis error string into an LSP `Diagnostic` placed at the top of
/// the file (we have no source location for these yet).
pub fn analysis_error_to_diagnostic(msg: &str) -> Diagnostic {
    Diagnostic {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 1 },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        message: msg.to_string(),
        source: Some("fiber".into()),
        ..Default::default()
    }
}
