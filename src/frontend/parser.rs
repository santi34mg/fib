use std::collections::VecDeque;
use std::fmt;
use std::path::Path;

use crate::frontend::ast::Ast;
use crate::frontend::ast::statement::{Statement, StatementKind};
use crate::frontend::tokens::Keyword;
use crate::frontend::tokens::builtin::Builtin;
use crate::frontend::tokens::{Punctuation, Token, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseError {
    pub filename: Box<Path>,
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub source_line: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Standard format: file:line:column
        writeln!(f, "{:?}:{}:{}:", self.filename, self.line, self.column)?;
        writeln!(f, "{}", self.message)?;
        writeln!(f, "\t{}", self.source_line)?;
        let indent_len = self.column.saturating_sub(1).min(self.source_line.len());
        let indent = " ".repeat(indent_len);
        writeln!(f, "\t{}^", indent)
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;

pub struct Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    tokens: I,
    /// Lookahead buffer; comments are filtered out as tokens are pulled in.
    lookahead: VecDeque<Token>,
    filename: &'a Path,
    source_lines: Vec<String>,
    /// Position of the last consumed token, used for errors at end of input.
    last_pos: (usize, usize),
    /// When true, `Identifier {` is not parsed as a struct/variant literal.
    /// Set while parsing an `if` condition so `if done { ... }` treats the
    /// brace as the statement body, not a composite literal.
    no_struct_literal: bool,
}

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn new(tokens: I, filename: &'a Path) -> Self {
        let source = std::fs::read_to_string(filename).unwrap_or_default();
        Self::with_source(tokens, filename, &source)
    }

    pub fn with_source(tokens: I, filename: &'a Path, source: &str) -> Self {
        Self {
            tokens,
            lookahead: VecDeque::new(),
            filename,
            source_lines: source.lines().map(str::to_owned).collect(),
            last_pos: (0, 0),
            no_struct_literal: false,
        }
    }

    fn error(&self, message: &str, line: usize, column: usize) -> anyhow::Error {
        ParseError {
            filename: self.filename.into(),
            message: message.to_owned(),
            line,
            column,
            source_line: self
                .source_lines
                .get(line.saturating_sub(1))
                .cloned()
                .unwrap_or_default(),
        }
        .into()
    }

    /// Fill the lookahead buffer with up to `n` tokens, skipping comments.
    fn fill_lookahead(&mut self, n: usize) {
        while self.lookahead.len() < n {
            match self.tokens.next() {
                Some(t) if matches!(t.kind, TokenKind::Comment) => continue,
                Some(mut t) if matches!(t.kind, TokenKind::Punctuation(Punctuation::At)) => {
                    t.kind = match self.tokens.next() {
                        Some(next) => {
                            let name = match next.kind {
                                TokenKind::Identifier(name) => Some(name),
                                TokenKind::Builtin(Builtin::BuiltinType(_)) => {
                                    t.kind = TokenKind::Error(
                                        "types do not use '@'; remove '@' before the type"
                                            .to_owned(),
                                    );
                                    None
                                }
                                TokenKind::Builtin(builtin) => {
                                    t.kind = TokenKind::Builtin(builtin);
                                    None
                                }
                                _ => None,
                            };
                            if let Some(name) = name {
                                if name == "len" {
                                    TokenKind::Error("use '.len' instead of '@len'".to_owned())
                                } else {
                                    TokenKind::Error(format!(
                                        "unknown builtin '@{}'; functions use ordinary names",
                                        name
                                    ))
                                }
                            } else {
                                t.kind
                            }
                        }
                        None => TokenKind::Error("expected builtin name after '@'".to_owned()),
                    };
                    self.lookahead.push_back(t);
                }
                Some(t) => self.lookahead.push_back(t),
                None => break,
            }
        }
    }

    fn peek(&mut self) -> Option<Token> {
        self.fill_lookahead(1);
        self.lookahead.front().cloned()
    }

    fn peek_second(&mut self) -> Option<Token> {
        self.fill_lookahead(2);
        self.lookahead.get(1).cloned()
    }

    fn peek_n(&mut self, n: usize) -> Option<Token> {
        self.fill_lookahead(n);
        self.lookahead.get(n - 1).cloned()
    }

    fn next(&mut self) -> Option<Token> {
        self.fill_lookahead(1);
        let token = self.lookahead.pop_front();
        if let Some(t) = &token {
            self.last_pos = (t.line, t.column);
        }
        token
    }

    /// Consume and return the next token, or error if none.
    fn expect_next(&mut self, msg: &str) -> anyhow::Result<Token> {
        let (line, column) = self.last_pos;
        self.next().ok_or_else(|| self.error(msg, line, column))
    }

    /// Run `f` with struct/variant literals re-enabled (used inside
    /// parenthesized or bracketed subexpressions, where `Identifier {` is
    /// unambiguous again).
    fn allow_struct_literals<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let saved = std::mem::replace(&mut self.no_struct_literal, false);
        let result = f(self);
        self.no_struct_literal = saved;
        result
    }

    /// Consume and check the next token matches the token kind provided, or error.
    fn expect_token(&mut self, token_kind: TokenKind, msg: &str) -> anyhow::Result<Token> {
        let token = self.expect_next(msg)?;
        if token.kind == token_kind {
            Ok(token)
        } else {
            Err(self.error(msg, token.line, token.column))
        }
    }

    /// Consume the next token if it is any identifier, returning it. Errors otherwise.
    fn expect_identifier(&mut self, msg: &str) -> anyhow::Result<String> {
        let token = self.expect_next(msg)?;
        match token.kind {
            TokenKind::Identifier(name) => Ok(name),
            _ => Err(self.error(msg, token.line, token.column)),
        }
    }

    /// Consume the next token if it matches the predicate.
    fn consume_if<F>(&mut self, pred: F) -> Option<Token>
    where
        F: FnOnce(&Token) -> bool,
    {
        if let Some(token) = self.peek()
            && pred(&token)
        {
            return self.next();
        }
        None
    }

    pub fn parse(&mut self) -> anyhow::Result<Ast> {
        let mut statements: Vec<Statement> = Vec::new();

        while let Some(token) = self.peek() {
            let span = crate::diagnostics::Span::new(token.line, token.column);
            match token.kind {
                TokenKind::Keyword(Keyword::Function | Keyword::Extern) => {
                    let function = self.parse_function_declaration()?;
                    statements.push(Statement {
                        kind: StatementKind::FunctionDeclaration(function),
                        span,
                    });
                }
                TokenKind::Comment => {
                    self.next();
                }
                _ => statements.push(self.parse_statement()?),
            }
        }

        Ok(Ast::from(statements))
    }
}

mod access;
mod additive;
mod atom;
mod binary;
mod bitwise_and;
mod bitwise_or;
mod bitwise_xor;
mod body;
mod call;
mod cast;
mod comparison;
mod enum_literal;
mod equality;
mod expression;
mod function_declaration;
mod function_type;

#[cfg(test)]
mod declaration_tests;
mod logical_and;
mod logical_or;
mod pointer;
mod primary;
mod qualified_name;
mod shift;
mod statement;
mod struct_literal;
mod switch;
mod term;
mod test;
mod tuple;
mod type_expression;
mod type_fields;
mod unary;
mod variable_declaration;
