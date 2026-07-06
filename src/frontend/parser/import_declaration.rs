use crate::frontend::{ast::imports::ImportDeclaration, parser::ParseResult, tokens::{Keyword, Punctuation, Token, TokenKind}};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parses an import declaration: `import a::b::c`, `import a::b as x`, `import a::b::{X, y}`
    pub fn parse_import_declaration(&mut self) -> ParseResult<ImportDeclaration> {
        self.expect_token(TokenKind::Keyword(Keyword::Import), "expected 'import'")?;

        // Parse the first path segment (must be an identifier)
        let first = self.expect_identifier("expected module path after 'import'")?;
        let mut path = vec![first];

        // Continue consuming `::identifier` segments
        while let Some(token) = self.peek() {
            if !matches!(token.kind, TokenKind::Punctuation(Punctuation::DoubleColon)) {
                break;
            }
            // Peek at what follows the `::`
            if let Some(next) = self.peek_second() {
                match &next.kind {
                    TokenKind::Identifier(_) => {
                        self.next(); // consume `::`
                        let seg_token = self.next().unwrap();
                        if let TokenKind::Identifier(id) = seg_token.kind {
                            path.push(id);
                        }
                    }
                    TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => {
                        // `::{ ... }` selective import
                        self.next(); // consume `::`
                        self.next(); // consume `{`
                        let mut selective = Vec::new();
                        loop {
                            let name =
                                self.expect_identifier("expected symbol name in selective import")?;
                            selective.push(name);
                            if self
                                .consume_if(|t| {
                                    matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                                })
                                .is_none()
                            {
                                break;
                            }
                        }
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
                            "expected '}' to close selective import",
                        )?;
                        return Ok(ImportDeclaration {
                            path,
                            alias: None,
                            selective: Some(selective),
                        });
                    }
                    _ => {
                        // consume the `::` so it doesn't leak into the outer parser
                        self.next();
                        let t = next.clone();
                        return Err(self.error(
                            "expected identifier or '{' after '::'",
                            t.line,
                            t.column,
                        ));
                    }
                }
            } else {
                // `::` at end of input — consume it and report the error
                let dc = self.next().unwrap();
                return Err(self.error(
                    "expected identifier or '{' after '::'",
                    dc.line,
                    dc.column,
                ));
            }
        }

        // Check for `as alias`
        let alias = if let Some(token) = self.peek()
            && matches!(token.kind, TokenKind::Keyword(Keyword::As))
        {
            self.next(); // consume `as`
            Some(self.expect_identifier("expected alias name after 'as'")?)
        } else {
            None
        };

        Ok(ImportDeclaration {
            path,
            alias,
            selective: None,
        })
    }
}
