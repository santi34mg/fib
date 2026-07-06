use crate::frontend::{ast::type_expression::TypeExpression, parser::ParseResult, tokens::{Operator, Token, TokenKind}};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_pointer_type(
        &mut self,
        next_token: Token,
    ) -> ParseResult<TypeExpression> {
        let pointed_type = match self.parse_type_expression()? {
            Some(type_id) => type_id,
            None => {
                return Err(self.error("expected type", next_token.line, next_token.column));
            }
        };
        Ok(TypeExpression::Pointer {
            pointed_type: Box::new(pointed_type),
        })
    }

    pub fn parse_pointer(&mut self) -> ParseResult<TypeExpression> {
        let next_token = self.expect_token(
            TokenKind::Operator(Operator::Ampersand),
            "expected AMPERSAND (&) after token",
        )?;
        if let TokenKind::Operator(Operator::Ampersand) = next_token.kind {
            self.parse_pointer_type(next_token)
        } else {
            unreachable!()
        }
    }
}
