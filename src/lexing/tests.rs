#[cfg(test)]
mod tests {
    use crate::{
        lexing::Lexer,
        tokens::{Keyword, Literal, Operator, Punctuation, TokenKind, identifier::Identifier},
    };

    #[test]
    fn test_comment() {
        let test_string = "// This is a comment\nvar x = 5;";
        let expected = [
            TokenKind::Comment,
            TokenKind::Keyword(Keyword::Var),
            TokenKind::Identifier(Identifier {
                identifier: "x".to_string(),
            }),
            TokenKind::Operator(Operator::Assign),
            TokenKind::Literal(Literal::Integer(5)),
            TokenKind::Punctuation(Punctuation::Semicolon),
        ];
        let lexer = Lexer::new(test_string);
        let tokens: Vec<_> = lexer.collect();
        assert_ne!(tokens.len(), 0);
        tokens
            .iter()
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, e));
    }

    #[test]
    fn test_literal_integer() {
        let test_string = "1234 01234 0x12AB 0b1100";
        let expected = [1234, 01234, 0x12AB, 0b1100];
        let lexer = Lexer::new(test_string);
        lexer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Integer(e))))
    }

    #[test]
    fn test_literal_integer_fail() {
        // Invalid binary literal should produce an Error token rather than panicking
        let test_string = "0b4";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_literal_float() {
        let test_string = "3.14 0.14 12.4 1.0";
        let expected = [3.14, 0.14, 12.4, 1.0];
        let lexer = Lexer::new(test_string);
        lexer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Float(e))))
    }

    #[test]
    fn test_literal_bool() {
        let test_string = "true false";
        let expected = [true, false];
        let lexer = Lexer::new(test_string);
        lexer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Boolean(e))))
    }

    #[test]
    fn test_literal_character() {
        let test_string = "'c' 'e' 'r' 'Ñ'";
        let expected = ['c', 'e', 'r', 'Ñ'];
        let lexer = Lexer::new(test_string);
        lexer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Character(e))))
    }

    #[test]
    fn test_literal_character_escape() {
        let test_string = "'\\n' '\\t' '\\'' '\\\"' '\\x12' '\\u{03B1}' '\\0'";
        let expected = ['\n', '\t', '\'', '\"', '\x12', '\u{03B1}', '\0'];
        let lexer = Lexer::new(test_string);
        lexer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Character(e))))
    }

    #[test]
    fn test_literal_string() {
        let test_string = "\"Hello, World!\" \"Fiber is\n great!\"";
        let expected = ["Hello, World!", "Fiber is\n great!"];
        let lexer = Lexer::new(test_string);
        lexer.zip(expected).for_each(|(t, e)| {
            assert_eq!(t.kind, TokenKind::Literal(Literal::String(e.to_string())))
        });
    }

    #[test]
    fn test_if_keyword() {
        let test_string = "if";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::If)))
    }

    #[test]
    fn test_else_keyword() {
        let test_string = "else";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Else)))
    }

    #[test]
    fn test_for_keyword() {
        let test_string = "for";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::For)))
    }

    #[test]
    fn test_return_keyword() {
        let test_string = "ret";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Return)))
    }

    #[test]
    fn test_function_keyword() {
        let test_string = "fn";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Function)))
    }

    #[test]
    fn test_match_keyword() {
        let test_string = "switch";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Switch)))
    }

    #[test]
    fn test_when_keyword() {
        let test_string = "when";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::When)))
    }

    #[test]
    fn test_all_keywords() {
        let cases = [
            ("var", TokenKind::Keyword(Keyword::Var)),
            ("if", TokenKind::Keyword(Keyword::If)),
            ("else", TokenKind::Keyword(Keyword::Else)),
            ("for", TokenKind::Keyword(Keyword::For)),
            ("break", TokenKind::Keyword(Keyword::Break)),
            ("continue", TokenKind::Keyword(Keyword::Continue)),
            ("ret", TokenKind::Keyword(Keyword::Return)),
            ("struct", TokenKind::Keyword(Keyword::Struct)),
            ("extern", TokenKind::Keyword(Keyword::Extern)),
            ("defer", TokenKind::Keyword(Keyword::Defer)),
            ("import", TokenKind::Keyword(Keyword::Import)),
            ("as", TokenKind::Keyword(Keyword::As)),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Lexer::new(src).collect();
            assert_eq!(tokens.len(), 1, "expected one token for '{}'", src);
            assert_eq!(tokens[0].kind, expected);
        }
    }

    #[test]
    fn test_operators() {
        use crate::tokens::Operator;
        let cases = [
            ("+", Operator::Plus),
            ("-", Operator::Minus),
            ("*", Operator::Star),
            ("/", Operator::Slash),
            ("%", Operator::Percent),
            ("==", Operator::DoubleEquals),
            ("!=", Operator::Different),
            ("<", Operator::LesserThan),
            ("<=", Operator::LesserEqual),
            (">", Operator::GreaterThan),
            (">=", Operator::GreaterEqual),
            ("&&", Operator::LogicalAnd),
            ("||", Operator::LogicalOr),
            ("!", Operator::LogicalNot),
            ("&", Operator::Ampersand),
            ("|", Operator::Pipe),
            ("^", Operator::Caret),
            ("~", Operator::Tilde),
            ("<<", Operator::LeftShift),
            (">>", Operator::RightShift),
            ("=", Operator::Assign),
            ("+=", Operator::PlusAssign),
            ("-=", Operator::MinusAssign),
            ("*=", Operator::StarAssign),
            ("/=", Operator::SlashAssign),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Lexer::new(src).collect();
            assert_eq!(tokens.len(), 1, "expected one token for '{}'", src);
            assert_eq!(tokens[0].kind, TokenKind::Operator(expected));
        }
    }

    #[test]
    fn test_punctuation() {
        let cases = [
            ("(", Punctuation::OpeningParenthesis),
            (")", Punctuation::ClosingParenthesis),
            ("{", Punctuation::OpeningCurlyBrace),
            ("}", Punctuation::ClosingCurlyBrace),
            ("[", Punctuation::OpeningSquareBrace),
            ("]", Punctuation::ClosingSquareBrace),
            (";", Punctuation::Semicolon),
            (",", Punctuation::Comma),
            (":", Punctuation::Colon),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Lexer::new(src).collect();
            assert_eq!(tokens.len(), 1, "expected one token for '{}'", src);
            assert_eq!(tokens[0].kind, TokenKind::Punctuation(expected));
        }
    }

    #[test]
    fn test_identifier() {
        let test_string = "hello_world foo bar123 _private";
        let expected = ["hello_world", "foo", "bar123", "_private"];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens.len(), expected.len());
        for (tok, name) in tokens.iter().zip(expected.iter()) {
            assert_eq!(
                tok.kind,
                TokenKind::Identifier(Identifier {
                    identifier: name.to_string()
                })
            );
        }
    }

    #[test]
    fn test_identifier_not_keyword() {
        // These start with keyword prefixes but are identifiers
        let test_string = "forloop constant variable";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        for tok in &tokens {
            assert!(
                matches!(tok.kind, TokenKind::Identifier(_)),
                "expected identifier, got {:?}",
                tok.kind
            );
        }
    }

    #[test]
    fn test_integer_hex_lowercase() {
        let test_string = "0xff 0xdeadbeef";
        let expected: Vec<u64> = vec![0xff, 0xdeadbeef];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(tok.kind, TokenKind::Literal(Literal::Integer(*val)));
        }
    }

    #[test]
    fn test_integer_octal() {
        let test_string = "0o77 0o17";
        let expected: Vec<u64> = vec![0o77, 0o17];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(tok.kind, TokenKind::Literal(Literal::Integer(*val)));
        }
    }

    #[test]
    fn test_integer_binary() {
        let test_string = "0b1010 0b1111";
        let expected: Vec<u64> = vec![0b1010, 0b1111];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(tok.kind, TokenKind::Literal(Literal::Integer(*val)));
        }
    }

    #[test]
    fn test_invalid_octal_literal() {
        let test_string = "0o9";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_invalid_hex_literal() {
        let test_string = "0xGG";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_string_with_escapes() {
        let test_string = r#""hello\nworld" "tab\there""#;
        let expected = ["hello\nworld", "tab\there"];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(
                tok.kind,
                TokenKind::Literal(Literal::String(val.to_string()))
            );
        }
    }

    #[test]
    fn test_char_escape_backslash() {
        let test_string = r"'\\'";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Character('\\')));
    }

    #[test]
    fn test_char_escape_null() {
        let test_string = r"'\0'";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Character('\0')));
    }

    #[test]
    fn test_null_literal() {
        let test_string = "null";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Null));
    }

    #[test]
    fn test_token_position_tracking() {
        let test_string = "type x";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].column, 1);
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[1].column, 6);
    }

    #[test]
    fn test_token_position_multiline() {
        let test_string = "type\nx";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[1].column, 1);
    }

    #[test]
    fn test_multiline_comment_skipped() {
        // Comments appear as Comment tokens, not discarded
        let test_string = "// comment\nvar";
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword(Keyword::Var))
        );
    }

    #[test]
    fn test_empty_input() {
        let tokens: Vec<_> = Lexer::new("").collect();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let tokens: Vec<_> = Lexer::new("   \t\n  ").collect();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_builtins_require_at_prefix() {
        use crate::tokens::builtin::{Builtin, BuiltinFunction, BuiltinType};

        // Bare type names are now plain identifiers.
        let toks: Vec<_> = Lexer::new("int string bool").collect();
        assert!(
            toks.iter()
                .all(|t| matches!(t.kind, TokenKind::Identifier(_))),
            "bare type names should lex as identifiers: {:?}",
            toks
        );

        // `@name` resolves to a builtin type.
        let toks: Vec<_> = Lexer::new("@int @string").collect();
        assert_eq!(
            toks[0].kind,
            TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int1))
        );
        assert_eq!(
            toks[1].kind,
            TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::String))
        );

        // `@name` also resolves to a builtin function.
        let toks: Vec<_> = Lexer::new("@concat @str_len @str_eq").collect();
        assert_eq!(
            toks[0].kind,
            TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::Concat))
        );
        assert_eq!(
            toks[1].kind,
            TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrLen))
        );
        assert_eq!(
            toks[2].kind,
            TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrEq))
        );

        // An unknown `@name` is an error.
        let toks: Vec<_> = Lexer::new("@nope").collect();
        assert!(matches!(toks[0].kind, TokenKind::Error(_)));

        // A bare `@` not followed by an identifier stays punctuation.
        let toks: Vec<_> = Lexer::new("@ x").collect();
        assert_eq!(toks[0].kind, TokenKind::Punctuation(Punctuation::At));
    }

    #[test]
    fn test_complex_expression_tokens() {
        let test_string = "x += 42 * 3;";
        let expected = vec![
            TokenKind::Identifier(Identifier {
                identifier: "x".to_string(),
            }),
            TokenKind::Operator(Operator::PlusAssign),
            TokenKind::Literal(Literal::Integer(42)),
            TokenKind::Operator(Operator::Star),
            TokenKind::Literal(Literal::Integer(3)),
            TokenKind::Punctuation(Punctuation::Semicolon),
        ];
        let tokens: Vec<_> = Lexer::new(test_string).collect();
        assert_eq!(tokens.len(), expected.len());
        for (tok, exp) in tokens.iter().zip(expected.iter()) {
            assert_eq!(&tok.kind, exp);
        }
    }
}
