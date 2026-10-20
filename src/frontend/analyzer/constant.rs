//! Bounded checks for syntactic numeric constants, not a compile-time interpreter.
//! Integer magnitude covers all u128 values; floating exponent is independent of
//! host f64 range so float128 is not rejected for overflowing an intermediate f64.
use crate::frontend::{
    ast::expression::{Expression, ExpressionKind as E},
    semantic_type::{SemanticType as T, SemanticTypes, TypeId, type_error},
    tokens::{Literal, Operator as O, builtin::BuiltinType as B},
};

#[derive(Clone, Copy, Debug)]
struct Integer {
    negative: bool,
    magnitude: u128,
}
impl Integer {
    fn new(negative: bool, magnitude: u128) -> Self {
        Self {
            negative: negative && magnitude != 0,
            magnitude,
        }
    }
    fn signed(value: i128) -> Self {
        Self::new(value < 0, value.unsigned_abs())
    }
    fn as_signed(self) -> Option<i128> {
        if self.negative && self.magnitude == 1u128 << 127 {
            Some(i128::MIN)
        } else {
            i128::try_from(self.magnitude)
                .ok()
                .map(|n| if self.negative { -n } else { n })
        }
    }
    fn add(self, other: Self) -> Option<Self> {
        if self.negative == other.negative {
            Some(Self::new(
                self.negative,
                self.magnitude.checked_add(other.magnitude)?,
            ))
        } else if self.magnitude >= other.magnitude {
            Some(Self::new(self.negative, self.magnitude - other.magnitude))
        } else {
            Some(Self::new(other.negative, other.magnitude - self.magnitude))
        }
    }
    fn fits(self, b: &B) -> bool {
        match super::value_typing::integer(b) {
            Some((true, Some(width))) => {
                if self.negative {
                    self.magnitude <= 1u128 << (width - 1)
                } else {
                    self.magnitude < 1u128 << (width - 1)
                }
            }
            Some((false, Some(128))) | Some((false, None)) => !self.negative,
            Some((false, Some(width))) => !self.negative && self.magnitude < 1u128 << width,
            _ => false,
        }
    }
}
fn arithmetic_overflow() -> anyhow::Error {
    type_error("constant arithmetic is out of range")
}
fn integer(e: &Expression, types: &SemanticTypes) -> anyhow::Result<Option<Integer>> {
    let result = match &e.kind {
        E::Literal(Literal::Integer(v)) => Some(Integer::new(false, *v as u128)),
        E::UnaryExpression(u) => match (u.operator, integer(&u.expression, types)?) {
            (O::Minus, Some(n)) => Some(Integer::new(!n.negative, n.magnitude)),
            (O::Tilde, Some(n)) => {
                let ty = u
                    .expression
                    .annotation
                    .resolved
                    .and_then(|i| types.get(i.ty).ok());
                match ty {
                    Some(T::Builtin(b)) => match super::value_typing::integer(b) {
                        Some((false, Some(width))) => Some(Integer::new(
                            false,
                            !n.magnitude & (u128::MAX >> (128 - width)),
                        )),
                        Some((true, _)) => Some(Integer::signed(
                            !n.as_signed().ok_or_else(arithmetic_overflow)?,
                        )),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        },
        E::BinaryExpresison(b) => {
            match (integer(&b.left, types)?, integer(&b.right, types)?) {
                (Some(a), Some(c)) => Some(match b.operator {
                    O::Plus => a.add(c).ok_or_else(arithmetic_overflow)?,
                    O::Minus => a
                        .add(Integer::new(!c.negative, c.magnitude))
                        .ok_or_else(arithmetic_overflow)?,
                    O::Star => Integer::new(
                        a.negative != c.negative,
                        a.magnitude
                            .checked_mul(c.magnitude)
                            .ok_or_else(arithmetic_overflow)?,
                    ),
                    O::Slash | O::Percent if c.magnitude == 0 => {
                        return Err(type_error("constant division by zero"));
                    }
                    O::Slash => Integer::new(a.negative != c.negative, a.magnitude / c.magnitude),
                    O::Percent => Integer::new(a.negative, a.magnitude % c.magnitude),
                    O::Ampersand | O::Pipe | O::Caret => {
                        if !a.negative && !c.negative {
                            Integer::new(
                                false,
                                match b.operator {
                                    O::Ampersand => a.magnitude & c.magnitude,
                                    O::Pipe => a.magnitude | c.magnitude,
                                    _ => a.magnitude ^ c.magnitude,
                                },
                            )
                        } else {
                            let (a, c) = (
                                a.as_signed().ok_or_else(arithmetic_overflow)?,
                                c.as_signed().ok_or_else(arithmetic_overflow)?,
                            );
                            Integer::signed(match b.operator {
                                O::Ampersand => a & c,
                                O::Pipe => a | c,
                                _ => a ^ c,
                            })
                        }
                    }
                    O::LeftShift | O::RightShift => {
                        if c.negative {
                            return Err(type_error("shift count is out of range"));
                        }
                        if c.magnitude >= 128 {
                            if a.magnitude == 0 {
                                return Ok(Some(a));
                            }
                            // A symbolic usize may have a target width above this evaluator.
                            if b.left.annotation.resolved.is_some_and(|i| {
                                matches!(types.get(i.ty), Ok(T::Builtin(B::Usize)))
                            }) {
                                return Ok(None);
                            }
                            return Err(type_error("shift count is out of range"));
                        }
                        let count = c.magnitude as u32;
                        if b.operator == O::LeftShift {
                            Integer::new(
                                a.negative,
                                a.magnitude
                                    .checked_mul(1u128 << count)
                                    .ok_or_else(arithmetic_overflow)?,
                            )
                        } else if a.negative {
                            Integer::signed(a.as_signed().ok_or_else(arithmetic_overflow)? >> count)
                        } else {
                            Integer::new(false, a.magnitude >> count)
                        }
                    }
                    _ => return Ok(None),
                }),
                _ => None,
            }
        }
        E::Cast { expr, target_type } => {
            let target = target_type
                .annotation
                .resolved
                .and_then(|t| types.get(t).ok());
            if matches!(target,Some(T::Builtin(b)) if super::value_typing::integer(b).is_some()) {
                match integer(expr, types)? {
                    Some(n) => Some(n),
                    None => floating(expr, types)?
                        .map(WideFloat::truncated)
                        .transpose()?,
                }
            } else {
                None
            }
        }
        _ => None,
    };
    Ok(result)
}

#[derive(Clone, Copy)]
struct WideFloat {
    significand: f64,
    exponent: i32,
}
impl WideFloat {
    fn new(mut significand: f64, mut exponent: i32) -> Self {
        if significand == 0.0 {
            return Self {
                significand: 0.0,
                exponent: 0,
            };
        }
        while significand.abs() >= 2.0 {
            significand *= 0.5;
            exponent += 1;
        }
        while significand.abs() < 1.0 {
            significand *= 2.0;
            exponent -= 1;
        }
        Self {
            significand,
            exponent,
        }
    }
    fn from_f64(value: f64) -> anyhow::Result<Self> {
        if !value.is_finite() {
            return Err(type_error("floating-point value is out of range"));
        }
        Ok(Self::new(value, 0))
    }
    fn operation(self, other: Self, op: O) -> anyhow::Result<Option<Self>> {
        Ok(Some(match op {
            O::Plus | O::Minus => {
                let exponent = self.exponent.max(other.exponent);
                Self::new(
                    self.significand * 2.0f64.powi(self.exponent - exponent)
                        + if op == O::Plus {
                            other.significand
                        } else {
                            -other.significand
                        } * 2.0f64.powi(other.exponent - exponent),
                    exponent,
                )
            }
            O::Star => Self::new(
                self.significand * other.significand,
                self.exponent + other.exponent,
            ),
            O::Slash => {
                if other.significand == 0.0 {
                    return Err(type_error("constant division by zero"));
                }
                Self::new(
                    self.significand / other.significand,
                    self.exponent - other.exponent,
                )
            }
            _ => return Ok(None),
        }))
    }
    fn fits(self, b: &B) -> bool {
        let (exponent, max) = match b {
            B::Float16 => (15, 1.9990234375),
            B::Float32 => (127, 2.0 - 2.0f64.powi(-23)),
            B::Float64 => (1023, 2.0 - 2.0f64.powi(-52)),
            // Precision beyond the tokenizer's f64 payload is not invented.
            B::Float128 => (16383, 2.0),
            _ => return false,
        };
        self.exponent < exponent || (self.exponent == exponent && self.significand.abs() <= max)
    }
    fn truncated(self) -> anyhow::Result<Integer> {
        if self.exponent < 0 || self.significand == 0.0 {
            return Ok(Integer::new(false, 0));
        }
        if self.exponent >= 128 {
            return Err(arithmetic_overflow());
        }
        let significand =
            ((self.significand.abs().to_bits() & ((1u64 << 52) - 1)) | (1u64 << 52)) as u128;
        let magnitude = if self.exponent >= 52 {
            significand << (self.exponent - 52)
        } else {
            significand >> (52 - self.exponent)
        };
        Ok(Integer::new(self.significand.is_sign_negative(), magnitude))
    }
}
fn floating(e: &Expression, types: &SemanticTypes) -> anyhow::Result<Option<WideFloat>> {
    Ok(match &e.kind {
        E::Literal(Literal::Float(v)) => Some(WideFloat::from_f64(*v)?),
        E::Literal(Literal::Integer(v)) => Some(WideFloat::from_f64(*v as f64)?),
        E::UnaryExpression(u) if u.operator == O::Minus => {
            floating(&u.expression, types)?.map(|mut n| {
                n.significand = -n.significand;
                n
            })
        }
        E::BinaryExpresison(b) => match (floating(&b.left, types)?, floating(&b.right, types)?) {
            (Some(a), Some(c)) => a.operation(c, b.operator)?,
            _ => None,
        },
        E::Cast { expr, target_type } => {
            if matches!(target_type.annotation.resolved.and_then(|t| types.get(t).ok()),Some(T::Builtin(b)) if super::value_typing::integer(b).is_some())
            {
                integer(e, types)?
                    .map(|n| {
                        WideFloat::from_f64(if n.negative {
                            -(n.magnitude as f64)
                        } else {
                            n.magnitude as f64
                        })
                    })
                    .transpose()?
            } else {
                floating(expr, types)?
            }
        }
        _ => None,
    })
}
pub(super) fn integer_as_i128(
    e: &Expression,
    types: &SemanticTypes,
) -> anyhow::Result<Option<i128>> {
    integer(e, types)?
        .map(|n| {
            n.as_signed()
                .ok_or_else(|| type_error("integer constant is outside supported bounds"))
        })
        .transpose()
}
pub(super) fn is_zero(e: &Expression, types: &SemanticTypes) -> anyhow::Result<bool> {
    if e.annotation.resolved.is_some_and(|info| matches!(types.get(info.ty),Ok(T::Builtin(b)) if super::value_typing::integer(b).is_some())) {
        return Ok(integer(e,types)?.is_some_and(|n| n.magnitude == 0));
    }
    Ok(floating(e, types)?.is_some_and(|n| n.significand == 0.0))
}
pub(super) fn validate(e: &Expression, ty: TypeId, types: &SemanticTypes) -> anyhow::Result<()> {
    let T::Builtin(b) = types.get(ty)? else {
        return Ok(());
    };
    if super::value_typing::integer(b).is_some() {
        if let Some(n) = integer(e, types)?
            && !n.fits(b)
        {
            return Err(type_error(format!(
                "constant is out of range for {}",
                b.name()
            )));
        }
    } else if matches!(b, B::Float16 | B::Float32 | B::Float64 | B::Float128)
        && let Some(n) = floating(e, types)?
        && !n.fits(b)
    {
        return Err(type_error(format!(
            "constant is out of range for {}",
            b.name()
        )));
    }
    Ok(())
}
pub(super) fn check_cast(
    source: &Expression,
    target: TypeId,
    types: &SemanticTypes,
) -> anyhow::Result<()> {
    let T::Builtin(b) = types.get(target)? else {
        return Ok(());
    };
    if super::value_typing::integer(b).is_some() {
        let n = match integer(source, types)? {
            Some(n) => Some(n),
            None => floating(source, types)?
                .map(WideFloat::truncated)
                .transpose()?,
        };
        if n.is_some_and(|n| !n.fits(b)) {
            return Err(type_error(format!(
                "cast value is out of range for {}",
                b.name()
            )));
        }
    } else if matches!(b, B::Float16 | B::Float32 | B::Float64 | B::Float128)
        && let Some(n) = floating(source, types)?
        && !n.fits(b)
    {
        return Err(type_error(format!(
            "cast value is out of range for {}",
            b.name()
        )));
    }
    Ok(())
}
