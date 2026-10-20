use super::{ResolveContext, TypeContext, at_span};
use crate::frontend::{
    analysis_annotation::{ExpressionInfo, MemberInfo},
    analyzer::Analyzer,
    ast::{
        expression::{Expression, ExpressionKind},
        qualified_name::QualifiedIdentifier,
    },
    namespace::{ResolvedName, name_error},
    semantic_type::{SemanticTypes, TypeId},
};

impl Analyzer {
    pub(super) fn resolve_name_path(
        name: &mut QualifiedIdentifier,
        context: &ResolveContext<'_>,
    ) -> anyhow::Result<ResolvedName> {
        let Some(first) = name.segments.first() else {
            return Err(name_error("empty qualified name"));
        };
        let resolved = if name.segments.len() == 1 {
            match context.lexical.resolve_lexical(first) {
                Some(local) => local,
                None => ResolvedName::Entity(
                    context
                        .symbols
                        .resolve_path(context.file_id, &name.segments)?,
                ),
            }
        } else {
            // Canonical registered file paths bypass ordinary lexical bindings.
            if context.symbols.file_prefix(&name.segments).is_some() {
                ResolvedName::Entity(
                    context
                        .symbols
                        .resolve_path(context.file_id, &name.segments)?,
                )
            } else if let Some(local) = context.lexical.resolve_lexical(first) {
                let ResolvedName::Entity(entity) = local else {
                    return Err(name_error(format!("'{first}' is not a file namespace")));
                };
                ResolvedName::Entity(context.symbols.follow_path(
                    context.file_id,
                    entity,
                    &name.segments[1..],
                    &mut |e| context.symbols.canonical(e),
                )?)
            } else {
                ResolvedName::Entity(
                    context
                        .symbols
                        .resolve_path(context.file_id, &name.segments)?,
                )
            }
        };
        name.annotation.resolved = Some(resolved);
        Ok(resolved)
    }

    /// Resolve names while leaving type, expression, and member slots pending.
    pub fn resolve_name_expression(
        expression: &mut Expression,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        Self::resolve_name_expression_inner(expression, context)
            .map_err(|e| at_span(e, expression.span))
    }

    fn resolve_name_expression_inner(
        expression: &mut Expression,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        match &mut expression.kind {
            ExpressionKind::BinaryExpresison(binary) => {
                Self::resolve_name_expression(&mut binary.left, context)?;
                Self::resolve_name_expression(&mut binary.right, context)?;
            }
            ExpressionKind::UnaryExpression(unary) => {
                Self::resolve_name_expression(&mut unary.expression, context)?
            }
            ExpressionKind::QualifiedIdentifier(name) => {
                Self::resolve_name_path(name, context)?;
            }
            ExpressionKind::Call { callee, args } => {
                Self::resolve_name_expression(callee, context)?;
                for arg in args {
                    Self::resolve_name_expression(arg, context)?;
                }
            }
            ExpressionKind::ExprAcess { expr, .. } => Self::resolve_name_expression(expr, context)?,
            ExpressionKind::StructConstruct {
                struct_identifier,
                fields,
            } => {
                Self::resolve_name_path(struct_identifier, context)?;
                for field in fields {
                    Self::resolve_name_expression(&mut field.expression, context)?;
                }
            }
            ExpressionKind::Cast { expr, target_type } => {
                Self::resolve_name_expression(expr, context)?;
                target_type.resolve_name(context)?;
            }
            ExpressionKind::IndexAccess { object, index } => {
                Self::resolve_name_expression(object, context)?;
                Self::resolve_name_expression(index, context)?;
            }
            ExpressionKind::SliceLiteral {
                object, start, end, ..
            } => {
                Self::resolve_name_expression(object, context)?;
                if let Some(start) = start {
                    Self::resolve_name_expression(start, context)?;
                }
                if let Some(end) = end {
                    Self::resolve_name_expression(end, context)?;
                }
            }
            ExpressionKind::ArrayLiteral { elements } => {
                for element in elements {
                    Self::resolve_name_expression(element, context)?;
                }
            }
            ExpressionKind::TypeValue(ty) => ty.resolve_name(context)?,
            ExpressionKind::QualifiedAccess { .. } => {
                return Err(name_error(
                    "legacy qualified-access node has no name annotation",
                ));
            }
            ExpressionKind::Literal(_) | ExpressionKind::VoidExpression => {}
        }
        Ok(())
    }

    /// Infer/check a value and annotate its type and place category in situ.
    pub fn resolve_type_expression(
        expression: &mut Expression,
        context: &mut TypeContext<'_>,
        expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        context.expression(expression, expected)
    }
}

/// Used for selection and initializer-field validation after the owner type is
/// known. Deliberately has no mutable lexical table or source spelling access.
pub fn resolve_member(
    owner: TypeId,
    spelling: &str,
    types: &SemanticTypes,
) -> anyhow::Result<MemberInfo> {
    super::value_typing::member(owner, spelling, types)
}
