use crate::frontend::{ast::type_expression::TypeExpression, semantic_type::TypeId};

use super::{ResolveContext, TypeContext};

impl TypeExpression {
    /// Resolve referenced types without registering field or variant spellings
    /// in lexical scopes; member definitions belong to the semantic owner type.
    pub fn resolve_name(&mut self, context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        use crate::frontend::{analyzer::Analyzer, ast::type_expression::TypeExpressionKind as T};
        let result = (|| {
            match &mut self.kind {
                T::Computed(expression) => Analyzer::resolve_name_expression(expression, context)?,
                T::QualifiedIdentifier(name) => {
                    Analyzer::resolve_name_path(name, context)?;
                }
                T::Function {
                    argument_types,
                    return_type,
                    ..
                } => {
                    for ty in argument_types {
                        ty.resolve_name(context)?;
                    }
                    return_type.resolve_name(context)?;
                }
                T::Tuple { elements } => {
                    for ty in elements {
                        ty.resolve_name(context)?;
                    }
                }
                T::Readonly(inner) | T::Sentinel { inner, .. } => inner.resolve_name(context)?,
                T::Pointer { pointed_type } => pointed_type.resolve_name(context)?,
                T::Array { element_type, .. } | T::Slice { element_type } => {
                    element_type.resolve_name(context)?
                }
                T::Struct { fields } => {
                    for field in fields {
                        Analyzer::resolve_name_expression(&mut field.expression, context)?;
                    }
                }
                T::Enum { variants } => {
                    for variant in variants {
                        if let Some(payload) = &mut variant.payload {
                            Analyzer::resolve_name_expression(payload, context)?;
                        }
                    }
                }
                T::Builtin(_) => {}
            }
            Ok(())
        })();
        result.map_err(|e| super::at_span(e, self.span))
    }

    pub fn resolve_type(&mut self, context: &mut TypeContext<'_>) -> anyhow::Result<TypeId> {
        context.type_expression(self, false, false)
    }
}
