use crate::frontend::{ast::{expression::Expression, type_expression::TypeExpression}, parser::ParseResult, tokens::{Literal, Operator, Punctuation, Token, TokenKind, builtin::Builtin}};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_atom(&mut self) -> ParseResult<Expression> {
        // TODO: atom is too loaded, refactor into smaller functions
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
                    let member = self.expect_identifier("expected member name after '::'")?;
                    Expression::QualifiedAccess { module: id, member }
                // Check if next token is '{' — struct construction: TypeName { field: val, ... }
                } else if !self.no_struct_literal
                    && matches!(
                        self.peek(),
                        Some(Token {
                            kind: TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
                            ..
                        })
                    )
                {
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
                        let val = self.allow_struct_literals(|p| p.parse_expression())?;
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
                    elements.push(self.allow_struct_literals(|p| p.parse_expression())?);
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
                let inner_expr = self.allow_struct_literals(|p| p.parse_expression())?;
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                    "parse_atom: expected ')'",
                )?;
                Expression::Grouping(Box::new(inner_expr))
            }
            // A builtin type token in expression position — produces a comptime type value.
            // This allows passing builtin types as generic arguments: `identity(sint32, 42)`
            TokenKind::Builtin(Builtin::BuiltinType(bt)) => {
                Expression::TypeValue(TypeExpression::Builtin(bt))
            }
            // A builtin function call, e.g. `@concat(a, b)`. The parentheses are
            // mandatory — a bare `@concat` is an error.
            TokenKind::Builtin(Builtin::BuiltinFunction(bf)) => {
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::OpeningParenthesis),
                    "parse_atom: expected '(' after builtin function",
                )?;
                let mut args = Vec::new();
                if let Some(token) = self.peek()
                    && !matches!(
                        token.kind,
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                    )
                {
                    loop {
                        args.push(self.allow_struct_literals(|p| p.parse_expression())?);
                        if let Some(token) = self.peek()
                            && matches!(token.kind, TokenKind::Punctuation(Punctuation::Comma))
                        {
                            self.next(); // consume ','
                        } else {
                            break;
                        }
                    }
                }
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingParenthesis),
                    "parse_atom: expected ')' after builtin function arguments",
                )?;
                Expression::BuiltinCall { builtin: bf, args }
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
        while let Some(token) = self.peek() {
            if matches!(
                token.kind,
                TokenKind::Punctuation(Punctuation::OpeningParenthesis)
            ) {
                self.next(); // consume '('
                let mut args = Vec::new();
                if let Some(token) = self.peek()
                    && !matches!(
                        token.kind,
                        TokenKind::Punctuation(Punctuation::ClosingParenthesis)
                    )
                {
                    loop {
                        args.push(self.allow_struct_literals(|p| p.parse_expression())?);
                        if let Some(token) = self.peek() {
                            if matches!(token.kind, TokenKind::Punctuation(Punctuation::Comma)) {
                                self.next(); // consume ','
                            } else {
                                break;
                            }
                        } else {
                            break;
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
                    let index = self.allow_struct_literals(|p| p.parse_expression())?;
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                        "parse_atom: expected ']' after index expression",
                    )?;
                    expr = Expression::IndexAccess {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                } else {
                    let next_token =
                        self.expect_next("parse_atom: expected field name or operator after '.'")?;
                    match next_token.kind {
                        TokenKind::Operator(Operator::Star) => {
                            expr = Expression::Dereference(Box::new(expr));
                        }
                        TokenKind::Operator(Operator::Ampersand) => {
                            expr = Expression::AddressOf(Box::new(expr));
                        }
                        TokenKind::Identifier(f) => {
                            // If this is `TypeName.Variant { ... }` — an enum
                            // variant construction with payload — capture it
                            // here. Otherwise it's a plain field access.
                            if !self.no_struct_literal
                                && let Expression::Identifier(type_name) = &expr
                                && matches!(
                                    self.peek(),
                                    Some(Token {
                                        kind: TokenKind::Punctuation(
                                            Punctuation::OpeningCurlyBrace
                                        ),
                                        ..
                                    })
                                )
                            {
                                self.next(); // consume '{'
                                let mut fields = Vec::new();
                                while !matches!(
                                    self.peek(),
                                    Some(Token {
                                        kind: TokenKind::Punctuation(
                                            Punctuation::ClosingCurlyBrace
                                        ),
                                        ..
                                    }) | None
                                ) {
                                    let fname = self.expect_identifier(
                                        "expected field name in variant payload",
                                    )?;
                                    self.expect_token(
                                        TokenKind::Punctuation(Punctuation::Colon),
                                        "expected ':' after field name in variant payload",
                                    )?;
                                    let val =
                                        self.allow_struct_literals(|p| p.parse_expression())?;
                                    fields.push((fname, val));
                                    self.consume_if(|t| {
                                        matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma))
                                    });
                                }
                                self.next(); // consume '}'
                                let tn = type_name.clone();
                                expr = Expression::EnumVariantConstruct {
                                    type_name: tn,
                                    variant: f,
                                    fields,
                                };
                            } else {
                                expr = Expression::FieldAccess {
                                    object: Box::new(expr),
                                    field: f,
                                };
                            }
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
        }

        Ok(expr)
    }
}
