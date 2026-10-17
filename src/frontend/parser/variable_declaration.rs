use crate::frontend::{
    ast::{statement::StatementKind, variable_declaration::VariableDeclaration},
    tokens::{Keyword, Operator, Punctuation, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Parse a declaration's left-hand side once per binding, including its
    /// own `[pub] [const | comp]` modifiers and optional type. The final `:=` is split into
    /// `:` and `=` tokens by the tokenizer.
    fn parse_declaration_binding(&mut self) -> anyhow::Result<(VariableDeclaration, bool)> {
        let is_pub = self
            .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Pub)))
            .is_some();
        let modifier = self
            .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Const | Keyword::Comp)));
        let is_const = matches!(
            modifier.as_ref().map(|t| &t.kind),
            Some(TokenKind::Keyword(Keyword::Const))
        );
        let is_comp = matches!(
            modifier.as_ref().map(|t| &t.kind),
            Some(TokenKind::Keyword(Keyword::Comp))
        );
        let name = self.expect_identifier("expected variable name")?;
        let mut inferred = false;
        let ty = if self
            .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Colon)))
            .is_some()
        {
            if matches!(
                self.peek(),
                Some(Token {
                    kind: TokenKind::Operator(Operator::Assign),
                    ..
                })
            ) {
                inferred = true;
                None
            } else {
                Some(self.parse_type_expression()?.ok_or_else(|| {
                    let (line, column) = self
                        .peek()
                        .map(|token| (token.line, token.column))
                        .unwrap_or(self.last_pos);
                    self.error("expected type after ':'", line, column)
                })?)
            }
        } else {
            None
        };
        let mut binding = VariableDeclaration::new(name, ty, None);
        binding.is_const = is_const;
        binding.is_pub = is_pub;
        binding.is_comp = is_comp;
        Ok((binding, inferred))
    }

    /// Parse the same binding repeatedly while commas follow, then parse the
    /// shared initializer. Modifiers and type belong to each binding, not to
    /// the entire declaration statement.
    pub fn parse_variable_declaration(&mut self) -> anyhow::Result<StatementKind> {
        let (first, mut inferred) = self.parse_declaration_binding()?;
        let mut bindings = vec![first];
        while self
            .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)))
            .is_some()
        {
            if inferred {
                let (line, column) = self.last_pos;
                return Err(self.error("expected initializer after ':='", line, column));
            }
            let (binding, next_inferred) = self.parse_declaration_binding()?;
            bindings.push(binding);
            inferred = next_inferred;
        }

        let last = bindings.last().expect("at least one binding was parsed");
        if !inferred && last.ty.is_none() {
            let (line, column) = self
                .peek()
                .map(|token| (token.line, token.column))
                .unwrap_or(self.last_pos);
            return Err(self.error("expected ':' after variable name", line, column));
        }
        if !inferred
            && matches!(
                self.peek(),
                Some(Token {
                    kind: TokenKind::Punctuation(Punctuation::Semicolon),
                    ..
                })
            )
        {
            if bindings.iter().any(|binding| binding.ty.is_none()) {
                let (line, column) = self.last_pos;
                return Err(self.error(
                    "without an initializer, each binding needs its own type; add a type to every binding or use ':=' with an initializer",
                    line,
                    column,
                ));
            }
            return if bindings.len() == 1 {
                Ok(StatementKind::VariableDeclaration(bindings.remove(0)))
            } else {
                Ok(StatementKind::MultiVariableDeclaration {
                    bindings,
                    values: Vec::new(),
                })
            };
        }

        self.expect_token(
            TokenKind::Operator(Operator::Assign),
            "expected '=' after type",
        )?;
        if bindings.len() == 1 {
            let mut binding = bindings.remove(0);
            binding.expression = Some(self.parse_expression()?);
            Ok(StatementKind::VariableDeclaration(binding))
        } else {
            let values = self.parse_expression_list()?;
            Ok(StatementKind::MultiVariableDeclaration { bindings, values })
        }
    }
}
