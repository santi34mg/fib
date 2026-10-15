use crate::frontend::{
    tokenizer::Tokenizer,
    tokens::{
        Keyword, Literal, TokenKind,
        builtin::{Builtin, BuiltinFunction, BuiltinType},
        name::Name,
    },
};

impl<'input> Tokenizer<'input> {
    pub fn lex_identifier_or_keyword(&mut self) -> TokenKind {
        let start = self.position;
        self.skip_while(|c| c.is_alphanumeric() || c == '_');
        let name = &self.input[start..self.position];
        // for now, "__<identifier>" is reserved for internal identifiers
        match name {
            "var" | "const" => TokenKind::Keyword(Keyword::Const),
            "fn" => TokenKind::Keyword(Keyword::Function),
            "switch" => TokenKind::Keyword(Keyword::Switch),
            "when" => TokenKind::Keyword(Keyword::When),
            "type" => TokenKind::Keyword(Keyword::Type),
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
            "import" => TokenKind::Keyword(Keyword::Import),
            "true" => TokenKind::Literal(Literal::Boolean(true)),
            "false" => TokenKind::Literal(Literal::Boolean(false)),
            "null" => TokenKind::Literal(Literal::Null),
            "void" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Void)),
            "uint" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt1)),
            "uint2" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt2)),
            "uint4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt4)),
            "uint8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt8)),
            "uint16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::UInt16)),
            "int" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int1)),
            "int2" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int2)),
            "int4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int4)),
            "int8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int8)),
            "int16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Int16)),
            "float2" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float2)),
            "float4" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float4)),
            "float8" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float8)),
            "float16" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Float16)),
            "char" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Char)),
            "bool" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Boolean)),
            "string" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::String)),
            "never" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Never)),
            "usize" => TokenKind::Builtin(Builtin::BuiltinType(BuiltinType::Usize)),
            "concat" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::Concat)),
            "str_len" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrLen)),
            "str_eq" => TokenKind::Builtin(Builtin::BuiltinFunction(BuiltinFunction::StrEq)),
            // Builtin types are no longer bare keywords — they are spelled `@int`,
            // `@string`, etc. and handled in the `@` arm of lex_token.
            _ => {
                self.last_identifier_id += 1;
                TokenKind::Name(Name {
                    name: name.to_string(),
                    id: self.last_identifier_id,
                })
            }
        }
    }
}
