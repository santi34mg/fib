//! Expression inference, operand rules and contextual literals. Compatibility is
//! deliberately delegated only after both semantic types have been determined.
use super::{TypeContext, at_span, types::check_assignable};
use crate::frontend::{
    analysis_annotation::{ExpressionInfo, MemberInfo, MemberKind, ValueCategory as C},
    ast::expression::{Expression, ExpressionKind as E},
    semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
    tokens::{Literal, Operator as O, builtin::BuiltinType as B},
};
use std::collections::HashSet;

pub(super) fn integer(b: &B) -> Option<(bool, Option<u32>)> {
    Some(match b {
        B::Int8 => (true, Some(8)),
        B::Int16 => (true, Some(16)),
        B::Int32 => (true, Some(32)),
        B::Int64 => (true, Some(64)),
        B::Int128 => (true, Some(128)),
        B::UInt8 => (false, Some(8)),
        B::UInt16 => (false, Some(16)),
        B::UInt32 => (false, Some(32)),
        B::UInt64 => (false, Some(64)),
        B::UInt128 => (false, Some(128)),
        B::Usize => (false, None),
        _ => return None,
    })
}
fn float(b: &B) -> bool {
    matches!(b, B::Float16 | B::Float32 | B::Float64 | B::Float128)
}
fn numeric(t: &T) -> bool {
    matches!(t,T::Builtin(b) if integer(b).is_some() || float(b))
}
fn integral(t: &T) -> bool {
    matches!(t,T::Builtin(b) if integer(b).is_some())
}
fn fits_integer(value: i128, ty: &T) -> anyhow::Result<()> {
    let T::Builtin(b) = ty else {
        return Err(type_error("integer literal requires a numeric type"));
    };
    let Some((signed, width)) = integer(b) else {
        return Err(type_error("integer literal requires an integer type"));
    };
    let fits = match (signed, width) {
        (true, Some(128)) => true,
        (true, Some(w)) => value >= -(1i128 << (w - 1)) && value < (1i128 << (w - 1)),
        (false, Some(128)) => value >= 0,
        (false, Some(w)) => value >= 0 && (value as u128) < (1u128 << w),
        // usize is symbolic; target-width overflow belongs to target validation.
        (false, None) => value >= 0,
        _ => false,
    };
    if fits {
        Ok(())
    } else {
        Err(type_error(format!(
            "integer value {value} is out of range for {}",
            b.name()
        )))
    }
}
fn fits_float(value: f64, ty: &T) -> anyhow::Result<()> {
    let limit = match ty {
        T::Builtin(B::Float16) => 65504.0,
        T::Builtin(B::Float32) => f32::MAX as f64,
        T::Builtin(B::Float64 | B::Float128) => f64::MAX,
        _ => return Err(type_error("float literal requires a floating-point type")),
    };
    if !value.is_finite() || value.abs() > limit {
        return Err(type_error("floating-point value is out of range"));
    }
    Ok(())
}

pub(super) fn member(
    owner: TypeId,
    spelling: &str,
    types: &SemanticTypes,
) -> anyhow::Result<MemberInfo> {
    let owner = types.canonical(owner)?;
    let (kind, ty) = match types.get(owner)? {
        T::Struct(fields) => {
            let (i, f) = fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.spelling == spelling)
                .ok_or_else(|| type_error(format!("unknown member '{spelling}'")))?;
            (MemberKind::Field(i as u32), f.ty)
        }
        T::Enum(variants) => {
            let (i, v) = variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.spelling == spelling)
                .ok_or_else(|| type_error(format!("unknown member '{spelling}'")))?;
            let ty = if v.payload.is_some() {
                *types
                    .variant_constructors
                    .get(&(owner, i as u32))
                    .ok_or_else(|| type_error("incomplete variant constructor"))?
            } else {
                owner
            };
            (MemberKind::EnumVariant(i as u32), ty)
        }
        T::Array { .. } | T::Slice(_) | T::Builtin(B::String) if spelling == "len" => {
            let i = types
                .types
                .iter()
                .position(|t| *t == T::Builtin(B::Usize))
                .ok_or_else(|| type_error("missing semantic usize type"))?;
            (MemberKind::Length, TypeId(i as u32))
        }
        _ => return Err(type_error(format!("inapplicable member '{spelling}'"))),
    };
    Ok(MemberInfo { owner, kind, ty })
}

impl TypeContext<'_> {
    pub(super) fn expression(
        &mut self,
        expr: &mut Expression,
        expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        if self.state.depth >= 256 {
            return Err(at_span(
                type_error("type expression nesting limit exceeded"),
                expr.span,
            ));
        }
        self.state.depth += 1;
        let result = self.expression_inner(expr, expected);
        self.state.depth -= 1;
        let info = result.map_err(|e| self.diagnostic(e, Some(expr.span), self.file))?;
        if let Some(expected) = expected {
            check_assignable(expected, info.ty, self.types)
                .map_err(|e| self.diagnostic(e, Some(expr.span), self.file))?;
        }
        super::constant::validate(expr, info.ty, self.types)
            .map_err(|e| self.diagnostic(e, Some(expr.span), self.file))?;
        expr.annotation.resolved = Some(info);
        Ok(info)
    }
    fn expression_inner(
        &mut self,
        expr: &mut Expression,
        expected: Option<TypeId>,
    ) -> anyhow::Result<ExpressionInfo> {
        let ty = match &mut expr.kind {
            E::Literal(literal) => match literal {
                Literal::Integer(v) => {
                    let ty = expected
                        .filter(|t| self.types.get(*t).is_ok_and(numeric))
                        .unwrap_or_else(|| self.types.builtin(B::Int32));
                    if matches!(self.types.get(ty)?,T::Builtin(b) if float(b)) {
                        fits_float(*v as f64, self.types.get(ty)?)?;
                    } else {
                        fits_integer(*v as i128, self.types.get(ty)?)?;
                    }
                    ty
                }
                Literal::Float(v) => {
                    let ty = expected
                        .filter(|t| matches!(self.types.get(*t),Ok(T::Builtin(b)) if float(b)))
                        .unwrap_or_else(|| self.types.builtin(B::Float64));
                    fits_float(*v, self.types.get(ty)?)?;
                    ty
                }
                Literal::Boolean(_) => self.types.builtin(B::Boolean),
                Literal::Character(_) => self.types.builtin(B::Char),
                Literal::String(_) => self.types.builtin(B::String),
                Literal::Null => expected
                    .filter(|t| matches!(self.types.get(*t), Ok(T::Pointer(_))))
                    .ok_or_else(|| type_error("null requires a pointer context"))?,
            },
            E::QualifiedIdentifier(name) => {
                return self.name_info(
                    name.annotation
                        .resolved
                        .ok_or_else(|| type_error("pending expression name"))?,
                );
            }
            E::TypeValue(node) => {
                let ty = self.type_expression(node, true, true)?;
                return Ok(self.metatype(ty));
            }
            E::VoidExpression => self.types.builtin(B::Void),
            E::UnaryExpression(u) => {
                if u.operator == O::Minus
                    && let E::Literal(Literal::Integer(v)) = &u.expression.kind
                {
                    let ty = expected
                        .filter(|t| self.types.get(*t).is_ok_and(numeric))
                        .unwrap_or_else(|| self.types.builtin(B::Int32));
                    if matches!(self.types.get(ty)?,T::Builtin(b) if float(b)) {
                        fits_float(-(*v as f64), self.types.get(ty)?)?;
                    } else {
                        fits_integer(-(*v as i128), self.types.get(ty)?)?;
                    }
                    u.expression.annotation.resolved = Some(self.value(ty));
                    return Ok(self.value(ty));
                }
                let hint = if matches!(u.operator, O::Minus | O::Tilde) {
                    expected
                } else {
                    None
                };
                let info = self.expression(&mut u.expression, hint)?;
                match u.operator {
                    O::Minus if numeric(self.types.get(info.ty)?) => info.ty,
                    O::Tilde if integral(self.types.get(info.ty)?) => info.ty,
                    O::LogicalNot if matches!(self.types.get(info.ty)?, T::Builtin(B::Boolean)) => {
                        info.ty
                    }
                    O::AddressOf => {
                        if info.category != C::MutablePlace {
                            return Err(type_error(
                                "address requires a mutable place; immutable values cannot be addressed",
                            ));
                        }
                        self.types.intern(T::Pointer(info.ty))
                    }
                    O::Dereference => {
                        let T::Pointer(pointee) = self.types.get(info.ty)? else {
                            return Err(type_error("dereference operand must be a pointer"));
                        };
                        if self.types.is_opaque(*pointee)? {
                            return Err(type_error("cannot dereference opaque pointer"));
                        }
                        return Ok(ExpressionInfo {
                            ty: *pointee,
                            category: C::MutablePlace,
                            type_value: None,
                        });
                    }
                    _ => return Err(type_error("invalid unary operand type")),
                }
            }
            E::BinaryExpresison(b) => {
                let comparison = matches!(
                    b.operator,
                    O::DoubleEquals
                        | O::Different
                        | O::GreaterThan
                        | O::GreaterEqual
                        | O::LesserThan
                        | O::LesserEqual
                );
                let shift = matches!(b.operator, O::LeftShift | O::RightShift);
                let hint = if comparison { None } else { expected };
                // Obtain context from a typed right operand when the left is a literal.
                let contextual_left = matches!(b.left.kind, E::Literal(_))
                    || matches!(&b.left.kind,E::UnaryExpression(u) if matches!(u.expression.kind,E::Literal(_)));
                let (left, right) = if contextual_left
                    && !matches!(b.right.kind, E::Literal(_))
                    && hint.is_none()
                    && !shift
                {
                    let right = self.expression(&mut b.right, None)?;
                    let left = self.expression(&mut b.left, Some(right.ty))?;
                    (left, right)
                } else {
                    let left = self.expression(&mut b.left, hint)?;
                    if matches!(
                        b.operator,
                        O::Plus | O::Minus | O::Star | O::Slash | O::Percent
                    ) && !numeric(self.types.get(left.ty)?)
                    {
                        return Err(type_error("invalid binary operand type"));
                    }
                    let right =
                        self.expression(&mut b.right, if shift { None } else { Some(left.ty) })?;
                    (left, right)
                };
                let lt = self.types.get(left.ty)?;
                if matches!(lt, T::Parameter(_)) {
                    return Err(type_error(
                        "operations requiring type specialization are not implemented",
                    ));
                }
                match b.operator {
                    O::Plus | O::Minus | O::Star | O::Slash if numeric(lt) => {
                        if b.operator == O::Slash
                            && (super::constant::is_zero(&b.right, self.types)?)
                        {
                            return Err(type_error("constant division by zero"));
                        }
                        left.ty
                    }
                    O::Percent | O::Ampersand | O::Pipe | O::Caret if integral(lt) => left.ty,
                    O::LeftShift | O::RightShift
                        if integral(lt) && integral(self.types.get(right.ty)?) =>
                    {
                        if let Some(count) = super::constant::integer_as_i128(&b.right, self.types)?
                        {
                            let width = match lt {
                                T::Builtin(b) => integer(b).and_then(|(_, w)| w),
                                _ => None,
                            };
                            if count < 0 || width.is_some_and(|w| count >= w as i128) {
                                return Err(type_error("shift count is out of range"));
                            }
                        }
                        left.ty
                    }
                    O::LogicalAnd | O::LogicalOr if matches!(lt, T::Builtin(B::Boolean)) => left.ty,
                    O::GreaterThan | O::GreaterEqual | O::LesserThan | O::LesserEqual
                        if numeric(lt) || matches!(lt, T::Builtin(B::Char)) =>
                    {
                        self.types.builtin(B::Boolean)
                    }
                    O::DoubleEquals | O::Different
                        if numeric(lt)
                            || matches!(
                                lt,
                                T::Builtin(B::Boolean | B::Char | B::String | B::Void)
                                    | T::Pointer(_)
                            ) =>
                    {
                        self.types.builtin(B::Boolean)
                    }
                    _ => return Err(type_error("invalid binary operand types")),
                }
            }
            E::Call { callee, args } => {
                let info = self.expression(callee, None)?;
                let T::Function(signature) = self.types.get(info.ty)?.clone() else {
                    return Err(type_error("expression is not callable"));
                };
                if signature.variadic {
                    return Err(type_error("variadic calls are not implemented"));
                }
                if signature
                    .parameters
                    .iter()
                    .chain(std::iter::once(&signature.return_type))
                    .any(|t| {
                        matches!(
                            self.types.get(*t),
                            Ok(T::Builtin(B::MetaType) | T::Parameter(_))
                        )
                    })
                {
                    return Err(type_error(
                        "compile-time type calls and specialization are not implemented",
                    ));
                }
                if args.len() != signature.parameters.len() {
                    return Err(type_error("call argument arity mismatch"));
                }
                for (arg, ty) in args.iter_mut().zip(signature.parameters) {
                    self.expression(arg, Some(ty))?;
                }
                signature.return_type
            }
            E::ExprAcess {
                expr: receiver,
                accessed_identifier,
                member: slot,
            } => {
                let info = self.expression(receiver, None)?;
                let owner = info.type_value.unwrap_or(info.ty);
                self.types.builtin(B::Usize);
                let selected = member(owner, accessed_identifier, self.types)?;
                match selected.kind {
                    MemberKind::EnumVariant(_) if info.type_value.is_none() => {
                        return Err(type_error("enum variant member requires a type value"));
                    }
                    MemberKind::Field(_) | MemberKind::Length if info.type_value.is_some() => {
                        return Err(type_error("member requires a runtime receiver"));
                    }
                    _ => {}
                }
                *slot = crate::frontend::analysis_annotation::MemberAnnotation {
                    resolved: Some(selected),
                };
                return Ok(ExpressionInfo {
                    ty: selected.ty,
                    category: if matches!(selected.kind, MemberKind::Field(_)) {
                        info.category
                    } else {
                        C::Value
                    },
                    type_value: None,
                });
            }
            E::StructConstruct {
                struct_identifier,
                fields,
            } => {
                let info = self.name_info(
                    struct_identifier
                        .annotation
                        .resolved
                        .ok_or_else(|| type_error("pending struct name"))?,
                )?;
                let owner = info
                    .type_value
                    .ok_or_else(|| type_error("struct construction requires a type"))?;
                let T::Struct(definition) = self.types.get(owner)? else {
                    return Err(type_error("construction target is not a struct"));
                };
                let count = definition.len();
                let mut seen = HashSet::new();
                for field in fields.iter_mut() {
                    if !seen.insert(field.field_identifier.clone()) {
                        return Err(at_span(
                            type_error("duplicate initializer field"),
                            field.expression.span,
                        ));
                    }
                    let selected = member(owner, &field.field_identifier, self.types)?;
                    self.expression(&mut field.expression, Some(selected.ty))?;
                    field.annotation.resolved = Some(selected);
                }
                if seen.len() != count {
                    return Err(type_error("missing initializer field"));
                }
                owner
            }
            E::ArrayLiteral { elements } => {
                let contextual = expected.and_then(|t| match self.types.get(t) {
                    Ok(T::Array { element, length }) => Some((*element, *length)),
                    _ => None,
                });
                if let Some((_, length)) = contextual
                    && length != elements.len() as u64
                {
                    return Err(type_error("array length mismatch"));
                }
                let element = if let Some((element, _)) = contextual {
                    element
                } else if let Some(first) = elements.first_mut() {
                    self.expression(first, None)?.ty
                } else {
                    return Err(type_error("empty array requires a context"));
                };
                for value in elements.iter_mut() {
                    self.expression(value, Some(element))?;
                }
                self.types.intern(T::Array {
                    element,
                    length: elements.len() as u64,
                })
            }
            E::IndexAccess { object, index } => {
                let receiver = self.expression(object, None)?;
                let (element, length, category) = match self.types.get(receiver.ty)? {
                    T::Array { element, length } => (*element, Some(*length), receiver.category),
                    T::Slice(element) => (*element, None, C::MutablePlace),
                    _ => return Err(type_error("receiver is not indexable")),
                };
                self.index(index, length, false)?;
                return Ok(ExpressionInfo {
                    ty: element,
                    category,
                    type_value: None,
                });
            }
            E::SliceLiteral {
                object,
                start,
                end,
                inclusive,
            } => {
                let receiver = self.expression(object, None)?;
                let (element, length) = match self.types.get(receiver.ty)? {
                    T::Array { element, length } => (*element, Some(*length)),
                    T::Slice(element) => (*element, None),
                    _ => return Err(type_error("receiver does not support slicing")),
                };
                let from = if let Some(start) = start {
                    self.index(start, length, true)?;
                    super::constant::integer_as_i128(start, self.types)?
                } else {
                    Some(0)
                };
                let to = if let Some(end) = end {
                    self.index(end, length, !*inclusive)?;
                    super::constant::integer_as_i128(end, self.types)?
                        .map(|n| if *inclusive { n + 1 } else { n })
                } else {
                    length.map(|n| n as i128)
                };
                if let (Some(from), Some(to)) = (from, to)
                    && from > to
                {
                    return Err(type_error("slice bounds are reversed"));
                }
                self.types.intern(T::Slice(element))
            }
            E::Cast {
                expr: source,
                target_type,
            } => {
                let target = self.type_expression(target_type, false, false)?;
                // Explicit casts supply context only for null, not numeric source values.
                let info = self.expression(
                    source,
                    if matches!(source.kind, E::Literal(Literal::Null)) {
                        Some(target)
                    } else {
                        None
                    },
                )?;
                let from = self.types.get(info.ty)?;
                let to = self.types.get(target)?;
                let allowed = (numeric(from) && numeric(to))
                    || match (from, to) {
                        (T::Pointer(a), T::Pointer(b)) => {
                            a == b || self.types.is_opaque(*a)? || self.types.is_opaque(*b)?
                        }
                        _ => info.ty == target,
                    };
                if !allowed {
                    return Err(type_error("unsupported cast"));
                }
                super::constant::check_cast(source, target, self.types)?;
                target
            }
            E::QualifiedAccess { .. } => {
                return Err(type_error(
                    "legacy qualified access has no semantic annotation",
                ));
            }
        };
        Ok(self.value(ty))
    }
    fn index(
        &mut self,
        expr: &mut Expression,
        length: Option<u64>,
        allow_end: bool,
    ) -> anyhow::Result<()> {
        let info = self.expression(expr, None)?;
        if !integral(self.types.get(info.ty)?) {
            return Err(at_span(
                type_error("index/bound requires an integer"),
                expr.span,
            ));
        }
        if let Some(value) = super::constant::integer_as_i128(expr, self.types)?
            && (value < 0
                || length.is_some_and(|n| {
                    if allow_end {
                        value > n as i128
                    } else {
                        value >= n as i128
                    }
                }))
        {
            return Err(at_span(
                type_error("index or slice bounds out of bounds"),
                expr.span,
            ));
        }
        Ok(())
    }
}
