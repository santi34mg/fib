//! LLVM scalar instructions with defined wrapping and checked failure semantics.
use super::BackendError;
use crate::ir::{BinaryOp as B, IntegerWidth, IrType, TrapReason, UnaryOp};
use inkwell::{
    FloatPredicate as F, IntPredicate as I,
    builder::Builder,
    module::Module,
    types::AsTypeRef,
    types::{BasicTypeEnum, FloatType},
    values::AsValueRef,
    values::{BasicValueEnum, FloatValue, IntValue},
};
fn error(e: impl ToString) -> BackendError {
    BackendError::Invalid {
        message: e.to_string(),
    }
}

pub(super) fn float_constant<'c>(ty: FloatType<'c>, bits: u128) -> FloatValue<'c> {
    float_constant_ref(ty.get_context(), ty, bits)
}
pub(super) fn trap(
    builder: &Builder<'_>,
    module: &Module<'_>,
    _reason: TrapReason,
) -> Result<(), BackendError> {
    let function = module.get_function("llvm.trap").unwrap_or_else(|| {
        module.add_function(
            "llvm.trap",
            module.get_context().void_type().fn_type(&[], false),
            None,
        )
    });
    builder.build_call(function, &[], "").map_err(error)?;
    builder.build_unreachable().map_err(error)?;
    Ok(())
}
fn guard<'c>(
    builder: &Builder<'c>,
    module: &Module<'c>,
    valid: IntValue<'c>,
    reason: TrapReason,
) -> Result<(), BackendError> {
    let function = builder
        .get_insert_block()
        .and_then(|b| b.get_parent())
        .ok_or_else(|| error("missing scalar guard function"))?;
    let context = module.get_context();
    let next = context.append_basic_block(function, "scalar.ok");
    let fail = context.append_basic_block(function, &format!("trap.{reason:?}"));
    builder
        .build_conditional_branch(valid, next, fail)
        .map_err(error)?;
    builder.position_at_end(fail);
    trap(builder, module, reason)?;
    builder.position_at_end(next);
    Ok(())
}
pub(super) fn unary<'c>(
    builder: &Builder<'c>,
    op: UnaryOp,
    value: BasicValueEnum<'c>,
) -> Result<BasicValueEnum<'c>, BackendError> {
    Ok(match value {
        BasicValueEnum::IntValue(v) => match op {
            UnaryOp::Neg => builder.build_int_neg(v, "neg").map_err(error)?.into(),
            UnaryOp::Not => builder.build_not(v, "not").map_err(error)?.into(),
        },
        BasicValueEnum::FloatValue(v) if op == UnaryOp::Neg => {
            builder.build_float_neg(v, "neg").map_err(error)?.into()
        }
        _ => return Err(error("unsupported scalar unary operation")),
    })
}
pub(super) fn binary<'c>(
    builder: &Builder<'c>,
    module: &Module<'c>,
    op: B,
    ty: &IrType,
    rhs_ty: &IrType,
    a: BasicValueEnum<'c>,
    b: BasicValueEnum<'c>,
) -> Result<BasicValueEnum<'c>, BackendError> {
    if let IrType::Float(_) = ty {
        let (a, b) = (a.into_float_value(), b.into_float_value());
        return Ok(match op {
            B::Add => builder.build_float_add(a, b, "add").map_err(error)?.into(),
            B::Sub => builder.build_float_sub(a, b, "sub").map_err(error)?.into(),
            B::Mul => builder.build_float_mul(a, b, "mul").map_err(error)?.into(),
            B::Div => builder.build_float_div(a, b, "div").map_err(error)?.into(),
            _ => {
                let p = match op {
                    B::Eq => F::OEQ,
                    B::Ne => F::UNE,
                    B::Lt => F::OLT,
                    B::Le => F::OLE,
                    B::Gt => F::OGT,
                    B::Ge => F::OGE,
                    _ => return Err(error("invalid float operation")),
                };
                builder
                    .build_float_compare(p, a, b, "compare")
                    .map_err(error)?
                    .into()
            }
        });
    }
    let (mut a, mut b) = (a.into_int_value(), b.into_int_value());
    let mut division_overflow = None;
    let signed = matches!(ty, IrType::Integer { signed: true, .. });
    if matches!(op, B::Div | B::Rem) {
        let valid = builder
            .build_int_compare(I::NE, b, b.get_type().const_zero(), "nonzero")
            .map_err(error)?;
        guard(builder, module, valid, TrapReason::DivisionByZero)?;
        if signed {
            let width = a.get_type().get_bit_width();
            let bits = 1u128 << (width - 1);
            let min = a
                .get_type()
                .const_int_arbitrary_precision(&[bits as u64, (bits >> 64) as u64]);
            let is_min = builder
                .build_int_compare(I::EQ, a, min, "is.min")
                .map_err(error)?;
            let is_neg_one = builder
                .build_int_compare(I::EQ, b, b.get_type().const_all_ones(), "is.neg.one")
                .map_err(error)?;
            let overflow = builder
                .build_and(is_min, is_neg_one, "division.wrap")
                .map_err(error)?;
            division_overflow = Some((overflow, a));
            // A zero dividend avoids signed division poison even for signed i1,
            // where there is no representable positive-one divisor.
            a = builder
                .build_select(overflow, a.get_type().const_zero(), a, "safe.dividend")
                .map_err(error)?
                .into_int_value();
        }
    }
    if matches!(op, B::Shl | B::Shr) {
        let wide = module.get_context().custom_width_int_type(128);
        let count = if b.get_type() == wide {
            b
        } else if matches!(rhs_ty, IrType::Integer { signed: true, .. }) {
            builder
                .build_int_s_extend(b, wide, "shift.count")
                .map_err(error)?
        } else {
            builder
                .build_int_z_extend(b, wide, "shift.count")
                .map_err(error)?
        };
        let valid = builder
            .build_int_compare(
                I::ULT,
                count,
                wide.const_int(u64::from(a.get_type().get_bit_width()), false),
                "valid.shift",
            )
            .map_err(error)?;
        guard(builder, module, valid, TrapReason::InvalidShift)?;
        b = builder
            .build_int_cast(b, a.get_type(), "shift.width")
            .map_err(error)?;
    }
    let result = match op {
        B::Add => builder.build_int_add(a, b, "add"),
        B::Sub => builder.build_int_sub(a, b, "sub"),
        B::Mul => builder.build_int_mul(a, b, "mul"),
        B::And => builder.build_and(a, b, "and"),
        B::Or => builder.build_or(a, b, "or"),
        B::Xor => builder.build_xor(a, b, "xor"),
        B::Shl => builder.build_left_shift(a, b, "shl"),
        B::Shr => builder.build_right_shift(a, b, signed, "shr"),
        B::Div if signed => builder.build_int_signed_div(a, b, "div"),
        B::Div => builder.build_int_unsigned_div(a, b, "div"),
        B::Rem if signed => builder.build_int_signed_rem(a, b, "rem"),
        B::Rem => builder.build_int_unsigned_rem(a, b, "rem"),
        _ => {
            let p = match (op, signed) {
                (B::Eq, _) => I::EQ,
                (B::Ne, _) => I::NE,
                (B::Lt, true) => I::SLT,
                (B::Lt, false) => I::ULT,
                (B::Le, true) => I::SLE,
                (B::Le, false) => I::ULE,
                (B::Gt, true) => I::SGT,
                (B::Gt, false) => I::UGT,
                (B::Ge, true) => I::SGE,
                (B::Ge, false) => I::UGE,
                _ => return Err(error("invalid scalar binary operation")),
            };
            builder.build_int_compare(p, a, b, "compare")
        }
    }
    .map_err(error)?;
    if op == B::Div
        && let Some((overflow, dividend)) = division_overflow
    {
        return builder
            .build_select(overflow, dividend, result, "wrapped.quotient")
            .map_err(error);
    }
    Ok(result.into())
}

pub(super) fn cast<'c>(
    builder: &Builder<'c>,
    module: &Module<'c>,
    source: &IrType,
    dest: &IrType,
    llvm_dest: BasicTypeEnum<'c>,
    value: BasicValueEnum<'c>,
) -> Result<BasicValueEnum<'c>, BackendError> {
    if source == dest {
        return Ok(value);
    }
    Ok(match (source, dest) {
        (IrType::Integer { signed, .. }, IrType::Integer { .. }) => builder
            .build_int_cast_sign_flag(
                value.into_int_value(),
                llvm_dest.into_int_type(),
                *signed,
                "cast",
            )
            .map_err(error)?
            .into(),
        (IrType::Integer { signed, .. }, IrType::Float(_)) => {
            let (value, ty) = (value.into_int_value(), llvm_dest.into_float_type());
            if *signed {
                builder
                    .build_signed_int_to_float(value, ty, "cast")
                    .map_err(error)?
                    .into()
            } else {
                builder
                    .build_unsigned_int_to_float(value, ty, "cast")
                    .map_err(error)?
                    .into()
            }
        }
        (IrType::Float(_), IrType::Float(_)) => builder
            .build_float_cast(
                value.into_float_value(),
                llvm_dest.into_float_type(),
                "cast",
            )
            .map_err(error)?
            .into(),
        (
            IrType::Float(_),
            IrType::Integer {
                width: IntegerWidth::Bits(width),
                signed,
            },
        ) => {
            let value = value.into_float_value();
            let ty = value.get_type();
            let fw = ty.get_bit_width() as u16;
            let precision = match fw {
                16 => 11,
                32 => 24,
                64 => 53,
                128 => 113,
                _ => unreachable!(),
            };
            // Lower bounds account for truncation toward zero, including (-1,0) for unsigned.
            let lower_exclusive = !signed || *width <= precision;
            let magnitude = if *signed {
                num_bigint::BigUint::from(1u8) << (*width - 1)
            } else {
                num_bigint::BigUint::from(0u8)
            };
            let magnitude = if lower_exclusive {
                magnitude + num_bigint::BigUint::from(1u8)
            } else {
                magnitude
            };
            let lower = format!("-{magnitude}.0");
            let upper =
                (num_bigint::BigUint::from(1u8) << (width - u16::from(*signed))).to_string() + ".0";
            // Bounds may overflow a small float format; infinity remains a correct bound,
            // with a separate finite check to exclude infinities themselves.
            fn bound(text: &str, width: u16) -> u128 {
                use rustc_apfloat::{
                    Float, Round,
                    ieee::{Double, Half, Quad, Single},
                };
                match width {
                    16 => Half::from_str_r(text, Round::NearestTiesToEven)
                        .unwrap()
                        .value
                        .to_bits(),
                    32 => Single::from_str_r(text, Round::NearestTiesToEven)
                        .unwrap()
                        .value
                        .to_bits(),
                    64 => Double::from_str_r(text, Round::NearestTiesToEven)
                        .unwrap()
                        .value
                        .to_bits(),
                    128 => Quad::from_str_r(text, Round::NearestTiesToEven)
                        .unwrap()
                        .value
                        .to_bits(),
                    _ => unreachable!(),
                }
            }
            let context = module.get_context();
            let lo = float_constant_ref(context, ty, bound(&lower, fw));
            let hi = float_constant_ref(context, ty, bound(&upper, fw));
            let lower_ok = builder
                .build_float_compare(
                    if lower_exclusive { F::OGT } else { F::OGE },
                    value,
                    lo,
                    "cast.lower",
                )
                .map_err(error)?;
            let upper_ok = builder
                .build_float_compare(F::OLT, value, hi, "cast.upper")
                .map_err(error)?;
            let finite_lo = float_constant_ref(context, ty, bound("-inf", fw));
            let finite = builder
                .build_float_compare(F::OGT, value, finite_lo, "cast.finite")
                .map_err(error)?;
            let valid = builder
                .build_and(lower_ok, upper_ok, "cast.range")
                .map_err(error)?;
            let valid = builder
                .build_and(valid, finite, "cast.valid")
                .map_err(error)?;
            guard(builder, module, valid, TrapReason::InvalidFloatToInteger)?;
            if *signed {
                builder
                    .build_float_to_signed_int(value, llvm_dest.into_int_type(), "cast")
                    .map_err(error)?
                    .into()
            } else {
                builder
                    .build_float_to_unsigned_int(value, llvm_dest.into_int_type(), "cast")
                    .map_err(error)?
                    .into()
            }
        }
        _ => return Err(error("unsupported numeric cast")),
    })
}
fn float_constant_ref<'c>(
    context: inkwell::context::ContextRef<'c>,
    ty: FloatType<'c>,
    bits: u128,
) -> FloatValue<'c> {
    let integer = context
        .custom_width_int_type(ty.get_bit_width())
        .const_int_arbitrary_precision(&[bits as u64, (bits >> 64) as u64]);
    // SAFETY: both constants belong to this context, have the same bit width,
    // and LLVMConstBitCast returns a floating constant of exactly `ty`.
    unsafe {
        FloatValue::new(inkwell::llvm_sys::core::LLVMConstBitCast(
            integer.as_value_ref(),
            ty.as_type_ref(),
        ))
    }
}
