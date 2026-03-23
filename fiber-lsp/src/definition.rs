use fibc::driver::FrontendResult;
use fibc::token::TokenKind;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use crate::lookup::{find_declaration, token_at};

pub fn goto_definition(
    result: &FrontendResult,
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
