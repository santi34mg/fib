use crate::frontend::{
    ast::expression::{Expression, ExpressionKind},
    symbol_table::SymbolTable,
};

impl Expression {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        match self.kind {
            ExpressionKind::BinaryExpresison(ref mut binary_expression) => {
                binary_expression.left.resolve_name(symbol_table)?;
                binary_expression.right.resolve_name(symbol_table)?;
            }
            ExpressionKind::UnaryExpression(ref mut unary_expression) => {
                unary_expression.expression.resolve_name(symbol_table)?;
            }
            ExpressionKind::Literal(_) => {}
            ExpressionKind::UnqualifiedName(ref mut name) => {
                symbol_table.insert_name(name);
            }
            ExpressionKind::Call {
                ref mut callee,
                ref mut args,
            } => {
                callee.resolve_name(symbol_table)?;
                for arg in args {
                    arg.resolve_name(symbol_table)?;
                }
            }
            ExpressionKind::CallToBuiltin {
                builtin: _,
                ref mut args,
            } => {
                for arg in args {
                    arg.resolve_name(symbol_table)?;
                }
            }
            ExpressionKind::ExprAcess {
                ref mut expr,
                ref mut accesed_name,
            } => {
                expr.resolve_name(symbol_table)?;
                symbol_table.insert_name(accesed_name);
            }
            ExpressionKind::StructConstruct {
                ref mut struct_name,
                ref mut fields,
            } => {
                symbol_table.insert_name(struct_name);
                for field in fields {
                    field.expression.resolve_name(symbol_table)?;
                    // we update the field name string to have the type name
                    field.field_name.name =
                        format!("{}.{}", struct_name.name, field.field_name.name);
                    symbol_table.insert_name(&mut field.field_name);
                }
            }
            ExpressionKind::Cast {
                ref mut expr,
                ref mut target_type,
            } => {
                expr.resolve_name(symbol_table)?;
                target_type.resolve_name(symbol_table)?;
            }
            ExpressionKind::IndexAccess {
                ref mut object,
                ref mut index,
            } => {
                object.resolve_name(symbol_table)?;
                index.resolve_name(symbol_table)?;
            }
            ExpressionKind::SliceLiteral {
                ref mut object,
                ref mut start,
                ref mut end,
                inclusive: _,
            } => {
                object.resolve_name(symbol_table)?;
                if let Some(s) = start {
                    s.resolve_name(symbol_table)?;
                }
                if let Some(e) = end {
                    e.resolve_name(symbol_table)?;
                }
            }
            ExpressionKind::ArrayLiteral { ref mut elements } => {
                for e in elements {
                    e.resolve_name(symbol_table)?;
                }
            }
            ExpressionKind::QualifiedAccess { ref mut segments } => {
                for seg in segments {
                    symbol_table.insert_name(seg);
                }
            }
            ExpressionKind::TypeValue(ref mut type_expression) => {
                type_expression.resolve_name(symbol_table)?;
            }
            ExpressionKind::VoidExpression => {}
            ExpressionKind::QualifiedName(qualified_name) => todo!(),
        };
        Ok(())
    }
}
