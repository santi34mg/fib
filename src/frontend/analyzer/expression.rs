use anyhow::Context;

use crate::{
    build_plan::FileName,
    frontend::{
        analysis_annotation::{ExpressionInfo, MemberInfo},
        analyzer::Analyzer,
        ast::expression::{Expression, ExpressionKind},
        semantic_type::{SemanticTypes, TypeId},
    },
};

use super::{ResolveContext, TypeContext, not_implemented};

impl Analyzer {
    /// Resolve name uses, not binding sites. For member access resolve only the
    /// receiver here; never insert a field name or rewrite its source spelling.
    pub fn resolve_name_expression(
        expression: &mut Expression,
        context: &mut ResolveContext<'_>,
    ) -> anyhow::Result<()> {
        match expression.kind {
            ExpressionKind::BinaryExpresison(ref mut binary_expression) => {
                Self::resolve_name_expression(&mut binary_expression.left, context)?;
                Self::resolve_name_expression(&mut binary_expression.right, context)?;
            }
            ExpressionKind::UnaryExpression(ref mut unary_expression) => {
                Self::resolve_name_expression(&mut unary_expression.expression, context)?;
            }
            // TODO: re consider this but i think we have to do nothing
            ExpressionKind::Literal(_) => {}
            ExpressionKind::Call {
                ref mut callee,
                ref mut args,
            } => {
                Self::resolve_name_expression(callee, context)?;
                for arg in args {
                    Self::resolve_name_expression(arg, context)?;
                }
            }
            ExpressionKind::QualifiedIdentifier(ref mut qualified_identifier) => {
                // convention is builder sends file names as root::inter::last
                let mut qi = qualified_identifier.segments.clone();
                let last_segment = qi.pop().unwrap();
                let ns_path = qi.join("::");
                let file_id = context
                    .symbols
                    .files_by_name
                    .get(&FileName(ns_path))
                    .context("couldn't find referenced root file for namespace")?;
                let namespace = context
                    .symbols
                    .namespaces
                    .get(file_id)
                    // this one should never happen because we already verified in the previous pass
                    // (i think so)
                    .context("couldn't find referenced namespace")?;
                namespace
                    .declarations
                    .iter()
                    .find(|decl| decl.name == last_segment);
            }
            ExpressionKind::CallToBuiltin {
                ref mut builtin,
                ref mut args,
            } => todo!(),
            ExpressionKind::ExprAcess {
                ref mut expr,
                ref mut accessed_identifier,
                ref mut member,
            } => todo!(),
            ExpressionKind::StructConstruct {
                ref mut struct_identifier,
                ref mut fields,
            } => todo!(),
            ExpressionKind::Cast {
                ref mut expr,
                ref mut target_type,
            } => todo!(),
            ExpressionKind::IndexAccess {
                ref mut object,
                ref mut index,
            } => todo!(),
            ExpressionKind::SliceLiteral {
                ref mut object,
                ref mut start,
                ref mut end,
                ref mut inclusive,
            } => todo!(),
            ExpressionKind::ArrayLiteral { ref mut elements } => todo!(),
            ExpressionKind::QualifiedAccess { ref mut segments } => todo!(),
            ExpressionKind::TypeValue(ref mut type_expression) => todo!(),
            ExpressionKind::VoidExpression => todo!(),
        }
        Ok(())
    }

    /// Infer/check a value and annotate its type and place category in situ.
    pub fn resolve_type_expression(
        expression: &mut Expression,
        _context: &mut TypeContext<'_>,
        _expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        not_implemented("expression type resolution")
    }
}

/// Used for selection and initializer-field validation after the owner type is
/// known. Deliberately has no mutable lexical table or source spelling access.
pub fn resolve_member(
    _owner: TypeId,
    _spelling: &str,
    _types: &SemanticTypes,
) -> anyhow::Result<MemberInfo> {
    not_implemented("type member resolution")
}
