//! Expression inference, operand rules and contextual literals. Compatibility is
//! deliberately delegated only after both semantic types have been determined.
use super::{TypeContext, at_span, types::check_assignable};
use crate::frontend::{
    analysis_annotation::{ExpressionInfo, MemberInfo, MemberKind, ValueCategory},
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
pub(super) fn numeric(t: &T) -> bool {
    matches!(t, T::Builtin(b) if integer(b).is_some() || float(b))
}
/// Only direct same-family literals receive cast destination context.
pub(super) fn cast_context(
    source: &Expression,
    target: TypeId,
    types: &SemanticTypes,
) -> Option<TypeId> {
    let literal = match &source.kind {
        E::Literal(literal) => literal,
        E::UnaryExpression(u) if u.operator == O::Minus => match &u.expression.kind {
            E::Literal(literal) => literal,
            _ => return None,
        },
        _ => return None,
    };
    let target_kind = types.get(types.canonical(target).ok()?).ok()?;
    match (literal, target_kind) {
        (Literal::Null, _) => Some(target),
        (Literal::Integer(_), T::Builtin(b)) if integer(b).is_some() => Some(target),
        (Literal::Float(_), T::Builtin(b)) if float(b) => Some(target),
        _ => None,
    }
}
/// Returns whether a semantic type is an integer builtin. Unlike `integer`,
/// which also provides signedness and width, this predicate is convenient when
/// an operation only needs to enforce that its operand is is_integral_type.
pub(super) fn is_integral_type(t: &T) -> bool {
    matches!(t, T::Builtin(b) if integer(b).is_some())
}
fn check_literal(literal: &Literal, ty: &T, negative: bool) -> anyhow::Result<()> {
    let T::Builtin(b) = ty else {
        return Err(type_error("literal requires a numeric type"));
    };
    let numeric = if *b == B::Usize {
        crate::numeric::NumericType::Integer {
            width: 128,
            signed: false,
        }
    } else {
        crate::numeric::NumericType::builtin(b)
            .ok_or_else(|| type_error("literal requires a numeric type"))?
    };
    crate::numeric::NumericValue::literal(literal, numeric, negative)
        .map(|_| ())
        .map_err(|e| type_error(e.to_string()))
}
/// Determines the value category produced by indexing an array or slice, along
/// with the element type and (for arrays) its known length.
///
/// Inline array elements belong to the binding's storage: indexing through an
/// immutable or readonly place yields a readonly place for the element, including
/// arrays nested in struct fields. Slices are different: readonly on the slice
/// value itself does not qualify the slice's element view, so indexing a slice
/// yields a mutable place unless the slice type qualifies its elements.
/// Value-category arrays remain values when indexed; other writable elements
/// are mutable places.
pub(super) fn indexed(
    receiver: ExpressionInfo,
    types: &SemanticTypes,
) -> anyhow::Result<(TypeId, Option<u64>, ValueCategory)> {
    let (container, readonly) = types.container(receiver.ty)?;
    let (element, length, inherited) = match container {
        T::Array { element, length } => (
            *element,
            Some(*length),
            matches!(
                receiver.category,
                ValueCategory::ImmutablePlace | ValueCategory::ReadonlyPlace
            ),
        ),
        T::Slice(element) => (*element, None, false),
        T::Parameter(_) | T::TypeResult { .. } | T::MemberResult { .. } | T::IndexResult { .. } => {
            let recipe = T::IndexResult {
                owner: types.canonical(receiver.ty)?,
            };
            let index = types
                .types
                .iter()
                .position(|t| *t == recipe)
                .ok_or_else(|| type_error("missing dependent index recipe"))?;
            (
                TypeId(index as u32),
                None,
                receiver.category == ValueCategory::ReadonlyPlace,
            )
        }
        _ => return Err(type_error("receiver is not indexable")),
    };
    Ok((
        element,
        length,
        if readonly || inherited {
            ValueCategory::ReadonlyPlace
        } else if length.is_some() && receiver.category == ValueCategory::Value {
            ValueCategory::Value
        } else {
            ValueCategory::MutablePlace
        },
    ))
}

/// The same resolved-type cast policy is used by inference and validation.
pub(super) fn is_cast_allowed(
    from: TypeId,
    to: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<bool> {
    Ok(matches!(types.get(from)?, T::Builtin(B::Never))
        || ((numeric(types.get(from)?) || matches!(types.get(from)?, T::Builtin(B::Boolean)))
            && (numeric(types.get(to)?) || matches!(types.get(to)?, T::Builtin(B::Boolean))))
        || is_pointer_cast(from, to, types)?
        || check_assignable(to, from, types).is_ok())
}

pub(super) fn is_pointer_cast(
    from: TypeId,
    to: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<bool> {
    if types.sentinel(to)?.is_some() && types.sentinel(to)? != types.sentinel(from)? {
        return Ok(false);
    }
    let (T::Pointer(a), ar) = types.container(from)? else {
        return Ok(false);
    };
    let (T::Pointer(b), br) = types.container(to)? else {
        return Ok(false);
    };
    if ar != br {
        return Ok(false);
    }
    if super::types::check_assignable(*a, *b, types).is_ok()
        && super::types::check_assignable(*b, *a, types).is_ok()
    {
        return Ok(true);
    }
    // Opaque casts must not erase nested readonly qualifications and later
    // reconstruct a mutable pointer. No general reinterpretation policy exists.
    fn qualified(
        root: TypeId,
        types: &SemanticTypes,
        seen: &mut HashSet<TypeId>,
    ) -> anyhow::Result<bool> {
        let root = types.canonical(root)?;
        if !seen.insert(root) {
            return Ok(false);
        }
        let children = match types.get(root)? {
            T::Readonly(_) | T::Sentinel { .. } => return Ok(true),
            T::Pointer(t) | T::Slice(t) => vec![*t],
            T::Array { element, .. } => vec![*element],
            T::Tuple(ts) => ts.clone(),
            T::Struct(fs) => fs.iter().map(|f| f.ty).collect(),
            T::Enum(vs) => vs.iter().filter_map(|v| v.payload).collect(),
            T::Function(f) => f
                .parameters
                .iter()
                .copied()
                .chain([f.return_type])
                .collect(),
            _ => vec![],
        };
        for child in children {
            if qualified(child, types, seen)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    Ok((types.is_opaque(*a)? || types.is_opaque(*b)?)
        && !qualified(*a, types, &mut HashSet::new())?
        && !qualified(*b, types, &mut HashSet::new())?)
}

pub(super) fn member(
    owner: TypeId,
    spelling: &str,
    types: &SemanticTypes,
) -> anyhow::Result<MemberInfo> {
    let owner = types.canonical(owner)?;
    let (kind, ty) = match types.container(owner)?.0 {
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
            let ty = if types.enum_payload(v.payload)?.is_some() {
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
        T::Parameter(_) | T::TypeResult { .. } | T::MemberResult { .. } | T::IndexResult { .. } => {
            let recipe = T::MemberResult {
                owner,
                spelling: spelling.to_owned(),
            };
            let index = types
                .types
                .iter()
                .position(|t| *t == recipe)
                .ok_or_else(|| type_error("missing dependent member recipe"))?;
            (MemberKind::Dependent, TypeId(index as u32))
        }
        _ => return Err(type_error(format!("inapplicable member '{spelling}'"))),
    };
    Ok(MemberInfo { owner, kind, ty })
}

/// A field receiver may insert one checked dereference; never recurse here.
pub(super) fn field_receiver(
    info: ExpressionInfo,
    types: &SemanticTypes,
) -> anyhow::Result<(ExpressionInfo, bool)> {
    if info.type_value.is_none() && matches!(types.container(info.ty)?, (T::Pointer(_), _)) {
        return Ok((dereference_info(info.ty, types)?, true));
    }
    Ok((info, false))
}

/// Whether evaluation is guaranteed not to complete. A short-circuit RHS is
/// mandatory only when the LHS is known to select it.
pub(super) fn diverges(expr: &Expression, types: &SemanticTypes) -> bool {
    let child = |e: &Expression| diverges(e, types);
    match &expr.kind {
        E::BinaryExpresison(b) => {
            child(&b.left)
                || match b.operator {
                    O::LogicalAnd => super::known_boolean(&b.left) == Some(true) && child(&b.right),
                    O::LogicalOr => super::known_boolean(&b.left) == Some(false) && child(&b.right),
                    _ => child(&b.right),
                }
        }
        E::UnaryExpression(u) => child(&u.expression),
        E::Call { callee, args } => {
            child(callee)
                || args.iter().any(child)
                || expr
                    .annotation
                    .resolved
                    .is_some_and(|i| matches!(types.get(i.ty), Ok(T::Builtin(B::Never))))
        }
        E::ExprAcess { expr, .. } | E::Cast { expr, .. } => child(expr),
        E::StructConstruct { fields, .. } => fields.iter().any(|f| child(&f.expression)),
        E::ArrayLiteral { elements } => elements.iter().any(child),
        E::IndexAccess { object, index } => child(object) || child(index),
        E::SliceLiteral {
            object, start, end, ..
        } => child(object) || start.iter().chain(end).any(|e| child(e)),
        _ => false,
    }
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
        let mut info = result.map_err(|e| self.diagnostic(e, Some(expr.span), self.file))?;
        expr.annotation.resolved = Some(info);
        if diverges(expr, self.types) {
            let never = self.types.builtin(B::Never);
            info = self.value(never);
        }
        if let Some(expected) = expected {
            if info.type_value.is_none()
                && matches!(self.types.container(info.ty)?.0, T::Array { .. })
                && matches!(self.types.container(expected)?.0, T::Slice(_))
            {
                // Use the ordinary full-slice representation so all later passes
                // retain the backing place (or materialize an array temporary).
                self.prepare_index(info.ty)?;
                let (element, _, category) = indexed(info, self.types)?;
                let slice = self.types.intern(T::Slice(element));
                let slice = if let Some(value) = self.types.sentinel(info.ty)? {
                    self.types.intern(T::Sentinel {
                        inner: slice,
                        value,
                    })
                } else {
                    slice
                };
                let slice = if category == ValueCategory::ReadonlyPlace {
                    self.types.intern(T::Readonly(slice))
                } else {
                    slice
                };
                check_assignable(expected, slice, self.types)
                    .map_err(|e| self.diagnostic(e, Some(expr.span), self.file))?;
                let object = Box::new(expr.clone());
                expr.kind = E::SliceLiteral {
                    object,
                    start: None,
                    end: None,
                    inclusive: false,
                };
                info = self.value(slice);
            }
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
                Literal::Integer(_) => {
                    let ty = expected
                        .filter(|t| self.types.get(*t).is_ok_and(numeric))
                        .unwrap_or_else(|| self.types.builtin(B::Int32));
                    check_literal(literal, self.types.get(ty)?, false)?;
                    ty
                }
                Literal::Float(_) => {
                    let ty = expected
                        .filter(|t| matches!(self.types.get(*t),Ok(T::Builtin(b)) if float(b)))
                        .unwrap_or_else(|| self.types.builtin(B::Float64));
                    check_literal(literal, self.types.get(ty)?, false)?;
                    ty
                }
                Literal::Boolean(_) => self.types.builtin(B::Boolean),
                Literal::Character(value) => {
                    if u32::from(*value) > 255 {
                        return Err(type_error("character literal is out of range for uint8"));
                    }
                    self.types.builtin(B::UInt8)
                }
                Literal::String(value) => {
                    let element = self.types.builtin(B::UInt8);
                    if let Some(expected) = expected
                        && self.types.sentinel(expected)?.is_none()
                        && self.types.container(expected).is_ok_and(|(container, readonly)| readonly && matches!(container, T::Slice(inner) if self.types.canonical(*inner).ok() == Some(element)))
                    {
                        return Ok(self.value(expected));
                    }
                    let array = self.types.intern(T::Array {
                        element,
                        length: value.len() as u64,
                    });
                    let array = if let Some(value) = expected
                        .map(|t| self.types.sentinel(t))
                        .transpose()?
                        .flatten()
                    {
                        self.types.intern(T::Sentinel {
                            inner: array,
                            value,
                        })
                    } else {
                        array
                    };
                    self.types.intern(T::Readonly(array))
                }
                Literal::CString(_) => {
                    let element = self.types.builtin(B::UInt8);
                    let pointer = self.types.intern(T::Pointer(element));
                    let pointer = self.types.intern(T::Sentinel {
                        inner: pointer,
                        value: 0,
                    });
                    self.types.intern(T::Readonly(pointer))
                }
                Literal::Null => expected
                    .filter(|t| {
                        if self.types.sentinel(*t).ok().flatten().is_some() {
                            return false;
                        }
                        matches!(
                            self.types.container(*t),
                            Ok((T::Pointer(_) | T::Function(_), _))
                        )
                    })
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
                    && let E::Literal(literal @ Literal::Integer(_)) = &u.expression.kind
                {
                    let ty = expected
                        .filter(|t| self.types.get(*t).is_ok_and(numeric))
                        .unwrap_or_else(|| self.types.builtin(B::Int32));
                    check_literal(literal, self.types.get(ty)?, true)?;
                    u.expression.annotation.resolved = Some(self.value(ty));
                    return Ok(self.value(ty));
                }
                let hint = if matches!(u.operator, O::Minus | O::Tilde) {
                    expected
                } else {
                    None
                };
                let info = self.expression(&mut u.expression, hint)?;
                if matches!(self.types.get(info.ty)?, T::Builtin(B::Never)) {
                    return Ok(info);
                }
                if self.types.is_symbolic(info.ty)?
                    && matches!(u.operator, O::Minus | O::Tilde | O::LogicalNot)
                {
                    let ty = if u.operator == O::LogicalNot {
                        self.types.builtin(B::Boolean)
                    } else {
                        info.ty
                    };
                    return Ok(self.value(ty));
                }
                match u.operator {
                    O::Minus if numeric(self.types.get(info.ty)?) => info.ty,
                    O::Tilde if is_integral_type(self.types.get(info.ty)?) => info.ty,
                    O::LogicalNot if matches!(self.types.get(info.ty)?, T::Builtin(B::Boolean)) => {
                        info.ty
                    }
                    O::AddressOf => {
                        if info.category == ValueCategory::Value {
                            return Err(type_error("address requires a place"));
                        }
                        let pointer = self.types.intern(T::Pointer(info.ty));
                        if info.category == ValueCategory::MutablePlace {
                            pointer
                        } else {
                            self.types.intern(T::Readonly(pointer))
                        }
                    }
                    O::Dereference => return dereference_info(info.ty, self.types),
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
                let logical = matches!(b.operator, O::LogicalAnd | O::LogicalOr);
                let hint = if comparison {
                    None
                } else if logical {
                    Some(self.types.builtin(B::Boolean))
                } else {
                    let integer_only = matches!(
                        b.operator,
                        O::Percent
                            | O::Ampersand
                            | O::Pipe
                            | O::Caret
                            | O::LeftShift
                            | O::RightShift
                    );
                    expected.filter(|t| {
                        self.types.get(*t).is_ok_and(|ty| {
                            if integer_only {
                                is_integral_type(ty)
                            } else {
                                numeric(ty)
                            }
                        })
                    })
                };
                // Obtain context from a typed right operand when the left is a literal.
                let contextual_left = matches!(b.left.kind, E::Literal(_))
                    || matches!(&b.left.kind,E::UnaryExpression(u) if matches!(u.expression.kind,E::Literal(_)));
                let (left, right) = if contextual_left
                    && !matches!(b.right.kind, E::Literal(_))
                    && hint.is_none()
                    && !shift
                {
                    let right = self.expression(&mut b.right, None)?;
                    let context = (!matches!(self.types.get(right.ty)?, T::Builtin(B::Never)))
                        .then_some(right.ty);
                    let left = self.expression(&mut b.left, context)?;
                    (left, right)
                } else {
                    let left = self.expression(&mut b.left, hint)?;
                    if matches!(
                        b.operator,
                        O::Plus | O::Minus | O::Star | O::Slash | O::Percent
                    ) && !numeric(self.types.get(left.ty)?)
                        && !matches!(self.types.get(left.ty)?, T::Builtin(B::Never))
                        && !self.types.is_symbolic(left.ty)?
                    {
                        return Err(type_error("invalid binary operand type"));
                    }
                    let context = if shift {
                        None
                    } else if matches!(self.types.get(left.ty)?, T::Builtin(B::Never)) {
                        hint
                    } else {
                        Some(left.ty)
                    };
                    let right = self.expression(&mut b.right, context)?;
                    (left, right)
                };
                let left_never = matches!(self.types.get(left.ty)?, T::Builtin(B::Never));
                let right_never = matches!(self.types.get(right.ty)?, T::Builtin(B::Never));
                if left_never && right_never {
                    return Ok(left);
                }
                let operand = if left_never { right.ty } else { left.ty };
                let lt = self.types.get(operand)?;
                if matches!(b.operator, O::Percent | O::Slash)
                    && is_integral_type(lt)
                    && super::constant::is_zero(&b.right, self.types)?
                {
                    return Err(type_error(if b.operator == O::Percent {
                        "constant remainder by zero"
                    } else {
                        "constant division by zero"
                    }));
                }
                if shift
                    && super::constant::integer_as_i128(&b.right, self.types)?
                        .is_some_and(|n| n < 0)
                {
                    return Err(type_error("shift count is out of range"));
                }
                if self.types.is_symbolic(left.ty)? || self.types.is_symbolic(right.ty)? {
                    for operand in [left.ty, right.ty] {
                        if !self.types.is_symbolic(operand)?
                            && !matches!(self.types.get(operand)?, T::Builtin(B::Never))
                        {
                            let ty = self.types.get(operand)?;
                            let valid = if logical {
                                matches!(ty, T::Builtin(B::Boolean))
                            } else if shift
                                || matches!(
                                    b.operator,
                                    O::Percent | O::Ampersand | O::Pipe | O::Caret
                                )
                            {
                                is_integral_type(ty)
                            } else if matches!(b.operator, O::DoubleEquals | O::Different) {
                                numeric(ty)
                                    || matches!(ty, T::Builtin(B::Boolean | B::String | B::Void))
                                    || matches!(self.types.container(operand)?, (T::Pointer(_), _))
                            } else {
                                numeric(ty)
                            };
                            if !valid {
                                return Err(type_error("invalid binary operand type"));
                            }
                        }
                    }
                    let ty = if comparison || logical {
                        self.types.builtin(B::Boolean)
                    } else if self.types.is_symbolic(left.ty)? {
                        left.ty
                    } else {
                        right.ty
                    };
                    return Ok(self.value(ty));
                }
                match b.operator {
                    O::Plus | O::Minus | O::Star | O::Slash if numeric(lt) => left.ty,
                    O::Percent | O::Ampersand | O::Pipe | O::Caret if is_integral_type(lt) => {
                        left.ty
                    }
                    O::LeftShift | O::RightShift
                        if is_integral_type(lt)
                            && (right_never || is_integral_type(self.types.get(right.ty)?)) =>
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
                        if numeric(lt) =>
                    {
                        self.types.builtin(B::Boolean)
                    }
                    O::DoubleEquals | O::Different
                        if numeric(lt)
                            || matches!(
                                lt,
                                T::Builtin(B::Boolean | B::String | B::Void) | T::Pointer(_)
                            )
                            || matches!(self.types.container(operand)?, (T::Pointer(_), _)) =>
                    {
                        self.types.builtin(B::Boolean)
                    }
                    _ => return Err(type_error("invalid binary operand types")),
                }
            }
            E::Call { callee, args } => return self.call(callee, args),
            E::ExprAcess {
                expr: receiver,
                accessed_identifier,
                member: slot,
            } => {
                let info = self.expression(receiver, None)?;
                let (info, implicit_deref) = field_receiver(info, self.types)?;
                let owner = info.type_value.unwrap_or(info.ty);
                self.types.builtin(B::Usize);
                let canonical = self.types.canonical(owner)?;
                if matches!(
                    self.types.get(canonical)?,
                    T::Parameter(_)
                        | T::TypeResult { .. }
                        | T::MemberResult { .. }
                        | T::IndexResult { .. }
                ) {
                    self.types.intern(T::MemberResult {
                        owner: canonical,
                        spelling: accessed_identifier.clone(),
                    });
                }
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
                    source_span: slot.source_span,
                    resolved: Some(selected),
                    implicit_deref,
                };
                return Ok(ExpressionInfo {
                    ty: selected.ty,
                    category: if matches!(
                        selected.kind,
                        MemberKind::Field(_) | MemberKind::Dependent
                    ) {
                        info.category
                    } else {
                        ValueCategory::Value
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
                let contextual = expected.and_then(|t| match self.types.container(t) {
                    Ok((T::Array { element, length }, _)) => Some((*element, *length)),
                    Ok((T::Slice(element), _)) => Some((*element, elements.len() as u64)),
                    _ => None,
                });
                if let Some((_, length)) = contextual
                    && length != elements.len() as u64
                {
                    return Err(type_error("array length mismatch"));
                }
                let element = if let Some((element, _)) = contextual {
                    element
                } else if !elements.is_empty() {
                    let mut inferred = None;
                    for value in elements.iter_mut() {
                        let info = self.expression(value, None)?;
                        if !matches!(self.types.get(info.ty)?, T::Builtin(B::Never)) {
                            inferred = Some(info.ty);
                            break;
                        }
                    }
                    let Some(element) = inferred else {
                        let never = self.types.builtin(B::Never);
                        return Ok(self.value(never));
                    };
                    element
                } else {
                    return Err(type_error("empty array requires a context"));
                };
                for value in elements.iter_mut() {
                    self.expression(value, Some(element))?;
                }
                let array = self.types.intern(T::Array {
                    element,
                    length: elements.len() as u64,
                });
                let array = if let Some(value) = expected
                    .map(|t| self.types.sentinel(t))
                    .transpose()?
                    .flatten()
                {
                    self.types.intern(T::Sentinel {
                        inner: array,
                        value,
                    })
                } else {
                    array
                };
                if expected
                    .is_some_and(|t| self.types.container(t).is_ok_and(|(_, readonly)| readonly))
                {
                    self.types.intern(T::Readonly(array))
                } else {
                    array
                }
            }
            E::IndexAccess { object, index } => {
                let receiver = self.expression(object, None)?;
                if receiver.type_value.is_some() {
                    return Err(type_error("array types use prefix brackets: [N]T"));
                }
                self.prepare_index(receiver.ty)?;
                let (element, length, category) = indexed(receiver, self.types)?;
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
                if receiver.type_value.is_some() {
                    return Err(type_error("slice types use prefix brackets: []T"));
                }
                self.prepare_index(receiver.ty)?;
                let (element, length, category) = indexed(receiver, self.types)?;
                let from = if let Some(start) = start {
                    self.index(start, length, true)?;
                    super::constant::integer_as_i128(start, self.types)?
                } else {
                    Some(0)
                };
                let to = if let Some(end) = end {
                    self.index(end, length, !*inclusive)?;
                    super::constant::integer_as_i128(end, self.types)?
                        .map(|n| {
                            if *inclusive {
                                n.checked_add(1)
                                    .ok_or_else(|| type_error("inclusive slice bound overflows"))
                            } else {
                                Ok(n)
                            }
                        })
                        .transpose()?
                } else {
                    length.map(|n| n as i128)
                };
                if let (Some(from), Some(to)) = (from, to)
                    && from > to
                {
                    return Err(type_error("slice bounds are reversed"));
                }
                let slice = self.types.intern(T::Slice(element));
                let slice = if end.is_none()
                    && let Some(value) = self.types.sentinel(receiver.ty)?
                {
                    self.types.intern(T::Sentinel {
                        inner: slice,
                        value,
                    })
                } else {
                    slice
                };
                if category == ValueCategory::ReadonlyPlace {
                    self.types.intern(T::Readonly(slice))
                } else {
                    slice
                }
            }
            E::Cast {
                expr: source,
                target_type,
            } => {
                let target = self.type_expression(target_type, false, false)?;
                let context = cast_context(source, target, self.types);
                let info = self.expression(source, context)?;
                let from = self.types.get(info.ty)?;
                let allowed = is_cast_allowed(info.ty, target, self.types)?;
                if matches!(from, T::Builtin(B::Never)) {
                    return Ok(info);
                }
                if !allowed {
                    let (source_container, source_readonly) = self.types.container(info.ty)?;
                    let (target_container, target_readonly) = self.types.container(target)?;
                    if let (T::Slice(source_element), T::Slice(target_element)) =
                        (source_container, target_container)
                    {
                        let different_elements =
                            check_assignable(*target_element, *source_element, self.types).is_err();
                        if source_readonly && !target_readonly {
                            let element_note = if different_elements {
                                " Slice casts also cannot convert element types."
                            } else {
                                ""
                            };
                            return Err(type_error(format!(
                                "cannot cast a readonly slice to a writable slice: this would remove `const` protection from the original storage.{element_note} Copy the elements into a writable buffer, converting each element as needed"
                            )));
                        }
                        if different_elements {
                            return Err(type_error(
                                "cannot cast between slices with different element types: slice casts do not convert elements; copy the elements into a new buffer, converting each element as needed",
                            ));
                        }
                    }
                    return Err(type_error("unsupported cast"));
                }
                super::constant::check_cast(source, target, self.types)?;
                target
            }
        };
        Ok(self.value(ty))
    }
    fn prepare_index(&mut self, owner: TypeId) -> anyhow::Result<()> {
        let owner = self.types.canonical(owner)?;
        if matches!(
            self.types.get(owner)?,
            T::Parameter(_) | T::TypeResult { .. } | T::MemberResult { .. } | T::IndexResult { .. }
        ) {
            self.types.intern(T::IndexResult { owner });
        }
        Ok(())
    }
    fn index(
        &mut self,
        expr: &mut Expression,
        length: Option<u64>,
        allow_end: bool,
    ) -> anyhow::Result<()> {
        let info = self.expression(expr, None)?;
        if !is_integral_type(self.types.get(info.ty)?)
            && !matches!(self.types.get(info.ty)?, T::Builtin(B::Never))
            && !self.types.is_symbolic(info.ty)?
        {
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

/// Shared checked/unchecked dereference typing. Checking only changes execution.
pub(super) fn dereference_info(
    ty: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<ExpressionInfo> {
    let (container, readonly) = types.container(ty)?;
    let T::Pointer(pointee) = container else {
        return Err(type_error("dereference operand must be a pointer"));
    };
    if types.is_opaque(*pointee)? {
        return Err(type_error("cannot dereference opaque pointer"));
    }
    Ok(ExpressionInfo {
        ty: *pointee,
        category: if readonly {
            ValueCategory::ReadonlyPlace
        } else {
            ValueCategory::MutablePlace
        },
        type_value: None,
    })
}

/// Recomputed independently by annotation validation; compiler identities select the operation.
pub(super) fn intrinsic_info(
    builtin: crate::builtins::BuiltinFunction,
    args: &[ExpressionInfo],
    types: &SemanticTypes,
) -> anyhow::Result<ExpressionInfo> {
    let find = |ty: T| {
        types
            .types
            .iter()
            .position(|t| *t == ty)
            .map(|i| TypeId(i as u32))
            .ok_or_else(|| type_error("missing intrinsic result type"))
    };
    use crate::builtins::BuiltinFunction as F;
    if let Some(arity) = builtin.syscall_arity() {
        if args.len() != arity + 1 || !matches!(types.get(args[0].ty)?, T::Builtin(B::UInt64)) {
            return Err(type_error(
                "syscall requires a uint64 number and zero through six arguments",
            ));
        }
        for arg in &args[1..] {
            if !matches!(
                types.container(arg.ty)?.0,
                T::Builtin(B::UInt64) | T::Pointer(_)
            ) {
                return Err(type_error(
                    "syscall arguments must be uint64 words or typed pointers",
                ));
            }
        }
        return Ok(ExpressionInfo {
            ty: find(T::Builtin(B::Int64))?,
            category: ValueCategory::Value,
            type_value: None,
        });
    }
    match (builtin, args) {
        (F::Sizeof, [argument]) => {
            let ty = argument
                .type_value
                .ok_or_else(|| type_error("sizeof requires a type argument"))?;
            if matches!(
                types.get(types.canonical(ty)?)?,
                T::Builtin(B::Void | B::Never | B::MetaType) | T::IntrinsicFunction
            ) {
                return Err(type_error("sizeof requires a sized runtime type"));
            }
            Ok(ExpressionInfo {
                ty: find(T::Builtin(B::Usize))?,
                category: ValueCategory::Value,
                type_value: None,
            })
        }
        (F::PointerToWord, [pointer]) => {
            if !matches!(types.container(pointer.ty)?.0, T::Pointer(_)) {
                return Err(type_error("pointer_to_word requires a typed pointer"));
            }
            Ok(ExpressionInfo {
                ty: find(T::Builtin(B::UInt64))?,
                category: ValueCategory::Value,
                type_value: None,
            })
        }
        (F::PointerFromWord, [target, word]) => {
            if !matches!(types.get(word.ty)?, T::Builtin(B::UInt64)) {
                return Err(type_error("pointer_from_word requires uint64"));
            }
            let target = target
                .type_value
                .ok_or_else(|| type_error("pointer_from_word requires a type argument"))?;
            Ok(ExpressionInfo {
                ty: find(T::Pointer(target))?,
                category: ValueCategory::Value,
                type_value: None,
            })
        }
        (F::UncheckedDeref, [pointer]) => dereference_info(pointer.ty, types),
        (F::SliceFromParts, [pointer, length]) => {
            let (T::Pointer(element), readonly) = types.container(pointer.ty)? else {
                return Err(type_error("slice_from_parts requires a typed pointer"));
            };
            if types.is_opaque(*element)? {
                return Err(type_error("slice elements cannot be void"));
            }
            if !matches!(types.get(length.ty)?, T::Builtin(B::Usize)) {
                return Err(type_error("slice length must be usize"));
            }
            let ty = find(T::Slice(*element))?;
            let ty = if readonly { find(T::Readonly(ty))? } else { ty };
            Ok(ExpressionInfo {
                ty,
                category: ValueCategory::Value,
                type_value: None,
            })
        }
        (F::SliceData, [slice]) => {
            let (T::Slice(element), readonly) = types.container(slice.ty)? else {
                return Err(type_error("slice_data requires a slice"));
            };
            let ty = find(T::Pointer(*element))?;
            let ty = if let Some(value) = types.sentinel(slice.ty)? {
                find(T::Sentinel { inner: ty, value })?
            } else {
                ty
            };
            let ty = if readonly { find(T::Readonly(ty))? } else { ty };
            Ok(ExpressionInfo {
                ty,
                category: ValueCategory::Value,
                type_value: None,
            })
        }
        _ => Err(type_error("invalid intrinsic invocation")),
    }
}
