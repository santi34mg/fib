use crate::diagnostics::Span;
use crate::frontend::{
    ast::type_expression::{TypeExpression, TypeExpressionKind},
    tokens::{Keyword, Literal, Operator, Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    // FIXME: there is something smelly about Ok(None) at the end
    /// Parses a type annotation, which can be a user-defined type (type identifier),
    /// or complex type (struct, variant or function).
    pub fn parse_type_expression(&mut self) -> anyhow::Result<Option<TypeExpression>> {
        let type_token = if let Some(t) = self.peek() {
            t
        } else {
            return Ok(None);
        };
        let span = Span::new(type_token.line, type_token.column);
        if matches!(type_token.kind, TokenKind::Keyword(Keyword::Const)) {
            self.next();
            let inner = self.parse_type_expression()?.ok_or_else(|| {
                self.error(
                    "expected pointer, slice or array after const",
                    type_token.line,
                    type_token.column,
                )
            })?;
            if !matches!(
                inner.kind,
                TypeExpressionKind::Pointer { .. }
                    | TypeExpressionKind::Sentinel { .. }
                    | TypeExpressionKind::Slice { .. }
                    | TypeExpressionKind::Array { .. }
            ) {
                return Err(self.error(
                    "const type qualifier requires a pointer, slice or array",
                    type_token.line,
                    type_token.column,
                ));
            }
            return Ok(Some(TypeExpression::at(
                TypeExpressionKind::Readonly(Box::new(inner)),
                span,
            )));
        }
        let var_type: TypeExpression = match type_token.kind {
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace) => {
                self.next();
                let pointer = self
                    .consume_if(|t| matches!(t.kind, TokenKind::Operator(Operator::Star)))
                    .is_some();
                let size = if matches!(
                    self.peek().map(|t| t.kind),
                    Some(TokenKind::Literal(Literal::Integer(_)))
                ) {
                    let token = self.next().unwrap();
                    let TokenKind::Literal(Literal::Integer(n)) = token.kind else {
                        unreachable!()
                    };
                    Some(num_traits::ToPrimitive::to_u64(&n).ok_or_else(|| {
                        self.error("array size exceeds uint64", token.line, token.column)
                    })?)
                } else {
                    None
                };
                let sentinel = if self
                    .consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Colon)))
                    .is_some()
                {
                    let token = self.expect_next("expected byte sentinel")?;
                    let TokenKind::Literal(Literal::Integer(n)) = token.kind else {
                        return Err(self.error("expected byte sentinel", token.line, token.column));
                    };
                    Some(num_traits::ToPrimitive::to_u8(&n).ok_or_else(|| {
                        self.error("sentinel must fit uint8", token.line, token.column)
                    })?)
                } else {
                    None
                };
                if pointer && (sentinel.is_none() || size.is_some()) {
                    return Err(self.error(
                        "sequence pointer requires a sentinel",
                        type_token.line,
                        type_token.column,
                    ));
                }
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
                    "expected ']' in array or slice type",
                )?;
                let element_type = Box::new(self.parse_type_expression()?.ok_or_else(|| {
                    self.error(
                        "expected element type after ']'",
                        type_token.line,
                        type_token.column,
                    )
                })?);
                let inner = TypeExpression::at(
                    if pointer {
                        TypeExpressionKind::Pointer {
                            pointed_type: element_type,
                        }
                    } else {
                        match size {
                            Some(size) => TypeExpressionKind::Array { element_type, size },
                            None => TypeExpressionKind::Slice { element_type },
                        }
                    },
                    span,
                );
                if let Some(value) = sentinel {
                    TypeExpression::at(
                        TypeExpressionKind::Sentinel {
                            inner: Box::new(inner),
                            value,
                        },
                        span,
                    )
                } else {
                    inner
                }
            }
            TokenKind::Keyword(ref keyword) => {
                self.next();
                match keyword {
                    Keyword::Struct => self.parse_struct_literal(&type_token)?,
                    Keyword::Enum => self.parse_enum_literal(&type_token)?,
                    Keyword::Callconv => {
                        // The shared prefix parser consumes the keyword itself.
                        let value = self.expect_next("expected calling convention string")?;
                        if !matches!(&value.kind, TokenKind::Literal(crate::frontend::tokens::Literal::String(name)) if name == "C")
                        {
                            return Err(self.error(
                                "unsupported calling convention; expected \"C\"",
                                value.line,
                                value.column,
                            ));
                        }
                        self.expect_token(
                            TokenKind::Keyword(Keyword::Function),
                            "expected fn after calling convention",
                        )?;
                        let mut ty = self.parse_function_type(&type_token)?;
                        if let TypeExpressionKind::Function {
                            calling_convention, ..
                        } = &mut ty.kind
                        {
                            *calling_convention = crate::frontend::semantic_type::CallConvention::C;
                        }
                        ty
                    }
                    Keyword::Extern => {
                        return Err(self.error(
                            "extern has been replaced by callconv \"C\"",
                            type_token.line,
                            type_token.column,
                        ));
                    }
                    Keyword::Function => self.parse_function_type(&type_token)?,
                    Keyword::Comp => {
                        self.expect_token(
                            TokenKind::Keyword(Keyword::Function),
                            "expected 'fn' after 'comp' in function type",
                        )?;
                        self.parse_function_type(&type_token)?
                    }
                    _ => return Err(self.error("not a type", type_token.line, type_token.column)),
                }
            }
            TokenKind::Operator(Operator::Star) => {
                self.next();
                self.parse_pointer_type(type_token)?
            }
            TokenKind::Punctuation(Punctuation::OpeningParenthesis) => {
                self.next(); // consume '('
                self.parse_tuple_type_expression(&type_token)?
            }
            TokenKind::Identifier(_) => TypeExpression::at(
                TypeExpressionKind::QualifiedIdentifier(self.parse_qualified_name()?),
                span,
            ),
            _ => {
                // Surface lexer diagnostics instead of silently reporting
                // "not a type" downstream; anything else is not a type.
                match &type_token.kind {
                    TokenKind::Error(msg) => {
                        return Err(self.error(msg, type_token.line, type_token.column));
                    }
                    TokenKind::Unknown(c) => {
                        return Err(self.error(
                            &format!("unknown character '{}'", c),
                            type_token.line,
                            type_token.column,
                        ));
                    }
                    _ => {
                        return Ok(None);
                    }
                }
            }
        };
        let mut var_type = var_type;
        if let TypeExpressionKind::QualifiedIdentifier(name) = &var_type.kind
            && matches!(
                self.peek().map(|t| t.kind),
                Some(TokenKind::Punctuation(Punctuation::OpeningParenthesis))
            )
        {
            use crate::frontend::ast::expression::{Expression, ExpressionKind};
            let call = self.parse_call(Expression::at(
                ExpressionKind::QualifiedIdentifier(name.clone()),
                span,
            ))?;
            var_type = TypeExpression::at(TypeExpressionKind::Computed(Box::new(call)), span);
        }
        Ok(Some(var_type))
    }
}
