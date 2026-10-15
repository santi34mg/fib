use crate::{
    diagnostics::Span,
    frontend::{
        next_ast::expression::{Expression, ExpressionKind},
        tokens::{
            Punctuation, Token, TokenKind,
            builtin::{Builtin, BuiltinType},
        },
    },
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
        if let Some(Token {
            kind: TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Void)),
            line,
            column,
            ..
        }) = self.peek()
        {
            return Ok(Expression {
                kind: ExpressionKind::VoidExpression,
                span: Span::new(line, column),
            });
        }
        self.parse_logical_or()
    }
}
