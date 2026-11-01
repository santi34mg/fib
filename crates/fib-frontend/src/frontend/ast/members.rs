//! Borrowed declaration traversal shared by analysis and lowering. Members
//! remain owned by their source type; this does not create synthetic AST copies.
use super::{
    expression::{Expression, ExpressionKind as E},
    statement::{Statement, StatementKind as S},
    type_expression::{TypeExpression, TypeExpressionKind as T},
    variable_declaration::VariableDeclaration,
};

pub fn declarations(nodes: &[Statement]) -> Vec<&Statement> {
    let mut result = Vec::new();
    statements(nodes, &mut result);
    result
}

fn variable<'a>(node: &'a VariableDeclaration, out: &mut Vec<&'a Statement>) {
    if let Some(annotation) = &node.ty {
        ty(annotation, out);
    }
    if let Some(value) = &node.expression {
        expression(value, out);
    }
}

fn statements<'a>(nodes: &'a [Statement], out: &mut Vec<&'a Statement>) {
    for node in nodes {
        match &node.kind {
            S::VariableDeclaration(v) => variable(v, out),
            S::MultiVariableDeclaration { bindings, values } => {
                for binding in bindings {
                    variable(binding, out);
                }
                for value in values {
                    expression(value, out);
                }
            }
            S::FunctionDeclaration(f) => {
                for p in &f.signature.parameters {
                    ty(&p.parameter_type, out);
                }
                ty(&f.signature.return_type, out);
                if let Some(body) = &f.body {
                    statements(&body.statements, out);
                }
            }
            S::If {
                condition,
                then_branch,
                else_branch,
            } => {
                expression(condition, out);
                statements(then_branch, out);
                if let Some(branch) = else_branch {
                    statements(branch, out);
                }
            }
            S::While { condition, body } => {
                if let Some(condition) = condition {
                    expression(condition, out);
                }
                statements(body, out);
            }
            S::Defer(body) => statements(body, out),
            S::Switch {
                subject: value,
                arms,
            } => {
                expression(value, out);
                for arm in arms {
                    statements(&arm.body, out);
                }
            }
            S::ExpressionStatement(e) => expression(e, out),
            S::Return(Some(values)) => {
                for value in values {
                    expression(value, out);
                }
            }
            // Assignment targets/values cannot introduce declarations under
            // the current grammar, but may contain inline type expressions.
            S::Assignment { left, right } => {
                expression(left, out);
                expression(right, out);
            }
            S::DerefAssign { pointer, expr } => {
                expression(pointer, out);
                expression(expr, out);
            }
            S::IndexAssign {
                object,
                index,
                expr,
            } => {
                expression(object, out);
                expression(index, out);
                expression(expr, out);
            }
            S::IndexedCompoundAssignment { target, value, .. } => {
                expression(target, out);
                expression(value, out);
            }
            S::MultiAssignment { targets, values } => {
                for e in targets.iter().chain(values) {
                    expression(e, out);
                }
            }
            S::Return(None) | S::Break | S::Continue => {}
        }
    }
}

fn expression<'a>(node: &'a Expression, out: &mut Vec<&'a Statement>) {
    match &node.kind {
        E::TypeValue(t) => ty(t, out),
        E::BinaryExpresison(b) => {
            expression(&b.left, out);
            expression(&b.right, out);
        }
        E::UnaryExpression(u) => expression(&u.expression, out),
        E::Call { callee, args } => {
            expression(callee, out);
            for arg in args {
                expression(arg, out);
            }
        }
        E::ExprAcess { expr, .. } => expression(expr, out),
        E::StructConstruct { fields, .. } => {
            for f in fields {
                expression(&f.expression, out);
            }
        }
        E::Cast { expr, target_type } => {
            expression(expr, out);
            ty(target_type, out);
        }
        E::IndexAccess { object, index } => {
            expression(object, out);
            expression(index, out);
        }
        E::SliceLiteral {
            object, start, end, ..
        } => {
            expression(object, out);
            for e in start.iter().chain(end) {
                expression(e, out);
            }
        }
        E::ArrayLiteral { elements } => {
            for e in elements {
                expression(e, out);
            }
        }
        E::Literal(_) | E::QualifiedIdentifier(_) | E::VoidExpression => {}
    }
}

fn ty<'a>(node: &'a TypeExpression, out: &mut Vec<&'a Statement>) {
    match &node.kind {
        T::Struct { fields, members } => {
            for field in fields {
                expression(&field.expression, out);
            }
            out.extend(members);
            statements(members, out);
        }
        T::Enum { variants } => {
            for v in variants {
                if let Some(e) = &v.payload {
                    expression(e, out);
                }
            }
        }
        T::Computed(e) => expression(e, out),
        T::Function {
            argument_types,
            return_type,
            ..
        } => {
            for t in argument_types {
                ty(t, out);
            }
            ty(return_type, out);
        }
        T::Tuple { elements } => {
            for t in elements {
                ty(t, out);
            }
        }
        T::Pointer { pointed_type } => ty(pointed_type, out),
        T::Readonly(t) | T::Sentinel { inner: t, .. } => ty(t, out),
        T::Array { element_type, .. } | T::Slice { element_type } => ty(element_type, out),
        T::Builtin(_) | T::QualifiedIdentifier(_) => {}
    }
}
