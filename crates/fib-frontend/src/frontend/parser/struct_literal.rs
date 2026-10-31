use crate::diagnostics::Span;
use crate::frontend::ast::qualified_name::QualifiedIdentifier;
use crate::frontend::ast::structl_field::StructField;
use crate::frontend::{
    ast::{
        expression::{Expression, ExpressionKind},
        type_expression::{TypeExpression, TypeExpressionKind},
    },
    tokens::{Punctuation, Token, TokenKind},
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_struct_literal(&mut self, type_token: &Token) -> anyhow::Result<TypeExpression> {
        // consume 'struct' keyword
        match self.next() {
            Some(first_token) => match first_token.kind {
                TokenKind::Punctuation(Punctuation::OpeningCurlyBrace) => {
                    let (fields, members) = self.parse_type_fields()?;
                    Ok(TypeExpression::at(
                        TypeExpressionKind::Struct { fields, members },
                        Span::new(type_token.line, type_token.column),
                    ))
                }
                _ => Err(self.error(
                    "expected an open curly brace",
                    first_token.line,
                    first_token.column,
                )),
            },
            None => Err(self.error(
                "expected a type keyword",
                type_token.line,
                type_token.column,
            )),
        }
    }

    /// Parse `{ name: expr, ... }` field lists shared by struct construction
    /// (`Type { ... }`) and enum variant payloads (`Type.Variant { ... }`).
    /// Assumes `{` was already consumed; consumes the closing `}`.
    ///
    /// `context` names the construct for errors (`"struct construction"`,
    /// `"variant payload"`). Like the original loops, a missing `}` at EOF
    /// is tolerated: the trailing `next()` result is discarded.
    // TODO: test this function
    pub fn parse_brace_fields(&mut self, context: &str) -> anyhow::Result<Vec<StructField>> {
        let mut fields = Vec::new();
        while !matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
                ..
            }) | None
        ) {
            let field_name =
                self.expect_identifier(&format!("expected field name in {}", context))?;
            let field_span = self.last_identifier_span;
            self.mark_name("property");
            self.expect_token(
                TokenKind::Punctuation(Punctuation::Colon),
                &format!("expected ':' after field name in {}", context),
            )?;
            let expression = self.allow_struct_literals(|p| p.parse_expression())?;
            fields.push(StructField {
                field_identifier: field_name,
                annotation: crate::frontend::analysis_annotation::MemberAnnotation {
                    source_span: field_span.map(Into::into),
                    ..Default::default()
                },
                expression,
            });
            // optional comma
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        self.next(); // consume '}'
        Ok(fields)
    }

    /// Parse `TypeName { field: value, ... }` after the type name.
    /// Assumes `{` was already consumed; `span` is the type name's position.
    pub fn parse_struct_construct(
        &mut self,
        struct_name: QualifiedIdentifier,
        span: Span,
    ) -> anyhow::Result<Expression> {
        let fields = self.parse_brace_fields("struct construction")?;
        Ok(Expression::at(
            ExpressionKind::StructConstruct {
                struct_identifier: struct_name,
                fields,
            },
            span,
        ))
    }
}
