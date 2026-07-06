use std::collections::VecDeque;
use std::fmt;
use std::path::Path;

use crate::frontend::ast::{
    Ast, declaration::DeclarationNode};
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::{Keyword, Token, TokenKind};

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
    pub fn new(tokens: I, filename: &'a Path, source: String) -> Self {
        Self {
            tokens,
            lookahead: VecDeque::new(),
            filename,
            source_lines: source.lines().map(|s| s.to_string()).collect(),
            last_pos: (0, 0),
            no_struct_literal: false,
        }
    }

    fn error(&self, message: &str, line: usize, column: usize) -> ParseError {
        let source_line = self
            .source_lines
            .get(line.saturating_sub(1))
            .cloned()
            .unwrap_or_default();
        ParseError {
            filename: self.filename.into(),
            message: message.to_string(),
            line,
            column,
            source_line,
        }
    }

    /// Fill the lookahead buffer with up to `n` tokens, skipping comments.
    fn fill_lookahead(&mut self, n: usize) {
        while self.lookahead.len() < n {
            match self.tokens.next() {
                Some(t) if matches!(t.kind, TokenKind::Comment) => continue,
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

    fn next(&mut self) -> Option<Token> {
        self.fill_lookahead(1);
        let token = self.lookahead.pop_front();
        if let Some(t) = &token {
            self.last_pos = (t.line, t.column);
        }
        token
    }

    /// Consume and return the next token, or error if none.
    fn expect_next(&mut self, msg: &str) -> ParseResult<Token> {
        let (line, column) = self.last_pos;
        self.next().ok_or_else(|| self.error(msg, line, column))
    }

    /// Run `f` with struct/variant literals re-enabled (used inside
    /// parenthesized or bracketed subexpressions, where `Identifier {` is
    /// unambiguous again).
    fn allow_struct_literals<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> ParseResult<T>,
    ) -> ParseResult<T> {
        let saved = std::mem::replace(&mut self.no_struct_literal, false);
        let result = f(self);
        self.no_struct_literal = saved;
        result
    }

    /// Consume and check the next token matches the token kind provided, or error.
    fn expect_token(&mut self, token_kind: TokenKind, msg: &str) -> ParseResult<Token> {
        let token = self.expect_next(msg)?;
        if token.kind == token_kind {
            Ok(token)
        } else {
            Err(self.error(msg, token.line, token.column))
        }
    }

    /// Consume the next token if it is any identifier, returning it. Errors otherwise.
    fn expect_identifier(&mut self, msg: &str) -> ParseResult<Identifier> {
        let token = self.expect_next(msg)?;
        match token.kind {
            TokenKind::Identifier(id) => Ok(id),
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

    pub fn parse(&mut self) -> ParseResult<Ast> {
        let mut declarations: Vec<DeclarationNode> = Vec::new();

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Comment => {
                    self.next();
                    continue;
                }
                TokenKind::Keyword(Keyword::Import) => {
                    let import = self.parse_import_declaration()?;
                    declarations.push(DeclarationNode::ImportDeclaration(import));
                }
                TokenKind::Keyword(Keyword::Function) => {
                    let func = self.parse_function_declaration()?;
                    declarations.push(DeclarationNode::FunctionDeclaration(func));
                }
                TokenKind::Keyword(Keyword::Extern) => {
                    let func = self.parse_function_declaration()?;
                    declarations.push(DeclarationNode::FunctionDeclaration(func));
                }
                TokenKind::Keyword(Keyword::Type) => {
                    let ty = self.parse_type_declaration()?;
                    declarations.push(DeclarationNode::TypeDeclaration(ty))
                }
                _ => {
                    return Err(self.error(
                        "expected 'import', 'fn', 'extern' or 'type'",
                        token.line,
                        token.column,
                    ));
                }
            }
        }

        let mut ast = Ast::new();
        ast.declarations = declarations;

        Ok(ast)
    }
}

mod additive;
mod assignment;
mod atom;
mod bitwise_and;
mod bitwise_or;
mod bitwise_xor;
mod body;
mod cast;
mod comparison;
mod enum_literal;
mod equality;
mod expression;
mod function_declaration;
mod function_type;
mod identifier_statement;
mod import_declaration;
mod logical_and;
mod logical_or;
mod pointer;
mod shift;
mod statement;
mod struct_literal;
mod switch;
mod term;
mod test;
mod tuple;
mod type_declaration;
mod type_expression;
mod type_fields;
mod unary;
mod variable_declaration;
