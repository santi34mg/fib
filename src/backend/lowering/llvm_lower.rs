use std::collections::HashMap;

use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::tokens::Operator;
use crate::frontend::tokens::builtin::{BuiltinFunction, BuiltinType};
use crate::frontend::typed_ast::{
    ScopeKind, SymbolTable, Ty, TypedBinding, TypedDecl, TypedExpr, TypedExprKind, TypedFunction,
    TypedPattern, TypedProgram, TypedStatement, TypedSwitchArm, TypedSymbol,
};
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

/// Per-function lowering state. This is the designated owner for the
/// `llvm_lower.rs` split (phase 4): `types.rs` keeps `map_type_to_llvm`,
/// `expr.rs` gets `codegen_expr`, `stmt.rs` gets `codegen_stmt`, and all of
/// them take `&FunctionLowering` instead of the current
/// `(ctx, vars, scope, loop_ctx, deferred_stack)` tuple. Introduced now so
/// new helpers (`insert_block`, `emit_if`, IR consumer) build on it instead
/// of adding more free-function params.
#[allow(dead_code)]
struct FunctionLowering<'ctx, 'r> {
    ctx: &'r CodegenCtx<'ctx, 'r>,
    function: FunctionValue<'ctx>,
    vars: HashMap<Identifier, PointerValue<'ctx>>,
    scope: SymbolTable,
    deferred_stack: Vec<Vec<TypedStatement>>,
}

#[allow(dead_code)]
impl<'ctx, 'r> FunctionLowering<'ctx, 'r> {
    fn new(
        ctx: &'r CodegenCtx<'ctx, 'r>,
        function: FunctionValue<'ctx>,
        vars: HashMap<Identifier, PointerValue<'ctx>>,
        scope: SymbolTable,
    ) -> Self {
        Self {
            ctx,
            function,
            vars,
            scope,
            deferred_stack: vec![Vec::new()],
        }
    }

    /// Current insert block with context (replaces `get_insert_block().unwrap()`).
    fn insert_block(&self, what: &str) -> Result<BasicBlock<'ctx>, Box<dyn Error>> {
        self.ctx
            .builder
            .get_insert_block()
            .ok_or_else(|| format!("lowering '{}': no insert block", what).into())
    }
}

/// Current insert block with context (replaces `get_insert_block().unwrap()`).
fn insert_block<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    what: &str,
) -> Result<BasicBlock<'ctx>, Box<dyn Error>> {
    ctx.builder
        .get_insert_block()
        .ok_or_else(|| format!("lowering '{}': no insert block", what).into())
}

/// Enclosing function with context (replaces `get_parent().unwrap()`).
fn parent_function<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    what: &str,
) -> Result<FunctionValue<'ctx>, Box<dyn Error>> {
    Ok(insert_block(ctx, what)?
        .get_parent()
        .ok_or_else(|| format!("lowering '{}': insert block has no parent function", what))?)
}

/// Lower Typed into LLVM IR represented as a string.
pub fn lower(compilation_unit: TypedProgram, module_name: &str) -> Result<String, Box<dyn Error>> {
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
            TypedDecl::Function(typed_function) => {
                let function_name = typed_function.name.value.clone();
                let fn_params: Vec<BasicMetadataTypeEnum> = typed_function
                    .params
                    .iter()
                    .map(|param| {
                        map_type_to_llvm(
                            &param.1.clone(),
                            &ctx,
                            compilation_unit.symbol_table.clone(),
                        )
                        .map(BasicMetadataTypeEnum::from)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let fn_ty: FunctionType;
                if let Ty::Builtin(BuiltinType::Void) = typed_function.return_type {
                    fn_ty = ctx
                        .void_type()
                        .fn_type(&fn_params, typed_function.is_variadic);
                } else {
                    let ret_ty = map_type_to_llvm(
                        &typed_function.return_type,
                        &ctx,
                        compilation_unit.symbol_table.clone(),
                    )?;
                    fn_ty = ret_ty.fn_type(&fn_params, typed_function.is_variadic);
                }

                // Extern functions: emit a declaration with External linkage and no body.
                if typed_function.is_extern {
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
                    typed_function.clone(),
                    compilation_unit.symbol_table.clone(),
                )?;
                // Function scope for params; exited after the body so params
                // never leak into the next function's lowering.
                let mut fn_scope = compilation_unit.symbol_table.clone();
                fn_scope.enter_scope(ScopeKind::Function);
                for (param_name, param_ty) in typed_function.params.iter() {
                    fn_scope.insert(
                        param_name.clone(),
                        TypedSymbol::Binding(TypedBinding {
                            name: param_name.clone(),
                            ty: param_ty.clone(),
                            init: None,
                            mutable: true,
                        }),
                    );
                }
                let mut fn_deferred: Vec<Vec<TypedStatement>> = vec![Vec::new()];
                for stmt in typed_function.body.iter() {
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
                if let Ty::Builtin(BuiltinType::Void) = typed_function.return_type
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
                fn_scope.exit_scope();
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
            TypedDecl::Type(_) => {
                // Type declarations are registered in the scope during analysis.
                // No LLVM IR needs to be emitted for them.
            }
            TypedDecl::Const(typed_binding) => {
                // The builder is only positioned inside a function while one
                // is being emitted; a module-level const has no such context.
                if builder.get_insert_block().is_none() {
                    return Err(format!(
                        "module-level constant '{}' is not supported in lowering yet",
                        typed_binding.name
                    )
                    .into());
                }
                let ty = map_type_to_llvm(
                    &typed_binding.ty,
                    &ctx,
                    compilation_unit.symbol_table.clone(),
                )?;
                let alloca = match builder.build_alloca(ty, &format!("{}_addr", typed_binding.name))
                {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!(
                            "Failed to create alloca for parameter '{}': {}",
                            typed_binding.name, e
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
                        &mut compilation_unit.symbol_table.clone(),
                        &typed_binding
                            .init
                            .ok_or_else(|| "no init for binding".to_string())?,
                    )?,
                );
                vars.insert(typed_binding.name, alloca);
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

/// Coerce an LLVM int value to `target_type`, widening with `zext` when
/// the source Fib value is `@uint*` and `sext` otherwise (mirrors the
/// `Cast` int->int rule keyed off `inner.inferred_type`).
fn coerce_int_to_llvm_type<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    value: BasicValueEnum<'ctx>,
    target_type: BasicTypeEnum<'ctx>,
    is_unsigned: bool,
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
                if is_unsigned {
                    Ok(ctx
                        .builder
                        .build_int_z_extend(iv, it, "zexttmp")?
                        .as_basic_value_enum())
                } else {
                    Ok(ctx
                        .builder
                        .build_int_s_extend(iv, it, "sextmp")?
                        .as_basic_value_enum())
                }
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

fn store_lvalue<'ctx, 'r>(
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

/// Look up a function by name, declaring it with external linkage if it does
/// not already exist. Used to bind libc functions (`strlen`, `malloc`, ...)
/// that back the string builtins.
fn get_or_declare<'ctx>(
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
fn call_result<'ctx>(
    cs: inkwell::values::CallSiteValue<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match cs.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(v) => Ok(v),
        inkwell::values::ValueKind::Instruction(_) => {
            Err("expected a return value from libc call".into())
        }
    }
}

fn codegen_expr<'ctx, 'r>(
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
        TypedExprKind::Binary {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, vars, current_scope, left)?;
            // `&&` / `||` short-circuit: only evaluate the RHS when the LHS
            // doesn't already decide the result.
            if matches!(operator, Operator::LogicalAnd | Operator::LogicalOr) {
                let func = parent_function(ctx, "short-circuit")?;
                let lhs_bb = insert_block(ctx, "short-circuit lhs")?;
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
                return Ok(phi.as_basic_value());
            }
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
                Operator::Plus => {
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
                Operator::Minus => {
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

/// Create an alloca for each parameter in the entry block
fn create_entry_allocas<'ctx>(
    ctx: &'ctx Context,
    function: FunctionValue<'ctx>,
    typed_fn: TypedFunction,
    current_scope: SymbolTable,
) -> Result<HashMap<Identifier, PointerValue<'ctx>>, Box<dyn Error>> {
    let mut vars = HashMap::new();

    let entry = function.get_first_basic_block().ok_or_else(|| {
        format!(
            "create_entry_allocas: function '{}' has no entry block",
            function.get_name().to_string_lossy()
        )
    })?;
    let builder_at_entry = ctx.create_builder();
    if let Some(first_instr) = entry.get_first_instruction() {
        builder_at_entry.position_before(&first_instr);
    } else {
        builder_at_entry.position_at_end(entry);
    }

    for (idx, (param_name, ty)) in typed_fn.params.into_iter().enumerate() {
        let param = function.get_nth_param(idx as u32).ok_or_else(|| {
            format!(
                "create_entry_allocas: missing param {} ('{}')",
                idx, param_name
            )
        })?;
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
    current_scope: &mut SymbolTable,
    frame: &[TypedStatement],
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
    current_scope: &mut SymbolTable,
    stack: &[Vec<TypedStatement>],
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

// ---------------------------------------------------------------------------
// IR consumer: `IrProgram -> LLVM` (phase 3, core subset).
//
// The direct `TypedProgram -> LLVM` path above stays until parity. This
// consumer proves the mechanical translation works: each `BasicBlock`
// becomes an LLVM block, each flat `Instruction` becomes 1-3 builder
// calls. Anything outside the core subset returns a descriptive error
// (never silently miscompiles).
// ---------------------------------------------------------------------------

use crate::ir as ir_mod;

/// Lower an IR program to LLVM IR text (core subset).
#[allow(dead_code)]
pub fn lower_ir(program: ir_mod::IrProgram, module_name: &str) -> Result<String, Box<dyn Error>> {
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

#[allow(dead_code)]
fn lower_ir_function<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    func: &ir_mod::IrFunction,
) -> Result<(), Box<dyn Error>> {
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

    if let Some(existing) = ctx.module.get_function(&func.name)
        && existing.count_basic_blocks() > 0
    {
        return Ok(()); // duplicate via diamond import; keep first body
    }
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

#[allow(dead_code)]
fn operand_ty(
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

#[allow(dead_code)]
fn operand_is_unsigned(op: &ir_mod::Operand, temp_tys: &HashMap<ir_mod::Temp, Ty>) -> bool {
    match op {
        ir_mod::Operand::Temp(t) => temp_tys.get(t).map(ir_mod::ty_is_unsigned).unwrap_or(false),
        ir_mod::Operand::ConstInt { ty, .. } => ir_mod::ty_is_unsigned(ty),
        ir_mod::Operand::ConstFloat { ty, .. } => ir_mod::ty_is_unsigned(ty),
        _ => false,
    }
}

#[allow(dead_code)]
fn resolve_operand<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    vars: &HashMap<ir_mod::SymbolId, PointerValue<'ctx>>,
    temps: &HashMap<ir_mod::Temp, BasicValueEnum<'ctx>>,
    sym_ty: &HashMap<ir_mod::SymbolId, Ty>,
    empty_scope: &SymbolTable,
    op: &ir_mod::Operand,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
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

#[allow(dead_code)]
fn build_ir_binary<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    op: ir_mod::BinOp,
    l: BasicValueEnum<'ctx>,
    r: BasicValueEnum<'ctx>,
    ty: &Ty,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
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
#[allow(dead_code)]
fn build_ir_cast<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    src: BasicValueEnum<'ctx>,
    from: &Ty,
    target: &Ty,
    dst_ty: BasicTypeEnum<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
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

#[cfg(all(test, feature = "llvm"))]
mod backend_tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use std::path::PathBuf;

    fn lower_ir_src(src: &str) -> String {
        let tokens: Vec<_> = Lexer::new(src).collect();
        let path = PathBuf::from("test.fib");
        let mut parser = Parser::new(tokens.into_iter(), &path, src.to_string());
        let ast = parser.parse().expect("parse failed");
        let typed =
            crate::frontend::analyze::analyze(ast, &Default::default()).expect("analysis failed");
        let ir = crate::ir::lower_typed_program(typed).expect("ir lower failed");
        lower_ir(ir, "test").expect("llvm lower_ir failed")
    }

    #[test]
    fn coerce_unsigned_widening_uses_zext() {
        let ctx = Context::create();
        let module = ctx.create_module("coerce_test");
        let builder = ctx.create_builder();
        let cctx = CodegenCtx {
            ctx: &ctx,
            module: &module,
            builder: &builder,
        };
        // fn f(i8) -> i64 { zext/sext param; ret }
        for (is_unsigned, name) in [(true, "fu"), (false, "fs")] {
            let i8_ty = ctx.i8_type();
            let i64_ty = ctx.i64_type();
            let fn_ty = i64_ty.fn_type(&[i8_ty.into()], false);
            let function = module.add_function(name, fn_ty, None);
            let entry = ctx.append_basic_block(function, "entry");
            builder.position_at_end(entry);
            let param = function.get_nth_param(0).unwrap().into_int_value();
            let coerced = coerce_int_to_llvm_type(
                &cctx,
                param.as_basic_value_enum(),
                i64_ty.as_basic_type_enum(),
                is_unsigned,
            )
            .expect("coerce");
            builder.build_return(Some(&coerced)).unwrap();
        }
        let ir = module.print_to_string().to_string();
        assert!(
            ir.contains("zext i8"),
            "expected zext for unsigned widening, got:\n{}",
            ir
        );
        assert!(
            ir.contains("sext i8"),
            "expected sext for signed widening, got:\n{}",
            ir
        );
    }

    #[test]
    fn map_type_covers_unsigned_kinds() {
        let ctx = Context::create();
        let scope = SymbolTable::new();
        let cases = [
            (BuiltinType::UInt1, 8u32),
            (BuiltinType::UInt2, 16),
            (BuiltinType::UInt4, 32),
            (BuiltinType::UInt8, 64),
            (BuiltinType::UInt16, 128),
            (BuiltinType::Int1, 8),
            (BuiltinType::Int8, 64),
        ];
        for (bt, bits) in cases {
            let llvm_ty =
                map_type_to_llvm(&Ty::Builtin(bt.clone()), &ctx, scope.clone()).expect("map type");
            match llvm_ty {
                BasicTypeEnum::IntType(it) => {
                    assert_eq!(it.get_bit_width(), bits, "wrong width for {:?}", bt)
                }
                other => panic!("expected int type for {:?}, got {:?}", bt, other),
            }
        }
        // Void is never a value type.
        assert!(map_type_to_llvm(&Ty::Builtin(BuiltinType::Void), &ctx, scope).is_err());
    }

    #[test]
    fn ir_consumer_lowers_straight_line() {
        let ir = lower_ir_src("fn main() @int { x := 1\ny := 2\nreturn x + y }");
        assert!(ir.contains("define"), "expected define, got:\n{}", ir);
        assert!(ir.contains("add"), "expected add, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }

    #[test]
    fn ir_consumer_uses_unsigned_div() {
        let ir = lower_ir_src("fn main() @uint8 { x: @uint8 = 200\ny: @uint8 = 3\nreturn x / y }");
        assert!(
            ir.contains("udiv"),
            "expected udiv for @uint8 division, got:\n{}",
            ir
        );
    }

    #[test]
    fn ir_consumer_uses_unsigned_cmp() {
        let ir = lower_ir_src("fn main() @bool { x: @uint8 = 200\ny: @uint8 = 3\nreturn x > y }");
        // Unsigned greater-than lowers to `icmp ugt`.
        assert!(
            ir.contains("ugt"),
            "expected ugt for @uint8 comparison, got:\n{}",
            ir
        );
    }

    #[test]
    fn ir_consumer_lowers_if_and_loop() {
        let ir = lower_ir_src(
            "fn main() @int { x := 0\nif x == 0 { x = 1 } else { x = 2 }\nfor (i: @int = 0; i < 10; i = i + 1) { x = x + i }\nreturn x }",
        );
        assert!(ir.contains("br i1"), "expected cond br, got:\n{}", ir);
        assert!(ir.contains("ret"), "expected ret, got:\n{}", ir);
    }
}
