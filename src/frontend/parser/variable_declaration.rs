use crate::frontend::{ast::variable_declaration::VariableDeclaration, parser::ParseResult, tokens::{Keyword, Operator, Punctuation, Token, TokenKind}};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parses the legacy variable declaration syntax: `var <type> <name> = <init>[;]`.
    pub fn parse_variable_declaration(&mut self) -> ParseResult<VariableDeclaration> {
        let var_token = self.expect_token(
            TokenKind::Keyword(Keyword::Var),
            "parse_variable_declaration: expected 'var' keyword",
        )?;

        let var_type = self.parse_type_expression()?;

        let ident = if let TokenKind::Identifier(ident) = self
            .expect_next("parse_variable_declaration: expected identifier")?
            .kind
        {
            ident
        } else {
            return Err(self.error("expected identifier", var_token.line, var_token.column));
        };

        let expr =
            match self.consume_if(|t| matches!(t.kind, TokenKind::Operator(Operator::Assign))) {
                Some(_) => Some(self.parse_expression()?),
                None => None,
            };

        Ok(VariableDeclaration::new(ident, var_type, expr))
    }


    /// Parses the colon-based variable declaration syntax:
    /// - `name: type = init` for an explicit type annotation
    /// - `name := init` for an inferred type
    ///
    /// The initializer is optional for explicit typed declarations; `name: type`
    /// declares an uninitialized binding that must be assigned before use.
    pub fn parse_colon_variable_declaration(&mut self) -> ParseResult<VariableDeclaration> {
        let ident_token = self.expect_next("expected identifier")?;
        let ident_line = ident_token.line;
        let ident_column = ident_token.column;
        let ident = match ident_token.kind {
            TokenKind::Identifier(ident) => ident,
            _ => return Err(self.error("expected identifier", ident_line, ident_column)),
        };

        self.expect_token(
            TokenKind::Punctuation(Punctuation::Colon),
            "expected ':' after variable name",
        )?;

        if self
            .consume_if(|t| matches!(t.kind, TokenKind::Operator(Operator::Assign)))
            .is_some()
        {
            let expr = self.parse_expression()?;
            return Ok(VariableDeclaration::new(ident, None, Some(expr)));
        }

        let var_type = self.parse_type_expression()?.ok_or_else(|| {
            let (line, column) = self
                .peek()
                .map(|token| (token.line, token.column))
                .unwrap_or((ident_line, ident_column));
            self.error("expected type after ':'", line, column)
        })?;

        let expr =
            match self.consume_if(|t| matches!(t.kind, TokenKind::Operator(Operator::Assign))) {
                Some(_) => Some(self.parse_expression()?),
                None => None,
            };

        Ok(VariableDeclaration::new(ident, Some(var_type), expr))
    }
}
