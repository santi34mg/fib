use std::collections::HashMap;

use crate::hir::{
    CompilationUnit, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction, HIRStmt,
    HIRSymbol, HIRTypeKind, Scope,
};
use crate::token::Operator;
use crate::token::builtin::BuiltinType;
use crate::token::identifier::Identifier;
use inkwell::IntPredicate;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::values::BasicMetadataValueEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use std::error::Error;

/// Lower HIR into LLVM IR represented as a string.
pub fn lower(
    mut compilation_unit: CompilationUnit,
    module_name: &str,
) -> Result<String, Box<dyn Error>> {
    let ctx = Context::create();
    let module = ctx.create_module(module_name);
    let builder = ctx.create_builder();
    let mut vars: HashMap<Identifier, PointerValue> = HashMap::new();

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
                    todo!()
                } else {
                    let ret_ty = map_type_to_llvm(
                        &hir_function.return_type,
                        &ctx,
                        compilation_unit.scope_root.clone(),
                    )
                    .unwrap();
                    fn_ty = ret_ty.fn_type(&fn_params, false);
                }
                let function = module.add_function(&function_name, fn_ty, None);
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
                        }),
                    );
                }
                for stmt in hir_function.body.iter() {
                    codegen_stmt(
                        &ctx,
                        &builder,
                        &module,
                        &mut entry_vars,
                        &mut fn_scope,
                        stmt,
                    )?;
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
            HIRDeclaration::HIRConst(hir_binding) => {
                let ty = map_type_to_llvm(&hir_binding.ty, &ctx, compilation_unit.scope_root.clone())?;
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
                let _ = builder.build_store(
                    alloca,
                    codegen_expr(
                        &ctx,
                        &builder,
                        &module,
                        &mut vars,
                        &mut compilation_unit.scope_root.clone(),
                        &hir_binding.init.ok_or_else(|| format!("no init for binding"))?,
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

fn codegen_expr<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    expr: &HIRExpression,
) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
    match &expr.expression {
        HIRExpressionKind::LiteralInt { value } => {
            if let BasicTypeEnum::IntType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx, Scope::new())?
            {
                Ok(ty.const_int(*value as u64, true).as_basic_value_enum())
            } else {
                unreachable!()
            }
        }
        HIRExpressionKind::LiteralBool(b) => Ok(ctx
            .bool_type()
            .const_int(*b as u64, false)
            .as_basic_value_enum()),
        HIRExpressionKind::LiteralString { value } => {
            let ptr = builder.build_global_string_ptr(value, "str")?;
            Ok(ptr.as_pointer_value().as_basic_value_enum())
        }
        HIRExpressionKind::Identifier(name) => {
            let ty = if let HIRSymbol::Binding(var) = current_scope
                .symbols
                .get(&name)
                .ok_or_else(|| format!("didnt find type for name {}", name))?
            {
                map_type_to_llvm(&var.ty, ctx, current_scope.clone())?
            } else {
                return Err(format!("codegen_expr: {} is not a variable", name).into());
            };
            let ptr = vars
                .get(&name)
                .ok_or_else(|| format!("codegen_expr: didnt find ptr for name {}", name))?;
            let load = builder.build_load(ty, *ptr, &format!("load_{}", name))?;
            Ok(load)
        }
        // TODO: Null?
        HIRExpressionKind::Null => Ok(ctx.i64_type().const_int(0, false).as_basic_value_enum()),
        HIRExpressionKind::Binary {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, builder, module, vars, current_scope, &left)?;
            let r = codegen_expr(ctx, builder, module, vars, current_scope, &right)?;
            match operator {
                Operator::Plus => Ok(builder
                    .build_int_add(l.into_int_value(), r.into_int_value(), "addtmp")?
                    .as_basic_value_enum()),
                Operator::Minus => Ok(builder
                    .build_int_sub(l.into_int_value(), r.into_int_value(), "subtmp")?
                    .as_basic_value_enum()),
                Operator::Star => Ok(builder
                    .build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?
                    .as_basic_value_enum()),
                Operator::Slash => Ok(builder
                    .build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?
                    .as_basic_value_enum()),
                Operator::GreaterThan => Ok(builder
                    .build_int_compare(
                        IntPredicate::SGT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "gttmp",
                    )?
                    .as_basic_value_enum()),
                Operator::GreaterEqual => Ok(builder
                    .build_int_compare(
                        IntPredicate::SGE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "getmp",
                    )?
                    .as_basic_value_enum()),
                Operator::LesserThan => Ok(builder
                    .build_int_compare(
                        IntPredicate::SLT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "lttmp",
                    )?
                    .as_basic_value_enum()),
                Operator::LesserEqual => Ok(builder
                    .build_int_compare(
                        IntPredicate::SLE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "letmp",
                    )?
                    .as_basic_value_enum()),
                // TODO: implement binary manipulation operations
                Operator::LeftShift => todo!(),
                Operator::RightShift => todo!(),
                Operator::Ampersand => todo!(),
                Operator::Pipe => todo!(),
                Operator::Caret => todo!(),
                _ => panic!(
                    "unsupported binary operatorerator in codegen: {:?}",
                    operator
                ),
            }
        }
        HIRExpressionKind::Call { callee, args } => {
            let mut arg_values = Vec::new();
            for a in args.iter() {
                let av = codegen_expr(ctx, builder, module, vars, current_scope, a)?;
                arg_values.push(av);
            }
            // Lookup function; if not declared yet, auto-declare it as an external
            // function using the argument types observed at this call site.
            let fnval = match module.get_function(&callee.identifier) {
                Some(f) => f,
                None => {
                    let param_types: Vec<BasicMetadataTypeEnum> =
                        arg_values.iter().map(|v| v.get_type().into()).collect();
                    let fn_ty = ctx.i32_type().fn_type(&param_types, false);
                    module.add_function(&callee.identifier, fn_ty, None)
                }
            };
            let md_args: Vec<BasicMetadataValueEnum> =
                arg_values.into_iter().map(|v| v.into()).collect();
            let call_site = builder.build_call(fnval, &md_args, "calltmp")?;
            Ok(call_site.try_as_basic_value().unwrap_basic())
        }
    }
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
                BuiltinType::UInt8 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::UInt16 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::UInt32 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::UInt64 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::SInt8 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::SInt16 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::SInt32 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::SInt64 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::Float16 => BasicTypeEnum::FloatType(ctx.f16_type()),
                BuiltinType::Float32 => BasicTypeEnum::FloatType(ctx.f32_type()),
                BuiltinType::Float64 => BasicTypeEnum::FloatType(ctx.f64_type()),
                BuiltinType::Float128 => BasicTypeEnum::FloatType(ctx.f128_type()),
                BuiltinType::String => {
                    BasicTypeEnum::PointerType(ctx.ptr_type(inkwell::AddressSpace::default()))
                }
                _ => todo!("type not implemented yet"),
            };
            Ok(any_ty)
        }
        HIRTypeKind::Identifier(identifier) => {
            let symbol = current_scope
                .symbols
                .get(&identifier)
                .ok_or_else(|| format!("identifier {} not found in current scope", identifier))?;
            if let HIRSymbol::Type(ty) = symbol {
                return Ok(map_type_to_llvm(ty, ctx, current_scope.clone())?);
            } else {
                return Err(format!("symbol {:?} is not a type", symbol).into());
            }
        }
        HIRTypeKind::Struct => todo!("map_type_to_llvm: struct type kind not implemented yet"),
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

fn codegen_stmt<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    mut vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    stmt: &HIRStmt,
) -> Result<Option<BasicValueEnum<'ctx>>, Box<dyn Error>> {
    match stmt {
        HIRStmt::Binding(hir_binding) => {
            let ty = map_type_to_llvm(&hir_binding.ty, &ctx, current_scope.clone())?;
            let alloca = match builder.build_alloca(ty, &format!("{}_addr", hir_binding.name)) {
                Ok(a) => a,
                Err(e) => {
                    return Err(format!(
                        "Failed to create alloca for parameter '{}': {}",
                        hir_binding.name, e
                    )
                    .into());
                }
            };
            // store the param value into the alloca
            let _ = builder.build_store(
                alloca,
                codegen_expr(
                    &ctx,
                    &builder,
                    &module,
                    &mut vars,
                    &mut current_scope.clone(),
                    &hir_binding.init.as_ref().ok_or_else(|| format!("no init for binding"))?.clone(),
                )?,
            )?;
            vars.insert(hir_binding.name.clone(), alloca);
            // Register the binding in the scope so subsequent expressions
            // (e.g. `return x`) can look up its type via codegen_expr.
            current_scope.symbols.insert(
                hir_binding.name.clone(),
                HIRSymbol::Binding(crate::hir::HIRBinding {
                    name: hir_binding.name.clone(),
                    ty: hir_binding.ty.clone(),
                    init: None,
                }),
            );
            Ok(None)
        }

        HIRStmt::Assign { name, expr } => {
            let v = codegen_expr(ctx, builder, module, vars, current_scope, expr)?;
            if let Some(ptr) = vars.get(name) {
                let _ = builder.build_store(*ptr, v);
                Ok(None)
            } else {
                panic!("assignment to unknown variable '{}' in lowering", name);
            }
        }

        HIRStmt::Expr(e) => {
            let _ = codegen_expr(ctx, builder, module, vars, current_scope, e)?;
            Ok(None)
        }

        HIRStmt::Return(opt) => {
            if let Some(e) = opt {
                let v = codegen_expr(ctx, builder, module, vars, current_scope, e)?;
                // Cast to the function's declared return type when it differs
                // (e.g. literal `0` inferred as i32 inside a function returning i8).
                let func = builder.get_insert_block().unwrap().get_parent().unwrap();
                let ret_val = match (v, func.get_type().get_return_type()) {
                    (BasicValueEnum::IntValue(iv), Some(BasicTypeEnum::IntType(it))) => {
                        let src_bits = iv.get_type().get_bit_width();
                        let dst_bits = it.get_bit_width();
                        if src_bits > dst_bits {
                            builder.build_int_truncate(iv, it, "trunctmp")?.as_basic_value_enum()
                        } else if src_bits < dst_bits {
                            builder.build_int_s_extend(iv, it, "sextmp")?.as_basic_value_enum()
                        } else {
                            iv.as_basic_value_enum()
                        }
                    }
                    (v, _) => v,
                };
                let _ = builder.build_return(Some(&ret_val));
            } else {
                let _ = builder.build_return(None);
            }
            Ok(None)
        }

        HIRStmt::If(hir_if) => {
            // Retrieve the current function so we can append basic blocks to it.
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();

            // The merge block is always created.  When both branches terminate
            // (e.g. both end with `return`) the merge block will be unreachable,
            // but we still need the builder to be positioned somewhere valid for
            // any statements that follow the `if` in the enclosing block.  LLVM
            // will discard the unreachable block during optimisation / verification
            // is not bothered by it.
            let merge_bb = ctx.append_basic_block(func, "ifcont");

            // For an if-without-else, the false branch jumps straight to the
            // merge block, avoiding a superfluous empty `else` block.
            let else_bb = if hir_if.else_branch.is_some() {
                ctx.append_basic_block(func, "else")
            } else {
                merge_bb
            };
            // The then block is always needed.
            let then_bb = ctx.append_basic_block(func, "then");

            // Emit the condition in the current (predecessor) block, then
            // branch to the appropriate successors.  This terminates the
            // predecessor block.
            let cond_v = codegen_expr(ctx, builder, module, vars, current_scope, &hir_if.cond)?;
            let _ = builder.build_conditional_branch(cond_v.into_int_value(), then_bb, else_bb);

            // --- then branch ---
            builder.position_at_end(then_bb);
            for s in hir_if.then_branch.iter() {
                codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
            }
            // Only emit the fallthrough branch if the block has no terminator
            // yet (i.e. the branch body did not end with `return`).
            if builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                let _ = builder.build_unconditional_branch(merge_bb);
            }

            // --- else branch (only when one exists) ---
            if let Some(eb) = &hir_if.else_branch {
                builder.position_at_end(else_bb);
                for s in eb.iter() {
                    codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
                }
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    let _ = builder.build_unconditional_branch(merge_bb);
                }
            }

            // Position the builder at the merge block so that subsequent
            // statements in the enclosing block are emitted there.
            builder.position_at_end(merge_bb);
            Ok(None)
        }
        HIRStmt::For {
            init,
            cond,
            post,
            body,
        } => {
            // init
            if let Some(i) = init {
                codegen_stmt(ctx, builder, module, vars, current_scope, i)?;
            }

            let func = builder.get_insert_block().unwrap().get_parent().unwrap();

            let cond_bb = ctx.append_basic_block(func, "forcond");
            let body_bb = ctx.append_basic_block(func, "forbody");
            let after_bb = ctx.append_basic_block(func, "afterloop");

            // jump to condition first
            builder.build_unconditional_branch(cond_bb)?;
            builder.position_at_end(cond_bb);

            // condition
            if let Some(c) = cond {
                let cval = codegen_expr(ctx, builder, module, vars, current_scope, c)?;
                builder.build_conditional_branch(cval.into_int_value(), body_bb, after_bb)?;
            } else {
                // no condition = infinite loop
                builder.build_unconditional_branch(body_bb)?;
            }

            // body
            builder.position_at_end(body_bb);

            for s in body.iter() {
                codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
            }

            // post
            if let Some(p) = post {
                codegen_stmt(ctx, builder, module, vars, current_scope, p)?;
            }

            // jump back to condition if block didn't terminate
            if builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                builder.build_unconditional_branch(cond_bb)?;
            }

            // continue here after loop
            builder.position_at_end(after_bb);

            Ok(None)
        }
    }
}
