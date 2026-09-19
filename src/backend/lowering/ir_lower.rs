use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicMetadataValueEnum, BasicValue, BasicValueEnum, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::context::{CodegenCtx, coerce_int_to_llvm_type, insert_block};
use super::error::LowerError;
use super::types::map_type_to_llvm;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{SymbolTable, Ty};
use crate::ir as ir_mod;

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
pub fn lower_ir(program: ir_mod::IrProgram, module_name: &str) -> Result<String, LowerError> {
    let ctx = Context::create();
    let module = ctx.create_module(module_name);
    let builder = ctx.create_builder();
    let cctx = CodegenCtx {
        ctx: &ctx,
        module: &module,
        builder: &builder,
    };

    for func in &program.functions {
        lower_ir_function(&cctx, func)?;
    }

    Ok(module.print_to_string().to_string())
}

pub(super) fn lower_ir_function<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    func: &ir_mod::IrFunction,
) -> Result<(), LowerError> {
    use inkwell::module::Linkage;

    // Function type from IR signature (core subset: builtin types only;
    // nominal/qualified types need the type table in phase 2b).
    let empty_scope = SymbolTable::new();
    let param_tys: Vec<BasicMetadataTypeEnum> = func
        .params
        .iter()
        .map(|(_, _, ty)| {
            map_type_to_llvm(ty, ctx.ctx, empty_scope.clone()).map(BasicMetadataTypeEnum::from)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("lower_ir '{}': param type: {}", func.name, e))?;

    let fn_ty = if matches!(func.return_ty, Ty::Builtin(BuiltinType::Void)) {
        ctx.ctx.void_type().fn_type(&param_tys, func.is_variadic)
    } else {
        let ret = map_type_to_llvm(&func.return_ty, ctx.ctx, empty_scope.clone())
            .map_err(|e| format!("lower_ir '{}': return type: {}", func.name, e))?;
        ret.fn_type(&param_tys, func.is_variadic)
    };

    if func.is_extern {
        if ctx.module.get_function(&func.name).is_none() {
            ctx.module
                .add_function(&func.name, fn_ty, Some(Linkage::External));
        }
        return Ok(());
    }

    // No duplicate guard here: the driver deduplicates declarations
    // (`dedupe_declarations`) before lowering, so each function is defined once.
    let function = ctx.module.add_function(&func.name, fn_ty, None);

    // Symbol metadata for loads/stores/coercions.
    let sym_ty: HashMap<ir_mod::SymbolId, Ty> = func
        .symbols
        .iter()
        .map(|(s, _, t)| (*s, t.clone()))
        .chain(func.params.iter().map(|(s, _, t)| (*s, t.clone())))
        .collect();

    // Pre-create one LLVM block per IR block, in order.
    let mut label_blocks: HashMap<ir_mod::Label, BasicBlock<'ctx>> = HashMap::new();
    for block in &func.blocks {
        // IR labels are unique per function (fresh_label counter); LLVM
        // block names just need to be readable.
        let bb = ctx.ctx.append_basic_block(function, "irbb");
        // Duplicate IR labels cannot happen; keep first mapping on repeat.
        label_blocks.entry(block.label).or_insert(bb);
    }
    // Helper: fallthrough successor of IR block i is blocks[i+1].
    let block_order: Vec<ir_mod::Label> = func.blocks.iter().map(|b| b.label).collect();

    // Bind params: alloca + store actual arg (mirrors create_entry_allocas).
    let mut vars: HashMap<ir_mod::SymbolId, PointerValue<'ctx>> = HashMap::new();
    if let Some(first_label) = block_order.first() {
        let first_bb = label_blocks[first_label];
        ctx.builder.position_at_end(first_bb);
    }
    for (idx, (sym, name, ty)) in func.params.iter().enumerate() {
        let llvm_ty = map_type_to_llvm(ty, ctx.ctx, empty_scope.clone())?;
        let alloca = ctx
            .builder
            .build_alloca(llvm_ty, &format!("{}_addr", name))?;
        let param = function
            .get_nth_param(idx as u32)
            .ok_or_else(|| format!("lower_ir '{}': missing LLVM param {}", func.name, idx))?;
        ctx.builder.build_store(alloca, param)?;
        vars.insert(*sym, alloca);
    }
    let param_syms: std::collections::HashSet<ir_mod::SymbolId> =
        func.params.iter().map(|(s, _, _)| *s).collect();

    // Temp values + their Fib types (for unsigned-aware widening).
    let mut temps: HashMap<ir_mod::Temp, BasicValueEnum<'ctx>> = HashMap::new();
    let mut temp_tys: HashMap<ir_mod::Temp, Ty> = HashMap::new();

    for (bi, block) in func.blocks.iter().enumerate() {
        let bb = label_blocks[&block.label];
        // Position after any param allocas already emitted into the first
        // block: only reposition if this block has no instructions yet AND
        // the builder is elsewhere. Simplest correct rule: always position
        // at end of this block before emitting it (allocas above are at the
        // start of block 0; appending after them is fine).
        ctx.builder.position_at_end(bb);
        let next_bb: Option<BasicBlock<'ctx>> = block_order
            .get(bi + 1)
            .and_then(|l| label_blocks.get(l).copied());

        for instr in &block.instrs {
            match instr {
                ir_mod::Instruction::LabelDef(_) => {
                    // Already split into blocks; no code to emit.
                }
                ir_mod::Instruction::Alloca { sym, name, ty } => {
                    if param_syms.contains(sym) && vars.contains_key(sym) {
                        continue; // already bound above
                    }
                    let llvm_ty = map_type_to_llvm(ty, ctx.ctx, empty_scope.clone())
                        .map_err(|e| format!("lower_ir '{}': alloca {}: {}", func.name, name, e))?;
                    let alloca = ctx
                        .builder
                        .build_alloca(llvm_ty, &format!("{}_addr", name))?;
                    vars.insert(*sym, alloca);
                }
                ir_mod::Instruction::Store { dst, src } => {
                    let ptr = *vars.get(dst).ok_or_else(|| {
                        format!("lower_ir '{}': store to unknown {}", func.name, dst)
                    })?;
                    let val = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, src)?;
                    let sym_ty_ref = sym_ty.get(dst).ok_or_else(|| {
                        format!("lower_ir '{}': store: no type for {}", func.name, dst)
                    })?;
                    let target_ty = map_type_to_llvm(sym_ty_ref, ctx.ctx, empty_scope.clone())?;
                    let src_unsigned = operand_is_unsigned(src, &temp_tys);
                    let val = coerce_int_to_llvm_type(ctx, val, target_ty, src_unsigned)?;
                    ctx.builder.build_store(ptr, val)?;
                }
                ir_mod::Instruction::Load { dst, src } => {
                    let ptr = *vars.get(src).ok_or_else(|| {
                        format!("lower_ir '{}': load of unknown {}", func.name, src)
                    })?;
                    let sym_ty_ref = sym_ty.get(src).ok_or_else(|| {
                        format!("lower_ir '{}': load: no type for {}", func.name, src)
                    })?;
                    let llvm_ty = map_type_to_llvm(sym_ty_ref, ctx.ctx, empty_scope.clone())?;
                    let loaded = ctx.builder.build_load(llvm_ty, ptr, "irload")?;
                    temps.insert(*dst, loaded);
                    temp_tys.insert(*dst, sym_ty_ref.clone());
                }
                ir_mod::Instruction::Copy { dst, src } => {
                    let val = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, src)?;
                    temps.insert(*dst, val);
                    if let Some(ty) = operand_ty(src, &temp_tys, &sym_ty) {
                        temp_tys.insert(*dst, ty);
                    }
                }
                ir_mod::Instruction::Binary {
                    dst,
                    op,
                    lhs,
                    rhs,
                    ty,
                } => {
                    let l = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, lhs)?;
                    let r = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, rhs)?;
                    let val = build_ir_binary(ctx, *op, l, r, ty)?;
                    temps.insert(*dst, val);
                    // Comparisons produce `@bool`; arithmetic produces the
                    // operand type.
                    use ir_mod::BinOp as CmpOp;
                    let is_cmp = matches!(
                        op,
                        CmpOp::Eq | CmpOp::Ne | CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge
                    );
                    if is_cmp {
                        temp_tys.insert(*dst, Ty::Builtin(BuiltinType::Boolean));
                    } else {
                        temp_tys.insert(*dst, ty.clone());
                    }
                }
                ir_mod::Instruction::Cast {
                    dst,
                    src,
                    from,
                    target,
                } => {
                    let v = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, src)?;
                    let dst_ty = map_type_to_llvm(target, ctx.ctx, empty_scope.clone())
                        .map_err(|e| format!("lower_ir '{}': cast target: {}", func.name, e))?;
                    let val = build_ir_cast(ctx, v, from, target, dst_ty)?;
                    temps.insert(*dst, val);
                    temp_tys.insert(*dst, target.clone());
                }
                ir_mod::Instruction::Unary { dst, op, src, ty } => {
                    let v = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, src)?;
                    let val = match op {
                        ir_mod::UnOp::Not => ctx
                            .builder
                            .build_not(v.into_int_value(), "irnot")?
                            .as_basic_value_enum(),
                        ir_mod::UnOp::Neg => {
                            if ir_mod::ty_is_float(ty) {
                                ctx.builder
                                    .build_float_neg(v.into_float_value(), "irneg")?
                                    .as_basic_value_enum()
                            } else {
                                ctx.builder
                                    .build_int_neg(v.into_int_value(), "irneg")?
                                    .as_basic_value_enum()
                            }
                        }
                    };
                    temps.insert(*dst, val);
                    temp_tys.insert(*dst, ty.clone());
                }
                ir_mod::Instruction::Call {
                    dst,
                    callee,
                    args,
                    ret_ty,
                    ..
                } => {
                    let mut arg_vals = Vec::with_capacity(args.len());
                    for a in args {
                        arg_vals.push(resolve_operand(
                            ctx,
                            &vars,
                            &temps,
                            &sym_ty,
                            &empty_scope,
                            a,
                        )?);
                    }
                    let fnval = match ctx.module.get_function(callee) {
                        Some(f) => f,
                        None => {
                            let param_types: Vec<BasicMetadataTypeEnum> =
                                arg_vals.iter().map(|v| v.get_type().into()).collect();
                            let declared = if matches!(ret_ty, Ty::Builtin(BuiltinType::Void)) {
                                ctx.ctx.void_type().fn_type(&param_types, false)
                            } else {
                                let ret_llvm =
                                    map_type_to_llvm(ret_ty, ctx.ctx, empty_scope.clone())?;
                                ret_llvm.fn_type(&param_types, false)
                            };
                            ctx.module.add_function(callee, declared, None)
                        }
                    };
                    let md_args: Vec<BasicMetadataValueEnum> =
                        arg_vals.into_iter().map(|v| v.into()).collect();
                    let call_site = ctx.builder.build_call(fnval, &md_args, "ircall")?;
                    match call_site.try_as_basic_value() {
                        inkwell::values::ValueKind::Basic(v) => {
                            if let Some(d) = dst {
                                temps.insert(*d, v);
                                temp_tys.insert(*d, ret_ty.clone());
                            }
                        }
                        inkwell::values::ValueKind::Instruction(_) => {}
                    }
                }
                ir_mod::Instruction::Return { values } => {
                    if values.is_empty() {
                        ctx.builder.build_return(None)?;
                    } else if values.len() == 1 {
                        let v =
                            resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, &values[0])?;
                        // Coerce single return to the declared type.
                        let declared = function.get_type().get_return_type();
                        let v = match declared {
                            Some(t) => coerce_int_to_llvm_type(
                                ctx,
                                v,
                                t,
                                operand_is_unsigned(&values[0], &temp_tys),
                            )?,
                            None => v,
                        };
                        ctx.builder.build_return(Some(&v))?;
                    } else {
                        return Err(format!(
                            "lower_ir '{}': multi-return needs tuple support (phase 2b)",
                            func.name
                        )
                        .into());
                    }
                }
                ir_mod::Instruction::IfGoto { cond, target } => {
                    let c = resolve_operand(ctx, &vars, &temps, &sym_ty, &empty_scope, cond)?;
                    let then_bb = *label_blocks.get(target).ok_or_else(|| {
                        format!("lower_ir '{}': unknown label {}", func.name, target)
                    })?;
                    let else_bb = next_bb.ok_or_else(|| {
                        format!(
                            "lower_ir '{}': IfGoto as last block has no fallthrough",
                            func.name
                        )
                    })?;
                    ctx.builder
                        .build_conditional_branch(c.into_int_value(), then_bb, else_bb)?;
                }
                ir_mod::Instruction::Goto { target } => {
                    let bb = *label_blocks.get(target).ok_or_else(|| {
                        format!("lower_ir '{}': unknown label {}", func.name, target)
                    })?;
                    ctx.builder.build_unconditional_branch(bb)?;
                }
                ir_mod::Instruction::Unreachable => {
                    ctx.builder.build_unreachable()?;
                }
                other => {
                    return Err(format!(
                        "lower_ir '{}': {:?} needs phase 2b",
                        func.name,
                        std::mem::discriminant(other)
                    )
                    .into());
                }
            }
        }
        // Fallthrough: if this IR block has no LLVM terminator, branch to
        // the next block (matches direct lowering's merge positioning).
        let cur = insert_block(ctx, "ir fallthrough")?;
        if cur.get_terminator().is_none()
            && let Some(nb) = next_bb
        {
            ctx.builder.position_at_end(cur);
            ctx.builder.build_unconditional_branch(nb)?;
        }
    }

    // Seal unterminated blocks like the direct path does.
    let mut bb_opt = function.get_first_basic_block();
    while let Some(bb) = bb_opt {
        if bb.get_terminator().is_none() {
            ctx.builder.position_at_end(bb);
            ctx.builder.build_unreachable()?;
        }
        bb_opt = bb.get_next_basic_block();
    }
    Ok(())
}

pub(super) fn operand_ty(
    op: &ir_mod::Operand,
    temp_tys: &HashMap<ir_mod::Temp, Ty>,
    sym_ty: &HashMap<ir_mod::SymbolId, Ty>,
) -> Option<Ty> {
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
    vars: &HashMap<ir_mod::SymbolId, PointerValue<'ctx>>,
    temps: &HashMap<ir_mod::Temp, BasicValueEnum<'ctx>>,
    sym_ty: &HashMap<ir_mod::SymbolId, Ty>,
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
