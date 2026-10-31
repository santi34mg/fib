//! Structural walk used for declaration preparation and name-slot validation.
//! Scope and declaration-order semantics belong to the resolver, not this walk.
use crate::diagnostics::Span;
use crate::frontend::{
    analysis_annotation::BindingAnnotation,
    ast::{
        expression::{Expression, ExpressionKind as E},
        pattern::Pattern,
        qualified_name::QualifiedIdentifier,
        statement::{Statement, StatementKind as S},
        type_expression::{TypeExpression, TypeExpressionKind as T},
        variable_declaration::VariableDeclaration,
    },
};

type Result = anyhow::Result<()>;

pub(crate) trait Visitor {
    fn statement(&mut self, _node: &mut Statement) -> Result {
        Ok(())
    }
    fn expression(&mut self, _node: &mut Expression) -> Result {
        Ok(())
    }
    fn ty(&mut self, _node: &mut TypeExpression) -> Result {
        Ok(())
    }
    fn name(&mut self, _name: &mut QualifiedIdentifier) -> Result {
        Ok(())
    }
    fn binding(&mut self, _name: &str, _slot: &mut BindingAnnotation, _span: Span) -> Result {
        Ok(())
    }
    fn variable(
        &mut self,
        _var: &mut VariableDeclaration,
        _span: Span,
        _file: bool,
        _inline: bool,
    ) -> Result {
        Ok(())
    }
}

pub(crate) fn statements(v: &mut impl Visitor, nodes: &mut [Statement], file: bool) -> Result {
    for node in nodes {
        statement(v, node, file)?;
    }
    Ok(())
}

fn variable(
    v: &mut impl Visitor,
    var: &mut VariableDeclaration,
    span: Span,
    file: bool,
    inline: bool,
) -> Result {
    v.variable(var, span, file, inline)?;
    v.binding(&var.identifier, &mut var.annotation, span)?;
    if let Some(ty) = &mut var.ty {
        ty_expression(v, ty)?;
    }
    if let Some(expr) = &mut var.expression {
        expression(v, expr)?;
    }
    Ok(())
}

fn statement(v: &mut impl Visitor, node: &mut Statement, file: bool) -> Result {
    v.statement(node)?;
    match &mut node.kind {
        S::FunctionDeclaration(fun) => {
            v.binding(
                &fun.signature.identifier,
                &mut fun.signature.annotation,
                node.span,
            )?;
            for parameter in &mut fun.signature.parameters {
                v.binding(
                    &parameter.parameter_identifier,
                    &mut parameter.annotation,
                    parameter.parameter_type.span,
                )?;
                ty_expression(v, &mut parameter.parameter_type)?;
            }
            ty_expression(v, &mut fun.signature.return_type)?;
            if let Some(body) = &mut fun.body {
                statements(v, &mut body.statements, false)?;
            }
        }
        S::VariableDeclaration(var) => {
            let inline = super::inline_type(var.expression.as_ref());
            variable(v, var, node.span, file, inline)?;
        }
        S::MultiVariableDeclaration { bindings, values } => {
            for (i, binding) in bindings.iter_mut().enumerate() {
                variable(
                    v,
                    binding,
                    node.span,
                    file,
                    super::inline_type(values.get(i)),
                )?;
            }
            for value in values {
                expression(v, value)?;
            }
        }
        S::ExpressionStatement(expr) => expression(v, expr)?,
        S::Assignment { left, right } => {
            expression(v, left)?;
            expression(v, right)?;
        }
        S::DerefAssign { pointer, expr } => {
            expression(v, pointer)?;
            expression(v, expr)?;
        }
        S::IndexAssign {
            object,
            index,
            expr,
        } => {
            expression(v, object)?;
            expression(v, index)?;
            expression(v, expr)?;
        }
        S::Return(values) => {
            if let Some(values) = values {
                for value in values {
                    expression(v, value)?;
                }
            }
        }
        S::MultiAssignment { targets, values } => {
            for expr in targets.iter_mut().chain(values) {
                expression(v, expr)?;
            }
        }
        S::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression(v, condition)?;
            statements(v, then_branch, false)?;
            if let Some(branch) = else_branch {
                statements(v, branch, false)?;
            }
        }
        S::While { condition, body } => {
            if let Some(condition) = condition {
                expression(v, condition)?;
            }
            statements(v, body, false)?;
        }
        S::Defer(body) => statements(v, body, false)?,
        S::Switch { subject, arms } => {
            expression(v, subject)?;
            for arm in arms {
                if let Pattern::EnumVariant {
                    binding: Some(binding),
                    ..
                } = &mut arm.pattern
                {
                    v.binding(&binding.identifier, &mut binding.annotation, node.span)?;
                }
                statements(v, &mut arm.body, false)?;
            }
        }
        S::Break | S::Continue => {}
    }
    Ok(())
}

fn expression(v: &mut impl Visitor, expr: &mut Expression) -> Result {
    v.expression(expr)?;
    match &mut expr.kind {
        E::QualifiedIdentifier(name) => v.name(name)?,
        E::BinaryExpresison(binary) => {
            expression(v, &mut binary.left)?;
            expression(v, &mut binary.right)?;
        }
        E::UnaryExpression(unary) => expression(v, &mut unary.expression)?,
        E::Call { callee, args } => {
            expression(v, callee)?;
            for arg in args {
                expression(v, arg)?;
            }
        }
        E::ExprAcess { expr, .. } => expression(v, expr)?,
        E::StructConstruct {
            struct_identifier,
            fields,
        } => {
            v.name(struct_identifier)?;
            for field in fields {
                expression(v, &mut field.expression)?;
            }
        }
        E::Cast { expr, target_type } => {
            expression(v, expr)?;
            ty_expression(v, target_type)?;
        }
        E::IndexAccess { object, index } => {
            expression(v, object)?;
            expression(v, index)?;
        }
        E::SliceLiteral {
            object, start, end, ..
        } => {
            expression(v, object)?;
            if let Some(start) = start {
                expression(v, start)?;
            }
            if let Some(end) = end {
                expression(v, end)?;
            }
        }
        E::ArrayLiteral { elements } => {
            for element in elements {
                expression(v, element)?;
            }
        }
        E::TypeValue(ty) => ty_expression(v, ty)?,
        E::Literal(_) | E::VoidExpression => {}
    }
    Ok(())
}

fn ty_expression(v: &mut impl Visitor, ty: &mut TypeExpression) -> Result {
    v.ty(ty)?;
    match &mut ty.kind {
        T::Computed(expr) => expression(v, expr)?,
        T::QualifiedIdentifier(name) => v.name(name)?,
        T::Function {
            argument_types,
            return_type,
            ..
        } => {
            for ty in argument_types {
                ty_expression(v, ty)?;
            }
            ty_expression(v, return_type)?;
        }
        T::Tuple { elements } => {
            for ty in elements {
                ty_expression(v, ty)?;
            }
        }
        T::Readonly(inner) | T::Sentinel { inner, .. } => ty_expression(v, inner)?,
        T::Pointer { pointed_type } => ty_expression(v, pointed_type)?,
        T::Array { element_type, .. } | T::Slice { element_type } => {
            ty_expression(v, element_type)?
        }
        T::Struct { fields, members } => {
            for field in fields {
                expression(v, &mut field.expression)?;
            }
            statements(v, members, false)?;
        }
        T::Enum { variants } => {
            for variant in variants {
                if let Some(payload) = &mut variant.payload {
                    expression(v, payload)?;
                }
            }
        }
        T::Builtin(_) => {}
    }
    Ok(())
}
