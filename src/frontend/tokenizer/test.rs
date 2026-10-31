#[cfg(test)]
mod tests {
    use crate::frontend::tokens::builtin::BuiltinType;
    use crate::frontend::{
        tokenizer::Tokenizer,
        tokens::{Keyword, Literal, Operator, Punctuation, TokenKind},
    };

    #[test]
    fn unified_declaration_keywords() {
        let tokens: Vec<_> = Tokenizer::new("pub comp const public compute pub_value comp_value")
            .map(|token| token.kind)
            .collect();
        assert_eq!(tokens[0], TokenKind::Keyword(Keyword::Pub));
        assert_eq!(tokens[1], TokenKind::Keyword(Keyword::Comp));
        assert_eq!(tokens[2], TokenKind::Keyword(Keyword::Const));
        assert!(
            tokens[3..]
                .iter()
                .all(|kind| matches!(kind, TokenKind::Identifier(_)))
        );
        let tokens: Vec<_> = Tokenizer::new("pub\ncomp").collect();
        assert_eq!((tokens[0].line, tokens[0].column), (1, 1));
        assert_eq!((tokens[1].line, tokens[1].column), (2, 1));
    }

    #[test]
    fn metatype_name_lookup_matches_tokenization() {
        let metatype = BuiltinType::MetaType;
        assert_eq!(BuiltinType::from_name(metatype.name()), Some(metatype));
        let tokens: Vec<_> = Tokenizer::new("type").map(|token| token.kind).collect();
        assert_eq!(tokens, vec![TokenKind::Identifier("type".into())]);
    }

    #[test]
    fn test_comment() {
        let test_string = String::from("// This is a comment\nconst x := 5;");
        let expected = [
            TokenKind::Comment,
            TokenKind::Keyword(Keyword::Const),
            TokenKind::Identifier("x".to_string()),
            TokenKind::Punctuation(Punctuation::Colon),
            TokenKind::Operator(Operator::Assign),
            TokenKind::Literal(Literal::Integer(5u64.into())),
            TokenKind::Punctuation(Punctuation::Semicolon),
        ];
        let tokenizer = Tokenizer::new(&test_string);
        let tokens: Vec<_> = tokenizer.collect();
        assert_ne!(tokens.len(), 0);
        tokens
            .iter()
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, e));
    }

    #[test]
    fn test_literal_integer() {
        let test_string = "1234 01234 0x12AB 0b1100";
        let expected = [1234, 1234, 0x12AB, 0b1100];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.zip(expected).for_each(|(t, e)| {
            assert_eq!(
                t.kind,
                TokenKind::Literal(Literal::Integer((e as u64).into()))
            )
        })
    }

    #[test]
    fn test_literal_integer_fail() {
        // Invalid binary literal should produce an Error token rather than panicking
        let test_string = "0b4";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_literal_float() {
        let test_string = "3.14 0.14 12.4 1.0";
        #[allow(clippy::approx_constant)]
        let expected = ["3.14", "0.14", "12.4", "1.0"];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.zip(expected).for_each(|(t, e)| {
            assert_eq!(t.kind, TokenKind::Literal(Literal::Float(e.to_string())))
        })
    }

    #[test]
    fn test_literal_bool() {
        let test_string = "true false";
        let expected = [true, false];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Boolean(e))))
    }

    #[test]
    fn test_literal_character() {
        let test_string = "'c' 'e' 'r' 'Ñ'";
        let expected = ['c', 'e', 'r', 'Ñ'];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Character(e))))
    }

    #[test]
    fn test_literal_character_escape() {
        let test_string = "'\\n' '\\t' '\\'' '\\\"' '\\x12' '\\u{03B1}' '\\0'";
        let expected = ['\n', '\t', '\'', '\"', '\x12', '\u{03B1}', '\0'];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Character(e))))
    }

    #[test]
    fn test_literal_character_unescaped_quote_fail() {
        // An unescaped quote must be written as an escape (`'\''`); the bare
        // forms `'''`, `''` and `'''''` must all produce Error tokens rather
        // than being accepted as character literals.
        for test_string in ["'''", "''", "'''''"] {
            let tokens: Vec<_> = Tokenizer::new(test_string).collect();
            assert!(
                tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))),
                "expected an error token for {test_string:?}"
            );
        }
    }

    #[test]
    fn test_literal_character_escaped_quote_valid() {
        // The escaped quote form `'\''` must remain a valid character literal.
        let test_string = r#"'\''"#;
        let expected = ['\''];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer
            .zip(expected)
            .for_each(|(t, e)| assert_eq!(t.kind, TokenKind::Literal(Literal::Character(e))))
    }

    #[test]
    fn test_literal_string() {
        let test_string = "\"Hello, World!\" \"Fiber is\n great!\"";
        let expected = ["Hello, World!", "Fiber is\n great!"];
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.zip(expected).for_each(|(t, e)| {
            assert_eq!(t.kind, TokenKind::Literal(Literal::String(e.to_string())))
        });
    }

    #[test]
    fn test_if_keyword() {
        let test_string = "if";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::If)))
    }

    #[test]
    fn test_else_keyword() {
        let test_string = "else";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Else)))
    }

    #[test]
    fn test_for_keyword() {
        let test_string = "while";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::While)))
    }

    #[test]
    fn test_return_keyword() {
        let test_string = "return";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Return)))
    }

    #[test]
    fn test_function_keyword() {
        let test_string = "fn";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Function)))
    }

    #[test]
    fn test_match_keyword() {
        let test_string = "switch";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Switch)))
    }

    #[test]
    fn test_when_keyword() {
        let test_string = "when";
        let tokenizer = Tokenizer::new(test_string);
        tokenizer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::When)))
    }

    #[test]
    fn test_all_keywords() {
        let cases = [
            ("const", TokenKind::Keyword(Keyword::Const)),
            ("if", TokenKind::Keyword(Keyword::If)),
            ("else", TokenKind::Keyword(Keyword::Else)),
            ("while", TokenKind::Keyword(Keyword::While)),
            ("break", TokenKind::Keyword(Keyword::Break)),
            ("continue", TokenKind::Keyword(Keyword::Continue)),
            ("return", TokenKind::Keyword(Keyword::Return)),
            ("struct", TokenKind::Keyword(Keyword::Struct)),
            ("enum", TokenKind::Keyword(Keyword::Enum)),
            ("union", TokenKind::Keyword(Keyword::Union)),
            ("fn", TokenKind::Keyword(Keyword::Function)),
            ("switch", TokenKind::Keyword(Keyword::Switch)),
            ("when", TokenKind::Keyword(Keyword::When)),
            ("extern", TokenKind::Keyword(Keyword::Extern)),
            ("defer", TokenKind::Keyword(Keyword::Defer)),
            ("as", TokenKind::Keyword(Keyword::As)),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Tokenizer::new(src).collect();
            assert_eq!(tokens.len(), 1, "expected one token for '{}'", src);
            assert_eq!(tokens[0].kind, expected);
        }
    }

    #[test]
    fn test_operators() {
        use crate::frontend::tokens::Operator;
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
            ("%=", Operator::PercentAssign),
            ("->", Operator::ThinRightArrow),
            ("..", Operator::DoubleDot),
            ("...", Operator::Ellipsis),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Tokenizer::new(src).collect();
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
            (".", Punctuation::Dot),
            ("::", Punctuation::DoubleColon),
            ("@", Punctuation::At),
        ];
        for (src, expected) in cases {
            let tokens: Vec<_> = Tokenizer::new(src).collect();
            assert_eq!(tokens.len(), 1, "expected one token for '{}'", src);
            assert_eq!(tokens[0].kind, TokenKind::Punctuation(expected));
        }
    }

    #[test]
    fn test_identifier() {
        let test_string = "hello_world foo bar123 _private";
        let expected = ["hello_world", "foo", "bar123", "_private"];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens.len(), expected.len());
        for (tok, name) in tokens.iter().zip(expected.iter()) {
            assert_eq!(tok.kind, TokenKind::Identifier(name.to_string()));
        }
    }

    #[test]
    fn identifiers_are_strings_independent_of_occurrence() {
        let kinds: Vec<_> = Tokenizer::new("value value pkg::value _private café")
            .map(|token| token.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier("value".to_owned()),
                TokenKind::Identifier("value".to_owned()),
                TokenKind::Identifier("pkg".to_owned()),
                TokenKind::Punctuation(Punctuation::DoubleColon),
                TokenKind::Identifier("value".to_owned()),
                TokenKind::Identifier("_private".to_owned()),
                TokenKind::Identifier("café".to_owned()),
            ]
        );
    }

    #[test]
    fn type_is_a_metatype_builtin_not_an_identifier() {
        let kinds: Vec<_> = Tokenizer::new("type type_name Type")
            .map(|token| token.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Identifier(BuiltinType::MetaType.name().into()),
                TokenKind::Identifier("type_name".to_owned()),
                TokenKind::Identifier("Type".to_owned()),
            ]
        );
    }

    #[test]
    fn test_identifier_not_keyword() {
        // Keyword prefixes and former keywords are identifiers.
        let test_string = "forloop constant variable var import";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
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
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(
                tok.kind,
                TokenKind::Literal(Literal::Integer((*val).into()))
            );
        }
    }

    #[test]
    fn test_integer_octal() {
        let test_string = "0o77 0o17";
        let expected: Vec<u64> = vec![0o77, 0o17];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(
                tok.kind,
                TokenKind::Literal(Literal::Integer((*val).into()))
            );
        }
    }

    #[test]
    fn test_integer_binary() {
        let test_string = "0b1010 0b1111";
        let expected: Vec<u64> = vec![0b1010, 0b1111];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        for (tok, val) in tokens.iter().zip(expected.iter()) {
            assert_eq!(
                tok.kind,
                TokenKind::Literal(Literal::Integer((*val).into()))
            );
        }
    }

    #[test]
    fn test_invalid_octal_literal() {
        let test_string = "0o9";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_invalid_hex_literal() {
        let test_string = "0xGG";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }

    #[test]
    fn test_string_with_escapes() {
        let test_string = r#""hello\nworld" "tab\there""#;
        let expected = ["hello\nworld", "tab\there"];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
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
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Character('\\')));
    }

    #[test]
    fn test_char_escape_null() {
        let test_string = r"'\0'";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Character('\0')));
    }

    #[test]
    fn test_null_literal() {
        let test_string = "null";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Literal(Literal::Null));
    }

    #[test]
    fn test_token_position_tracking() {
        let test_string = "type x";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[0].column, 1);
        assert_eq!(tokens[1].line, 1);
        assert_eq!(tokens[1].column, 6);
    }

    #[test]
    fn test_token_position_multiline() {
        let test_string = "type\nx";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens[0].line, 1);
        assert_eq!(tokens[1].line, 2);
        assert_eq!(tokens[1].column, 1);
    }

    #[test]
    fn test_multiline_comment_skipped() {
        // Comments appear as Comment tokens, not discarded
        let test_string = "// comment\nconst";
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert!(
            tokens
                .iter()
                .any(|t| t.kind == TokenKind::Keyword(Keyword::Const))
        );
    }

    #[test]
    fn test_empty_input() {
        let tokens: Vec<_> = Tokenizer::new("").collect();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_whitespace_only() {
        let tokens: Vec<_> = Tokenizer::new("   \t\n  ").collect();
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_builtins() {
        use crate::frontend::tokens::builtin::BuiltinType;

        // Prelude type names and user-defined names all lex as identifiers.
        let toks: Vec<_> = Tokenizer::new("int8 string bool type").collect();
        assert!(
            toks.iter()
                .all(|t| matches!(t.kind, TokenKind::Identifier(_))),
            "type names should lex as identifiers: {:?}",
            toks
        );

        // `string` is no longer a prelude type, but remains a valid identifier.
        assert_eq!(BuiltinType::from_name("string"), None);
        let toks: Vec<_> = Tokenizer::new("int8 string").collect();
        assert_eq!(
            toks[0].kind,
            TokenKind::Identifier(BuiltinType::Int8.name().into())
        );
        assert_eq!(toks[1].kind, TokenKind::Identifier("string".into()));

        for (spelling, expected) in [
            ("uint8", BuiltinType::UInt8),
            ("uint16", BuiltinType::UInt16),
            ("uint32", BuiltinType::UInt32),
            ("uint64", BuiltinType::UInt64),
            ("uint128", BuiltinType::UInt128),
            ("int8", BuiltinType::Int8),
            ("int16", BuiltinType::Int16),
            ("int32", BuiltinType::Int32),
            ("int64", BuiltinType::Int64),
            ("int128", BuiltinType::Int128),
            ("float16", BuiltinType::Float16),
            ("float32", BuiltinType::Float32),
            ("float64", BuiltinType::Float64),
            ("float128", BuiltinType::Float128),
            ("void", BuiltinType::Void),
            ("bool", BuiltinType::Boolean),
            ("type", BuiltinType::MetaType),
            ("never", BuiltinType::Never),
            ("usize", BuiltinType::Usize),
        ] {
            assert_eq!(
                BuiltinType::from_name(expected.name()),
                Some(expected.clone())
            );
            assert_eq!(expected.to_string(), spelling);
            let tokens: Vec<_> = Tokenizer::new(spelling).collect();
            assert_eq!(tokens.len(), 1, "{spelling}");
            assert_eq!(
                tokens[0].kind,
                TokenKind::Identifier(spelling.into()),
                "{spelling}"
            );
        }
        for (alias, ty) in [("int", BuiltinType::Int32), ("uint", BuiltinType::UInt32)] {
            let tokens: Vec<_> = Tokenizer::new(alias).collect();
            assert_eq!(BuiltinType::from_name(alias), Some(ty));
            assert_eq!(tokens[0].kind, TokenKind::Identifier(alias.into()));
        }
        for old_spelling in [
            "int2", "int4", "uint2", "uint4", "float2", "float4", "float8",
        ] {
            let tokens: Vec<_> = Tokenizer::new(old_spelling).collect();
            assert!(
                matches!(tokens[0].kind, TokenKind::Identifier(_)),
                "{old_spelling}"
            );
        }

        // An unknown `@name` is two tokens
        let toks: Vec<_> = Tokenizer::new("@nope").collect();
        assert!(matches!(
            toks[0].kind,
            TokenKind::Punctuation(Punctuation::At)
        ));
        assert!(matches!(toks[1].kind, TokenKind::Identifier(_)));

        // A bare `@` not followed by an identifier stays punctuation.
        let toks: Vec<_> = Tokenizer::new("@ x").collect();
        assert_eq!(toks[0].kind, TokenKind::Punctuation(Punctuation::At));
    }

    #[test]
    fn dot_eq_is_one_token() {
        let tokens: Vec<_> = Tokenizer::new("a.=b").collect();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[1].kind, TokenKind::Operator(Operator::DotEq));
    }

    #[test]
    fn test_complex_expression_tokens() {
        let test_string = "x += 42 * 3;";
        let expected = [
            TokenKind::Identifier("x".to_string()),
            TokenKind::Operator(Operator::PlusAssign),
            TokenKind::Literal(Literal::Integer(42u64.into())),
            TokenKind::Operator(Operator::Star),
            TokenKind::Literal(Literal::Integer(3u64.into())),
            TokenKind::Punctuation(Punctuation::Semicolon),
        ];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        assert_eq!(tokens.len(), expected.len());
        for (tok, exp) in tokens.iter().zip(expected.iter()) {
            assert_eq!(&tok.kind, exp);
        }
    }

    // ── 04 maintainability: tokenizer dispatcher delegates per character class ──

    #[test]
    fn test_operator_slash_dot_dispatch() {
        // One token per dispatcher helper: operators, `/`, `.`, plus the
        // two-character forms. Identifiers separate the symbols.
        let test_string = "a == b != c << d >> e && f || g += h -> i / j /= k ... .. .";
        let expected = [
            TokenKind::Operator(Operator::DoubleEquals),
            TokenKind::Operator(Operator::Different),
            TokenKind::Operator(Operator::LeftShift),
            TokenKind::Operator(Operator::RightShift),
            TokenKind::Operator(Operator::LogicalAnd),
            TokenKind::Operator(Operator::LogicalOr),
            TokenKind::Operator(Operator::PlusAssign),
            TokenKind::Operator(Operator::ThinRightArrow),
            TokenKind::Operator(Operator::Slash),
            TokenKind::Operator(Operator::SlashAssign),
            TokenKind::Operator(Operator::Ellipsis),
            TokenKind::Operator(Operator::DoubleDot),
            TokenKind::Punctuation(Punctuation::Dot),
        ];
        let tokens: Vec<_> = Tokenizer::new(test_string).collect();
        let symbols: Vec<_> = tokens
            .iter()
            .filter(|t| !matches!(t.kind, TokenKind::Identifier(_)))
            .collect();
        assert_eq!(symbols.len(), expected.len());
        for (tok, exp) in symbols.iter().zip(expected.iter()) {
            assert_eq!(&tok.kind, exp);
        }
    }

    #[test]
    fn test_punctuation_at_and_string_dispatch() {
        let toks: Vec<_> = Tokenizer::new("f(a, b) { x; y } [z] : :: @").collect();
        let expected = [
            TokenKind::Identifier("f".to_string()),
            TokenKind::Punctuation(Punctuation::OpeningParenthesis),
            TokenKind::Identifier("a".to_string()),
            TokenKind::Punctuation(Punctuation::Comma),
            TokenKind::Identifier("b".to_string()),
            TokenKind::Punctuation(Punctuation::ClosingParenthesis),
            TokenKind::Punctuation(Punctuation::OpeningCurlyBrace),
            TokenKind::Identifier("x".to_string()),
            TokenKind::Punctuation(Punctuation::Semicolon),
            TokenKind::Identifier("y".to_string()),
            TokenKind::Punctuation(Punctuation::ClosingCurlyBrace),
            TokenKind::Punctuation(Punctuation::OpeningSquareBrace),
            TokenKind::Identifier("z".to_string()),
            TokenKind::Punctuation(Punctuation::ClosingSquareBrace),
            TokenKind::Punctuation(Punctuation::Colon),
            TokenKind::Punctuation(Punctuation::DoubleColon),
            TokenKind::Punctuation(Punctuation::At),
        ];
        assert_eq!(toks.len(), expected.len());
        for (tok, exp) in toks.iter().zip(expected.iter()) {
            assert_eq!(&tok.kind, exp);
        }

        // String escapes still lex through the string helper, and bad
        // escapes/builtins still surface as Error tokens.
        let toks: Vec<_> = Tokenizer::new(r#""hi\n""#).collect();
        assert_eq!(
            toks[0].kind,
            TokenKind::Literal(Literal::String("hi\n".to_string()))
        );
        let toks: Vec<_> = Tokenizer::new(r#""unterminated"#).collect();
        assert_eq!(toks.len(), 1);
        assert!(matches!(toks[0].kind, TokenKind::Error(_)));
        let toks: Vec<_> = Tokenizer::new("@nope").collect();
        assert_eq!(toks.len(), 2);
        assert!(matches!(
            toks[0].kind,
            TokenKind::Punctuation(Punctuation::At)
        ));
        assert!(matches!(toks[1].kind, TokenKind::Identifier(_)));
    }

    // ── 05 testing: char/string escape + unknown-char error branches ──

    #[test]
    fn test_unknown_character_is_direct_unknown_token() {
        // `tokenizer.rs:82` — `$` lexes to `TokenKind::Unknown`, not `Error`.
        let toks: Vec<_> = Tokenizer::new("$").collect();
        assert_eq!(toks.len(), 1);
        assert_eq!(toks[0].kind, TokenKind::Unknown('$'));
    }

    #[test]
    fn test_unterminated_char_literal_errors() {
        // `tokenizer.rs:380` — EOF right after the opening quote.
        let toks: Vec<_> = Tokenizer::new("'").collect();
        assert_eq!(toks.len(), 1);
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(msg.contains("unterminated character"), "got: {}", msg);
        } else {
            panic!("expected Error, got {:?}", toks[0].kind);
        }
    }

    #[test]
    fn test_unterminated_char_escape_errors() {
        // `tokenizer.rs:385` — backslash with nothing after it.
        let toks: Vec<_> = Tokenizer::new("'\\").collect();
        assert_eq!(toks.len(), 1);
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(
                msg.contains("unterminated character escape"),
                "got: {}",
                msg
            );
        } else {
            panic!("expected Error, got {:?}", toks[0].kind);
        }
    }

    #[test]
    fn test_unterminated_hex_escape_errors() {
        // `tokenizer.rs:399` — `\x` with fewer than two hex digits before EOF.
        for src in ["'\\x1", "'\\x"] {
            let toks: Vec<_> = Tokenizer::new(src).collect();
            assert!(
                matches!(toks[0].kind, TokenKind::Error(_)),
                "expected Error for {:?}, got {:?}",
                src,
                toks.iter().map(|t| &t.kind).collect::<Vec<_>>()
            );
            if let TokenKind::Error(msg) = &toks[0].kind {
                assert!(msg.contains("unterminated hex"), "got: {}", msg);
            }
        }
    }

    #[test]
    fn test_invalid_char_escape_errors() {
        // Bad `\q` escape — the char helper rejects it instead of emitting a char.
        let toks: Vec<_> = Tokenizer::new("'\\q'").collect();
        assert!(
            matches!(toks[0].kind, TokenKind::Error(_)),
            "expected Error, got {:?}",
            toks[0].kind
        );
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(msg.contains("Invalid escape"), "got: {}", msg);
        }
    }

    #[test]
    fn test_invalid_hex_escape_errors() {
        // Non-hex digits after `\x` — the `is_ascii_hexdigit` branch fails.
        let toks: Vec<_> = Tokenizer::new("'\\xZZ'").collect();
        assert!(
            matches!(toks[0].kind, TokenKind::Error(_)),
            "expected Error, got {:?}",
            toks[0].kind
        );
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(msg.contains("hex"), "got: {}", msg);
        }
    }

    #[test]
    fn test_unterminated_string_escape_errors() {
        // `tokenizer.rs:325` — backslash is the last char before EOF inside a string.
        let toks: Vec<_> = Tokenizer::new("\"abc\\").collect();
        assert_eq!(toks.len(), 1);
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(msg.contains("unterminated string escape"), "got: {}", msg);
        } else {
            panic!("expected Error, got {:?}", toks[0].kind);
        }
    }

    #[test]
    fn test_unterminated_string_literal_errors() {
        // `tokenizer.rs:314` — EOF before the closing quote (direct tokenizer-level pin;
        // the parser surfaces the same message, covered in parser tests).
        let toks: Vec<_> = Tokenizer::new("\"abc").collect();
        assert_eq!(toks.len(), 1);
        if let TokenKind::Error(msg) = &toks[0].kind {
            assert!(msg.contains("unterminated string"), "got: {}", msg);
        } else {
            panic!("expected Error, got {:?}", toks[0].kind);
        }
    }
}

#[test]
fn former_builtin_function_names_are_identifiers() {
    use crate::frontend::{tokenizer::Tokenizer, tokens::TokenKind};
    let tokens: Vec<_> = Tokenizer::new("concat str_len str_eq").collect();
    for (token, name) in tokens.iter().zip(["concat", "str_len", "str_eq"]) {
        assert_eq!(token.kind, TokenKind::Identifier(name.into()));
    }
}

#[test]
fn wide_numeric_literals_retain_every_digit() {
    use super::Tokenizer;
    use crate::frontend::tokens::{Literal, TokenKind};
    let integer = "340282366920938463463374607431768211456";
    let decimal = "1.0000000596046447753906250000000000000000001";
    let source = format!("{integer} {decimal} 1..2");
    let tokens = Tokenizer::new(&source).collect::<Vec<_>>();
    assert_eq!(
        tokens[0].kind,
        TokenKind::Literal(Literal::Integer(integer.parse().unwrap()))
    );
    assert_eq!(
        tokens[1].kind,
        TokenKind::Literal(Literal::Float(decimal.into()))
    );
    assert!(matches!(
        tokens[3].kind,
        TokenKind::Operator(crate::frontend::tokens::Operator::DoubleDot)
    ));
}
