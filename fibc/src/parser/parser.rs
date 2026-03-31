use std::fmt;
use std::iter::Peekable;
use std::path::Path;

use crate::ast::ast::{
    Ast, ConstantDeclaration, DeclarationNode, Expression, Field, FunctionBody,
    FunctionDeclaration, FunctionParameter, FunctionSignature, ImportDeclaration, PointerVariant,
    StatementNode, TypeExpression, VariableDeclaration,
};
use crate::token::builtin::Builtin;
use crate::token::{Keyword, Literal, Operator, Punctuation, Token, TokenKind};

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
    I: Iterator<Item = Token> + Clone,
{
    tokens: Peekable<I>,
    filename: &'a Path,
    source_lines: Vec<String>,
}

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token> + Clone,
{
    pub fn new(tokens: I, filename: &'a Path, source: String) -> Self {
        Self {
            tokens: tokens.peekable(),
            filename,
            source_lines: source.lines().map(|s| s.to_string()).collect(),
        }
    }

    fn error<'err>(&'err self, message: &str, line: usize, column: usize) -> ParseError {
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

    fn peek(&self) -> Option<Token> {
        // Clone the internal Peekable iterator and peek on the clone so we don't need &mut
        let mut cloned = self.tokens.clone();
        cloned.peek().cloned()
    }

    fn peek_second(&self) -> Option<Token> {
        let mut cloned = self.tokens.clone();
        cloned.next(); // skip first token
        cloned.peek().cloned()
    }

    fn next(&mut self) -> Option<Token> {
        self.tokens.next()
    }

    /// Consume and return the next token, or error if none.
    fn expect_next(&mut self, msg: &str) -> ParseResult<Token> {
        self.next().ok_or_else(|| {
            let line = 0;
            let column = 0;
            self.error(msg, line, column)
        })
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
    fn expect_identifier(&mut self, msg: &str) -> ParseResult<Token> {
        let token = self.expect_next(msg)?;
        if matches!(token.kind, TokenKind::Identifier(_)) {
            Ok(token)
        } else {
            Err(self.error(msg, token.line, token.column))
        }
    }

    /// Consume the next token if it matches the predicate.
    fn consume_if<F>(&mut self, pred: F) -> Option<Token>
    where
        F: FnOnce(&Token) -> bool,
    {
        if let Some(token) = self.peek() {
            if pred(&token) {
                return self.next();
            }
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
                // For now, treat other top-level constructs as statements or ignore
                _ => {
                    // try to parse a statement and keep it as a top-level declaration
                    if let Some(stmt) = self.parse_statement()? {
                        declarations.push(DeclarationNode::Statement(stmt));
                    } else {
                        break;
                    }
                }
            }
        }

        let mut ast = Ast::new();
        ast.declarations = declarations;

        Ok(ast)
    }

    fn parse_statement_some(&mut self) -> ParseResult<StatementNode> {
        loop {
            match self.parse_statement()? {
                Some(statement) => return Ok(statement),
                None => {
                    // A comment was consumed; keep trying or error at EOF
                    if self.peek().is_none() {
                        return Err(self.error("expected a statement", 0, 0));
                    }
                }
            }
        }
    }

    fn parse_statement(&mut self) -> ParseResult<Option<StatementNode>> {
        let stmt = if let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Comment => {
                    self.next();
                    return Ok(None);
                }
                TokenKind::Keyword(Keyword::If) => {
                    self.next(); // consume 'if'
                    let condition = self.parse_expression()?;
                    // Parse then-branch using shared parse_body
                    let then_branch = self.parse_body()?;
                    // Check for optional else
                    let else_branch = if let Some(token) = self.peek() {
                        if matches!(token.kind, TokenKind::Keyword(Keyword::Else)) {
                            self.next(); // consume 'else'
                            let else_stmts = self.parse_body()?;
                            Some(else_stmts)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    StatementNode::If {
                        condition,
                        then_branch,
                        else_branch,
                    }
                }
                TokenKind::Keyword(Keyword::Const) => {
                    let stmt = self.parse_constant_declaration()?;
                    StatementNode::ConstantDeclaration(stmt)
                }
                TokenKind::Keyword(Keyword::Var) => {
                    let stmt = self.parse_variable_declaration()?;
                    StatementNode::VariableDeclaration(stmt)
                }
                TokenKind::Keyword(Keyword::Defer) => {
                    self.next(); // consume 'defer'
                    let inner = self.parse_statement_some()?;
                    StatementNode::Defer(Box::new(inner))
                }
                TokenKind::Keyword(Keyword::Break) => {
                    self.next(); // consume 'break'
                    if let Some(t) = self.peek() {
                        if matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)) {
                            self.next();
                        }
                    }
                    StatementNode::Break
                }
                TokenKind::Keyword(Keyword::Continue) => {
                    self.next(); // consume 'continue'
                    if let Some(t) = self.peek() {
                        if matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)) {
                            self.next();
                        }
                    }
                    StatementNode::Continue
                }
                TokenKind::Keyword(Keyword::Return) => {
                    self.next(); // consume 'return'
                    // Optionally parse an expression after return
                    if let Some(token) = self.peek() {
                        // If next token is not a semicolon or block close, parse expression
                        match token.kind {
                            TokenKind::Punctuation(Punctuation::Semicolon)
                            | TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) => {
                                StatementNode::Return(None)
                            }
                            _ => {
                                let expr = self.parse_expression()?;
                                StatementNode::Return(Some(expr))
                            }
                        }
                    } else {
                        StatementNode::Return(None)
                    }
                }
                TokenKind::Identifier(_) => {
                    // Use two-token lookahead: if the token after the identifier
                    // is '=', parse as an assignment; otherwise delegate entirely
                    // to parse_expression so that calls, bare identifiers, etc.
                    // are handled by parse_atom without duplicating that logic here.
                    if matches!(
                        self.peek_second(),
                        Some(Token {
                            kind: TokenKind::Operator(Operator::Assign),
                            ..
                        })
                    ) {
                        let id = if let TokenKind::Identifier(id) = self.next().unwrap().kind {
                            id
                        } else {
                            unreachable!()
                        };
                        self.expect_token(TokenKind::Operator(Operator::Assign), "expected '='")?;
                        let expr = self.parse_expression()?;
                        StatementNode::Assignment {
                            identifier: id,
                            expr,
                        }
                    } else if matches!(
                        self.peek_second(),
                        Some(Token {
                            kind: TokenKind::Operator(
                                Operator::PlusAssign
                                    | Operator::MinusAssign
                                    | Operator::StarAssign
                                    | Operator::SlashAssign
                                    | Operator::PercentAssign
                            ),
                            ..
                        })
                    ) {
                        let id = if let TokenKind::Identifier(id) = self.next().unwrap().kind {
                            id
                        } else {
                            unreachable!()
                        };
                        let compound_op_token = self.next().unwrap();
                        let binary_op = match compound_op_token.kind {
                            TokenKind::Operator(Operator::PlusAssign) => Operator::Plus,
                            TokenKind::Operator(Operator::MinusAssign) => Operator::Minus,
                            TokenKind::Operator(Operator::StarAssign) => Operator::Star,
                            TokenKind::Operator(Operator::SlashAssign) => Operator::Slash,
                            TokenKind::Operator(Operator::PercentAssign) => Operator::Percent,
                            _ => unreachable!(),
                        };
                        let rhs = self.parse_expression()?;
                        // Desugar: x op= rhs  =>  x = x op rhs
                        let expr = Expression::Binary {
                            left: Box::new(Expression::Identifier(id.clone())),
                            operator: binary_op,
                            right: Box::new(rhs),
                        };
                        StatementNode::Assignment {
                            identifier: id,
                            expr,
                        }
                    } else if matches!(
                        self.peek_second(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::Dot),
                            ..
                        })
                    ) {
                        // Could be a field assignment: `obj.field = expr`
                        // We parse the identifier and dot, check for field name then '='.
                        // If the next token after the field name is '=', it's a field assign.
                        // Otherwise fall back to expression statement.
                        let obj_id = if let TokenKind::Identifier(id) = self.next().unwrap().kind {
                            id
                        } else {
                            unreachable!()
                        };
                        // consume '.'
                        self.next();
                        // Check for `.[` — index assign
                        if matches!(
                            self.peek(),
                            Some(Token {
                                kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                                ..
                            })
                        ) {
                            self.next(); // consume '['
                            let index = self.parse_expression()?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                                "expected ']' after index expression",
                            )?;
                            if matches!(
                                self.peek(),
                                Some(Token {
                                    kind: TokenKind::Operator(Operator::Assign),
                                    ..
                                })
                            ) {
                                self.next(); // consume '='
                                let rhs = self.parse_expression()?;
                                StatementNode::IndexAssign {
                                    object: Expression::Identifier(obj_id),
                                    index,
                                    expr: rhs,
                                }
                            } else {
                                // expression statement
                                StatementNode::ExpressionStatement(Expression::IndexAccess {
                                    object: Box::new(Expression::Identifier(obj_id)),
                                    index: Box::new(index),
                                })
                            }
                        } else {
                            let after_dot_token =
                                self.expect_next("expected field name or operator after '.'")?;
                            // Check if it's `.*` (deref assign) or a field name
                            if matches!(after_dot_token.kind, TokenKind::Operator(Operator::Star)) {
                                // `obj.* = expr` — dereference assignment
                                if matches!(
                                    self.peek(),
                                    Some(Token {
                                        kind: TokenKind::Operator(Operator::Assign),
                                        ..
                                    })
                                ) {
                                    self.next(); // consume '='
                                    let expr = self.parse_expression()?;
                                    StatementNode::DerefAssign {
                                        pointer: Expression::Identifier(obj_id),
                                        expr,
                                    }
                                } else {
                                    // `obj.*` as expression statement
                                    let base = Expression::Dereference(Box::new(
                                        Expression::Identifier(obj_id),
                                    ));
                                    StatementNode::ExpressionStatement(base)
                                }
                            } else {
                                let field_id =
                                    if let TokenKind::Identifier(f) = after_dot_token.kind {
                                        f
                                    } else {
                                        return Err(self.error(
                                            "expected field name",
                                            after_dot_token.line,
                                            after_dot_token.column,
                                        ));
                                    };
                                // if next is '=', it's a field assignment
                                if matches!(
                                    self.peek(),
                                    Some(Token {
                                        kind: TokenKind::Operator(Operator::Assign),
                                        ..
                                    })
                                ) {
                                    self.next(); // consume '='
                                    let expr = self.parse_expression()?;
                                    StatementNode::FieldAssign {
                                        object: obj_id,
                                        field: field_id,
                                        expr,
                                    }
                                } else {
                                    // Rebuild a FieldAccess expression and continue as expression statement
                                    let base = Expression::FieldAccess {
                                        object: Box::new(Expression::Identifier(obj_id)),
                                        field: field_id,
                                    };
                                    // We already consumed the field access; now parse rest of the expression
                                    // by treating `base` as the already-parsed left side.
                                    // Since we can't easily re-enter the expression parser mid-stream,
                                    // just wrap as an expression statement.
                                    StatementNode::ExpressionStatement(base)
                                }
                            }
                        } // close else { for non-.[ case
                    } else {
                        let expr = self.parse_expression()?;
                        StatementNode::ExpressionStatement(expr)
                    }
                }
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
                            let statement = self
                                .parse_statement()?
                                .ok_or_else(|| self.error("expected statement", 0, 0))?;
                            self.expect_token(
                                TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                                "expected ')'",
                            )?;
                            Some(Box::new(statement))
                        }
                    };
                    let body = self.parse_body()?;
                    StatementNode::For {
                        initializer,
                        condition,
                        post_operation,
                        body,
                    }
                }
                TokenKind::Literal(_) => {
                    let expr = self.parse_expression()?;
                    StatementNode::ExpressionStatement(expr)
                }
                TokenKind::Operator(Operator::LogicalNot) => {
                    let expr = self.parse_expression()?;
                    StatementNode::ExpressionStatement(expr)
                }
                TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                    let expr = self.parse_expression()?;
                    StatementNode::ExpressionStatement(expr)
                }
                TokenKind::Keyword(Keyword::While) => {
                    self.next(); // consume "while"
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                        "expected '('",
                    )?;
                    let expr = self.parse_expression()?;
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                        "expected ')'",
                    )?;
                    let body = self.parse_body()?;
                    StatementNode::For {
                        initializer: None,
                        condition: Some(expr),
                        post_operation: None,
                        body,
                    }
                }
                TokenKind::Keyword(Keyword::Else)
                | TokenKind::Operator(_)
                | TokenKind::Punctuation(_)
                | TokenKind::Builtin(_)
                | TokenKind::Error(_)
                | TokenKind::Keyword(_)
                | TokenKind::Unknown(_) => {
                    let t = token.clone();
                    return Err(self.error("unsupported", t.line, t.column));
                }
            }
        } else {
            return Ok(None);
        };

        // Optionally consume a semicolon if present
        self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)));
        Ok(Some(stmt))
    }

    /// Parses a constant declaration statement: const [<type>] <name> = <init>[;]
    fn parse_constant_declaration(&mut self) -> ParseResult<ConstantDeclaration> {
        let const_token = self.expect_token(
            TokenKind::Keyword(Keyword::Const),
            "parse_constant_declaration: expected 'const' keyword",
        )?;

        // Lookahead: if the next token is an identifier immediately followed by `=`,
        // there is no type annotation (e.g. `const x = 5`).
        let var_type = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Identifier(_),
                ..
            })
        ) && matches!(
            self.peek_second(),
            Some(Token {
                kind: TokenKind::Operator(Operator::Assign),
                ..
            })
        ) {
            None
        } else {
            self.parse_type()?
        };

        let ident = if let TokenKind::Identifier(ident) = self
            .expect_next("parse_constant_declaration: expected identifier")?
            .kind
        {
            ident
        } else {
            return Err(self.error("expected identifier", const_token.line, const_token.column));
        };

        self.expect_token(TokenKind::Operator(Operator::Assign), "expected = operator")?;

        // When the type annotation is `type`, parse the RHS as a type expression
        // and wrap it in Expression::TypeValue so it flows through the normal const path.
        let expr = if matches!(var_type, Some(TypeExpression::TypeKeyword)) {
            let te = self.parse_type()?.ok_or_else(|| {
                let (line, col) = self.peek().map_or((0, 0), |t| (t.line, t.column));
                self.error("expected a type expression after '='", line, col)
            })?;
            // consume optional semicolon
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)));
            Expression::TypeValue(te)
        } else {
            self.parse_expression()?
        };

        Ok(ConstantDeclaration::new(ident, var_type, expr))
    }

    /// Parses a variable declaration statement: var <type> <name> = <init>[;]
    fn parse_variable_declaration(&mut self) -> ParseResult<VariableDeclaration> {
        let const_token = self.expect_token(
            TokenKind::Keyword(Keyword::Var),
            "parse_variable_declaration: expected 'var' keyword",
        )?;

        let var_type = self.parse_type()?;

        let ident = if let TokenKind::Identifier(ident) = self
            .expect_next("parse_variable_declaration: expected identifier")?
            .kind
        {
            ident
        } else {
            return Err(self.error("expected identifier", const_token.line, const_token.column));
        };

        let expr =
            match self.consume_if(|t| matches!(t.kind, TokenKind::Operator(Operator::Assign))) {
                Some(_) => Some(self.parse_expression()?),
                None => None,
            };

        // optional semicolon
        // self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Semicolon)));

        Ok(VariableDeclaration::new(ident, var_type, expr))
    }

    /// Parses a type annotation, which can be a user-defined type (type identifier), or complex type (struct, variant or function).
    /// Returns `Ok(Some(TypeIdentifier))` if a type is successfully parsed, `Ok(None)` if the next token is not a type, or `Err` if there's a syntax error while parsing a type.
    pub fn parse_type(&mut self) -> ParseResult<Option<TypeExpression>> {
        let type_token = if let Some(t) = self.peek() {
            t
        } else {
            return Ok(None);
        };
        let var_type: TypeExpression = match type_token.kind {
            TokenKind::Builtin(Builtin::BuiltinType(builtin_type)) => {
                self.next();
                TypeExpression::Builtin(builtin_type)
            }
            TokenKind::Keyword(ref keyword) => {
                self.next();
                match keyword {
                    Keyword::Struct => self.parse_struct_literal(&type_token)?,
                    Keyword::Function => self.parse_function_type(&type_token)?,
                    Keyword::Type => TypeExpression::TypeKeyword,
                    _ => return Err(self.error("not a type", type_token.line, type_token.column)),
                }
            }
            TokenKind::Operator(Operator::Star) => {
                self.next();
                self.parse_pointer_type(PointerVariant::Raw, type_token)?
            }
            TokenKind::Identifier(module) => {
                self.next();
                // Check for `::` — qualified type: module::TypeName
                if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                        ..
                    })
                ) {
                    self.next(); // consume `::`
                    let name_token = self.expect_identifier("expected type name after '::'")?;
                    let name = if let TokenKind::Identifier(id) = name_token.kind {
                        id
                    } else {
                        unreachable!()
                    };
                    TypeExpression::QualifiedIdentifier { module, name }
                } else {
                    TypeExpression::Identifier(module)
                }
            }
            _ => {
                return Ok(None);
            }
        };
        // Postfix array type: type[size]
        let var_type = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                ..
            })
        ) {
            self.next(); // consume '['
            let size_token = self.expect_next("expected array size")?;
            let size = if let TokenKind::Literal(Literal::Integer(n)) = size_token.kind {
                n
            } else {
                return Err(self.error(
                    "expected integer array size",
                    size_token.line,
                    size_token.column,
                ));
            };
            self.expect_token(
                TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                "expected ']' after array size",
            )?;
            TypeExpression::Array {
                element_type: Box::new(var_type),
                size,
            }
        } else {
            var_type
        };
        Ok(Some(var_type))
    }

    fn parse_function_type(&mut self, type_token: &Token) -> ParseResult<TypeExpression> {
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
            let argument_type = match self.parse_type()? {
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
        let return_type = match self.parse_type()? {
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

    #[allow(dead_code)]
    fn parse_pointer(&mut self, pointer_variant: PointerVariant) -> ParseResult<TypeExpression> {
        let next_token = self.expect_token(
            TokenKind::Operator(Operator::Ampersand),
            "expected AMPERSAND (&) after token",
        )?;
        if let TokenKind::Operator(Operator::Ampersand) = next_token.kind {
            self.parse_pointer_type(pointer_variant, next_token)
        } else {
            unreachable!()
        }
    }

    fn parse_pointer_type(
        &mut self,
        pointer_variant: PointerVariant,
        next_token: Token,
    ) -> ParseResult<TypeExpression> {
        let pointed_type = match self.parse_type()? {
            Some(type_id) => type_id,
            None => {
                return Err(self.error("expected type", next_token.line, next_token.column));
            }
        };
        Ok(TypeExpression::Pointer {
            pointer_variant,
            pointed_type: Box::new(pointed_type),
        })
    }

    fn parse_struct_literal(&mut self, type_token: &Token) -> ParseResult<TypeExpression> {
        // consume 'struct' keyword
        match self.next() {
            Some(first_token) => match first_token.kind {
                TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => {
                    let fields = self.parse_type_fields()?;
                    Ok(TypeExpression::Struct { fields })
                }
                _ => {
                    return Err(self.error(
                        "expected an open curly brace",
                        first_token.line,
                        first_token.column,
                    ));
                }
            },
            None => {
                return Err(self.error(
                    "expected a type keyword",
                    type_token.line,
                    type_token.column,
                ));
            }
        }
    }

    fn parse_type_fields(&mut self) -> ParseResult<Vec<Field>> {
        let mut fields: Vec<Field> = Vec::new();
        while let Some(next_token) = self.peek() {
            match next_token.kind {
                TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) => {
                    self.next(); // consume the closing curly brace
                    break;
                }
                _ => {}
            }
            let line = next_token.line;
            let column = next_token.column;
            let field_type = match self.parse_type()? {
                Some(field_type) => field_type,
                None => {
                    return Err(self.error("expected a type identifier", line, column));
                }
            };
            let name_token = self.expect_identifier("expected field name")?;
            let label = if let TokenKind::Identifier(id) = name_token.kind {
                id
            } else {
                unreachable!()
            };
            fields.push(Field {
                label,
                type_id: field_type,
            });
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        Ok(fields)
    }

    fn parse_expression(&mut self) -> ParseResult<Expression> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_logical_and()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::LogicalOr) => {
                    self.next();
                    let right = Box::new(self.parse_logical_and()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::LogicalOr,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_or()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::LogicalAnd) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_or()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::LogicalAnd,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_bitwise_or(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_xor()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Pipe) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_xor()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Pipe,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_bitwise_xor(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_bitwise_and()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Caret) => {
                    self.next();
                    let right = Box::new(self.parse_bitwise_and()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Caret,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_bitwise_and(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_equality()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::Ampersand) => {
                    self.next();
                    let right = Box::new(self.parse_equality()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: Operator::Ampersand,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// Parse equality and comparison expressions (==, !=, >, <, >=, <=)
    fn parse_equality(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_comparison()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op @ (Operator::DoubleEquals | Operator::Different)) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_comparison()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_cast()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(
                    op @ (Operator::GreaterThan
                    | Operator::LesserThan
                    | Operator::GreaterEqual
                    | Operator::LesserEqual),
                ) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_cast()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_cast(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_shift()?;
        while self
            .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::As)))
            .is_some()
        {
            let target_type = self.parse_type()?.ok_or_else(|| {
                let (line, col) = self.peek().map_or((0, 0), |t| (t.line, t.column));
                self.error("expected type after 'as'", line, col)
            })?;
            expr = Expression::Cast {
                expr: Box::new(expr),
                target_type,
            };
        }
        Ok(expr)
    }

    fn parse_shift(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_additive()?;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op @ (Operator::LeftShift | Operator::RightShift)) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_additive()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_term()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(op @ (Operator::Plus | Operator::Minus)) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_term()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> ParseResult<Expression> {
        let mut expr = self.parse_unary()?;

        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(
                    op @ (Operator::Star | Operator::Slash | Operator::Percent),
                ) => {
                    let op = *op;
                    self.next();
                    let right = Box::new(self.parse_unary()?);
                    expr = Expression::Binary {
                        left: Box::new(expr),
                        operator: op,
                        right,
                    };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> ParseResult<Expression> {
        if let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Operator(Operator::LogicalNot) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::LogicalNot,
                        expression: Box::new(expr),
                    })
                }
                TokenKind::Operator(Operator::Minus) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::Minus,
                        expression: Box::new(expr),
                    })
                }
                TokenKind::Operator(Operator::Tilde) => {
                    self.next();
                    let expr = self.parse_unary()?;
                    Ok(Expression::Unary {
                        operator: Operator::Tilde,
                        expression: Box::new(expr),
                    })
                }
                _ => self.parse_atom(),
            }
        } else {
            self.parse_atom()
        }
    }

    fn parse_atom(&mut self) -> ParseResult<Expression> {
        let token = self.expect_next("parse_atom: expected a token, found none")?;
        let mut expr = match token.kind {
            TokenKind::Literal(Literal::Integer(integer_literal)) => {
                Expression::Literal(Literal::Integer(integer_literal))
            }
            TokenKind::Literal(Literal::Float(float_literal)) => {
                Expression::Literal(Literal::Float(float_literal))
            }
            TokenKind::Literal(Literal::Boolean(boolean_literal)) => {
                Expression::Literal(Literal::Boolean(boolean_literal))
            }
            TokenKind::Literal(Literal::Character(char_literal)) => {
                Expression::Literal(Literal::Character(char_literal))
            }
            TokenKind::Literal(Literal::String(s)) => Expression::Literal(Literal::String(s)),
            TokenKind::Identifier(id) => {
                // Check if next token is `::` — qualified access: module::member
                if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::DoubleColon),
                        ..
                    })
                ) {
                    self.next(); // consume `::`
                    let member_token = self.expect_identifier("expected member name after '::'")?;
                    let member = if let TokenKind::Identifier(m) = member_token.kind {
                        m
                    } else {
                        unreachable!()
                    };
                    Expression::QualifiedAccess { module: id, member }
                // Check if next token is '{' — struct construction: TypeName { field: val, ... }
                } else if matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                        ..
                    })
                ) {
                    self.next(); // consume '{'
                    let mut fields = Vec::new();
                    while !matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
                            ..
                        }) | None
                    ) {
                        let fname_token =
                            self.expect_next("expected field name in struct construction")?;
                        let fname = if let TokenKind::Identifier(f) = fname_token.kind {
                            f
                        } else {
                            return Err(self.error(
                                "expected field name",
                                fname_token.line,
                                fname_token.column,
                            ));
                        };
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::Colon),
                            "expected ':' after field name in struct construction",
                        )?;
                        let val = self.parse_expression()?;
                        fields.push((fname, val));
                        // optional comma
                        self.consume_if(|t| {
                            matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                        });
                    }
                    self.next(); // consume '}'
                    Expression::StructConstruct {
                        type_name: id,
                        fields,
                    }
                } else {
                    Expression::Identifier(id)
                }
            }
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => {
                let mut elements = Vec::new();
                while !matches!(
                    self.peek(),
                    Some(Token {
                        kind: TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                        ..
                    }) | None
                ) {
                    elements.push(self.parse_expression()?);
                    self.consume_if(|t| {
                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                    });
                }
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                    "parse_atom: expected ']' after array literal",
                )?;
                Expression::ArrayLiteral { elements }
            }
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                let inner_expr = self.parse_expression()?;
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                    "parse_atom: expected ')'",
                )?;
                Expression::Grouping(Box::new(inner_expr))
            }
            // A builtin type token in expression position — produces a comptime type value.
            // This allows passing builtin types as generic arguments: `identity(sint32, 42)`
            TokenKind::Builtin(crate::token::builtin::Builtin::BuiltinType(bt)) => {
                Expression::TypeValue(TypeExpression::Builtin(bt))
            }
            TokenKind::Literal(Literal::Null) => Expression::Literal(Literal::Null),
            _ => {
                return Err(self.error(
                    &format!("parse_atom: expected an atom, found {:?}", token.kind),
                    token.line,
                    token.column,
                ));
            }
        };

        // Parse postfix operations: function calls and field access
        loop {
            if let Some(token) = self.peek() {
                if matches!(
                    token.kind,
                    TokenKind::Punctuation(Punctuation::OpeningParenthesis)
                ) {
                    self.next(); // consume '('
                    let mut args = Vec::new();
                    if let Some(token) = self.peek() {
                        if !matches!(
                            token.kind,
                            TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                        ) {
                            loop {
                                args.push(self.parse_expression()?);
                                if let Some(token) = self.peek() {
                                    if matches!(
                                        token.kind,
                                        TokenKind::Punctuation(Punctuation::Comma)
                                    ) {
                                        self.next(); // consume ','
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                        "parse_atom: expected ')' after function call arguments",
                    )?;
                    expr = Expression::Call {
                        callee: Box::new(expr),
                        args,
                    };
                } else if matches!(token.kind, TokenKind::Punctuation(Punctuation::Dot)) {
                    self.next(); // consume '.'
                    // Check for `.[ index ]` before consuming
                    if matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
                            ..
                        })
                    ) {
                        self.next(); // consume '['
                        let index = self.parse_expression()?;
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                            "parse_atom: expected ']' after index expression",
                        )?;
                        expr = Expression::IndexAccess {
                            object: Box::new(expr),
                            index: Box::new(index),
                        };
                    } else {
                        let next_token = self
                            .expect_next("parse_atom: expected field name or operator after '.'")?;
                        match next_token.kind {
                            TokenKind::Operator(Operator::Star) => {
                                expr = Expression::Dereference(Box::new(expr));
                            }
                            TokenKind::Operator(Operator::Ampersand) => {
                                expr = Expression::AddressOf(Box::new(expr));
                            }
                            TokenKind::Identifier(f) => {
                                expr = Expression::FieldAccess {
                                    object: Box::new(expr),
                                    field: f,
                                };
                            }
                            _ => {
                                return Err(self.error(
                                    "expected field name, '.*', '.&', or '.[' after '.'",
                                    next_token.line,
                                    next_token.column,
                                ));
                            }
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parses an import declaration: `import a::b::c`, `import a::b as x`, `import a::b::{X, y}`
    fn parse_import_declaration(&mut self) -> ParseResult<ImportDeclaration> {
        self.expect_token(TokenKind::Keyword(Keyword::Import), "expected 'import'")?;

        // Parse the first path segment (must be an identifier)
        let first = self.expect_identifier("expected module path after 'import'")?;
        let first_id = if let TokenKind::Identifier(id) = first.kind {
            id
        } else {
            unreachable!()
        };
        let mut path = vec![first_id];

        // Continue consuming `::identifier` segments
        while let Some(token) = self.peek() {
            if !matches!(token.kind, TokenKind::Punctuation(Punctuation::DoubleColon)) {
                break;
            }
            // Peek at what follows the `::`
            if let Some(next) = self.peek_second() {
                match &next.kind {
                    TokenKind::Identifier(_) => {
                        self.next(); // consume `::`
                        let seg_token = self.next().unwrap();
                        if let TokenKind::Identifier(id) = seg_token.kind {
                            path.push(id);
                        }
                    }
                    TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => {
                        // `::{ ... }` selective import
                        self.next(); // consume `::`
                        self.next(); // consume `{`
                        let mut selective = Vec::new();
                        loop {
                            let name_token =
                                self.expect_identifier("expected symbol name in selective import")?;
                            if let TokenKind::Identifier(id) = name_token.kind {
                                selective.push(id);
                            }
                            if self
                                .consume_if(|t| {
                                    matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                                })
                                .is_none()
                            {
                                break;
                            }
                        }
                        self.expect_token(
                            TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
                            "expected '}' to close selective import",
                        )?;
                        return Ok(ImportDeclaration {
                            path,
                            alias: None,
                            selective: Some(selective),
                        });
                    }
                    _ => {
                        // consume the `::` so it doesn't leak into the outer parser
                        self.next();
                        let t = next.clone();
                        return Err(self.error(
                            "expected identifier or '{' after '::'",
                            t.line,
                            t.column,
                        ));
                    }
                }
            } else {
                // `::` at end of input — consume it and report the error
                let dc = self.next().unwrap();
                return Err(self.error(
                    "expected identifier or '{' after '::'",
                    dc.line,
                    dc.column,
                ));
            }
        }

        // Check for `as alias`
        let alias = if let Some(token) = self.peek()
            && matches!(token.kind, TokenKind::Keyword(Keyword::As))
        {
            self.next(); // consume `as`
            let alias_token = self.expect_identifier("expected alias name after 'as'")?;
            if let TokenKind::Identifier(id) = alias_token.kind {
                Some(id)
            } else {
                unreachable!()
            }
        } else {
            None
        };

        Ok(ImportDeclaration {
            path,
            alias,
            selective: None,
        })
    }

    fn parse_function_declaration(&mut self) -> ParseResult<FunctionDeclaration> {
        // Check for optional 'extern' prefix
        let is_extern = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Keyword(Keyword::Extern),
                ..
            })
        ) {
            self.next(); // consume 'extern'
            true
        } else {
            false
        };

        self.expect_token(
            TokenKind::Keyword(Keyword::Function),
            "parse_function_declaration: expected 'fn' keyword",
        )?;

        // Function name
        let name_token =
            self.expect_identifier("parse_function_declaration: expected function name")?;
        let name = if let TokenKind::Identifier(n) = name_token.kind {
            n
        } else {
            unreachable!()
        };

        // Parameters
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            "parse_function_declaration: expected '('",
        )?;
        let mut args = Vec::new();
        let mut is_variadic = false;
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(Punctuation::ClosingParenthesis) => {
                    self.next();
                    break;
                }
                TokenKind::Operator(Operator::Ellipsis) => {
                    self.next(); // consume '...'
                    is_variadic = true;
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                        "expected ')' after '...' in variadic parameter list",
                    )?;
                    break;
                }
                _ => {
                    let line = token.line;
                    let column = token.column;

                    let argument_type = match self.parse_type()? {
                        Some(type_id) => type_id,
                        None => {
                            return Err(self.error("expected parameter type", line, column));
                        }
                    };

                    let param_name_token = self
                        .expect_identifier("parse_function_declaration: expected parameter name")?;
                    let argument_name = if let TokenKind::Identifier(n) = param_name_token.kind {
                        n
                    } else {
                        unreachable!()
                    };

                    args.push(FunctionParameter {
                        parameter_name: argument_name,
                        parameter_type: argument_type,
                    });

                    // Optional comma
                    self.consume_if(|t| {
                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                    });
                }
            }
        }

        // Return type
        let return_type = self.parse_type()?;

        // Function body: extern functions use ';' or have no body; regular functions use '{...}'
        let body = if let Some(token) = self.peek() {
            if matches!(token.kind, TokenKind::Punctuation(Punctuation::Semicolon)) {
                self.next(); // consume ';'
                None
            } else if is_extern {
                // extern fn without ';' — just no body
                None
            } else {
                Some(FunctionBody {
                    statements: self.parse_body()?,
                })
            }
        } else {
            None
        };

        Ok(FunctionDeclaration {
            signature: FunctionSignature {
                name,
                parameters: args,
                return_type,
            },
            body,
            is_extern,
            is_variadic,
        })
    }

    /// Parse a block body: expects '{' then parses statements until matching '}'.
    fn parse_body(&mut self) -> ParseResult<Vec<StatementNode>> {
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            "parse_body: expected '{'",
        )?;

        let mut stmts = Vec::new();
        loop {
            match self.peek() {
                None => {
                    return Err(self.error("unclosed block: expected '}'", 0, 0));
                }
                Some(token)
                    if matches!(
                        token.kind,
                        TokenKind::Punctuation(Punctuation::ClosingCurlyBrace)
                    ) =>
                {
                    self.next();
                    break;
                }
                _ => {}
            }
            if let Some(stmt) = self.parse_statement()? {
                stmts.push(stmt);
            }
        }
        Ok(stmts)
    }
}
