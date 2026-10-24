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
/// A binding's constness is intentionally ignored: a mutable binding may hold
/// an array whose elements can be changed, and a const binding does not make
/// those elements readonly. Instead, readonly is inherited from the access
/// path. For inline arrays, indexing through a readonly place yields a
/// readonly place for the element. Slices are different: readonly on the slice
/// value itself does not qualify the slice's element view, so indexing a slice
/// yields a mutable place unless the slice type or access path is readonly.
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
            receiver.category == ValueCategory::ReadonlyPlace,
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
        || (numeric(types.get(from)?) && numeric(types.get(to)?))
        || is_pointer_cast(from, to, types)?
        || check_assignable(to, from, types).is_ok())
}

pub(super) fn is_pointer_cast(
    from: TypeId,
    to: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<bool> {
    let (T::Pointer(a), ar) = types.container(from)? else {
        return Ok(false);
    };
    let (T::Pointer(b), br) = types.container(to)? else {
        return Ok(false);
    };
    if ar != br {
        return Ok(false);
    }
    if super::types::check_assignable(*a, *b, types).is_ok() {
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
            T::Readonly(_) => return Ok(true),
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
                Literal::Character(_) => self.types.builtin(B::Char),
                Literal::String(_) => self.types.builtin(B::String),
                Literal::Null => expected
                    .filter(|t| matches!(self.types.container(*t), Ok((T::Pointer(_), _))))
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
                    O::Dereference => {
                        let (container, readonly) = self.types.container(info.ty)?;
                        let T::Pointer(pointee) = container else {
                            return Err(type_error("dereference operand must be a pointer"));
                        };
                        if self.types.is_opaque(*pointee)? {
                            return Err(type_error("cannot dereference opaque pointer"));
                        }
                        return Ok(ExpressionInfo {
                            ty: *pointee,
                            category: if readonly {
                                ValueCategory::ReadonlyPlace
                            } else {
                                ValueCategory::MutablePlace
                            },
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
                                    || matches!(
                                        ty,
                                        T::Builtin(B::Char | B::Boolean | B::String | B::Void)
                                    )
                                    || matches!(self.types.container(operand)?, (T::Pointer(_), _))
                            } else if comparison {
                                numeric(ty) || matches!(ty, T::Builtin(B::Char))
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
                    resolved: Some(selected),
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
                let allowed = is_cast_allowed(info.ty, target, self.types)?;
                if matches!(from, T::Builtin(B::Never)) {
                    return Ok(info);
                }
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
