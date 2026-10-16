use crate::frontend::{
    ast::type_expression::{TypeExpression, TypeExpressionKind},
    symbol_table::SymbolTable,
};

impl TypeExpression {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        match self.kind {
            TypeExpressionKind::Builtin(_) => {}
            TypeExpressionKind::Function {
                ref mut argument_types,
                ref mut return_type,
            } => {
                for ty in argument_types {
                    ty.resolve_name(symbol_table)?;
                }
                return_type.resolve_name(symbol_table)?;
            }
            TypeExpressionKind::Tuple { ref mut elements } => {
                for e in elements {
                    e.resolve_name(symbol_table)?;
                }
            }
            TypeExpressionKind::Pointer {
                ref mut pointed_type,
            } => {
                pointed_type.resolve_name(symbol_table)?;
            }
            // struct { a: @int32, b: @int32 }
            TypeExpressionKind::Struct { ref mut fields } => {
                for field in fields {
                    // we don't have to do anything to the field names because here we only get the struct definition, the name is outside
                    field.expression.resolve_name(symbol_table)?;
                }
            }
            TypeExpressionKind::Enum { variants } => todo!(),
            TypeExpressionKind::Array { element_type, size } => todo!(),
            TypeExpressionKind::Slice { element_type } => todo!(),
            TypeExpressionKind::UnqualifiedName(name) => todo!(),
            TypeExpressionKind::QualifiedName(qualified_name) => todo!(),
            TypeExpressionKind::TypeKeyword => todo!(),
        }

        Ok(())
    }
}
