use std::collections::HashMap;
use std::error::Error;

use inkwell::types::{BasicMetadataTypeEnum, BasicTypeEnum, FunctionType};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PointerValue,
};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};

use super::context::{CodegenCtx, coerce_int_to_llvm_type, insert_block, parent_function};
use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::builtin::{BuiltinFunction, BuiltinType};
use crate::frontend::typed_ast::{
    BinOp, LogicalOp, SymbolTable, Ty, TypedExpr, TypedExprKind, TypedSymbol,
};

/// Compute a pointer to the lvalue represented by `expr`. Supports identifiers,
/// field access chains, index access, and dereferences. Used by AddressOf and
/// by assignment lowering.
pub(super) fn compute_lvalue_ptr<'ctx, 'r>(
    ctx: &'r CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut SymbolTable,
    expr: &TypedExpr,
) -> Result<PointerValue<'ctx>, Box<dyn Error>> {
    match &expr.expression {
        TypedExprKind::Identifier(name) => {
            let ptr = *vars
                .get(name)
                .ok_or_else(|| format!("compute_lvalue_ptr: no alloca for identifier {}", name))?;
            Ok(ptr)
        }
        TypedExprKind::Deref(inner) => {
            let v = codegen_expr(ctx, vars, current_scope, inner)?;
            Ok(v.into_pointer_value())
        }
        TypedExprKind::FieldAccess {
            object,
            field: _,
            field_index,
        } => {
            let base_ptr = compute_lvalue_ptr(ctx, vars, current_scope, object)?;
            let struct_ty =
                map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = struct_ty else {
                return Err("compute_lvalue_ptr: FieldAccess on non-struct type".into());
            };
            let gep =
                ctx.builder
                    .build_struct_gep(st, base_ptr, *field_index as u32, "fieldptr")?;
            Ok(gep)
        }
        TypedExprKind::IndexAccess { object, index } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let elem_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            match &object.inferred_type {
                Ty::Array { .. } => {
                    let arr_ty =
                        map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
                    let base_ptr = compute_lvalue_ptr(ctx, vars, current_scope, object)?;
                    let i32_zero = ctx.ctx.i32_type().const_int(0, false);
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            arr_ty,
                            base_ptr,
                            &[i32_zero, idx_val.into_int_value()],
                            "arr_idx_ptr",
                        )?
                    };
                    Ok(gep)
                }
                _ => {
                    let ptr_val = codegen_expr(ctx, vars, current_scope, object)?;
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            elem_ty,
                            ptr_val.into_pointer_value(),
                            &[idx_val.into_int_value()],
                            "idx_ptr",
                        )?
                    };
                    Ok(gep)
                }
            }
        }
        _ => Err("compute_lvalue_ptr: not an lvalue expression".into()),
    }
}
pub(super) fn build_tuple_value<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut SymbolTable,
    exprs: &[TypedExpr],
    tuple_ty: inkwell::types::StructType<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    if tuple_ty.count_fields() as usize != exprs.len() {
        return Err(format!(
            "return arity mismatch: function expects {} value(s), return has {} expression(s)",
            tuple_ty.count_fields(),
            exprs.len()
        )
        .into());
    }
    let alloca = ctx.builder.build_alloca(tuple_ty, "multirettmp")?;
    for (idx, expr) in exprs.iter().enumerate() {
        let raw_val = codegen_expr(ctx, vars, current_scope, expr)?;
        let field_ty = tuple_ty
            .get_field_type_at_index(idx as u32)
            .ok_or_else(|| format!("tuple return has no field at index {}", idx))?;
        let val = coerce_int_to_llvm_type(
            ctx,
            raw_val,
            field_ty,
            crate::ir::ty_is_unsigned(&expr.inferred_type),
        )?;
        let gep = ctx
            .builder
            .build_struct_gep(tuple_ty, alloca, idx as u32, "multiretfield")?;
        ctx.builder.build_store(gep, val)?;
    }
    Ok(ctx.builder.build_load(tuple_ty, alloca, "multiretload")?)
}

pub(super) fn store_lvalue<'ctx, 'r>(
    ctx: &'r CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut SymbolTable,
    target: &TypedExpr,
    value: BasicValueEnum<'ctx>,
    is_unsigned: bool,
) -> Result<(), Box<dyn Error>> {
    let ptr = compute_lvalue_ptr(ctx, vars, current_scope, target)?;
    let target_ty = map_type_to_llvm(&target.inferred_type, ctx.ctx, current_scope.clone())?;
    let value = coerce_int_to_llvm_type(ctx, value, target_ty, is_unsigned)?;
    ctx.builder.build_store(ptr, value)?;
    Ok(())
}

pub(super) fn unpack_tuple_value<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    tuple_value: BasicValueEnum<'ctx>,
    expected_count: usize,
) -> Result<Vec<BasicValueEnum<'ctx>>, Box<dyn Error>> {
    let BasicValueEnum::StructValue(struct_value) = tuple_value else {
        return Err("multi-assignment expected a multiple-return tuple value".into());
    };
    let tuple_ty = struct_value.get_type();
    if tuple_ty.count_fields() as usize != expected_count {
        return Err(format!(
            "multi-assignment arity mismatch: tuple has {} value(s), target list has {}",
            tuple_ty.count_fields(),
            expected_count
        )
        .into());
    }
    let alloca = ctx.builder.build_alloca(tuple_ty, "multiassigntmp")?;
    ctx.builder.build_store(alloca, struct_value)?;
    let mut values = Vec::new();
    for idx in 0..expected_count {
        let field_ty = tuple_ty
            .get_field_type_at_index(idx as u32)
            .ok_or_else(|| format!("tuple value has no field at index {}", idx))?;
        let gep = ctx
            .builder
            .build_struct_gep(tuple_ty, alloca, idx as u32, "multiassignfield")?;
        values.push(ctx.builder.build_load(field_ty, gep, "multiassignload")?);
    }
    Ok(values)
}

/// Look up a function by name, declaring it with external linkage if it does
/// not already exist. Used to bind libc functions (`strlen`, `malloc`, ...)
/// that back the string builtins.
pub(super) fn get_or_declare<'ctx>(
    ctx: &CodegenCtx<'ctx, '_>,
    name: &str,
    fn_ty: FunctionType<'ctx>,
) -> FunctionValue<'ctx> {
    match ctx.module.get_function(name) {
        Some(f) => f,
        None => ctx
            .module
            .add_function(name, fn_ty, Some(inkwell::module::Linkage::External)),
    }
}

/// Extract the basic return value of a call site, erroring if the call is void.
pub(super) fn call_result<'ctx>(
    cs: inkwell::values::CallSiteValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match cs.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(v),
        inkwell::values::ValueKind::Instruction(_) => {
            Err("expected a return value from libc call".into())
        }
    }
}

pub(super) fn codegen_expr<'ctx, 'r>(
    ctx: &'r CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut SymbolTable,
    expr: &TypedExpr,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match &expr.expression {
        TypedExprKind::LiteralInt { value } => {
            if let BasicTypeEnum::IntType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx.ctx, SymbolTable::new())?
            {
                let sign_extend = !matches!(
                    expr.inferred_type,
                    Ty::Builtin(
                        BuiltinType::UInt1
                        | BuiltinType::UInt2
                        | BuiltinType::UInt4
                        | BuiltinType::UInt8
                        | BuiltinType::UInt16
                    )
                );
                Ok(ty.const_int(*value, sign_extend).as_basic_value_enum())
            } else {
                Err(format!(
                    "codegen_expr: integer literal has non-int type {}",
                    expr.inferred_type
                )
                .into())
            }
        }
        TypedExprKind::LiteralFloat { value } => {
            if let BasicTypeEnum::FloatType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx.ctx, SymbolTable::new())?
            {
                Ok(ty.const_float(*value).as_basic_value_enum())
            } else {
                Err(format!(
                    "codegen_expr: float literal has non-float type {}",
                    expr.inferred_type
                )
                .into())
            }
        }
        TypedExprKind::LiteralBool(b) => Ok(ctx.ctx
            .bool_type()
            .const_int(*b as u64, false)
            .as_basic_value_enum()),
        TypedExprKind::LiteralString { value } => {
            let ptr = ctx.builder.build_global_string_ptr(value, "str")?;
            Ok(ptr.as_pointer_value().as_basic_value_enum())
        }
        TypedExprKind::Identifier(name) => {
            let ty = if let TypedSymbol::Binding(var) = current_scope
                .lookup(name)
                .ok_or_else(|| format!("didnt find type for name {}", name))?
            {
                map_type_to_llvm(&var.ty, ctx.ctx, current_scope.clone())?
            } else {
                return Err(format!("codegen_expr: {} is not a variable", name).into());
            };
            let ptr = vars
                .get(name)
                .ok_or_else(|| format!("codegen_expr: didnt find ptr for name {}", name))?;
            let load = ctx.builder.build_load(ty, *ptr, &format!("load_{}", name))?;
            Ok(load)
        }
        TypedExprKind::Null => {
            Ok(ctx.ctx.ptr_type(AddressSpace::default()).const_null().as_basic_value_enum())
        }
        TypedExprKind::ShortCircuit {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, vars, current_scope, left)?;
            // `&&` / `||` short-circuit: only evaluate the RHS when the LHS
            // doesn't already decide the result.
            let func = parent_function(ctx, "short-circuit")?;
            let lhs_bb = insert_block(ctx, "short-circuit lhs")?;
            let rhs_bb = ctx.ctx.append_basic_block(func, "sc_rhs");
            let merge_bb = ctx.ctx.append_basic_block(func, "sc_merge");
            let is_and = matches!(operator, LogicalOp::And);
                if is_and {
                    ctx.builder
                        .build_conditional_branch(l.into_int_value(), rhs_bb, merge_bb)?;
                } else {
                    ctx.builder
                        .build_conditional_branch(l.into_int_value(), merge_bb, rhs_bb)?;
                }
                ctx.builder.position_at_end(rhs_bb);
                let r = codegen_expr(ctx, vars, current_scope, right)?;
                // RHS evaluation may itself have produced new blocks.
                let rhs_end_bb = insert_block(ctx, "short-circuit rhs end")?;
                ctx.builder.build_unconditional_branch(merge_bb)?;
                ctx.builder.position_at_end(merge_bb);
                let phi = ctx.builder.build_phi(ctx.ctx.bool_type(), "sctmp")?;
                let short_val = ctx
                    .ctx
                    .bool_type()
                    .const_int(if is_and { 0 } else { 1 }, false);
                phi.add_incoming(&[
                    (&short_val, lhs_bb),
                    (&r.into_int_value(), rhs_end_bb),
                ]);
                Ok(phi.as_basic_value())
            }
        TypedExprKind::Binary {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, vars, current_scope, left)?;
            let r = codegen_expr(ctx, vars, current_scope, right)?;
            let is_float = matches!(
                left.inferred_type,
                Ty::Builtin(
                    BuiltinType::Float2
                    | BuiltinType::Float4
                    | BuiltinType::Float8
                    | BuiltinType::Float16
                )
            );
            let is_unsigned = matches!(
                left.inferred_type,
                Ty::Builtin(
                    BuiltinType::UInt1
                    | BuiltinType::UInt2
                    | BuiltinType::UInt4
                    | BuiltinType::UInt8
                    | BuiltinType::UInt16
                )
            );
            match operator {
                BinOp::Add => {
                    if is_float {
                        Ok(ctx.builder.build_float_add(l.into_float_value(), r.into_float_value(), "faddtmp")?.as_basic_value_enum())
                    } else if let Ty::Pointer(inner_ty) = &left.inferred_type {
                        let elem_ty = map_type_to_llvm(inner_ty, ctx.ctx, current_scope.clone())?;
                        let gep = unsafe {
                            ctx.builder.build_gep(
                                elem_ty,
                                l.into_pointer_value(),
                                &[r.into_int_value()],
                                "ptr_add",
                            )?
                        };
                        Ok(gep.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder
                            .build_int_add(l.into_int_value(), r.into_int_value(), "addtmp")?
                            .as_basic_value_enum())
                    }
                }
                BinOp::Sub => {
                    if is_float {
                        Ok(ctx.builder.build_float_sub(l.into_float_value(), r.into_float_value(), "fsubtmp")?.as_basic_value_enum())
                    } else if let Ty::Pointer(inner_ty) = &left.inferred_type {
                        let elem_ty = map_type_to_llvm(inner_ty, ctx.ctx, current_scope.clone())?;
                        let neg_idx = ctx.builder.build_int_neg(r.into_int_value(), "neg_idx")?;
                        let gep = unsafe {
                            ctx.builder.build_gep(
                                elem_ty,
                                l.into_pointer_value(),
                                &[neg_idx],
                                "ptr_sub",
                            )?
                        };
                        Ok(gep.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder
                            .build_int_sub(l.into_int_value(), r.into_int_value(), "subtmp")?
                            .as_basic_value_enum())
                    }
                }
                BinOp::Mul => {
                    if is_float {
                        Ok(ctx.builder.build_float_mul(l.into_float_value(), r.into_float_value(), "fmultmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Div => {
                    if is_float {
                        Ok(ctx.builder.build_float_div(l.into_float_value(), r.into_float_value(), "fdivtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_unsigned_div(l.into_int_value(), r.into_int_value(), "udivtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Rem => {
                    if is_float {
                        Ok(ctx.builder.build_float_rem(l.into_float_value(), r.into_float_value(), "fremtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_unsigned_rem(l.into_int_value(), r.into_int_value(), "uremtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_signed_rem(l.into_int_value(), r.into_int_value(), "remtmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Gt => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OGT, l.into_float_value(), r.into_float_value(), "fgttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::UGT, l.into_int_value(), r.into_int_value(), "ugttmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SGT, l.into_int_value(), r.into_int_value(), "gttmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Ge => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OGE, l.into_float_value(), r.into_float_value(), "fgetmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::UGE, l.into_int_value(), r.into_int_value(), "ugetmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SGE, l.into_int_value(), r.into_int_value(), "getmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Lt => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OLT, l.into_float_value(), r.into_float_value(), "flttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::ULT, l.into_int_value(), r.into_int_value(), "ulttmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SLT, l.into_int_value(), r.into_int_value(), "lttmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Le => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OLE, l.into_float_value(), r.into_float_value(), "fletmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::ULE, l.into_int_value(), r.into_int_value(), "uletmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SLE, l.into_int_value(), r.into_int_value(), "letmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Eq => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OEQ, l.into_float_value(), r.into_float_value(), "feqtmp")?.as_basic_value_enum())
                    } else if l.is_pointer_value() {
                        Ok(ctx.builder.build_int_compare(IntPredicate::EQ, l.into_pointer_value(), r.into_pointer_value(), "peqtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eqtmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Ne => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::ONE, l.into_float_value(), r.into_float_value(), "fnetmp")?.as_basic_value_enum())
                    } else if l.is_pointer_value() {
                        Ok(ctx.builder.build_int_compare(IntPredicate::NE, l.into_pointer_value(), r.into_pointer_value(), "pnetmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "netmp")?.as_basic_value_enum())
                    }
                }
                BinOp::Shl => Ok(ctx.builder
                    .build_left_shift(l.into_int_value(), r.into_int_value(), "shltmp")?
                    .as_basic_value_enum()),
                BinOp::Shr => Ok(ctx.builder
                    .build_right_shift(l.into_int_value(), r.into_int_value(), !is_unsigned, "shrtmp")?
                    .as_basic_value_enum()),
                BinOp::And => Ok(ctx.builder
                    .build_and(l.into_int_value(), r.into_int_value(), "bandtmp")?
                    .as_basic_value_enum()),
                BinOp::Or => Ok(ctx.builder
                    .build_or(l.into_int_value(), r.into_int_value(), "bortmp")?
                    .as_basic_value_enum()),
                BinOp::Xor => Ok(ctx.builder
                    .build_xor(l.into_int_value(), r.into_int_value(), "xortmp")?
                    .as_basic_value_enum()),
            }
        }
        TypedExprKind::Call { callee, args } => {
            let mut arg_values = Vec::new();
            for a in args.iter() {
                let av = codegen_expr(ctx, vars, current_scope, a)?;
                arg_values.push(av);
            }
            // Lookup function; if not declared yet, auto-declare it as an external
            // function using the argument types observed at this call site.
            let fnval = match ctx.module.get_function(&callee.value) {
                Some(f) => f,
                None => {
                    let param_types: Vec<BasicMetadataTypeEnum> =
                        arg_values.iter().map(|v| v.get_type().into()).collect();
                    let fn_ty = if let Ty::Builtin(BuiltinType::Void) = &expr.inferred_type {
                        ctx.ctx.void_type().fn_type(&param_types, false)
                    } else {
                        let ret_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
                        match ret_ty {
                            BasicTypeEnum::IntType(it) => it.fn_type(&param_types, false),
                            BasicTypeEnum::PointerType(pt) => pt.fn_type(&param_types, false),
                            BasicTypeEnum::FloatType(ft) => ft.fn_type(&param_types, false),
                            BasicTypeEnum::StructType(st) => st.fn_type(&param_types, false),
                            other => return Err(format!("unsupported return type for auto-declared function: {:?}", other).into()),
                        }
                    };
                    ctx.module.add_function(&callee.value, fn_ty, None)
                }
            };
            let md_args: Vec<BasicMetadataValueEnum> =
                arg_values.into_iter().map(|v| v.into()).collect();
            let call_site = ctx.builder.build_call(fnval, &md_args, "calltmp")?;
            match call_site.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => Ok(v),
                inkwell::values::ValueKind::Instruction(_) => {
                    // void return — return a dummy i32 zero (the value won't be used)
                    Ok(ctx.ctx.i32_type().const_int(0, false).as_basic_value_enum())
                }
            }
        }
        TypedExprKind::BuiltinCall { builtin, args } => {
            let ptr_ty = ctx.ctx.ptr_type(AddressSpace::default());
            let i64_ty = ctx.ctx.i64_type();
            let mut arg_vals = Vec::with_capacity(args.len());
            for a in args.iter() {
                arg_vals.push(codegen_expr(ctx, vars, current_scope, a)?);
            }
            match builtin {
                // strlen(s) -> i64
                BuiltinFunction::StrLen => {
                    let strlen =
                        get_or_declare(ctx, "strlen", i64_ty.fn_type(&[ptr_ty.into()], false));
                    let cs = ctx
                        .builder
                        .build_call(strlen, &[arg_vals[0].into()], "strlen")?;
                    call_result(cs)
                }
                // strcmp(a, b) == 0 -> i1
                BuiltinFunction::StrEq => {
                    let i32_ty = ctx.ctx.i32_type();
                    let strcmp = get_or_declare(
                        ctx,
                        "strcmp",
                        i32_ty.fn_type(&[ptr_ty.into(), ptr_ty.into()], false),
                    );
                    let cs = ctx.builder.build_call(
                        strcmp,
                        &[arg_vals[0].into(), arg_vals[1].into()],
                        "strcmp",
                    )?;
                    let res = call_result(cs)?.into_int_value();
                    let eq = ctx.builder.build_int_compare(
                        IntPredicate::EQ,
                        res,
                        i32_ty.const_zero(),
                        "streq",
                    )?;
                    Ok(eq.as_basic_value_enum())
                }
                // malloc(len(a)+len(b)+1); copy a, then b incl. its NUL -> ptr
                BuiltinFunction::Concat => {
                    let strlen =
                        get_or_declare(ctx, "strlen", i64_ty.fn_type(&[ptr_ty.into()], false));
                    let malloc =
                        get_or_declare(ctx, "malloc", ptr_ty.fn_type(&[i64_ty.into()], false));
                    let memcpy = get_or_declare(
                        ctx,
                        "memcpy",
                        ptr_ty.fn_type(&[ptr_ty.into(), ptr_ty.into(), i64_ty.into()], false),
                    );
                    let a = arg_vals[0].into_pointer_value();
                    let b = arg_vals[1].into_pointer_value();
                    let la = call_result(ctx.builder.build_call(strlen, &[a.into()], "la")?)?
                        .into_int_value();
                    let lb = call_result(ctx.builder.build_call(strlen, &[b.into()], "lb")?)?
                        .into_int_value();
                    let one = i64_ty.const_int(1, false);
                    let sum = ctx.builder.build_int_add(la, lb, "lsum")?;
                    let total = ctx.builder.build_int_add(sum, one, "ltotal")?;
                    let buf = call_result(ctx.builder.build_call(malloc, &[total.into()], "buf")?)?
                        .into_pointer_value();
                    // memcpy(buf, a, la)
                    ctx.builder
                        .build_call(memcpy, &[buf.into(), a.into(), la.into()], "cpa")?;
                    // dst = buf + la
                    let dst = unsafe {
                        ctx.builder
                            .build_gep(ctx.ctx.i8_type(), buf, &[la], "dst")?
                    };
                    // memcpy(dst, b, lb + 1) — copies b's trailing NUL terminator too
                    let lb1 = ctx.builder.build_int_add(lb, one, "lb1")?;
                    ctx.builder
                        .build_call(memcpy, &[dst.into(), b.into(), lb1.into()], "cpb")?;
                    Ok(buf.as_basic_value_enum())
                }
            }
        }
        TypedExprKind::FieldAccess { object, field: _, field_index } => {
            // We need the pointer to the struct object, then GEP into it.
            // The object expression should be an Identifier whose alloca we can find.
            let struct_ptr = match &object.expression {
                TypedExprKind::Identifier(name) => {
                    *vars.get(name).ok_or_else(|| {
                        format!("codegen_expr: no alloca for struct identifier {}", name)
                    })?
                }
                _ => {
                    // For more complex cases (e.g. nested field access), codegen
                    // the object and store it to a temporary alloca first.
                    let obj_val = codegen_expr(ctx, vars, current_scope, object)?;
                    let tmp = ctx.builder.build_alloca(obj_val.get_type(), "struct_tmp")?;
                    ctx.builder.build_store(tmp, obj_val)?;
                    tmp
                }
            };
            let struct_ty = map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = struct_ty else {
                return Err("codegen_expr: FieldAccess on non-struct type".to_string().into());
            };
            let gep = ctx.builder.build_struct_gep(st, struct_ptr, *field_index as u32, "fieldptr")?;
            let field_ty = st
                .get_field_type_at_index(*field_index as u32)
                .ok_or_else(|| format!("codegen_expr: no field at index {}", field_index))?;
            let loaded = ctx.builder.build_load(field_ty, gep, "fieldload")?;
            Ok(loaded)
        }
        TypedExprKind::StructConstruct { type_name: _, fields } => {
            // Allocate a struct, fill each field, then load the whole value.
            let struct_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = struct_ty else {
                return Err("codegen_expr: StructConstruct on non-struct type".to_string().into());
            };
            let alloca = ctx.builder.build_alloca(st, "structtmp")?;
            for (idx, (_, field_expr)) in fields.iter().enumerate() {
                let val = codegen_expr(ctx, vars, current_scope, field_expr)?;
                let gep = ctx.builder.build_struct_gep(st, alloca, idx as u32, "fieldptr")?;
                ctx.builder.build_store(gep, val)?;
            }
            let loaded = ctx.builder.build_load(st, alloca, "structload")?;
            Ok(loaded)
        }
        TypedExprKind::AddressOf(inner) => {
            let ptr = compute_lvalue_ptr(ctx, vars, current_scope, inner)?;
            Ok(ptr.as_basic_value_enum())
        }
        TypedExprKind::Deref(inner) => {
            // Codegen the pointer expression, then load through it.
            let ptr_val = codegen_expr(ctx, vars, current_scope, inner)?;
            let pointee_llvm_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let loaded = ctx.builder.build_load(pointee_llvm_ty, ptr_val.into_pointer_value(), "deref")?;
            Ok(loaded)
        }
        TypedExprKind::Cast { expr: inner, target_type } => {
            let src = codegen_expr(ctx, vars, current_scope, inner)?;
            let dst_ty = map_type_to_llvm(target_type, ctx.ctx, current_scope.clone())?;
            match (src, dst_ty) {
                // int -> int
                (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(it)) => {
                    let src_bits = iv.get_type().get_bit_width();
                    let dst_bits = it.get_bit_width();
                    if src_bits > dst_bits {
                        Ok(ctx.builder.build_int_truncate(iv, it, "cast_trunc")?.as_basic_value_enum())
                    } else if src_bits < dst_bits {
                        // Use signed extend for signed types, zero-extend otherwise
                        let signed = matches!(
                            inner.inferred_type,
                            Ty::Builtin(
                                BuiltinType::Int1
                                | BuiltinType::Int2
                                | BuiltinType::Int4
                                | BuiltinType::Int8
                                | BuiltinType::Int16
                            )
                        );
                        if signed {
                            Ok(ctx.builder.build_int_s_extend(iv, it, "cast_sext")?.as_basic_value_enum())
                        } else {
                            Ok(ctx.builder.build_int_z_extend(iv, it, "cast_zext")?.as_basic_value_enum())
                        }
                    } else {
                        Ok(iv.as_basic_value_enum())
                    }
                }
                // int -> ptr
                (BasicValueEnum::IntValue(iv), BasicTypeEnum::PointerType(pt)) => {
                    Ok(ctx.builder.build_int_to_ptr(iv, pt, "cast_itoptr")?.as_basic_value_enum())
                }
                // ptr -> int
                (BasicValueEnum::PointerValue(pv), BasicTypeEnum::IntType(it)) => {
                    Ok(ctx.builder.build_ptr_to_int(pv, it, "cast_ptrtoi")?.as_basic_value_enum())
                }
                // ptr -> ptr (opaque pointers: no-op)
                (BasicValueEnum::PointerValue(pv), BasicTypeEnum::PointerType(_)) => {
                    Ok(pv.as_basic_value_enum())
                }
                // int -> float
                (BasicValueEnum::IntValue(iv), BasicTypeEnum::FloatType(ft)) => {
                    let signed = matches!(
                        inner.inferred_type,
                        Ty::Builtin(
                            BuiltinType::Int1
                            | BuiltinType::Int2
                            | BuiltinType::Int4
                            | BuiltinType::Int8
                            | BuiltinType::Int16
                        )
                    );
                    if signed {
                        Ok(ctx.builder.build_signed_int_to_float(iv, ft, "cast_sitofp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_unsigned_int_to_float(iv, ft, "cast_uitofp")?.as_basic_value_enum())
                    }
                }
                // float -> int
                (BasicValueEnum::FloatValue(fv), BasicTypeEnum::IntType(it)) => {
                    let signed = matches!(
                        target_type,
                        Ty::Builtin(
                            BuiltinType::Int1
                            | BuiltinType::Int2
                            | BuiltinType::Int4
                            | BuiltinType::Int8
                            | BuiltinType::Int16
                        )
                    );
                    if signed {
                        Ok(ctx.builder.build_float_to_signed_int(fv, it, "cast_fptosi")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_float_to_unsigned_int(fv, it, "cast_fptoui")?.as_basic_value_enum())
                    }
                }
                // float -> float (extend or truncate)
                (BasicValueEnum::FloatValue(fv), BasicTypeEnum::FloatType(ft)) => {
                    Ok(ctx.builder.build_float_cast(fv, ft, "cast_fp")?.as_basic_value_enum())
                }
                (src, dst) => Err(format!(
                    "codegen_expr: unsupported cast from {:?} to {:?}",
                    src.get_type(),
                    dst
                )
                .into()),
            }
        }
        TypedExprKind::ArrayLiteral { elements } => {
            let arr_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let alloca = ctx.builder.build_alloca(arr_ty, "arrtmp")?;
            let i32_zero = ctx.ctx.i32_type().const_int(0, false);
            for (i, elem) in elements.iter().enumerate() {
                let val = codegen_expr(ctx, vars, current_scope, elem)?;
                let idx = ctx.ctx.i32_type().const_int(i as u64, false);
                let elem_ptr = unsafe {
                    ctx.builder.build_gep(
                        arr_ty,
                        alloca,
                        &[i32_zero, idx],
                        "arr_elem_ptr",
                    )?
                };
                ctx.builder.build_store(elem_ptr, val)?;
            }
            let loaded = ctx.builder.build_load(arr_ty, alloca, "arrload")?;
            Ok(loaded)
        }
        TypedExprKind::IndexAccess { object, index } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let elem_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            match &object.inferred_type {
                Ty::Array { .. } => {
                    let arr_ty = map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
                    // Need a pointer to the array for GEP — store to temp alloca
                    let arr_val = codegen_expr(ctx, vars, current_scope, object)?;
                    let alloca = ctx.builder.build_alloca(arr_ty, "arridxtmp")?;
                    ctx.builder.build_store(alloca, arr_val)?;
                    let i32_zero = ctx.ctx.i32_type().const_int(0, false);
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            arr_ty,
                            alloca,
                            &[i32_zero, idx_val.into_int_value()],
                            "arr_idx_ptr",
                        )?
                    };
                    let loaded = ctx.builder.build_load(elem_ty, gep, "arr_idx_load")?;
                    Ok(loaded)
                }
                _ => {
                    let ptr_val = codegen_expr(ctx, vars, current_scope, object)?;
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            elem_ty,
                            ptr_val.into_pointer_value(),
                            &[idx_val.into_int_value()],
                            "idx_ptr",
                        )?
                    };
                    let loaded = ctx.builder.build_load(elem_ty, gep, "idx_load")?;
                    Ok(loaded)
                }
            }
        }
        TypedExprKind::QualifiedAccess { module: module_name, name } => {
            // Qualified access: look up the mangled name in the LLVM module.
            let mangled = format!("{}__{}", module_name, name.value);
            if let Some(ptr) = vars.get(name) {
                let ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
                Ok(ctx.builder.build_load(ty, *ptr, &mangled)?)
            } else if let Some(func) = ctx.module.get_function(&mangled) {
                Ok(func.as_global_value().as_pointer_value().as_basic_value_enum())
            } else {
                Err(format!("QualifiedAccess: '{}::{}' not found in lowering", module_name, name).into())
            }
        }
        TypedExprKind::ComptimeType(_) => {
            Err("compiler bug: ComptimeType expression reached LLVM lowering — type values must not appear in runtime code".into())
        }
        TypedExprKind::EnumLiteral { discriminant, .. } => {
            // Determine the LLVM representation from the enum's resolved type.
            let llvm_ty =
                map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let i32_ty = ctx.ctx.i32_type();
            let tag = i32_ty.const_int(*discriminant as u64, false);
            match llvm_ty {
                BasicTypeEnum::IntType(_) => Ok(tag.as_basic_value_enum()),
                BasicTypeEnum::StructType(st) => {
                    // Tagged enum: build `{ tag, undef payload }` constant.
                    let payload_field_ty = st
                        .get_field_type_at_index(1)
                        .ok_or("EnumLiteral: tagged enum struct missing payload field")?;
                    let payload_undef = match payload_field_ty {
                        BasicTypeEnum::ArrayType(at) => at.get_undef().as_basic_value_enum(),
                        other => {
                            return Err(format!(
                                "EnumLiteral: unexpected payload type {:?}",
                                other
                            )
                            .into());
                        }
                    };
                    Ok(st
                        .const_named_struct(&[tag.as_basic_value_enum(), payload_undef])
                        .as_basic_value_enum())
                }
                other => Err(format!("EnumLiteral: unexpected enum LLVM type {:?}", other).into()),
            }
        }
        TypedExprKind::EnumVariantConstruct {
            discriminant,
            fields,
            enum_type,
            ..
        } => {
            // Lower the enum struct type, alloca it, write tag, build the
            // payload struct and store it via a bitcast pointer.
            let llvm_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(enum_st) = llvm_ty else {
                return Err(format!(
                    "EnumVariantConstruct: enum LLVM type is not a struct: {:?}",
                    llvm_ty
                )
                .into());
            };
            let alloca = ctx.builder.build_alloca(enum_st, "enumtmp")?;
            // Store tag at field 0.
            let tag_ptr = ctx
                .builder
                .build_struct_gep(enum_st, alloca, 0, "enumtag")?;
            ctx.builder.build_store(
                tag_ptr,
                ctx.ctx.i32_type().const_int(*discriminant as u64, false),
            )?;
            // Build the payload struct value, then store it at the payload region.
            let Ty::Enum { variants } = enum_type.as_ref() else {
                return Err("EnumVariantConstruct: enum_type is not an enum".into());
            };
            let v = variants
                .iter()
                .find(|v| v.discriminant == *discriminant)
                .ok_or("EnumVariantConstruct: variant not found in enum_type")?;
            let payload_spec = v
                .payload
                .as_ref()
                .ok_or("EnumVariantConstruct: variant has no payload spec")?;
            // Build payload struct type matching the declared field order.
            let payload_field_tys: Vec<BasicTypeEnum> = payload_spec
                .iter()
                .map(|(_, t)| map_type_to_llvm(t, ctx.ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            let payload_st = ctx.ctx.struct_type(&payload_field_tys, false);
            // GEP payload region, then bitcast to the variant's payload struct
            // pointer, then store each field.
            let payload_ptr = ctx
                .builder
                .build_struct_gep(enum_st, alloca, 1, "enumpayload")?;
            for (idx, (fname, _)) in payload_spec.iter().enumerate() {
                let val = fields
                    .iter()
                    .find(|(n, _)| n == fname)
                    .map(|(_, e)| e)
                    .ok_or_else(|| {
                        format!("EnumVariantConstruct: missing payload field '{}'", fname)
                    })?;
                let v_val = codegen_expr(ctx, vars, current_scope, val)?;
                let field_ptr =
                    ctx.builder
                        .build_struct_gep(payload_st, payload_ptr, idx as u32, "varfldptr")?;
                ctx.builder.build_store(field_ptr, v_val)?;
            }
            // Load the whole enum struct as the value.
            let loaded = ctx.builder.build_load(enum_st, alloca, "enumload")?;
            Ok(loaded)
        }
    }
}
