use crate::frontend::{ast::type_expression::TypeExpression, semantic_type::TypeId};

use super::{ResolveContext, TypeContext};

impl TypeExpression {
    /// Resolve referenced types without registering field or variant spellings
    /// in lexical scopes; member definitions belong to the semantic owner type.
    pub fn resolve_name(&mut self, context: &mut ResolveContext<'_>) -> anyhow::Result<()> {
        self.resolve_name_with_member_owner(context, None)
    }

    pub(super) fn resolve_name_with_member_owner(
        &mut self,
        context: &mut ResolveContext<'_>,
        owner: Option<(&str, crate::frontend::identifier::DeclId)>,
    ) -> anyhow::Result<()> {
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
                T::Struct { fields, members } => {
                    for field in fields {
                        Analyzer::resolve_name_expression(&mut field.expression, context)?;
                    }
                    context.scoped(crate::frontend::scope_kind::ScopeKind::Block, |context| {
                        if let Some((name, id)) = owner {
                            context.lexical.insert_binding(name.to_owned(), crate::frontend::symbol_table::LocalBinding::CompAlias(crate::frontend::identifier::Entity::Declaration(id)))?;
                        }
                        for member in members {
                            if let crate::frontend::ast::statement::StatementKind::FunctionDeclaration(function) = &mut member.kind {
                                function.resolve_name(context)?;
                            }
                        }
                        Ok(())
                    })?;
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
