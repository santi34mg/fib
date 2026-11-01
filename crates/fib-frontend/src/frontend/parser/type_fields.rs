use crate::{
    diagnostics::Span,
    frontend::{
        ast::{
            expression::{Expression, ExpressionKind},
            structl_field::StructField,
        },
        tokens::{Keyword, Operator, Punctuation, Token, TokenKind},
    },
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_type_fields(
        &mut self,
    ) -> anyhow::Result<(
        Vec<StructField>,
        Vec<crate::frontend::ast::statement::Statement>,
    )> {
        let mut fields: Vec<StructField> = Vec::new();
        let mut members = Vec::new();
        while let Some(next_token) = self.peek() {
            if let TokenKind::Punctuation(Punctuation::ClosingCurlyBrace) = next_token.kind {
                self.next(); // consume the closing curly brace
                break;
            }
            if matches!(next_token.kind, TokenKind::Keyword(Keyword::Comp)) {
                use crate::frontend::ast::statement::{Statement, StatementKind};
                self.next();
                let name = self.expect_identifier("expected comp member name")?;
                let name_span = self.last_identifier_span;
                self.mark_name("function");
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::Colon),
                    "expected ':=' after comp member name",
                )?;
                self.expect_token(
                    TokenKind::Operator(Operator::Assign),
                    "expected ':=' after comp member name",
                )?;
                let is_comp = self
                    .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Comp)))
                    .is_some();
                self.expect_token(
                    TokenKind::Keyword(Keyword::Function),
                    "expected fn initializer for comp member",
                )?;
                let position = Span::new(next_token.line, next_token.column);
                let function = self.parse_function_tail(
                    crate::frontend::ast::function_signature::FunctionSignature {
                        identifier: name,
                        is_comp,
                        calling_convention: Default::default(),
                        annotation: crate::frontend::analysis_annotation::BindingAnnotation {
                            source_span: name_span.map(Into::into),
                            ..Default::default()
                        },
                        parameters: Vec::new(),
                        generic_parameters: Vec::new(),
                        return_type: crate::frontend::ast::type_expression::TypeExpression::at(
                            crate::frontend::ast::type_expression::TypeExpressionKind::Builtin(
                                crate::frontend::tokens::builtin::BuiltinType::Void,
                            ),
                            position,
                        ),
                    },
                    (false, None),
                    position,
                )?;
                if function.body.is_none() {
                    return Err(self.error(
                        "comp member function requires a body",
                        next_token.line,
                        next_token.column,
                    ));
                }
                self.expect_token(
                    TokenKind::Punctuation(Punctuation::Semicolon),
                    "expected ';' after comp member initializer",
                )?;
                members.push(Statement {
                    kind: StatementKind::FunctionDeclaration(function),
                    span: Span::new(next_token.line, next_token.column),
                });
                continue;
            }
            let name = self.expect_identifier("expected field name")?;
            let field_span = self.last_identifier_span;
            self.mark_name("property");
            self.expect_token(
                TokenKind::Punctuation(Punctuation::Colon),
                "expected ':' after field name",
            )?;
            let line = next_token.line;
            let column = next_token.column;
            let field_type = match self.parse_type_expression()? {
                Some(field_type) => field_type,
                None => {
                    return Err(self.error("expected a type identifier", line, column));
                }
            };
            fields.push(StructField {
                field_identifier: name,
                annotation: crate::frontend::analysis_annotation::MemberAnnotation {
                    source_span: field_span.map(Into::into),
                    ..Default::default()
                },
                expression: Expression::at(
                    ExpressionKind::TypeValue(field_type),
                    Span::new(line, column),
                ),
            });
            self.consume_if(|t| matches!(t.kind, TokenKind::Punctuation(Punctuation::Comma)));
        }
        Ok((fields, members))
    }
}
