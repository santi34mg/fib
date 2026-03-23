use fibc::hir::{HIRSymbol, Scope};
use fibc::token::{Token, TokenKind};
use fibc::token::identifier::Identifier;
use fibc::token::punctuation::Punctuation;
use fibc::token::Operator;

/// Find the token whose span contains `(line, col)` (1-based).
pub fn token_at(tokens: &[Token], line: usize, col: usize) -> Option<&Token> {
    tokens.iter().find(|t| {
        t.line == line && t.column <= col && col <= t.end_column
    })
}

/// Look up an identifier name recursively through all scopes.
pub fn find_symbol<'a>(name: &Identifier, scope: &'a Scope) -> Option<&'a HIRSymbol> {
    if let Some(sym) = scope.symbols.get(name) {
        return Some(sym);
    }
    for child in &scope.children_scope {
        if let Some(sym) = find_symbol(name, child) {
            return Some(sym);
        }
    }
    None
}

/// Find the declaration site of `name` in the user token stream.
///
/// Handles:
/// - `fn name`, `var name`, `const name`, `type name`
/// - Function parameters: `(name :` or `, name :`
pub fn find_declaration<'a>(name: &Identifier, tokens: &'a [Token]) -> Option<&'a Token> {
    use fibc::token::keyword::Keyword;

    let decl_keywords = [Keyword::Function, Keyword::Var, Keyword::Const, Keyword::Type];

    for (i, tok) in tokens.iter().enumerate() {
        match &tok.kind {
            // `fn name` / `type name` — name is the first identifier after the keyword
            TokenKind::Keyword(Keyword::Function) | TokenKind::Keyword(Keyword::Type) => {
                for next in tokens[i + 1..].iter() {
                    match &next.kind {
                        TokenKind::Comment => continue,
                        TokenKind::Identifier(id) if id == name => return Some(next),
                        _ => break,
                    }
                }
            }
            // `var <type...> <name>` / `const <type...> <name>` — name is the last
            // identifier before `=` or `;` (type comes between keyword and name)
            TokenKind::Keyword(Keyword::Var) | TokenKind::Keyword(Keyword::Const) => {
                let mut last_matching: Option<&Token> = None;
                for next in tokens[i + 1..].iter() {
                    match &next.kind {
                        TokenKind::Identifier(id) => {
                            if id == name {
                                last_matching = Some(next);
                            }
                        }
                        TokenKind::Operator(Operator::Assign)
                        | TokenKind::Punctuation(Punctuation::Semicolon) => break,
                        TokenKind::Keyword(k) if decl_keywords.contains(k) => break,
                        _ => {}
                    }
                }
                if last_matching.is_some() {
                    return last_matching;
                }
            }
            // function parameter: `( name :` or `, name :`
            TokenKind::Punctuation(Punctuation::OpeningParenthesis)
            | TokenKind::Punctuation(Punctuation::Comma) => {
                if let Some(next) = tokens.get(i + 1) {
                    if let TokenKind::Identifier(id) = &next.kind {
                        if id == name {
                            if let Some(after) = tokens.get(i + 2) {
                                if matches!(after.kind, TokenKind::Punctuation(Punctuation::Colon)) {
                                    return Some(next);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}
