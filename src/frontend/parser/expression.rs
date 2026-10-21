use crate::frontend::{
    ast::expression::Expression,
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_expression_list(&mut self) -> anyhow::Result<Vec<Expression>> {
        let mut exprs = vec![self.parse_expression()?];
        while self
            .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)))
            .is_some()
        {
            exprs.push(self.parse_expression()?);
        }
        Ok(exprs)
    }

    pub fn parse_expression(&mut self) -> anyhow::Result<Expression> {
        self.parse_logical_or()
    }
}
