//! Exact fixed-width evaluation of syntactically constant numeric expressions.
//! Identifiers and calls remain runtime expressions; this is not an interpreter.
use crate::{
    frontend::{
        ast::expression::{Expression, ExpressionKind as E},
        semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
        tokens::{Literal, Operator as O, builtin::BuiltinType as B},
    },
    numeric::{NumericType, NumericValue},
};

pub(super) fn numeric_type(ty: TypeId, types: &SemanticTypes) -> Option<NumericType> {
    match types.get(types.canonical(ty).ok()?).ok()? {
        T::Builtin(B::Usize) => Some(NumericType::Integer {
            width: 128,
            signed: false,
        }),
        T::Builtin(b) => NumericType::builtin(b),
        _ => None,
    }
}
pub(crate) fn evaluate_numeric(
    e: &Expression,
    types: &SemanticTypes,
) -> anyhow::Result<Option<NumericValue>> {
    let Some(ty) = e
        .annotation
        .resolved
        .and_then(|i| numeric_type(i.ty, types))
    else {
        return Ok(None);
    };
    Ok(match &e.kind {
        E::Literal(literal @ (Literal::Integer(_) | Literal::Float(_))) => {
            Some(NumericValue::literal(literal, ty, false)?)
        }
        E::UnaryExpression(u) if matches!(u.operator, O::Minus | O::Tilde) => {
            if u.operator == O::Minus
                && let E::Literal(literal @ Literal::Integer(_)) = &u.expression.kind
            {
                Some(NumericValue::literal(literal, ty, true)?)
            } else {
                evaluate_numeric(&u.expression, types)?
                    .map(|v| v.unary(u.operator))
                    .transpose()?
            }
        }
        E::BinaryExpresison(b) => match (
            evaluate_numeric(&b.left, types)?,
            evaluate_numeric(&b.right, types)?,
        ) {
            (Some(a), Some(c)) => Some(a.binary(c, b.operator)?),
            _ => None,
        },
        E::Cast { expr, .. } => {
            if let Some(value) = evaluate_numeric(expr, types)? {
                Some(value.cast(ty)?)
            } else {
                evaluate_boolean(expr, types)?
                    .map(|b| {
                        NumericValue {
                            ty: NumericType::Integer {
                                width: 8,
                                signed: false,
                            },
                            bits: u128::from(b),
                        }
                        .cast(ty)
                    })
                    .transpose()?
            }
        }
        _ => None,
    })
}
pub(super) fn integer_as_i128(
    e: &Expression,
    types: &SemanticTypes,
) -> anyhow::Result<Option<i128>> {
    evaluate_numeric(e, types)?
        .map(|v| {
            v.integer_i128()
                .ok_or_else(|| type_error("integer constant is outside supported bounds"))
        })
        .transpose()
}
pub(super) fn is_zero(e: &Expression, types: &SemanticTypes) -> anyhow::Result<bool> {
    Ok(evaluate_numeric(e, types)?.is_some_and(NumericValue::is_zero))
}
pub(super) fn validate(e: &Expression, _ty: TypeId, types: &SemanticTypes) -> anyhow::Result<()> {
    let result = (|| {
        // Validate known failing operands even when their left sibling is runtime.
        if let E::BinaryExpresison(b) = &e.kind
            && let Some(NumericType::Integer { width, .. }) = b
                .left
                .annotation
                .resolved
                .and_then(|i| numeric_type(i.ty, types))
            && let Some(rhs) = evaluate_numeric(&b.right, types)?
        {
            if matches!(b.operator, O::Slash | O::Percent) && rhs.is_zero() {
                return Err(type_error("constant division by zero"));
            }
            if matches!(b.operator, O::LeftShift | O::RightShift) {
                let negative = matches!(rhs.ty, NumericType::Integer { width, signed: true } if crate::numeric::signed(rhs.bits,width) < 0);
                if negative || rhs.bits >= u128::from(width) {
                    return Err(type_error("shift count is out of range"));
                }
            }
        }
        evaluate_numeric(e, types).map(|_| ())
    })();
    result.map_err(|e| type_error(e.to_string()))
}
pub(super) fn check_cast(
    source: &Expression,
    target: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    if let Some(ty) = numeric_type(target, types)
        && let Some(value) = evaluate_numeric(source, types)?
    {
        value.cast(ty).map_err(|e| type_error(e.to_string()))?;
    }
    Ok(())
}

/// Constant Boolean casts use the same zero test as runtime (NaN is nonzero).
pub(crate) fn evaluate_boolean(
    e: &Expression,
    types: &SemanticTypes,
) -> anyhow::Result<Option<bool>> {
    Ok(match &e.kind {
        E::Literal(Literal::Boolean(value)) => Some(*value),
        E::UnaryExpression(u) if u.operator == O::LogicalNot => {
            evaluate_boolean(&u.expression, types)?.map(|b| !b)
        }
        E::Cast { expr, .. }
            if e.annotation
                .resolved
                .is_some_and(|i| matches!(types.get(i.ty), Ok(T::Builtin(B::Boolean)))) =>
        {
            if let Some(value) = evaluate_numeric(expr, types)? {
                Some(!value.is_zero())
            } else {
                evaluate_boolean(expr, types)?
            }
        }
        _ => None,
    })
}
