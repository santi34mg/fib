use crate::frontend::{
    ast::{
        function_body::FunctionBody, function_declaration::FunctionDeclaration,
        function_parameter::FunctionParameter, function_signature::FunctionSignature,
    },
    parser::ParseResult,
    tokens::{Keyword, Operator, Punctuation, Token, TokenKind},
};

use super::Parser;

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    pub fn parse_function_declaration(&mut self) -> ParseResult<FunctionDeclaration> {
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
        let name = self.expect_identifier("parse_function_declaration: expected function name")?;

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
                    let argument_name = self
                        .expect_identifier("parse_function_declaration: expected parameter name")?;

                    self.expect_token(
                        TokenKind::Punctuation(Punctuation::Colon),
                        "expected ':' after parameter name",
                    )?;

                    let line = token.line;
                    let column = token.column;

                    let argument_type = match self.parse_type_expression()? {
                        Some(type_id) => type_id,
                        None => {
                            return Err(self.error("expected parameter type", line, column));
                        }
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
        let return_type = self.parse_type_expression()?;

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
}
