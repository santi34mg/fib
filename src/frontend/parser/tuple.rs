use crate::frontend::{
    ast::type_expression::TypeExpression,
    parser::ParseResult,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_tuple_type_expression(
        &mut self,
        type_token: &Token,
    ) -> ParseResult<TypeExpression> {
        let mut elements = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(
                t.kind,
                TokenKind::Punctuation(Punctuation::ClosingParenthesis)
            ) {
                self.next(); // consume ')'
                break;
            }
            let element = match self.parse_type_expression()? {
                Some(type_id) => type_id,
                None => {
                    return Err(self.error(
                        "expected type in multiple return type list",
                        type_token.line,
                        type_token.column,
                    ));
                }
            };
            elements.push(element);
            if let Some(t) = self.peek()
                && !matches!(
                    t.kind,
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                )
            {
                self.expect_token(TokenKind::Punctuation(Punctuation::Comma), "expected ','")?;
            }
        }
        if elements.is_empty() {
            return Err(self.error(
                "expected at least one type in parenthesized type list",
                type_token.line,
                type_token.column,
            ));
        }
        if elements.len() == 1 {
            Ok(elements.remove(0))
        } else {
            Ok(TypeExpression::Tuple { elements })
        }
    }
}
