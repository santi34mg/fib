use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{
        Keyword, Literal, TokenKind,
        builtin::{Builtin, BuiltinFunction, BuiltinType},
    },
};

impl<'input> Tokenizer<'input> {
    pub fn lex_identifier_or_keyword(&mut self) -> TokenKind {
        let start = self.position;
        self.skip_while(|c| c.is_alphanumeric() || c == '_');
        let identifier = &self.input[start..self.position];
        // for now, "__<identifier>" is reserved for internal identifiers
        match identifier {
            "const" => TokenKind::Keyword(Keyword::Const),
            "fn" => TokenKind::Keyword(Keyword::Function),
            "switch" => TokenKind::Keyword(Keyword::Switch),
            "when" => TokenKind::Keyword(Keyword::When),
            "struct" => TokenKind::Keyword(Keyword::Struct),
            "enum" => TokenKind::Keyword(Keyword::Enum),
            "union" => TokenKind::Keyword(Keyword::Union),
            "if" => TokenKind::Keyword(Keyword::If),
            "else" => TokenKind::Keyword(Keyword::Else),
            "while" => TokenKind::Keyword(Keyword::While),
            "break" => TokenKind::Keyword(Keyword::Break),
            "continue" => TokenKind::Keyword(Keyword::Continue),
            "return" => TokenKind::Keyword(Keyword::Return),
            "as" => TokenKind::Keyword(Keyword::As),
            "extern" => TokenKind::Keyword(Keyword::Extern),
            "defer" => TokenKind::Keyword(Keyword::Defer),

            "true" => TokenKind::Literal(Literal::Boolean(true)),
            "false" => TokenKind::Literal(Literal::Boolean(false)),
            "null" => TokenKind::Literal(Literal::Null),

            "type" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::MetaType)),

            "concat" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::Concat)),
            "str_len" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrLen)),
            "str_eq" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrEq)),

            _ => {
                if let Some(ty) = BuiltinType::from_name(identifier) {
                    return TokenKind::Builtin(Builtin::BuiltinType(ty));
                }
                self.last_identifier_id += 1;
                TokenKind::Identifier(identifier.to_string())
            }
        }
    }
}
