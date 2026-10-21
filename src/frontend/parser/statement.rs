use anyhow::bail;

use crate::frontend::{
    ast::{
        binary_expression::BinaryExpression,
        expression::{Expression, ExpressionKind},
        statement::{Statement, StatementKind},
        variable_declaration::VariableDeclaration,
    },
    tokens::{Keyword, Operator, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_statement(&mut self) -> anyhow::Result<Statement> {
        loop {
            match self.parse_statement_opt()? {
                Some(statement) => return Ok(statement),
                None => {
                    // A comment was consumed; keep trying or error at EOF
                    if self.peek().is_none() {
                        let (line, column) = self.last_pos;
                        return Err(self.error("expected a statement", line, column));
                    }
                }
            }
        }
    }

    pub fn parse_statement_opt(&mut self) -> anyhow::Result<Option<Statement>> {
        let Some((stmt, line, column)) = self.parse_statement_inner()? else {
            return Ok(None);
        };

        // Every statement must end with `;` — including block statements
        // like `if ... { ... };`, `while (...) { ... };` and `switch ...;`.
        // `defer`'s single `;` terminates the whole `defer <stmt>;`.
        self.expect_semicolon()?;
        Ok(Some(Statement {
            kind: stmt,
            span: crate::diagnostics::Span::new(line, column),
        }))
    }

    /// Consume the mandatory trailing `;`, surfacing lexer diagnostics
    /// (`Error`/`Unknown`) verbatim when they appear where `;` was expected.
    fn expect_semicolon(&mut self) -> anyhow::Result<()> {
        match self.peek() {
            Some(t) => match t.kind {
                TokenKind::Punctuation(Punctuation::Semicolon) => {
                    self.next();
                    Ok(())
                }
                TokenKind::Error(msg) => Err(self.error(&msg, t.line, t.column)),
                TokenKind::Unknown(c) => {
                    Err(self.error(&format!("unknown character '{}'", c), t.line, t.column))
                }
                _ => Err(self.error("expected ';' at end of statement", t.line, t.column)),
            },
            None => {
                let (line, column) = self.last_pos;
                Err(self.error("expected ';' at end of statement", line, column))
            }
        }
    }

    /// Parse one inner statement (no trailing `;`), skipping comments.
    /// Errors at EOF. Used for `defer <stmt>` and `else if ...` chains
    /// where the outer statement owns the terminator.
    fn parse_statement_inner_some(&mut self) -> anyhow::Result<(StatementKind, usize, usize)> {
        loop {
            match self.parse_statement_inner()? {
                Some(stmt) => return Ok(stmt),
                None => {
                    if self.peek().is_none() {
                        let (line, column) = self.last_pos;
                        return Err(self.error("expected a statement", line, column));
                    }
                }
            }
        }
    }

    /// Parse a single statement without consuming its trailing `;`.
    /// Returns `Ok(None)` for comments and EOF so callers can skip/retry.
    /// `parse_statement` wraps this and enforces the `;`; `defer` and
    /// `else if` use it directly because the outer statement owns the `;`.
    fn parse_statement_inner(&mut self) -> anyhow::Result<Option<(StatementKind, usize, usize)>> {
        let line = self.peek().map(|t| t.line).unwrap_or(0);
        let column = self.peek().map(|t| t.column).unwrap_or(0);
        let stmt = if let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Comment => {
                    self.next();
                    return Ok(None);
                }
                TokenKind::Keyword(Keyword::If) => {
                    self.next(); // consume 'if'
                    // Inside the condition a bare `Identifier {` would be
                    // ambiguous with the then-block; disable struct literals.
                    let saved = std::mem::replace(&mut self.no_struct_literal, true);
                    let condition = self.parse_expression();
                    self.no_struct_literal = saved;
                    let condition = condition?;
                    // Parse then-branch using shared parse_body
                    let then_branch = self.parse_body()?;
                    // Check for optional else
                    let else_branch = if let Some(token) = self.peek() {
                        if matches!(token.kind, TokenKind::Keyword(Keyword::Else)) {
                            self.next(); // consume 'else'
                            // Allow `else if` without requiring extra braces: if the next
                            // token is `if`, parse it as a single statement and wrap.
                            if matches!(
                                self.peek(),
                                Some(Token {
                                    kind: TokenKind::Keyword(Keyword::If),
                                    ..
                                })
                            ) {
                                // `else if` chains share the outer `;`: parse the
                                // inner `if` without its own terminator.
                                let (kind, iline, icolumn) = self.parse_statement_inner_some()?;
                                let inner = Statement {
                                    kind,
                                    span: crate::diagnostics::Span::new(iline, icolumn),
                                };
                                Some(vec![inner])
                            } else {
                                let else_stmts = self.parse_body()?;
                                Some(else_stmts)
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    StatementKind::If {
                        condition,
                        then_branch,
                        else_branch,
                    }
                }

                TokenKind::Keyword(Keyword::Pub | Keyword::Const | Keyword::Comp) => {
                    self.parse_variable_declaration()?
                }
                TokenKind::Keyword(Keyword::Defer) => {
                    self.next(); // consume 'defer'
                    // Single `;` terminates the whole `defer <stmt>;`,
                    // so the inner statement is parsed without one.
                    let (kind, iline, icolumn) = self.parse_statement_inner_some()?;
                    let inner = Statement {
                        kind,
                        span: crate::diagnostics::Span::new(iline, icolumn),
                    };
                    StatementKind::Defer(Box::new(inner))
                }
                TokenKind::Keyword(Keyword::Break) => {
                    self.next(); // consume 'break'
                    // Trailing `;` enforced by `parse_statement`.
                    StatementKind::Break
                }
                TokenKind::Keyword(Keyword::Continue) => {
                    self.next(); // consume 'continue'
                    // Trailing `;` enforced by `parse_statement`.
                    StatementKind::Continue
                }
                TokenKind::Keyword(Keyword::Return) => {
                    self.next(); // consume 'return'
                    // Optionally parse one or more comma-separated expressions after return.
                    // A bare `return` leaves the `;` for `parse_statement` to enforce,
                    // so `return }` correctly errors with "expected ';'".
                    if let Some(token) = self.peek() {
                        match token.kind {
                            TokenKind::Punctuation(Punctuation::Semicolon)
                            | TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) => {
                                StatementKind::Return(None)
                            }
                            _ => {
                                let exprs = self.parse_expression_list()?;
                                StatementKind::Return(Some(exprs))
                            }
                        }
                    } else {
                        StatementKind::Return(None)
                    }
                }
                TokenKind::Identifier(_)
                    if matches!(
                        self.peek_second(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::Colon | Punctuation::Comma),
                            ..
                        })
                    ) =>
                {
                    self.parse_variable_declaration()?
                }
                TokenKind::Identifier(_)
                | TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                    let expr = self.parse_binary(
                        &[
                            Operator::Assign,
                            Operator::Def,
                            Operator::PlusAssign,
                            Operator::MinusAssign,
                            Operator::StarAssign,
                            Operator::SlashAssign,
                            Operator::PercentAssign,
                        ],
                        Self::parse_expression,
                    )?;
                    if let ExpressionKind::BinaryExpresison(bexpr) = &expr.kind
                        && matches!(
                            bexpr.operator,
                            Operator::Def
                                | Operator::Assign
                                | Operator::PlusAssign
                                | Operator::MinusAssign
                                | Operator::StarAssign
                                | Operator::SlashAssign
                                | Operator::PercentAssign
                        )
                    {
                        let ExpressionKind::BinaryExpresison(bexpr) = expr.kind else {
                            unreachable!()
                        };
                        match bexpr.operator {
                            Operator::Def => {
                                if let ExpressionKind::QualifiedIdentifier(mut name) =
                                    bexpr.left.kind
                                    && name.segments.len() == 1
                                {
                                    StatementKind::VariableDeclaration(VariableDeclaration::new(
                                        name.segments.remove(0),
                                        None,
                                        Some(*bexpr.right),
                                    ))
                                } else {
                                    bail!("expected a name on left hand side of :=")
                                }
                            }
                            Operator::Assign => StatementKind::Assignment {
                                left: *bexpr.left,
                                right: *bexpr.right,
                            },
                            Operator::PlusAssign
                            | Operator::MinusAssign
                            | Operator::StarAssign
                            | Operator::SlashAssign
                            | Operator::PercentAssign => {
                                let operator = match bexpr.operator {
                                    Operator::PlusAssign => Operator::Plus,
                                    Operator::MinusAssign => Operator::Minus,
                                    Operator::StarAssign => Operator::Star,
                                    Operator::SlashAssign => Operator::Slash,
                                    Operator::PercentAssign => Operator::Percent,
                                    _ => unreachable!(),
                                };
                                let left = *bexpr.left;
                                if !matches!(&left.kind, ExpressionKind::QualifiedIdentifier(name) if name.segments.len() == 1)
                                {
                                    return Err(self.error(
                                        "compound assignment requires a variable name",
                                        line,
                                        column,
                                    ));
                                }
                                let right = Expression::at(
                                    ExpressionKind::BinaryExpresison(BinaryExpression {
                                        left: Box::new(left.clone()),
                                        operator,
                                        right: bexpr.right,
                                    }),
                                    left.span,
                                );
                                StatementKind::Assignment { left, right }
                            }
                            _ => unreachable!(),
                        }
                    } else {
                        StatementKind::ExpressionStatement(expr)
                    }
                }
                TokenKind::Keyword(Keyword::While) => {
                    self.next(); // consume 'while'
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                        "expected '(' after while keyword",
                    )?;
                    let condition = if matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                            ..
                        })
                    ) {
                        None
                    } else {
                        Some(self.parse_expression()?)
                    };
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                        "expected ')' after condition",
                    )?;
                    let body = self.parse_body()?;
                    StatementKind::While { condition, body }
                }
                TokenKind::Literal(_)
                | TokenKind::Builtin(_)
                | TokenKind::Operator(Operator::LogicalNot)
                | TokenKind::Operator(Operator::Minus)
                | TokenKind::Operator(Operator::Tilde)
                | TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => {
                    let expr = self.parse_expression()?;
                    StatementKind::ExpressionStatement(expr)
                }
                TokenKind::Keyword(Keyword::Switch) => {
                    self.next(); // consume 'switch'
                    self.parse_switch_statement()?
                }
                TokenKind::Keyword(Keyword::Else)
                | TokenKind::Operator(_)
                | TokenKind::Punctuation(_)
                | TokenKind::Keyword(_) => {
                    let t = token.clone();
                    return Err(self.error(
                        &format!("cannot start a statement with {:?}", t.kind),
                        t.line,
                        t.column,
                    ));
                }
                // Surface lexer diagnostics verbatim instead of burying
                // them under "cannot start a statement with Error(...)".
                TokenKind::Error(msg) => {
                    let t = token.clone();
                    return Err(self.error(msg, t.line, t.column));
                }
                TokenKind::Unknown(c) => {
                    let t = token.clone();
                    return Err(self.error(
                        &format!("unknown character '{}'", c),
                        t.line,
                        t.column,
                    ));
                }
            }
        } else {
            return Ok(None);
        };

        Ok(Some((stmt, line, column)))
    }
}
