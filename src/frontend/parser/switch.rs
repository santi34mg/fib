use crate::frontend::{
    ast::{pattern::Pattern, statement::StatementKind, switch::SwitchArm},
    parser::ParseResult,
    tokens::{Keyword, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_switch_statement(&mut self) -> ParseResult<StatementKind> {
        // 'switch' already consumed by caller.
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            "expected '(' after 'switch'",
        )?;
        let subject = self.parse_expression()?;
        self.expect_token(
            TokenKind::Punctuation(Punctuation::ClosingParenthesis),
            "expected ')' after switch subject",
        )?;
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            "expected '{' to open switch body",
        )?;
        let mut arms: Vec<SwitchArm> = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(
                t.kind,
                TokenKind::Punctuation(Punctuation::ClosingCurlyBrace)
            ) {
                self.next();
                break;
            }
            self.expect_token(
                TokenKind::Keyword(Keyword::When),
                "expected 'when' inside switch body",
            )?;
            // Pattern: either `.VariantName`, `_` (wildcard), or `else`
            let pattern = match self.peek() {
                Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::Dot),
                    ..
                }) => {
                    self.next(); // consume '.'
                    let name = self.expect_identifier("expected variant name after '.'")?;
                    // Optional `(binding)` for variants carrying a payload.
                    let binding = if matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                            ..
                        })
                    ) {
                        self.next(); // consume '('
                        let b = self.expect_identifier("expected binding name in pattern")?;
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                            "expected ')' after pattern binding",
                        )?;
                        Some(b)
                    } else {
                        None
                    };
                    Pattern::EnumVariant {
                        variant: name,
                        binding,
                    }
                }
                Some(Token {
                    kind: TokenKind::Keyword(Keyword::Else),
                    ..
                }) => {
                    self.next();
                    Pattern::Wildcard
                }
                _ => {
                    let tok = self.peek().unwrap();
                    return Err(self.error(
                        "expected '.' followed by variant name, or 'else'",
                        tok.line,
                        tok.column,
                    ));
                }
            };
            let body = self.parse_body()?;
            arms.push(SwitchArm { pattern, body });
        }
        Ok(StatementKind::Switch { subject, arms })
    }
}
