use crate::frontend::{
    ast::{expression::Expression, statement::StatementKind},
    parser::ParseResult,
    tokens::{Operator, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_identifier_statement(&mut self) -> ParseResult<StatementKind> {
        // Variable declaration: `name: type = init`, `name: type`, or `name := init`.
        if matches!(
            self.peek_second(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::Colon),
                ..
            })
        ) {
            let declaration = self.parse_colon_variable_declaration()?;
            return Ok(StatementKind::VariableDeclaration(declaration));
        }

        // Parse the LHS as a full expression. Then look for an assignment
        // operator. If found, dispatch based on the LHS expression shape.
        // Otherwise, treat the parsed expression as an expression statement.
        let lhs = self.parse_expression()?;

        // Go-style multi-assignment: `a, b = f()` or `a, b = 1, 2`.
        if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::Comma),
                ..
            })
        ) {
            let mut targets = vec![lhs];
            while self
                .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)))
                .is_some()
            {
                targets.push(self.parse_expression()?);
            }
            if self
                .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Colon)))
                .is_some()
            {
                self.expect_token(
                    TokenKind::Operator(Operator::Assign),
                    "expected '=' after ':' in multi-variable declaration",
                )?;
                let mut identifiers = Vec::new();
                let (line, column) = self.last_pos;
                for target in targets {
                    match target {
                        Expression::Identifier(identifier) => identifiers.push(identifier),
                        _ => return Err(self.error("invalid declaration target", line, column)),
                    }
                }
                let values = self.parse_expression_list()?;
                return Ok(StatementKind::MultiVariableDeclaration {
                    identifiers,
                    values,
                });
            }

            self.expect_token(
                TokenKind::Operator(Operator::Assign),
                "expected '=' after multi-assignment targets",
            )?;
            let values = self.parse_expression_list()?;
            return Ok(StatementKind::MultiAssignment { targets, values });
        }

        // Check for plain assignment.
        if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Operator(Operator::Assign),
                ..
            })
        ) {
            self.next(); // consume '='
            let rhs = self.parse_expression()?;
            return self.lhs_to_assignment(lhs, rhs);
        }

        // Check for compound assignment: desugar `lhs op= rhs` to `lhs = lhs op rhs`.
        let compound_op = match self.peek() {
            Some(Token {
                kind: TokenKind::Operator(op),
                ..
            }) => match op {
                Operator::PlusAssign => Some(Operator::Plus),
                Operator::MinusAssign => Some(Operator::Minus),
                Operator::StarAssign => Some(Operator::Star),
                Operator::SlashAssign => Some(Operator::Slash),
                Operator::PercentAssign => Some(Operator::Percent),
                _ => None,
            },
            _ => None,
        };
        if let Some(binop) = compound_op {
            self.next(); // consume the compound op
            let rhs = self.parse_expression()?;
            let combined = Expression::Binary {
                left: Box::new(lhs.clone()),
                operator: binop,
                right: Box::new(rhs),
            };
            return self.lhs_to_assignment(lhs, combined);
        }

        Ok(StatementKind::ExpressionStatement(lhs))
    }
}
