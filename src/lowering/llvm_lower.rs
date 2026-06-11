use std::collections::HashMap;

use crate::hir::{
    CompilationUnit, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction, HIRStmt,
    HIRSymbol, HIRTypeKind, Scope,
};
use crate::tokens::Operator;
use crate::tokens::builtin::BuiltinType;
use crate::tokens::identifier::Identifier;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::BasicMetadataValueEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::{AddressSpace, FloatPredicate, IntPredicate};
use std::error::Error;

struct LoopContext<'ctx> {
    break_bb: BasicBlock<'ctx>,
    continue_bb: BasicBlock<'ctx>,
    /// Depth of the deferred-statement stack at loop entry. `break` and
    /// `continue` run the deferred frames above this depth (those opened
    /// inside the loop) before jumping.
    deferred_depth: usize,
}

struct CodegenCtx<'ctx, 'r> {
    ctx: &'ctx Context,
    module: &'r Module<'ctx>,
    builder: &'r Builder<'ctx>,
}

/// Lower HIR into LLVM IR represented as a string.
pub fn lower(
    compilation_unit: CompilationUnit,
    module_name: &str,
) -> Result<String, Box<dyn Error>> {
    let ctx = Context::create();
    let module: Module<'_> = ctx.create_module(module_name);
    let builder: Builder<'_> = ctx.create_builder();
    let mut vars: HashMap<Identifier, PointerValue> = HashMap::new();

    let codegen_ctx = CodegenCtx {
        ctx: &ctx,
        module: &module,
        builder: &builder,
    };

    // Create function declarations and bodies
    for declaration in compilation_unit.declarations {
        match declaration {
            HIRDeclaration::HIRFunction(hir_function) => {
                let function_name = hir_function.name.identifier.clone();
                let fn_params: Vec<BasicMetadataTypeEnum> = hir_function
                    .params
                    .iter()
                    .map(|param| {
                        map_type_to_llvm(
                            &param.1.clone(),
                            &ctx,
                            compilation_unit.scope_root.clone(),
                        )
                        .unwrap()
                        .into()
                    })
                    .collect();

                let fn_ty: FunctionType;
                if let HIRTypeKind::Builtin(BuiltinType::Void) = hir_function.return_type {
                    fn_ty = ctx
                        .void_type()
                        .fn_type(&fn_params, hir_function.is_variadic);
                } else {
                    let ret_ty = map_type_to_llvm(
                        &hir_function.return_type,
                        &ctx,
                        compilation_unit.scope_root.clone(),
                    )?;
                    fn_ty = ret_ty.fn_type(&fn_params, hir_function.is_variadic);
                }

                // Extern functions: emit a declaration with External linkage and no body.
                if hir_function.is_extern {
                    if module.get_function(&function_name).is_none() {
                        module.add_function(
                            &function_name,
                            fn_ty,
                            Some(inkwell::module::Linkage::External),
                        );
                    }
                    continue;
                }

                // Reuse an existing forward declaration (e.g. auto-declared at a call site)
                // rather than creating a duplicate with a mangled name. If the
                // function already has a body, this is a duplicate declaration
                // (the same module can be reached through several import
                // paths) — skip it instead of corrupting the existing one.
                let function = match module.get_function(&function_name) {
                    Some(f) if f.count_basic_blocks() > 0 => continue,
                    Some(f) => f,
                    None => module.add_function(&function_name, fn_ty, None),
                };
                let entry = ctx.append_basic_block(function, "entry");
                builder.position_at_end(entry);
                let mut entry_vars = create_entry_allocas(
                    &ctx,
                    function,
                    hir_function.clone(),
                    compilation_unit.scope_root.clone(),
                )?;
                // Build a function-level scope that includes module symbols
                // plus the function's own parameters, so identifier lookups
                // (e.g. `val` in `val >= 0`) resolve correctly during codegen.
                let mut fn_scope = compilation_unit.scope_root.clone();
                for (param_name, param_ty) in hir_function.params.iter() {
                    fn_scope.symbols.insert(
                        param_name.clone(),
                        HIRSymbol::Binding(crate::hir::HIRBinding {
                            name: param_name.clone(),
                            ty: param_ty.clone(),
                            init: None,
                            mutable: true,
                        }),
                    );
                }
                let mut fn_deferred: Vec<Vec<crate::hir::HIRStmt>> = vec![Vec::new()];
                for stmt in hir_function.body.iter() {
                    codegen_stmt(
                        &codegen_ctx,
                        &mut entry_vars,
                        &mut fn_scope,
                        stmt,
                        None,
                        &mut fn_deferred,
                    )?;
                }
                // For void functions, if the current block at the end of
                // codegen has no terminator (i.e. the function falls off the
                // end without an explicit `return`), emit `ret void` so the
                // function returns cleanly.
                if let HIRTypeKind::Builtin(BuiltinType::Void) = hir_function.return_type
                    && let Some(cur_bb) = builder.get_insert_block()
                    && cur_bb.get_terminator().is_none()
                {
                    emit_frames_from(
                        &codegen_ctx,
                        &mut entry_vars,
                        &mut fn_scope,
                        &fn_deferred,
                        0,
                    )?;
                    let _ = builder.build_return(None);
                }
                // Seal any basic blocks that have no terminator (e.g. an
                // unreachable merge block after an if where both branches
                // return).  LLVM requires every block to have a terminator.
                let mut bb_opt = function.get_first_basic_block();
                while let Some(bb) = bb_opt {
                    if bb.get_terminator().is_none() {
                        builder.position_at_end(bb);
                        let _ = builder.build_unreachable();
                    }
                    bb_opt = bb.get_next_basic_block();
                }
            }
            HIRDeclaration::HIRType(_) => {
                // Type declarations are registered in the scope during analysis.
                // No LLVM IR needs to be emitted for them.
            }
            HIRDeclaration::HIRConst(hir_binding) => {
                // The builder is only positioned inside a function while one
                // is being emitted; a module-level const has no such context.
                if builder.get_insert_block().is_none() {
                    return Err(format!(
                        "module-level constant '{}' is not supported in lowering yet",
                        hir_binding.name
                    )
                    .into());
                }
                let ty =
                    map_type_to_llvm(&hir_binding.ty, &ctx, compilation_unit.scope_root.clone())?;
                let alloca = match builder.build_alloca(ty, &format!("{}_addr", hir_binding.name)) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!(
                            "Failed to create alloca for parameter '{}': {}",
                            hir_binding.name, e
                        );
                        continue;
                    }
                };
                // store the param value into the alloca
                let _ = codegen_ctx.builder.build_store(
                    alloca,
                    codegen_expr(
                        &codegen_ctx,
                        &mut vars,
                        &mut compilation_unit.scope_root.clone(),
                        &hir_binding
                            .init
                            .ok_or_else(|| "no init for binding".to_string())?,
                    )?,
                );
                vars.insert(hir_binding.name, alloca);
            }
        }
    }

    // Return LLVM IR as string.
    // Some LLVM builds (with opaque pointers) print pointer types as `ptr` which
    // older clang versions reject. For now, post-process the printed IR to
    // restore typed pointers for our simple i64-based lowering.
    let ir = module.print_to_string().to_string();
    Ok(ir)
}

/// Compute a pointer to the lvalue represented by `expr`. Supports identifiers,
/// field access chains, index access, and dereferences. Used by AddressOf and
/// by assignment lowering.
fn compute_lvalue_ptr<'ctx, 'r>(
    ctx: &'r CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    expr: &HIRExpression,
) -> Result<PointerValue<'ctx>, Box<dyn Error>> {
    match &expr.expression {
        HIRExpressionKind::Identifier(name) => {
            let ptr = *vars
                .get(name)
                .ok_or_else(|| format!("compute_lvalue_ptr: no alloca for identifier {}", name))?;
            Ok(ptr)
        }
        HIRExpressionKind::Deref(inner) => {
            let v = codegen_expr(ctx, vars, current_scope, inner)?;
            Ok(v.into_pointer_value())
        }
        HIRExpressionKind::FieldAccess {
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
        HIRExpressionKind::IndexAccess { object, index } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let elem_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            match &object.inferred_type {
                HIRTypeKind::Array { .. } => {
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

fn coerce_int_to_llvm_type<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    value: BasicValueEnum<'ctx>,
    target_type: BasicTypeEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match (value, target_type) {
        (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(it)) => {
            let src_bits = iv.get_type().get_bit_width();
            let dst_bits = it.get_bit_width();
            if src_bits > dst_bits {
                Ok(ctx
                    .builder
                    .build_int_truncate(iv, it, "trunctmp")?
                    .as_basic_value_enum())
            } else if src_bits < dst_bits {
                Ok(ctx
                    .builder
                    .build_int_s_extend(iv, it, "sextmp")?
                    .as_basic_value_enum())
            } else {
                Ok(iv.as_basic_value_enum())
            }
        }
        (value, _) => Ok(value),
    }
}

fn build_tuple_value<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    exprs: &[HIRExpression],
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
        let val = coerce_int_to_llvm_type(ctx, raw_val, field_ty)?;
        let gep = ctx
            .builder
            .build_struct_gep(tuple_ty, alloca, idx as u32, "multiretfield")?;
        ctx.builder.build_store(gep, val)?;
    }
    Ok(ctx.builder.build_load(tuple_ty, alloca, "multiretload")?)
}

fn store_lvalue<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    target: &HIRExpression,
    value: BasicValueEnum<'ctx>,
) -> Result<(), Box<dyn Error>> {
    let ptr = compute_lvalue_ptr(ctx, vars, current_scope, target)?;
    let target_ty = map_type_to_llvm(&target.inferred_type, ctx.ctx, current_scope.clone())?;
    let value = coerce_int_to_llvm_type(ctx, value, target_ty)?;
    ctx.builder.build_store(ptr, value)?;
    Ok(())
}

fn unpack_tuple_value<'ctx, 'r>(
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

fn codegen_expr<'ctx, 'r>(
    ctx: &'r CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    expr: &HIRExpression,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match &expr.expression {
        HIRExpressionKind::LiteralInt { value } => {
            if let BasicTypeEnum::IntType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx.ctx, Scope::new())?
            {
                let sign_extend = !matches!(
                    expr.inferred_type,
                    HIRTypeKind::Builtin(
                        BuiltinType::UInt1
                        | BuiltinType::UInt2
                        | BuiltinType::UInt4
                        | BuiltinType::UInt8
                        | BuiltinType::UInt16
                    )
                );
                Ok(ty.const_int(*value, sign_extend).as_basic_value_enum())
            } else {
                unreachable!()
            }
        }
        HIRExpressionKind::LiteralFloat { value } => {
            if let BasicTypeEnum::FloatType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx.ctx, Scope::new())?
            {
                Ok(ty.const_float(*value).as_basic_value_enum())
            } else {
                unreachable!()
            }
        }
        HIRExpressionKind::LiteralBool(b) => Ok(ctx.ctx
            .bool_type()
            .const_int(*b as u64, false)
            .as_basic_value_enum()),
        HIRExpressionKind::LiteralString { value } => {
            let ptr = ctx.builder.build_global_string_ptr(value, "str")?;
            Ok(ptr.as_pointer_value().as_basic_value_enum())
        }
        HIRExpressionKind::Identifier(name) => {
            let ty = if let HIRSymbol::Binding(var) = current_scope
                .symbols
                .get(name)
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
        HIRExpressionKind::Null => {
            Ok(ctx.ctx.ptr_type(AddressSpace::default()).const_null().as_basic_value_enum())
        }
        HIRExpressionKind::Binary {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, vars, current_scope, left)?;
            // `&&` / `||` short-circuit: only evaluate the RHS when the LHS
            // doesn't already decide the result.
            if matches!(operator, Operator::LogicalAnd | Operator::LogicalOr) {
                let func = ctx
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_parent()
                    .unwrap();
                let lhs_bb = ctx.builder.get_insert_block().unwrap();
                let rhs_bb = ctx.ctx.append_basic_block(func, "sc_rhs");
                let merge_bb = ctx.ctx.append_basic_block(func, "sc_merge");
                let is_and = matches!(operator, Operator::LogicalAnd);
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
                let rhs_end_bb = ctx.builder.get_insert_block().unwrap();
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
                return Ok(phi.as_basic_value());
            }
            let r = codegen_expr(ctx, vars, current_scope, right)?;
            let is_float = matches!(
                left.inferred_type,
                HIRTypeKind::Builtin(
                    BuiltinType::Float2
                    | BuiltinType::Float4
                    | BuiltinType::Float8
                    | BuiltinType::Float16
                )
            );
            let is_unsigned = matches!(
                left.inferred_type,
                HIRTypeKind::Builtin(
                    BuiltinType::UInt1
                    | BuiltinType::UInt2
                    | BuiltinType::UInt4
                    | BuiltinType::UInt8
                    | BuiltinType::UInt16
                )
            );
            match operator {
                Operator::Plus => {
                    if is_float {
                        Ok(ctx.builder.build_float_add(l.into_float_value(), r.into_float_value(), "faddtmp")?.as_basic_value_enum())
                    } else if let HIRTypeKind::Pointer(inner_ty) = &left.inferred_type {
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
                Operator::Minus => {
                    if is_float {
                        Ok(ctx.builder.build_float_sub(l.into_float_value(), r.into_float_value(), "fsubtmp")?.as_basic_value_enum())
                    } else if let HIRTypeKind::Pointer(inner_ty) = &left.inferred_type {
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
                Operator::Star => {
                    if is_float {
                        Ok(ctx.builder.build_float_mul(l.into_float_value(), r.into_float_value(), "fmultmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?.as_basic_value_enum())
                    }
                }
                Operator::Slash => {
                    if is_float {
                        Ok(ctx.builder.build_float_div(l.into_float_value(), r.into_float_value(), "fdivtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_unsigned_div(l.into_int_value(), r.into_int_value(), "udivtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?.as_basic_value_enum())
                    }
                }
                Operator::Percent => {
                    if is_float {
                        Ok(ctx.builder.build_float_rem(l.into_float_value(), r.into_float_value(), "fremtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_unsigned_rem(l.into_int_value(), r.into_int_value(), "uremtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_signed_rem(l.into_int_value(), r.into_int_value(), "remtmp")?.as_basic_value_enum())
                    }
                }
                Operator::GreaterThan => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OGT, l.into_float_value(), r.into_float_value(), "fgttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::UGT, l.into_int_value(), r.into_int_value(), "ugttmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SGT, l.into_int_value(), r.into_int_value(), "gttmp")?.as_basic_value_enum())
                    }
                }
                Operator::GreaterEqual => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OGE, l.into_float_value(), r.into_float_value(), "fgetmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::UGE, l.into_int_value(), r.into_int_value(), "ugetmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SGE, l.into_int_value(), r.into_int_value(), "getmp")?.as_basic_value_enum())
                    }
                }
                Operator::LesserThan => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OLT, l.into_float_value(), r.into_float_value(), "flttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::ULT, l.into_int_value(), r.into_int_value(), "ulttmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SLT, l.into_int_value(), r.into_int_value(), "lttmp")?.as_basic_value_enum())
                    }
                }
                Operator::LesserEqual => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OLE, l.into_float_value(), r.into_float_value(), "fletmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(ctx.builder.build_int_compare(IntPredicate::ULE, l.into_int_value(), r.into_int_value(), "uletmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::SLE, l.into_int_value(), r.into_int_value(), "letmp")?.as_basic_value_enum())
                    }
                }
                Operator::DoubleEquals => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::OEQ, l.into_float_value(), r.into_float_value(), "feqtmp")?.as_basic_value_enum())
                    } else if l.is_pointer_value() {
                        Ok(ctx.builder.build_int_compare(IntPredicate::EQ, l.into_pointer_value(), r.into_pointer_value(), "peqtmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eqtmp")?.as_basic_value_enum())
                    }
                }
                Operator::Different => {
                    if is_float {
                        Ok(ctx.builder.build_float_compare(FloatPredicate::ONE, l.into_float_value(), r.into_float_value(), "fnetmp")?.as_basic_value_enum())
                    } else if l.is_pointer_value() {
                        Ok(ctx.builder.build_int_compare(IntPredicate::NE, l.into_pointer_value(), r.into_pointer_value(), "pnetmp")?.as_basic_value_enum())
                    } else {
                        Ok(ctx.builder.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "netmp")?.as_basic_value_enum())
                    }
                }
                // LogicalAnd / LogicalOr are short-circuited above.
                Operator::LeftShift => Ok(ctx.builder
                    .build_left_shift(l.into_int_value(), r.into_int_value(), "shltmp")?
                    .as_basic_value_enum()),
                Operator::RightShift => Ok(ctx.builder
                    .build_right_shift(l.into_int_value(), r.into_int_value(), !is_unsigned, "shrtmp")?
                    .as_basic_value_enum()),
                Operator::Ampersand => Ok(ctx.builder
                    .build_and(l.into_int_value(), r.into_int_value(), "bandtmp")?
                    .as_basic_value_enum()),
                Operator::Pipe => Ok(ctx.builder
                    .build_or(l.into_int_value(), r.into_int_value(), "bortmp")?
                    .as_basic_value_enum()),
                Operator::Caret => Ok(ctx.builder
                    .build_xor(l.into_int_value(), r.into_int_value(), "xortmp")?
                    .as_basic_value_enum()),
                op => Err(format!("unsupported binary operator in codegen: {:?}", op).into()),
            }
        }
        HIRExpressionKind::Call { callee, args } => {
            let mut arg_values = Vec::new();
            for a in args.iter() {
                let av = codegen_expr(ctx, vars, current_scope, a)?;
                arg_values.push(av);
            }
            // Lookup function; if not declared yet, auto-declare it as an external
            // function using the argument types observed at this call site.
            let fnval = match ctx.module.get_function(&callee.identifier) {
                Some(f) => f,
                None => {
                    let param_types: Vec<BasicMetadataTypeEnum> =
                        arg_values.iter().map(|v| v.get_type().into()).collect();
                    let fn_ty = if let HIRTypeKind::Builtin(BuiltinType::Void) = &expr.inferred_type {
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
                    ctx.module.add_function(&callee.identifier, fn_ty, None)
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
        HIRExpressionKind::FieldAccess { object, field: _, field_index } => {
            // We need the pointer to the struct object, then GEP into it.
            // The object expression should be an Identifier whose alloca we can find.
            let struct_ptr = match &object.expression {
                HIRExpressionKind::Identifier(name) => {
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
        HIRExpressionKind::StructConstruct { type_name: _, fields } => {
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
        HIRExpressionKind::AddressOf(inner) => {
            let ptr = compute_lvalue_ptr(ctx, vars, current_scope, inner)?;
            Ok(ptr.as_basic_value_enum())
        }
        HIRExpressionKind::Deref(inner) => {
            // Codegen the pointer expression, then load through it.
            let ptr_val = codegen_expr(ctx, vars, current_scope, inner)?;
            let pointee_llvm_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            let loaded = ctx.builder.build_load(pointee_llvm_ty, ptr_val.into_pointer_value(), "deref")?;
            Ok(loaded)
        }
        HIRExpressionKind::Cast { expr: inner, target_type } => {
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
                            HIRTypeKind::Builtin(
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
                        HIRTypeKind::Builtin(
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
                        HIRTypeKind::Builtin(
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
        HIRExpressionKind::ArrayLiteral { elements } => {
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
        HIRExpressionKind::IndexAccess { object, index } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let elem_ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
            match &object.inferred_type {
                HIRTypeKind::Array { .. } => {
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
        HIRExpressionKind::QualifiedAccess { module: module_name, name } => {
            // Qualified access: look up the mangled name in the LLVM module.
            let mangled = format!("{}__{}", module_name, name.identifier);
            if let Some(ptr) = vars.get(name) {
                let ty = map_type_to_llvm(&expr.inferred_type, ctx.ctx, current_scope.clone())?;
                Ok(ctx.builder.build_load(ty, *ptr, &mangled)?)
            } else if let Some(func) = ctx.module.get_function(&mangled) {
                Ok(func.as_global_value().as_pointer_value().as_basic_value_enum())
            } else {
                Err(format!("QualifiedAccess: '{}::{}' not found in lowering", module_name, name).into())
            }
        }
        HIRExpressionKind::ComptimeType(_) => {
            Err("compiler bug: ComptimeType expression reached LLVM lowering — type values must not appear in runtime code".into())
        }
        HIRExpressionKind::EnumLiteral { discriminant, .. } => {
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
        HIRExpressionKind::EnumVariantConstruct {
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
            let HIRTypeKind::Enum { variants } = enum_type.as_ref() else {
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

fn round_up(n: usize, align: usize) -> usize {
    if align <= 1 { n } else { n.div_ceil(align) * align }
}

/// Size and alignment of a HIR type in bytes, mirroring LLVM's struct layout
/// rules (fields aligned to their natural alignment, structs padded to the
/// largest field alignment). Used to size the payload region of tagged
/// unions — undersizing it would let payload stores write out of bounds.
fn hir_type_size_align(ty: &HIRTypeKind, scope: &Scope) -> (usize, usize) {
    match ty {
        HIRTypeKind::Builtin(b) => {
            let s = match b {
                BuiltinType::Boolean | BuiltinType::Char => 1,
                BuiltinType::UInt1 | BuiltinType::Int1 | BuiltinType::Never => 1,
                BuiltinType::UInt2 | BuiltinType::Int2 | BuiltinType::Float2 => 2,
                BuiltinType::UInt4 | BuiltinType::Int4 | BuiltinType::Float4 => 4,
                BuiltinType::UInt8 | BuiltinType::Int8 | BuiltinType::Float8 => 8,
                BuiltinType::UInt16 | BuiltinType::Int16 | BuiltinType::Float16 => 16,
                BuiltinType::String => 8,
                BuiltinType::Void => 0,
            };
            (s, s.max(1))
        }
        HIRTypeKind::Pointer(_) | HIRTypeKind::Function { .. } => (8, 8),
        HIRTypeKind::Array { element_type, size } => {
            let (s, a) = hir_type_size_align(element_type, scope);
            (round_up(s, a) * (*size as usize), a)
        }
        HIRTypeKind::Struct { fields } => {
            struct_layout_size_align(fields.iter().map(|(_, t)| t.as_ref()), scope)
        }
        HIRTypeKind::Tuple { elements } => struct_layout_size_align(elements.iter(), scope),
        HIRTypeKind::Enum { variants } => {
            let payload = enum_max_payload_bytes(variants, scope);
            if payload == 0 {
                (4, 4)
            } else {
                // Matches the lowered shape `{ i32 tag, [N x i64] payload }`.
                let words = payload.div_ceil(8);
                (8 + words * 8, 8)
            }
        }
        HIRTypeKind::Identifier(id) => match scope.symbols.get(id) {
            Some(HIRSymbol::Type(inner)) => hir_type_size_align(inner, scope),
            _ => (0, 1),
        },
        HIRTypeKind::QualifiedIdentifier { module, name } => {
            if let Some(m) = scope.modules.get(module)
                && let Some(HIRSymbol::Type(inner)) = m.exports.get(name)
            {
                hir_type_size_align(inner, scope)
            } else {
                (0, 1)
            }
        }
        HIRTypeKind::Type => (0, 1),
    }
}

/// Lay out a sequence of field types like an LLVM struct and return the
/// padded total size and alignment.
fn struct_layout_size_align<'t>(
    field_types: impl Iterator<Item = &'t HIRTypeKind>,
    scope: &Scope,
) -> (usize, usize) {
    let mut offset = 0usize;
    let mut align = 1usize;
    for ty in field_types {
        let (s, a) = hir_type_size_align(ty, scope);
        offset = round_up(offset, a) + s;
        align = align.max(a);
    }
    (round_up(offset, align), align)
}

/// Returns the maximum padded payload size (in bytes) across the variants of
/// an enum, or 0 if the enum has no payload-carrying variants.
fn enum_max_payload_bytes(variants: &[crate::hir::HIREnumVariant], scope: &Scope) -> usize {
    variants
        .iter()
        .filter_map(|v| {
            v.payload
                .as_ref()
                .map(|fs| struct_layout_size_align(fs.iter().map(|(_, t)| t), scope).0)
        })
        .max()
        .unwrap_or(0)
}

fn map_type_to_llvm<'ctx>(
    ty: &HIRTypeKind,
    ctx: &'ctx Context,
    current_scope: Scope,
) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
    match ty {
        HIRTypeKind::Builtin(builtin) => {
            let any_ty = match builtin {
                BuiltinType::Boolean => BasicTypeEnum::IntType(ctx.bool_type()),
                // TODO: make unsigned truly unsigned
                BuiltinType::UInt1 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::UInt2 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::UInt4 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::UInt8 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::UInt16 => BasicTypeEnum::IntType(ctx.i128_type()),
                BuiltinType::Int1 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::Int2 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::Int4 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::Int8 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::Int16 => BasicTypeEnum::IntType(ctx.i128_type()),
                BuiltinType::Float2 => BasicTypeEnum::FloatType(ctx.f16_type()),
                BuiltinType::Float4 => BasicTypeEnum::FloatType(ctx.f32_type()),
                BuiltinType::Float8 => BasicTypeEnum::FloatType(ctx.f64_type()),
                BuiltinType::Float16 => BasicTypeEnum::FloatType(ctx.f128_type()),
                BuiltinType::String => {
                    BasicTypeEnum::PointerType(ctx.ptr_type(AddressSpace::default()))
                }
                BuiltinType::Char => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::Never => BasicTypeEnum::IntType(ctx.bool_type()),
                BuiltinType::Void => return Err("void type cannot be used as a value type".into()),
            };
            Ok(any_ty)
        }
        HIRTypeKind::Identifier(identifier) => {
            let symbol = current_scope
                .symbols
                .get(identifier)
                .ok_or_else(|| format!("identifier {} not found in current scope", identifier))?;
            if let HIRSymbol::Type(ty) = symbol {
                map_type_to_llvm(ty, ctx, current_scope.clone())
            } else {
                Err(format!("symbol {:?} is not a type", symbol).into())
            }
        }
        HIRTypeKind::Struct { fields } => {
            let field_types: Vec<BasicTypeEnum> = fields
                .iter()
                .map(|(_, ty)| map_type_to_llvm(ty, ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            Ok(ctx.struct_type(&field_types, false).into())
        }
        HIRTypeKind::Tuple { elements } => {
            let field_types: Vec<BasicTypeEnum> = elements
                .iter()
                .map(|ty| map_type_to_llvm(ty, ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            Ok(ctx.struct_type(&field_types, false).into())
        }
        HIRTypeKind::Enum { variants } => {
            let payload_bytes = enum_max_payload_bytes(variants, &current_scope);
            if payload_bytes == 0 {
                Ok(ctx.i32_type().into())
            } else {
                // Use an i64-word payload so the region is 8-byte aligned;
                // variant payload structs are stored into it via struct GEPs.
                let tag_ty: BasicTypeEnum = ctx.i32_type().into();
                let payload_words = payload_bytes.div_ceil(8);
                let payload_ty: BasicTypeEnum =
                    ctx.i64_type().array_type(payload_words as u32).into();
                Ok(ctx.struct_type(&[tag_ty, payload_ty], false).into())
            }
        }
        HIRTypeKind::Pointer(_) => Ok(ctx.ptr_type(AddressSpace::default()).into()),
        HIRTypeKind::Array { element_type, size } => {
            let elem_ty = map_type_to_llvm(element_type, ctx, current_scope)?;
            Ok(elem_ty.array_type(*size as u32).into())
        }
        HIRTypeKind::Function { .. } => {
            // Function pointers are opaque `ptr` in LLVM 16.
            Ok(ctx.ptr_type(AddressSpace::default()).into())
        }
        HIRTypeKind::Type => {
            Err("compiler bug: HIRTypeKind::Type reached LLVM lowering — comptime type values must not appear in runtime code".into())
        }
        HIRTypeKind::QualifiedIdentifier { module, name } => {
            // Resolve through the scope's imported modules
            let module_data = current_scope.modules.get(module).ok_or_else(|| {
                format!("map_type_to_llvm: module '{}' not found in scope", module)
            })?;
            let sym = module_data.exports.get(name).ok_or_else(|| {
                format!(
                    "map_type_to_llvm: '{}' not found in module '{}'",
                    name, module
                )
            })?;
            if let HIRSymbol::Type(inner_ty) = sym {
                let inner_ty = inner_ty.clone();
                map_type_to_llvm(&inner_ty, ctx, current_scope)
            } else {
                Err(format!("map_type_to_llvm: '{}::{}' is not a type", module, name).into())
            }
        }
    }
}

/// Create an alloca for each parameter in the entry block
fn create_entry_allocas<'ctx>(
    ctx: &'ctx Context,
    function: FunctionValue<'ctx>,
    hir_fn: HIRFunction,
    current_scope: Scope,
) -> Result<HashMap<Identifier, PointerValue<'ctx>>, Box<dyn Error>> {
    let mut vars = HashMap::new();

    let entry = function.get_first_basic_block().unwrap();
    let builder_at_entry = ctx.create_builder();
    if let Some(first_instr) = entry.get_first_instruction() {
        builder_at_entry.position_before(&first_instr);
    } else {
        builder_at_entry.position_at_end(entry);
    }

    for (idx, (param_name, ty)) in hir_fn.params.into_iter().enumerate() {
        let param = function.get_nth_param(idx as u32).unwrap();
        // param.get_type() is a BasicTypeEnum already; build_alloca expects BasicTypeEnum
        let alloca = match builder_at_entry.build_alloca(
            map_type_to_llvm(&ty, ctx, current_scope.clone())?,
            &format!("{}_addr", param_name),
        ) {
            Ok(a) => a,
            Err(e) => {
                eprintln!(
                    "Failed to create alloca for parameter '{}': {}",
                    param_name, e
                );
                continue;
            }
        };
        // store the param value into the alloca
        let _ = builder_at_entry.build_store(alloca, param);
        vars.insert(param_name.clone(), alloca);
    }
    Ok(vars)
}

fn emit_deferred_frame<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    frame: &[HIRStmt],
) -> Result<(), Box<dyn Error>> {
    // Emit deferred statements in reverse order (LIFO)
    for stmt in frame.iter().rev() {
        codegen_stmt(ctx, vars, current_scope, stmt, None, &mut vec![Vec::new()])?;
    }
    Ok(())
}

/// Emit the deferred frames at `stack[from..]`, innermost frame first. Used
/// on early exits: `return` unwinds everything (`from = 0`), `break` /
/// `continue` unwind the frames opened inside the loop.
fn emit_frames_from<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    stack: &[Vec<HIRStmt>],
    from: usize,
) -> Result<(), Box<dyn Error>> {
    for frame in stack[from.min(stack.len())..].iter().rev() {
        emit_deferred_frame(ctx, vars, current_scope, frame)?;
    }
    Ok(())
}

fn codegen_stmt<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    stmt: &HIRStmt,
    loop_ctx: Option<&LoopContext<'ctx>>,
    deferred_stack: &mut Vec<Vec<HIRStmt>>,
) -> Result<Option<BasicValueEnum<'ctx>>, Box<dyn Error>> {
    match stmt {
        HIRStmt::Binding(hir_binding) => {
            let ty = map_type_to_llvm(&hir_binding.ty, ctx.ctx, current_scope.clone())?;
            let alloca = match ctx
                .builder
                .build_alloca(ty, &format!("{}_addr", hir_binding.name))
            {
                Ok(a) => a,
                Err(e) => {
                    return Err(format!(
                        "Failed to create alloca for parameter '{}': {}",
                        hir_binding.name, e
                    )
                    .into());
                }
            };
            // Store the init value into the alloca. An uninitialized binding
            // is just an alloca; it holds undef until first assignment.
            if let Some(init) = &hir_binding.init {
                let _ = ctx.builder.build_store(
                    alloca,
                    codegen_expr(ctx, vars, &mut current_scope.clone(), init)?,
                )?;
            }
            vars.insert(hir_binding.name.clone(), alloca);
            // Register the binding in the scope so subsequent expressions
            // (e.g. `return x`) can look up its type via codegen_expr.
            current_scope.symbols.insert(
                hir_binding.name.clone(),
                HIRSymbol::Binding(crate::hir::HIRBinding {
                    name: hir_binding.name.clone(),
                    ty: hir_binding.ty.clone(),
                    init: None,
                    mutable: hir_binding.mutable,
                }),
            );
            Ok(None)
        }

        HIRStmt::Assign { name, expr } => {
            let v = codegen_expr(ctx, vars, current_scope, expr)?;
            if let Some(ptr) = vars.get(name) {
                let target_ty = match current_scope.symbols.get(name) {
                    Some(HIRSymbol::Binding(binding)) => {
                        map_type_to_llvm(&binding.ty, ctx.ctx, current_scope.clone())?
                    }
                    _ => v.get_type(),
                };
                let v = coerce_int_to_llvm_type(ctx, v, target_ty)?;
                let _ = ctx.builder.build_store(*ptr, v);
                Ok(None)
            } else {
                Err(format!("assignment to unknown variable '{}' in lowering", name).into())
            }
        }

        HIRStmt::MultiAssign { targets, values } => {
            let evaluated_values = if values.len() == 1
                && matches!(values[0].inferred_type, HIRTypeKind::Tuple { .. })
            {
                let tuple_value = codegen_expr(ctx, vars, current_scope, &values[0])?;
                unpack_tuple_value(ctx, tuple_value, targets.len())?
            } else {
                if targets.len() != values.len() {
                    return Err(format!(
                        "multi-assignment arity mismatch: {} target(s), {} value expression(s)",
                        targets.len(),
                        values.len()
                    )
                    .into());
                }
                let mut vals = Vec::new();
                for value in values {
                    vals.push(codegen_expr(ctx, vars, current_scope, value)?);
                }
                vals
            };

            for (target, value) in targets.iter().zip(evaluated_values.into_iter()) {
                store_lvalue(ctx, vars, current_scope, target, value)?;
            }
            Ok(None)
        }

        HIRStmt::MultiBinding { bindings, values } => {
            let evaluated_values = if values.len() == 1
                && matches!(values[0].inferred_type, HIRTypeKind::Tuple { .. })
            {
                let tuple_value = codegen_expr(ctx, vars, current_scope, &values[0])?;
                unpack_tuple_value(ctx, tuple_value, bindings.len())?
            } else {
                if bindings.len() != values.len() {
                    return Err(format!(
                        "multi-variable declaration arity mismatch: {} binding(s), {} value expression(s)",
                        bindings.len(),
                        values.len()
                    )
                    .into());
                }
                let mut vals = Vec::new();
                for value in values {
                    vals.push(codegen_expr(ctx, vars, current_scope, value)?);
                }
                vals
            };

            for (binding, value) in bindings.iter().zip(evaluated_values.into_iter()) {
                let ty = map_type_to_llvm(&binding.ty, ctx.ctx, current_scope.clone())?;
                let alloca = ctx
                    .builder
                    .build_alloca(ty, &format!("{}_addr", binding.name))?;
                let value = coerce_int_to_llvm_type(ctx, value, ty)?;
                ctx.builder.build_store(alloca, value)?;
                vars.insert(binding.name.clone(), alloca);
                current_scope.symbols.insert(
                    binding.name.clone(),
                    HIRSymbol::Binding(crate::hir::HIRBinding {
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                        init: None,
                        mutable: binding.mutable,
                    }),
                );
            }
            Ok(None)
        }

        HIRStmt::FieldAssign {
            object,
            field: _,
            field_index,
            expr,
        } => {
            let base_ptr = compute_lvalue_ptr(ctx, vars, current_scope, object)?;
            let obj_ty = map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = obj_ty else {
                return Err(format!(
                    "codegen_stmt: FieldAssign target is not a struct: {:?}",
                    object.inferred_type
                )
                .into());
            };
            let gep = ctx.builder.build_struct_gep(
                st,
                base_ptr,
                *field_index as u32,
                "fieldassignptr",
            )?;
            let val = codegen_expr(ctx, vars, current_scope, expr)?;
            ctx.builder.build_store(gep, val)?;
            Ok(None)
        }

        HIRStmt::DerefAssign { pointer, expr } => {
            let ptr_val = codegen_expr(ctx, vars, current_scope, pointer)?;
            let val = codegen_expr(ctx, vars, current_scope, expr)?;
            ctx.builder.build_store(ptr_val.into_pointer_value(), val)?;
            Ok(None)
        }

        HIRStmt::IndexAssign {
            object,
            index,
            expr,
        } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let val = codegen_expr(ctx, vars, current_scope, expr)?;
            match &object.inferred_type {
                HIRTypeKind::Array { .. } => {
                    let arr_ty =
                        map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
                    // Get the alloca for the array identifier directly
                    let arr_ptr = if let HIRExpressionKind::Identifier(name) = &object.expression {
                        *vars
                            .get(name)
                            .ok_or_else(|| format!("IndexAssign: array {} not found", name))?
                    } else {
                        let arr_val = codegen_expr(ctx, vars, current_scope, object)?;
                        let alloca = ctx.builder.build_alloca(arr_ty, "idxassigntmp")?;
                        ctx.builder.build_store(alloca, arr_val)?;
                        alloca
                    };
                    let i32_zero = ctx.ctx.i32_type().const_int(0, false);
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            arr_ty,
                            arr_ptr,
                            &[i32_zero, idx_val.into_int_value()],
                            "arr_assign_ptr",
                        )?
                    };
                    ctx.builder.build_store(gep, val)?;
                    Ok(None)
                }
                HIRTypeKind::Pointer(inner) => {
                    let ptr_val = codegen_expr(ctx, vars, current_scope, object)?;
                    let elem_ty = map_type_to_llvm(inner, ctx.ctx, current_scope.clone())?;
                    let gep = unsafe {
                        ctx.builder.build_gep(
                            elem_ty,
                            ptr_val.into_pointer_value(),
                            &[idx_val.into_int_value()],
                            "idx_assign_ptr",
                        )?
                    };
                    ctx.builder.build_store(gep, val)?;
                    Ok(None)
                }
                other => Err(
                    format!("codegen_stmt: IndexAssign on non-pointer/array {:?}", other).into(),
                ),
            }
        }

        HIRStmt::Expr(e) => {
            let _ = codegen_expr(ctx, vars, current_scope, e)?;
            Ok(None)
        }

        HIRStmt::Defer(inner) => {
            // Push onto the current deferred frame for later emission
            deferred_stack
                .last_mut()
                .expect("deferred stack is never empty")
                .push(*inner.clone());
            Ok(None)
        }

        HIRStmt::Return(opt) => {
            // Returning leaves every enclosing block: run all deferred
            // frames, innermost first.
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, 0)?;
            if let Some(ret) = opt {
                let func = ctx
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_parent()
                    .unwrap();
                let ret_ty = func.get_type().get_return_type();
                let ret_val = if ret.values.len() == 1 {
                    let v = codegen_expr(ctx, vars, current_scope, &ret.values[0])?;
                    match ret_ty {
                        Some(target_ty) => coerce_int_to_llvm_type(ctx, v, target_ty)?,
                        None => v,
                    }
                } else {
                    let Some(BasicTypeEnum::StructType(tuple_ty)) = ret_ty else {
                        return Err(
                            "multiple return expressions require a multiple return type".into()
                        );
                    };
                    build_tuple_value(ctx, vars, current_scope, &ret.values, tuple_ty)?
                };
                let _ = ctx.builder.build_return(Some(&ret_val));
            } else {
                let _ = ctx.builder.build_return(None);
            }
            Ok(None)
        }

        HIRStmt::If(hir_if) => {
            // Retrieve the current function so we can append basic blocks to it.
            let func = ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_parent()
                .unwrap();

            // The merge block is always created.  When both branches terminate
            // (e.g. both end with `return`) the merge block will be unreachable,
            // but we still need the builder to be positioned somewhere valid for
            // any statements that follow the `if` in the enclosing block.  LLVM
            // will discard the unreachable block during optimisation / verification
            // is not bothered by it.
            let merge_bb = ctx.ctx.append_basic_block(func, "ifcont");

            // For an if-without-else, the false branch jumps straight to the
            // merge block, avoiding a superfluous empty `else` block.
            let else_bb = if hir_if.else_branch.is_some() {
                ctx.ctx.append_basic_block(func, "else")
            } else {
                merge_bb
            };
            // The then block is always needed.
            let then_bb = ctx.ctx.append_basic_block(func, "then");

            // Emit the condition in the current (predecessor) block, then
            // branch to the appropriate successors.  This terminates the
            // predecessor block.
            let cond_v = codegen_expr(ctx, vars, current_scope, &hir_if.cond)?;
            let _ = ctx
                .builder
                .build_conditional_branch(cond_v.into_int_value(), then_bb, else_bb);

            // --- then branch ---
            ctx.builder.position_at_end(then_bb);
            deferred_stack.push(Vec::new());
            for s in hir_if.then_branch.iter() {
                codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
            }
            let then_deferred = deferred_stack.pop().expect("then frame");
            // Only emit the fallthrough branch if the block has no terminator
            // yet (i.e. the branch body did not end with `return`).
            if ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                emit_deferred_frame(ctx, vars, current_scope, &then_deferred)?;
                let _ = ctx.builder.build_unconditional_branch(merge_bb);
            }

            // --- else branch (only when one exists) ---
            if let Some(eb) = &hir_if.else_branch {
                ctx.builder.position_at_end(else_bb);
                deferred_stack.push(Vec::new());
                for s in eb.iter() {
                    codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
                }
                let else_deferred = deferred_stack.pop().expect("else frame");
                if ctx
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    emit_deferred_frame(ctx, vars, current_scope, &else_deferred)?;
                    let _ = ctx.builder.build_unconditional_branch(merge_bb);
                }
            }

            // Position the builder at the merge block so that subsequent
            // statements in the enclosing block are emitted there.
            ctx.builder.position_at_end(merge_bb);
            Ok(None)
        }
        HIRStmt::For {
            init,
            cond,
            post,
            body,
        } => {
            // init (no loop context yet)
            if let Some(i) = init {
                codegen_stmt(ctx, vars, current_scope, i, None, deferred_stack)?;
            }

            let func = ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_parent()
                .unwrap();

            let cond_bb = ctx.ctx.append_basic_block(func, "forcond");
            let body_bb = ctx.ctx.append_basic_block(func, "forbody");
            let post_bb = ctx.ctx.append_basic_block(func, "forpost");
            let after_bb = ctx.ctx.append_basic_block(func, "afterloop");

            // continue jumps to post_bb if there is a post-op, otherwise to cond_bb
            let continue_target = if post.is_some() { post_bb } else { cond_bb };
            let for_loop_ctx = LoopContext {
                break_bb: after_bb,
                continue_bb: continue_target,
                deferred_depth: deferred_stack.len(),
            };

            // jump to condition first
            ctx.builder.build_unconditional_branch(cond_bb)?;
            ctx.builder.position_at_end(cond_bb);

            // condition
            if let Some(c) = cond {
                let cval = codegen_expr(ctx, vars, current_scope, c)?;
                ctx.builder
                    .build_conditional_branch(cval.into_int_value(), body_bb, after_bb)?;
            } else {
                // no condition = infinite loop
                ctx.builder.build_unconditional_branch(body_bb)?;
            }

            // body
            ctx.builder.position_at_end(body_bb);
            deferred_stack.push(Vec::new());
            for s in body.iter() {
                codegen_stmt(
                    ctx,
                    vars,
                    current_scope,
                    s,
                    Some(&for_loop_ctx),
                    deferred_stack,
                )?;
            }
            let body_deferred = deferred_stack.pop().expect("loop body frame");
            // fall through to post if no terminator
            if ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                emit_deferred_frame(ctx, vars, current_scope, &body_deferred)?;
                ctx.builder.build_unconditional_branch(post_bb)?;
            }

            // post
            ctx.builder.position_at_end(post_bb);
            if let Some(p) = post {
                codegen_stmt(
                    ctx,
                    vars,
                    current_scope,
                    p,
                    Some(&for_loop_ctx),
                    deferred_stack,
                )?;
            }
            // jump back to condition if block didn't terminate
            if ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                ctx.builder.build_unconditional_branch(cond_bb)?;
            }

            // continue here after loop
            ctx.builder.position_at_end(after_bb);

            Ok(None)
        }
        HIRStmt::Break => {
            let lc = loop_ctx.ok_or("break outside of loop")?;
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, lc.deferred_depth)?;
            ctx.builder.build_unconditional_branch(lc.break_bb)?;
            let func = ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_parent()
                .unwrap();
            let dead_bb = ctx.ctx.append_basic_block(func, "dead");
            ctx.builder.position_at_end(dead_bb);
            Ok(None)
        }
        HIRStmt::Continue => {
            let lc = loop_ctx.ok_or("continue outside of loop")?;
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, lc.deferred_depth)?;
            ctx.builder.build_unconditional_branch(lc.continue_bb)?;
            let func = ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_parent()
                .unwrap();
            let dead_bb = ctx.ctx.append_basic_block(func, "dead");
            ctx.builder.position_at_end(dead_bb);
            Ok(None)
        }
        HIRStmt::Switch { subject, arms } => {
            let func = ctx
                .builder
                .get_insert_block()
                .unwrap()
                .get_parent()
                .unwrap();
            let merge_bb = ctx.ctx.append_basic_block(func, "switchcont");

            // Find a wildcard arm for the default block (if any). Otherwise
            // the merge block itself acts as the default.
            let mut default_bb = merge_bb;
            let mut wildcard_arm: Option<&Vec<crate::hir::HIRStmt>> = None;
            for arm in arms {
                if let crate::hir::HIRPattern::Wildcard = &arm.pattern {
                    default_bb = ctx.ctx.append_basic_block(func, "switchdefault");
                    wildcard_arm = Some(&arm.body);
                    break;
                }
            }

            // Allocate one BB per non-wildcard arm.
            #[allow(clippy::type_complexity)]
            let mut variant_blocks: Vec<(
                u64,
                inkwell::basic_block::BasicBlock,
                &crate::hir::HIRSwitchArm,
            )> = Vec::new();
            for arm in arms {
                if let crate::hir::HIRPattern::EnumVariant { discriminant, .. } = &arm.pattern {
                    let bb = ctx.ctx.append_basic_block(func, "switcharm");
                    variant_blocks.push((*discriminant as u64, bb, arm));
                }
            }

            // Emit subject and extract the tag depending on whether the enum
            // is tagged (struct) or plain (i32).
            let subj_val = codegen_expr(ctx, vars, current_scope, subject)?;
            let i32_ty = ctx.ctx.i32_type();

            // For tagged enums we also need a pointer to the subject's payload
            // region so the per-arm bindings can read it.
            let (tag_int, subject_alloca, subject_struct_ty): (
                inkwell::values::IntValue,
                Option<PointerValue>,
                Option<inkwell::types::StructType>,
            ) = match subj_val {
                BasicValueEnum::IntValue(iv) => (iv, None, None),
                BasicValueEnum::StructValue(sv) => {
                    let st = sv.get_type();
                    let alloca = ctx.builder.build_alloca(st, "switchsubj")?;
                    ctx.builder.build_store(alloca, sv)?;
                    let tag_ptr = ctx
                        .builder
                        .build_struct_gep(st, alloca, 0, "switchtagptr")?;
                    let tag = ctx.builder.build_load(i32_ty, tag_ptr, "switchtag")?;
                    (tag.into_int_value(), Some(alloca), Some(st))
                }
                other => {
                    return Err(format!(
                        "switch: subject has unsupported LLVM type {:?}",
                        other.get_type()
                    )
                    .into());
                }
            };

            let cases: Vec<(inkwell::values::IntValue, inkwell::basic_block::BasicBlock)> =
                variant_blocks
                    .iter()
                    .map(|(d, bb, _)| (i32_ty.const_int(*d, false), *bb))
                    .collect();
            ctx.builder.build_switch(tag_int, default_bb, &cases)?;

            // Emit each variant arm body, binding the payload if requested.
            for (_, bb, arm) in &variant_blocks {
                ctx.builder.position_at_end(*bb);
                // If the pattern carries a binding, materialize the payload as
                // a local struct alloca that the arm body can field-access.
                #[allow(clippy::type_complexity)]
                let mut bind_restore: Option<(
                    Identifier,
                    Option<PointerValue>,
                    Option<HIRSymbol>,
                )> = None;
                if let crate::hir::HIRPattern::EnumVariant {
                    binding: Some(b),
                    payload_ty: Some(payload_ty),
                    ..
                } = &arm.pattern
                {
                    let payload_llvm =
                        map_type_to_llvm(payload_ty, ctx.ctx, current_scope.clone())?;
                    let bind_alloca = ctx.builder.build_alloca(payload_llvm, "patbind")?;
                    if let (Some(subj_alloca), Some(subj_st)) = (subject_alloca, subject_struct_ty)
                    {
                        let payload_ptr =
                            ctx.builder
                                .build_struct_gep(subj_st, subj_alloca, 1, "subjpayload")?;
                        // Reinterpret the payload bytes as the variant's struct
                        // by loading and re-storing through the bind alloca.
                        let loaded =
                            ctx.builder
                                .build_load(payload_llvm, payload_ptr, "loadpayload")?;
                        ctx.builder.build_store(bind_alloca, loaded)?;
                    }
                    let prev = vars.insert(b.clone(), bind_alloca);
                    // Inject the binding into the scope so HIR FieldAccess can
                    // look up the struct fields.
                    let prev_sym = current_scope.symbols.insert(
                        b.clone(),
                        HIRSymbol::Binding(crate::hir::HIRBinding {
                            name: b.clone(),
                            ty: payload_ty.clone(),
                            init: None,
                            mutable: false,
                        }),
                    );
                    bind_restore = Some((b.clone(), prev, prev_sym));
                }

                deferred_stack.push(Vec::new());
                for s in arm.body.iter() {
                    codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
                }
                let arm_deferred = deferred_stack.pop().expect("switch arm frame");
                if ctx
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    emit_deferred_frame(ctx, vars, current_scope, &arm_deferred)?;
                    ctx.builder.build_unconditional_branch(merge_bb)?;
                }

                // Restore what the binding shadowed (if anything) for the next arm.
                if let Some((bid, prev, prev_sym)) = bind_restore {
                    match prev {
                        Some(p) => {
                            vars.insert(bid.clone(), p);
                        }
                        None => {
                            vars.remove(&bid);
                        }
                    }
                    match prev_sym {
                        Some(s) => {
                            current_scope.symbols.insert(bid, s);
                        }
                        None => {
                            current_scope.symbols.remove(&bid);
                        }
                    }
                }
            }

            // Emit the wildcard/default arm body if present.
            if let Some(body) = wildcard_arm {
                ctx.builder.position_at_end(default_bb);
                deferred_stack.push(Vec::new());
                for s in body.iter() {
                    codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
                }
                let def_deferred = deferred_stack.pop().expect("switch default frame");
                if ctx
                    .builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    emit_deferred_frame(ctx, vars, current_scope, &def_deferred)?;
                    ctx.builder.build_unconditional_branch(merge_bb)?;
                }
            }

            ctx.builder.position_at_end(merge_bb);
            Ok(None)
        }
    }
}
