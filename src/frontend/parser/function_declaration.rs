use crate::diagnostics::Span;
use crate::frontend::{
    ast::{
        function_body::FunctionBody,
        function_declaration::FunctionDeclaration,
        function_parameter::FunctionParameter,
        function_signature::FunctionSignature,
        type_expression::{TypeExpression, TypeExpressionKind},
    },
    tokens::{Keyword, Punctuation, Token, TokenKind, builtin::BuiltinType},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_function_declaration(&mut self) -> anyhow::Result<FunctionDeclaration> {
        let (is_pub, pub_scope) = self.parse_visibility()?;
        let is_comp = self
            .consume_if(|t| matches!(t.kind, TokenKind::Keyword(Keyword::Comp)))
            .is_some();
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

        let fn_kw_token = self.expect_token(
            TokenKind::Keyword(Keyword::Function),
            "parse_function_declaration: expected 'fn' keyword",
        )?;
        let mut line: usize = fn_kw_token.line;
        let mut column: usize = fn_kw_token.column;

        // Function name
        let identifier =
            self.expect_identifier("parse_function_declaration: expected function name")?;

        let name_span = self.last_identifier_span;
        self.mark_name("function");

        // Parameters
        self.expect_token(
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            "parse_function_declaration: expected '('",
        )?;
        let mut args = Vec::new();
        while let Some(token) = self.peek() {
            match &token.kind {
                TokenKind::Punctuation(Punctuation::ClosingParenthesis) => {
                    self.next();
                    break;
                }
                _ => {
                    let argument_name = self
                        .expect_identifier("parse_function_declaration: expected parameter name")?;

                    let parameter_span = self.last_identifier_span;
                    self.mark_name("parameter");
                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::Colon),
                        "expected ':' after parameter name",
                    )?;

                    line = token.line;
                    column = token.column;

                    let argument_type = match self.parse_type_expression()? {
                        Some(type_id) => type_id,
                        None => {
                            return Err(self.error("expected parameter type", line, column));
                        }
                    };

                    args.push(FunctionParameter {
                        parameter_identifier: argument_name,
                        annotation: crate::frontend::analysis_annotation::BindingAnnotation {
                            source_span: parameter_span.map(Into::into),
                            ..Default::default()
                        },
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
        let return_type = if matches!(
            self.peek(),
            Some(Token {
                kind: TokenKind::Punctuation(
                    Punctuation::OpeningCurlyBrace | Punctuation::Semicolon
                ),
                ..
            })
        ) {
            TypeExpression::at(
                TypeExpressionKind::Builtin(BuiltinType::Void),
                Span::new(line, column),
            )
        } else {
            self.parse_type_expression()?
                .ok_or_else(|| self.error("expected a type expression", line, column))?
        };

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
                identifier,
                is_comp,
                annotation: crate::frontend::analysis_annotation::BindingAnnotation {
                    source_span: name_span.map(Into::into),
                    ..Default::default()
                },
                parameters: args,
                return_type,
                generic_parameters: Vec::new(),
            },
            body,
            is_extern,
            // FIXME: parse function visibility properly
            is_pub,
            pub_scope,
        })
    }
}
