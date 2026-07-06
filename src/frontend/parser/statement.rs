use crate::frontend::{
    ast::statement::{Statement, StatementKind},
    parser::ParseResult,
    tokens::{Keyword, Operator, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_statement_some(&mut self) -> ParseResult<Statement> {
        loop {
            match self.parse_statement()? {
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

    pub fn parse_statement(&mut self) -> ParseResult<Option<Statement>> {
        let line = self.peek().map(|t| t.line).unwrap_or(0);
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
                                let inner = self.parse_statement_some()?;
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
                TokenKind::Keyword(Keyword::Var) => {
                    let stmt = self.parse_variable_declaration()?;
                    StatementKind::VariableDeclaration(stmt)
                }
                TokenKind::Keyword(Keyword::Defer) => {
                    self.next(); // consume 'defer'
                    let inner = self.parse_statement_some()?;
                    StatementKind::Defer(Box::new(inner))
                }
                TokenKind::Keyword(Keyword::Break) => {
                    self.next(); // consume 'break'
                    if let Some(t) = self.peek()
                        && matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon))
                    {
                        self.next();
                    }
                    StatementKind::Break
                }
                TokenKind::Keyword(Keyword::Continue) => {
                    self.next(); // consume 'continue'
                    if let Some(t) = self.peek()
                        && matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon))
                    {
                        self.next();
                    }
                    StatementKind::Continue
                }
                TokenKind::Keyword(Keyword::Return) => {
                    self.next(); // consume 'return'
                    // Optionally parse one or more comma-separated expressions after return.
                    if let Some(token) = self.peek() {
                        // If next token is not a semicolon or block close, parse expression list.
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
                TokenKind::Identifier(_) => self.parse_identifier_statement()?,
                TokenKind::Keyword(Keyword::For) => {
                    self.next(); // consume 'for'
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                        "expected '(' after for keyword",
                    )?;
                    let initializer = match self.consume_if(|t| {
                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon))
                    }) {
                        Some(_) => None,
                        None => {
                            let statement = self.parse_statement_some()?;
                            Some(Box::new(statement))
                        }
                    };
                    let condition = match self.consume_if(|t| {
                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon))
                    }) {
                        Some(_) => None,
                        None => {
                            let expression = self.parse_expression()?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::Semicolon),
                                "expected semicolon after condition expression",
                            )?;
                            Some(expression)
                        }
                    };
                    let post_operation = match self.consume_if(|t| {
                        matches!(
                            t.kind,
                            TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                        )
                    }) {
                        Some(_) => None,
                        None => {
                            let (line, column) = self.last_pos;
                            let statement = self
                                .parse_statement()?
                                .ok_or_else(|| self.error("expected statement", line, column))?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                                "expected ')'",
                            )?;
                            Some(Box::new(statement))
                        }
                    };
                    let body = self.parse_body()?;
                    StatementKind::For {
                        initializer,
                        condition,
                        post_operation,
                        body,
                    }
                }
                TokenKind::Literal(_)
                | TokenKind::Builtin(_)
                | TokenKind::Operator(Operator::LogicalNot)
                | TokenKind::Operator(Operator::Minus)
                | TokenKind::Operator(Operator::Tilde)
                | TokenKind::Punctuation(Punctuation::OpeningParenthesis)
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
                | TokenKind::Error(_)
                | TokenKind::Keyword(_)
                | TokenKind::Unknown(_) => {
                    let t = token.clone();
                    return Err(self.error(
                        &format!("cannot start a statement with {:?}", t.kind),
                        t.line,
                        t.column,
                    ));
                }
            }
        } else {
            return Ok(None);
        };

        // Optionally consume a semicolon if present
        self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)));
        Ok(Some(Statement { kind: stmt, line }))
    }
}
