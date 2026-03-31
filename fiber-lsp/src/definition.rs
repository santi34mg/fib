use fibc::driver::FrontendResponse;
use fibc::token::TokenKind;
use fibc::token::punctuation::Punctuation;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::lookup::{find_declaration, token_at};

pub fn goto_definition(
    result: &FrontendResponse,
    uri: Url,
    line: usize,
    col: usize,
) -> Option<Location> {
    // Find the identifier the cursor is on
    let tok = token_at(&result.tokens, line, col)?;
    let name = match &tok.kind {
        TokenKind::Identifier(id) => id,
        _ => return None,
    };

    // If the cursor is on the member of a qualified access (`module :: member`),
    // cross-file go-to-def is not yet supported — return None rather than jumping
    // to a wrong location in the current file.
    let tok_idx = result.tokens.iter().position(|t| std::ptr::eq(t, tok))?;
    if tok_idx >= 2 {
        let maybe_dcolon = &result.tokens[tok_idx - 1];
        if matches!(maybe_dcolon.kind, TokenKind::Punctuation(Punctuation::DoubleColon)) {
            return None;
        }
    }

    // Find where it was declared in the user's token stream.
    // If find_declaration returns None the symbol is stdlib/external — nothing to jump to.
    let decl_tok = find_declaration(name, &result.tokens)?;

    let start_line = decl_tok.line.saturating_sub(1) as u32;
    let start_col = decl_tok.column.saturating_sub(1) as u32;
    Some(Location {
        uri,
        range: Range {
            start: Position { line: start_line, character: start_col },
            end: Position { line: start_line, character: start_col + name.identifier.len() as u32 },
        },
    })
}
