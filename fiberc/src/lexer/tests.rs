#[cfg(test)]
mod tests {
    use crate::{
        lexer::Lexer,
        token::{Keyword, Literal, Operator, Punctuation, TokenKind},
    };

    #[test]
    fn test_comment() {
        let test_string = "// This is a comment\nlet x = 5;";
        let expected = [
            TokenKind::Comment,
            TokenKind::Keyword(Keyword::Let),
            TokenKind::Identifier("x".to_string()),
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
    #[should_panic]
    fn test_literal_integer_fail() {
        let test_string = "0b4";
        let lexer = Lexer::new(test_string);
        let _tokens: Vec<_> = lexer.collect();
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
    fn test_let_keyword() {
        let test_string = "let";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Let)))
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
        let test_string = "return";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Return)))
    }

    #[test]
    fn test_function_keyword() {
        let test_string = "function";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Function)))
    }

    #[test]
    fn test_module_keyword() {
        let test_string = "module";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Module)))
    }

    #[test]
    fn test_use_keyword() {
        let test_string = "use";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Use)))
    }

    #[test]
    fn test_public_keyword() {
        let test_string = "public";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Public)))
    }

    #[test]
    fn test_private_keyword() {
        let test_string = "private";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Private)))
    }

    #[test]
    fn test_mutable_keyword() {
        let test_string = "mut";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Mutable)))
    }

    #[test]
    fn test_match_keyword() {
        let test_string = "match";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Match)))
    }

    #[test]
    fn test_when_keyword() {
        let test_string = "when";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::When)))
    }

    #[test]
    fn test_coroutine_keyword() {
        let test_string = "coroutine";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Coroutine)))
    }

    #[test]
    fn test_spawn_keyword() {
        let test_string = "spawn";
        let lexer = Lexer::new(test_string);
        lexer.for_each(|t| assert_eq!(t.kind, TokenKind::Keyword(Keyword::Spawn)))
    }
}
