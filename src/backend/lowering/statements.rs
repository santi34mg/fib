use std::collections::HashMap;
use std::error::Error;

use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::context::{
    CodegenCtx, LoopContext, coerce_int_to_llvm_type, emit_deferred_frame, emit_frames_from,
    insert_block, parent_function,
};
use super::expressions::{
    build_tuple_value, codegen_expr, compute_lvalue_ptr, store_lvalue, unpack_tuple_value,
};
use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::typed_ast::{
    SymbolTable, Ty, TypedBinding, TypedExprKind, TypedPattern, TypedStatement, TypedSwitchArm,
    TypedSymbol,
};

pub(super) fn codegen_stmt<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut SymbolTable,
    stmt: &TypedStatement,
    loop_ctx: Option<&LoopContext<'ctx>>,
    deferred_stack: &mut Vec<Vec<TypedStatement>>,
) -> Result<Option<BasicValueEnum<'ctx>>, Box<dyn Error>> {
    match stmt {
        TypedStatement::Binding(typed_binding) => {
            let ty = map_type_to_llvm(&typed_binding.ty, ctx.ctx, current_scope.clone())?;
            let alloca = match ctx
                .builder
                .build_alloca(ty, &format!("{}_addr", typed_binding.name))
            {
                Ok(a) => a,
                Err(e) => {
                    return Err(format!(
                        "Failed to create alloca for parameter '{}': {}",
                        typed_binding.name, e
                    )
                    .into());
                }
            };
            // Store the init value into the alloca. An uninitialized binding
            // is just an alloca; it holds undef until first assignment.
            if let Some(init) = &typed_binding.init {
                let _ = ctx.builder.build_store(
                    alloca,
                    codegen_expr(ctx, vars, &mut current_scope.clone(), init)?,
                )?;
            }
            vars.insert(typed_binding.name.clone(), alloca);
            // Register the binding in the scope so subsequent expressions
            // (e.g. `return x`) can look up its type via codegen_expr.
            current_scope.insert(
                typed_binding.name.clone(),
                TypedSymbol::Binding(TypedBinding {
                    name: typed_binding.name.clone(),
                    ty: typed_binding.ty.clone(),
                    init: None,
                    mutable: typed_binding.mutable,
                }),
            );
            Ok(None)
        }

        TypedStatement::Assign { name, expr } => {
            let v = codegen_expr(ctx, vars, current_scope, expr)?;
            if let Some(ptr) = vars.get(name) {
                let target_ty = match current_scope.lookup(name) {
                    Some(TypedSymbol::Binding(binding)) => {
                        map_type_to_llvm(&binding.ty, ctx.ctx, current_scope.clone())?
                    }
                    _ => v.get_type(),
                };
                let v = coerce_int_to_llvm_type(
                    ctx,
                    v,
                    target_ty,
                    crate::ir::ty_is_unsigned(&expr.inferred_type),
                )?;
                let _ = ctx.builder.build_store(*ptr, v);
                Ok(None)
            } else {
                Err(format!("assignment to unknown variable '{}' in lowering", name).into())
            }
        }

        TypedStatement::MultiAssign { targets, values } => {
            // Track unsignedness alongside each evaluated value so widening
            // uses `zext` for `@uint*` sources (tuple-unpacked values fall
            // back to the target type).
            let evaluated: Vec<(BasicValueEnum, Option<bool>)> =
                if values.len() == 1 && matches!(values[0].inferred_type, Ty::Tuple { .. }) {
                    let tuple_value = codegen_expr(ctx, vars, current_scope, &values[0])?;
                    unpack_tuple_value(ctx, tuple_value, targets.len())?
                        .into_iter()
                        .map(|v| (v, None))
                        .collect()
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
                        let unsigned = crate::ir::ty_is_unsigned(&value.inferred_type);
                        vals.push((
                            codegen_expr(ctx, vars, current_scope, value)?,
                            Some(unsigned),
                        ));
                    }
                    vals
                };

            for (target, (value, unsigned_opt)) in targets.iter().zip(evaluated) {
                let unsigned = unsigned_opt
                    .unwrap_or_else(|| crate::ir::ty_is_unsigned(&target.inferred_type));
                store_lvalue(ctx, vars, current_scope, target, value, unsigned)?;
            }
            Ok(None)
        }

        TypedStatement::MultiBinding { bindings, values } => {
            let evaluated: Vec<(BasicValueEnum, Option<bool>)> = if values.len() == 1
                && matches!(values[0].inferred_type, Ty::Tuple { .. })
            {
                let tuple_value = codegen_expr(ctx, vars, current_scope, &values[0])?;
                unpack_tuple_value(ctx, tuple_value, bindings.len())?
                    .into_iter()
                    .map(|v| (v, None))
                    .collect()
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
                    let unsigned = crate::ir::ty_is_unsigned(&value.inferred_type);
                    vals.push((
                        codegen_expr(ctx, vars, current_scope, value)?,
                        Some(unsigned),
                    ));
                }
                vals
            };

            for (binding, (value, unsigned_opt)) in bindings.iter().zip(evaluated) {
                let ty = map_type_to_llvm(&binding.ty, ctx.ctx, current_scope.clone())?;
                let alloca = ctx
                    .builder
                    .build_alloca(ty, &format!("{}_addr", binding.name))?;
                let unsigned =
                    unsigned_opt.unwrap_or_else(|| crate::ir::ty_is_unsigned(&binding.ty));
                let value = coerce_int_to_llvm_type(ctx, value, ty, unsigned)?;
                ctx.builder.build_store(alloca, value)?;
                vars.insert(binding.name.clone(), alloca);
                current_scope.insert(
                    binding.name.clone(),
                    TypedSymbol::Binding(TypedBinding {
                        name: binding.name.clone(),
                        ty: binding.ty.clone(),
                        init: None,
                        mutable: binding.mutable,
                    }),
                );
            }
            Ok(None)
        }

        TypedStatement::FieldAssign {
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

        TypedStatement::DerefAssign { pointer, expr } => {
            let ptr_val = codegen_expr(ctx, vars, current_scope, pointer)?;
            let val = codegen_expr(ctx, vars, current_scope, expr)?;
            ctx.builder.build_store(ptr_val.into_pointer_value(), val)?;
            Ok(None)
        }

        TypedStatement::IndexAssign {
            object,
            index,
            expr,
        } => {
            let idx_val = codegen_expr(ctx, vars, current_scope, index)?;
            let val = codegen_expr(ctx, vars, current_scope, expr)?;
            match &object.inferred_type {
                Ty::Array { .. } => {
                    let arr_ty =
                        map_type_to_llvm(&object.inferred_type, ctx.ctx, current_scope.clone())?;
                    // Get the alloca for the array identifier directly
                    let arr_ptr = if let TypedExprKind::Identifier(name) = &object.expression {
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
                Ty::Pointer(inner) => {
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

        TypedStatement::Expr(e) => {
            let _ = codegen_expr(ctx, vars, current_scope, e)?;
            Ok(None)
        }

        TypedStatement::Defer(inner) => {
            // Push onto the current deferred frame for later emission
            deferred_stack
                .last_mut()
                .ok_or("defer outside of function body")?
                .push(*inner.clone());
            Ok(None)
        }

        TypedStatement::Return(opt) => {
            // Returning leaves every enclosing block: run all deferred
            // frames, innermost first.
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, 0)?;
            if let Some(ret) = opt {
                let func = parent_function(ctx, "return")?;
                let ret_ty = func.get_type().get_return_type();
                let ret_val = if ret.values.len() == 1 {
                    let v = codegen_expr(ctx, vars, current_scope, &ret.values[0])?;
                    match ret_ty {
                        Some(target_ty) => coerce_int_to_llvm_type(
                            ctx,
                            v,
                            target_ty,
                            crate::ir::ty_is_unsigned(&ret.values[0].inferred_type),
                        )?,
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

        TypedStatement::If(typed_if) => {
            // Retrieve the current function so we can append basic blocks to it.
            let func = parent_function(ctx, "if")?;

            // The merge block is always created.  When both branches terminate
            // (e.g. both end with `return`) the merge block will be unreachable,
            // but we still need the builder to be positioned somewhere valid for
            // any statements that follow the `if` in the enclosing block.  LLVM
            // will discard the unreachable block during optimisation / verification
            // is not bothered by it.
            let merge_bb = ctx.ctx.append_basic_block(func, "ifcont");

            // For an if-without-else, the false branch jumps straight to the
            // merge block, avoiding a superfluous empty `else` block.
            let else_bb = if typed_if.else_branch.is_some() {
                ctx.ctx.append_basic_block(func, "else")
            } else {
                merge_bb
            };
            // The then block is always needed.
            let then_bb = ctx.ctx.append_basic_block(func, "then");

            // Emit the condition in the current (predecessor) block, then
            // branch to the appropriate successors.  This terminates the
            // predecessor block.
            let cond_v = codegen_expr(ctx, vars, current_scope, &typed_if.cond)?;
            let _ = ctx
                .builder
                .build_conditional_branch(cond_v.into_int_value(), then_bb, else_bb);

            // --- then branch ---
            ctx.builder.position_at_end(then_bb);
            deferred_stack.push(Vec::new());
            for s in typed_if.then_branch.iter() {
                codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
            }
            let then_deferred = deferred_stack.pop().ok_or("missing then frame")?;
            // Only emit the fallthrough branch if the block has no terminator
            // yet (i.e. the branch body did not end with `return`).
            if insert_block(ctx, "if then end")?.get_terminator().is_none() {
                emit_deferred_frame(ctx, vars, current_scope, &then_deferred)?;
                let _ = ctx.builder.build_unconditional_branch(merge_bb);
            }

            // --- else branch (only when one exists) ---
            if let Some(eb) = &typed_if.else_branch {
                ctx.builder.position_at_end(else_bb);
                deferred_stack.push(Vec::new());
                for s in eb.iter() {
                    codegen_stmt(ctx, vars, current_scope, s, loop_ctx, deferred_stack)?;
                }
                let else_deferred = deferred_stack.pop().ok_or("missing else frame")?;
                if insert_block(ctx, "if else end")?.get_terminator().is_none() {
                    emit_deferred_frame(ctx, vars, current_scope, &else_deferred)?;
                    let _ = ctx.builder.build_unconditional_branch(merge_bb);
                }
            }

            // Position the builder at the merge block so that subsequent
            // statements in the enclosing block are emitted there.
            ctx.builder.position_at_end(merge_bb);
            Ok(None)
        }
        TypedStatement::For {
            init,
            cond,
            post,
            body,
        } => {
            // init (no loop context yet)
            if let Some(i) = init {
                codegen_stmt(ctx, vars, current_scope, i, None, deferred_stack)?;
            }

            let func = parent_function(ctx, "for")?;

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
            let body_deferred = deferred_stack.pop().ok_or("missing loop body frame")?;
            // fall through to post if no terminator
            if insert_block(ctx, "for body end")?
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
            if insert_block(ctx, "for post end")?
                .get_terminator()
                .is_none()
            {
                ctx.builder.build_unconditional_branch(cond_bb)?;
            }

            // continue here after loop
            ctx.builder.position_at_end(after_bb);

            Ok(None)
        }
        TypedStatement::Break => {
            let lc = loop_ctx.ok_or("break outside of loop")?;
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, lc.deferred_depth)?;
            ctx.builder.build_unconditional_branch(lc.break_bb)?;
            let func = parent_function(ctx, "break")?;
            let dead_bb = ctx.ctx.append_basic_block(func, "dead");
            ctx.builder.position_at_end(dead_bb);
            Ok(None)
        }
        TypedStatement::Continue => {
            let lc = loop_ctx.ok_or("continue outside of loop")?;
            let frames = deferred_stack.clone();
            emit_frames_from(ctx, vars, current_scope, &frames, lc.deferred_depth)?;
            ctx.builder.build_unconditional_branch(lc.continue_bb)?;
            let func = parent_function(ctx, "continue")?;
            let dead_bb = ctx.ctx.append_basic_block(func, "dead");
            ctx.builder.position_at_end(dead_bb);
            Ok(None)
        }
        TypedStatement::Switch { subject, arms } => {
            let func = parent_function(ctx, "switch")?;
            let merge_bb = ctx.ctx.append_basic_block(func, "switchcont");

            // Find a wildcard arm for the default block (if any). Otherwise
            // the merge block itself acts as the default.
            let mut default_bb = merge_bb;
            let mut wildcard_arm: Option<&Vec<TypedStatement>> = None;
            for arm in arms {
                if let TypedPattern::Wildcard = &arm.pattern {
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
                &TypedSwitchArm,
            )> = Vec::new();
            for arm in arms {
                if let TypedPattern::EnumVariant { discriminant, .. } = &arm.pattern {
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
                    Option<TypedSymbol>,
                )> = None;
                if let TypedPattern::EnumVariant {
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
                    // Inject the binding into the scope so Typed FieldAccess can
                    // look up the struct fields.
                    let prev_sym = current_scope.insert(
                        b.clone(),
                        TypedSymbol::Binding(TypedBinding {
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
                let arm_deferred = deferred_stack.pop().ok_or("missing switch arm frame")?;
                if insert_block(ctx, "switch arm end")?
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
                            current_scope.insert(bid, s);
                        }
                        None => {
                            current_scope.remove(&bid);
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
                let def_deferred = deferred_stack.pop().ok_or("missing switch default frame")?;
                if insert_block(ctx, "switch default end")?
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
