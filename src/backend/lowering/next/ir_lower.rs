use std::collections::HashMap;

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::context::CodegenCtx;
use super::error::LowerError;
use super::types::map_type_to_llvm;
use crate::frontend::symbol_table::SymbolTable;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::ty::Ty;
use crate::frontend::typed_ast::typed_symbol_id::NameId;
use crate::ir::{self as ir_mod, Temp};

// ---------------------------------------------------------------------------
// IR consumer: `IrProgram -> LLVM` (phase 3, core subset).
//
// The direct `TypedProgram -> LLVM` path above stays until parity. This
// consumer proves the mechanical translation works: each `BasicBlock`
// becomes an LLVM block, each flat `Instruction` becomes 1-3 builder
// calls. Anything outside the core subset returns a descriptive error
// (never silently miscompiles).
// ---------------------------------------------------------------------------

/// Lower an IR program to LLVM IR text (core subset).
pub fn lower_ir(program: ir_mod::Ir, module_name: &str) -> Result<String, LowerError> {
    let ctx = Context::create();
    let module = ctx.create_module(module_name);
    let builder = ctx.create_builder();
    let cctx = CodegenCtx {
        ctx: &ctx,
        module: &module,
        builder: &builder,
    };

    todo!();

    Ok(module.print_to_string().to_string())
}

pub(super) fn operand_ty<'t>(
    op: &ir_mod::Operand<'t>,
    temp_tys: &HashMap<Temp, Ty<'t>>,
    sym_ty: &HashMap<NameId, Ty<'t>>,
) -> Option<Ty<'t>> {
    match op {
        ir_mod::Operand::Temp(t) => temp_tys.get(t).cloned(),
        ir_mod::Operand::Symbol(s) => sym_ty.get(s).cloned(),
        ir_mod::Operand::ConstInt { ty, .. } => Some(ty.clone()),
        ir_mod::Operand::ConstFloat { ty, .. } => Some(ty.clone()),
        ir_mod::Operand::ConstBool(_) => Some(Ty::Builtin(BuiltinType::Boolean)),
        ir_mod::Operand::StringLit(_) => Some(Ty::Builtin(BuiltinType::String)),
        ir_mod::Operand::Null => None,
    }
}

pub(super) fn operand_is_unsigned(
    op: &ir_mod::Operand,
    temp_tys: &HashMap<ir_mod::Temp, Ty>,
) -> bool {
    match op {
        ir_mod::Operand::Temp(t) => temp_tys.get(t).map(ir_mod::ty_is_unsigned).unwrap_or(false),
        ir_mod::Operand::ConstInt { ty, .. } => ir_mod::ty_is_unsigned(ty),
        ir_mod::Operand::ConstFloat { ty, .. } => ir_mod::ty_is_unsigned(ty),
        _ => false,
    }
}

pub(super) fn resolve_operand<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &HashMap<NameId, PointerValue<'ctx>>,
    temps: &HashMap<ir_mod::Temp, BasicValueEnum<'ctx>>,
    sym_ty: &HashMap<NameId, Ty>,
    empty_scope: &SymbolTable,
    op: &ir_mod::Operand,
) -> Result<BasicValueEnum<'ctx>, LowerError> {
    match op {
        ir_mod::Operand::Temp(t) => temps
            .get(t)
            .copied()
            .ok_or_else(|| format!("lower_ir: unknown temp {}", t).into()),
        ir_mod::Operand::Symbol(s) => {
            let ptr = *vars
                .get(s)
                .ok_or_else(|| format!("lower_ir: unknown symbol {}", s))?;
            let ty = sym_ty
                .get(s)
                .ok_or_else(|| format!("lower_ir: no type for symbol {}", s))?;
            let llvm_ty = map_type_to_llvm(ty, ctx.ctx, empty_scope.clone())?;
            Ok(ctx.builder.build_load(llvm_ty, ptr, "irsymload")?)
        }
        ir_mod::Operand::ConstInt { value, ty } => {
            let llvm_ty = map_type_to_llvm(ty, ctx.ctx, empty_scope.clone())?;
            if let BasicTypeEnum::IntType(it) = llvm_ty {
                Ok(it
                    .const_int(*value, !ir_mod::ty_is_unsigned(ty))
                    .as_basic_value_enum())
            } else {
                Err(format!("lower_ir: ConstInt with non-int type {}", ty).into())
            }
        }
        ir_mod::Operand::ConstFloat { value, ty } => {
            let llvm_ty = map_type_to_llvm(ty, ctx.ctx, empty_scope.clone())?;
            if let BasicTypeEnum::FloatType(ft) = llvm_ty {
                Ok(ft.const_float(*value).as_basic_value_enum())
            } else {
                Err(format!("lower_ir: ConstFloat with non-float type {}", ty).into())
            }
        }
        ir_mod::Operand::ConstBool(v) => Ok(ctx
            .ctx
            .bool_type()
            .const_int(*v as u64, false)
            .as_basic_value_enum()),
        ir_mod::Operand::StringLit(s) => Ok(ctx
            .builder
            .build_global_string_ptr(s, "irstr")?
            .as_pointer_value()
            .as_basic_value_enum()),
        ir_mod::Operand::Null => Ok(ctx
            .ctx
            .ptr_type(AddressSpace::default())
            .const_null()
            .as_basic_value_enum()),
    }
}

pub(super) fn build_ir_binary<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    op: ir_mod::BinOp,
    l: BasicValueEnum<'ctx>,
    r: BasicValueEnum<'ctx>,
    ty: &Ty,
) -> Result<BasicValueEnum<'ctx>, LowerError> {
    use ir_mod::BinOp as Op;
    let is_float = ir_mod::ty_is_float(ty);
    let is_unsigned = ir_mod::ty_is_unsigned(ty);
    match op {
        Op::Add => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_add(l.into_float_value(), r.into_float_value(), "irfadd")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_add(l.into_int_value(), r.into_int_value(), "iradd")?
                    .as_basic_value_enum())
            }
        }
        Op::Sub => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_sub(l.into_float_value(), r.into_float_value(), "irfsub")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_sub(l.into_int_value(), r.into_int_value(), "irsub")?
                    .as_basic_value_enum())
            }
        }
        Op::Mul => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_mul(l.into_float_value(), r.into_float_value(), "irfmul")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_mul(l.into_int_value(), r.into_int_value(), "irmul")?
                    .as_basic_value_enum())
            }
        }
        Op::Div => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_div(l.into_float_value(), r.into_float_value(), "irfdiv")?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_unsigned_div(l.into_int_value(), r.into_int_value(), "irudiv")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_signed_div(l.into_int_value(), r.into_int_value(), "irsdiv")?
                    .as_basic_value_enum())
            }
        }
        Op::Rem => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_rem(l.into_float_value(), r.into_float_value(), "irfrem")?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_unsigned_rem(l.into_int_value(), r.into_int_value(), "irurem")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_signed_rem(l.into_int_value(), r.into_int_value(), "irsrem")?
                    .as_basic_value_enum())
            }
        }
        Op::Shl => Ok(ctx
            .builder
            .build_left_shift(l.into_int_value(), r.into_int_value(), "irshl")?
            .as_basic_value_enum()),
        Op::Shr => Ok(ctx
            .builder
            .build_right_shift(
                l.into_int_value(),
                r.into_int_value(),
                !is_unsigned,
                "irshr",
            )?
            .as_basic_value_enum()),
        Op::And => Ok(ctx
            .builder
            .build_and(l.into_int_value(), r.into_int_value(), "irand")?
            .as_basic_value_enum()),
        Op::Or => Ok(ctx
            .builder
            .build_or(l.into_int_value(), r.into_int_value(), "iror")?
            .as_basic_value_enum()),
        Op::Xor => Ok(ctx
            .builder
            .build_xor(l.into_int_value(), r.into_int_value(), "irxor")?
            .as_basic_value_enum()),
        Op::Eq => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::OEQ,
                        l.into_float_value(),
                        r.into_float_value(),
                        "ireq",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        l.into_int_value(),
                        r.into_int_value(),
                        "ireq",
                    )?
                    .as_basic_value_enum())
            }
        }
        Op::Ne => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::ONE,
                        l.into_float_value(),
                        r.into_float_value(),
                        "irne",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::NE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irne",
                    )?
                    .as_basic_value_enum())
            }
        }
        Op::Lt => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::OLT,
                        l.into_float_value(),
                        r.into_float_value(),
                        "irlt",
                    )?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::ULT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irult",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::SLT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irslt",
                    )?
                    .as_basic_value_enum())
            }
        }
        Op::Le => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::OLE,
                        l.into_float_value(),
                        r.into_float_value(),
                        "irle",
                    )?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::ULE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irule",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::SLE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irsle",
                    )?
                    .as_basic_value_enum())
            }
        }
        Op::Gt => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::OGT,
                        l.into_float_value(),
                        r.into_float_value(),
                        "irgt",
                    )?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::UGT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irugt",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::SGT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irsgt",
                    )?
                    .as_basic_value_enum())
            }
        }
        Op::Ge => {
            if is_float {
                Ok(ctx
                    .builder
                    .build_float_compare(
                        FloatPredicate::OGE,
                        l.into_float_value(),
                        r.into_float_value(),
                        "irge",
                    )?
                    .as_basic_value_enum())
            } else if is_unsigned {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::UGE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "iruge",
                    )?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_int_compare(
                        IntPredicate::SGE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "irsge",
                    )?
                    .as_basic_value_enum())
            }
        }
    }
}

/// Lower an IR `Cast` (mirrors the `codegen_expr` cast arms: int->int
/// keys off the source type, int->float off source, float->int off target).
pub(super) fn build_ir_cast<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    src: BasicValueEnum<'ctx>,
    from: &Ty,
    target: &Ty,
    dst_ty: BasicTypeEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, LowerError> {
    use crate::frontend::tokens::builtin::BuiltinType;
    let src_signed = matches!(
        from,
        Ty::Builtin(
            BuiltinType::Int1
                | BuiltinType::Int2
                | BuiltinType::Int4
                | BuiltinType::Int8
                | BuiltinType::Int16
        )
    );
    let tgt_signed = matches!(
        target,
        Ty::Builtin(
            BuiltinType::Int1
                | BuiltinType::Int2
                | BuiltinType::Int4
                | BuiltinType::Int8
                | BuiltinType::Int16
        )
    );
    match (src, dst_ty) {
        (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(it)) => {
            let src_bits = iv.get_type().get_bit_width();
            let dst_bits = it.get_bit_width();
            if src_bits > dst_bits {
                Ok(ctx
                    .builder
                    .build_int_truncate(iv, it, "ircast_trunc")?
                    .as_basic_value_enum())
            } else if src_bits < dst_bits {
                if src_signed {
                    Ok(ctx
                        .builder
                        .build_int_s_extend(iv, it, "ircast_sext")?
                        .as_basic_value_enum())
                } else {
                    Ok(ctx
                        .builder
                        .build_int_z_extend(iv, it, "ircast_zext")?
                        .as_basic_value_enum())
                }
            } else {
                Ok(iv.as_basic_value_enum())
            }
        }
        (BasicValueEnum::IntValue(iv), BasicTypeEnum::PointerType(pt)) => Ok(ctx
            .builder
            .build_int_to_ptr(iv, pt, "ircast_itoptr")?
            .as_basic_value_enum()),
        (BasicValueEnum::PointerValue(pv), BasicTypeEnum::IntType(it)) => Ok(ctx
            .builder
            .build_ptr_to_int(pv, it, "ircast_ptrtoi")?
            .as_basic_value_enum()),
        (BasicValueEnum::PointerValue(pv), BasicTypeEnum::PointerType(_)) => {
            Ok(pv.as_basic_value_enum())
        }
        (BasicValueEnum::IntValue(iv), BasicTypeEnum::FloatType(ft)) => {
            if src_signed {
                Ok(ctx
                    .builder
                    .build_signed_int_to_float(iv, ft, "ircast_sitofp")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_unsigned_int_to_float(iv, ft, "ircast_uitofp")?
                    .as_basic_value_enum())
            }
        }
        (BasicValueEnum::FloatValue(fv), BasicTypeEnum::IntType(it)) => {
            if tgt_signed {
                Ok(ctx
                    .builder
                    .build_float_to_signed_int(fv, it, "ircast_fptosi")?
                    .as_basic_value_enum())
            } else {
                Ok(ctx
                    .builder
                    .build_float_to_unsigned_int(fv, it, "ircast_fptoui")?
                    .as_basic_value_enum())
            }
        }
        (BasicValueEnum::FloatValue(fv), BasicTypeEnum::FloatType(ft)) => Ok(ctx
            .builder
            .build_float_cast(fv, ft, "ircast_fp")?
            .as_basic_value_enum()),
        (src, dst) => Err(format!(
            "lower_ir: unsupported cast from {:?} to {:?}",
            src.get_type(),
            dst
        )
        .into()),
    }
}
