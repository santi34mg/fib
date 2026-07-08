use crate::frontend::{
    ast::type_expression::TypeExpression,
    parser::ParseResult,
    tokens::{Operator, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_function_type(&mut self, type_token: &Token) -> ParseResult<TypeExpression> {
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            "expected '('",
        )?;
        let mut argument_types: Vec<TypeExpression> = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(
                t.kind,
                TokenKind::Punctuation(Punctuation::ClosingParenthesis)
            ) {
                break;
            }
            let argument_type = match self.parse_type_expression()? {
                Some(type_id) => type_id,
                None => {
                    return Err(self.error(
                        "expected type identifier",
                        type_token.line,
                        type_token.column,
                    ));
                }
            };
            argument_types.push(argument_type);
            if let Some(t) = self.peek()
                && !matches!(
                    t.kind,
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                )
            {
                self.expect_token(TokenKind::Punctuation(Punctuation::Comma), "expected ','")?;
            }
        }
        // consume ')'
        self.next();
        // expect '->'
        let arrow = self.expect_token(
            TokenKind::Operator(Operator::ThinRightArrow),
            "expected '->'",
        )?;
        let return_type = match self.parse_type_expression()? {
            Some(rt) => rt,
            None => {
                return Err(self.error("expected a return type", arrow.line, arrow.column));
            }
        };
        Ok(TypeExpression::Function {
            argument_types,
            return_type: Box::new(return_type),
        })
    }
}
