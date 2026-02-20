use std::collections::HashMap;

use crate::hir::{HIRExpr, HIRFunction, HIRStmt, Type};
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::BasicMetadataValueEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};

fn llvm_type_for_int<'ctx>(ctx: &'ctx Context) -> BasicTypeEnum<'ctx> {
    ctx.i64_type().as_basic_type_enum()
}

fn map_hir_type_to_llvm<'ctx>(_ty: &Type, ctx: &'ctx Context) -> BasicTypeEnum<'ctx> {
    // Minimal mapping: Int -> i64, Bool -> i64, Unit -> void handled separately
    llvm_type_for_int(ctx)
}

fn create_entry_allocas<'ctx>(
    ctx: &'ctx Context,
    _module: &Module<'ctx>,
    _builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    hir_fn: &HIRFunction,
) -> HashMap<String, PointerValue<'ctx>> {
    let mut vars = std::collections::HashMap::new();

    // Create an alloca for each parameter in the entry block
    let entry = function.get_first_basic_block().unwrap();
    let builder_at_entry = ctx.create_builder();
    if let Some(first_instr) = entry.get_first_instruction() {
        builder_at_entry.position_before(&first_instr);
    } else {
        builder_at_entry.position_at_end(entry);
    }

    for (i, (name, _ty)) in hir_fn.params.iter().enumerate() {
        let param = function.get_nth_param(i as u32).unwrap();
        // param.get_type() is a BasicTypeEnum already; build_alloca expects BasicTypeEnum
        let alloca = match builder_at_entry.build_alloca(
            ctx.i64_type().as_basic_type_enum(),
            &format!("{}_addr", name),
        ) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Failed to create alloca for parameter '{}': {}", name, e);
                continue;
            }
        };
        // store the param value into the alloca
        let _ = builder_at_entry.build_store(alloca, param);
        vars.insert(name.clone(), alloca);
    }

    vars
}

fn codegen_expr<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    vars: &mut std::collections::HashMap<String, PointerValue<'ctx>>,
    expr: &HIRExpr,
) -> Result<BasicValueEnum<'ctx>, BuilderError> {
    match expr {
        HIRExpr::LiteralInt(i) => Ok(ctx
            .i64_type()
            .const_int(*i as u64, true)
            .as_basic_value_enum()),
        HIRExpr::LiteralBool(b) => {
            let v = if *b { 1 } else { 0 };
            Ok(ctx.i64_type().const_int(v, false).as_basic_value_enum())
        }
        HIRExpr::Var(name) => {
            let ptr = vars.get(name).unwrap();
            let load = builder.build_load(
                ctx.i64_type().as_basic_type_enum(),
                *ptr,
                &format!("load_{}", name),
            )?;
            Ok(load)
        }
        HIRExpr::Null => Ok(ctx.i64_type().const_int(0, false).as_basic_value_enum()),
        HIRExpr::Binary { left, op, right } => {
            let l = codegen_expr(ctx, builder, module, vars, left)?;
            let r = codegen_expr(ctx, builder, module, vars, right)?;
            // All arithmetic as i64 for minimal pass
            if op.contains("Plus") || op.contains("Add") {
                Ok(builder
                    .build_int_add(l.into_int_value(), r.into_int_value(), "addtmp")?
                    .as_basic_value_enum())
            } else if op.contains("Minus") {
                Ok(builder
                    .build_int_sub(l.into_int_value(), r.into_int_value(), "subtmp")?
                    .as_basic_value_enum())
            } else if op.contains("Multiply") {
                Ok(builder
                    .build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?
                    .as_basic_value_enum())
            } else if op.contains("Divide") {
                Ok(builder
                    .build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?
                    .as_basic_value_enum())
            } else {
                panic!("unsupported binary operator in codegen: {}", op);
            }
        }
        HIRExpr::Call { callee, args } => {
            // Resolve callee by name from module
            // For minimal lowering we assume function exists and returns i64
            let mut arg_values = Vec::new();
            for a in args.iter() {
                let av = codegen_expr(ctx, builder, module, vars, a)?;
                arg_values.push(av);
            }
            // lookup function by name
            if let Some(fnval) = module.get_function(callee) {
                // convert BasicValueEnum args into BasicMetadataValueEnum expected by build_call
                let mut md_args: Vec<BasicMetadataValueEnum> = Vec::new();
                for (i, _) in fnval.get_type().get_param_types().iter().enumerate() {
                    let v = arg_values[i].clone();
                    md_args.push(v.into());
                }
                let call_site = builder.build_call(fnval, &md_args, "calltmp")?;
                // try_as_basic_value left is a BasicValueOption; for our minimal lowering assume a value
                Ok(call_site.try_as_basic_value().unwrap_basic())
            } else {
                panic!("unknown function '{}' in lowering", callee);
            }
        }
    }
}

fn codegen_stmt<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    vars: &mut std::collections::HashMap<String, PointerValue<'ctx>>,
    stmt: &HIRStmt,
) -> Result<Option<BasicValueEnum<'ctx>>, BuilderError> {
    match stmt {
        HIRStmt::Let {
            name,
            init,
            ty: _ty,
        } => {
            // allocate and optionally initialize
            let alloca = builder.build_alloca(
                ctx.i64_type().as_basic_type_enum(),
                &format!("{}_alloca", name),
            )?;
            if let Some(init_expr) = init {
                let v = codegen_expr(ctx, builder, module, vars, init_expr)?;
                let _ = builder.build_store(alloca, v);
            } else {
                let _ = builder.build_store(
                    alloca,
                    ctx.i64_type().const_int(0, false).as_basic_value_enum(),
                );
            }
            vars.insert(name.clone(), alloca);
            Ok(None)
        }
        HIRStmt::Assign { name, expr } => {
            let v = codegen_expr(ctx, builder, module, vars, expr)?;
            if let Some(ptr) = vars.get(name) {
                let _ = builder.build_store(*ptr, v);
                Ok(None)
            } else {
                panic!("assignment to unknown variable '{}' in lowering", name);
            }
        }
        HIRStmt::Expr(e) => {
            let _ = codegen_expr(ctx, builder, module, vars, e)?;
            Ok(None)
        }
        HIRStmt::Return(opt) => {
            if let Some(e) = opt {
                let v = codegen_expr(ctx, builder, module, vars, e)?;
                let _ = builder.build_return(Some(&v));
            } else {
                let _ = builder.build_return(None);
            }
            Ok(None)
        }
        HIRStmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            // create blocks
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let then_bb = ctx.append_basic_block(func, "then");
            let else_bb = ctx.append_basic_block(func, "else");
            let merge_bb = ctx.append_basic_block(func, "ifcont");

            let cond_v = codegen_expr(ctx, builder, module, vars, cond)?;
            // compare cond != 0
            let zero = ctx.i64_type().const_int(0, false);
            let cond_bool = builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cond_v.into_int_value(),
                zero,
                "ifcond",
            )?;
            let _ = builder.build_conditional_branch(cond_bool, then_bb, else_bb);

            // then
            builder.position_at_end(then_bb);
            for s in then_branch.iter() {
                codegen_stmt(ctx, builder, module, vars, s)?;
            }
            if builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                let _ = builder.build_unconditional_branch(merge_bb);
            }

            // else
            builder.position_at_end(else_bb);
            if let Some(eb) = else_branch {
                for s in eb.iter() {
                    codegen_stmt(ctx, builder, module, vars, s)?;
                }
            }
            if builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                let _ = builder.build_unconditional_branch(merge_bb);
            }

            // continue
            builder.position_at_end(merge_bb);
            Ok(None)
        }
        HIRStmt::For {
            init,
            cond,
            post,
            body,
        } => {
            // Lower for to while:
            // init
            if let Some(i) = init {
                codegen_stmt(ctx, builder, module, vars, i)?;
            }
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let loop_bb = ctx.append_basic_block(func, "loop");
            let after_bb = ctx.append_basic_block(func, "afterloop");

            let _ = builder.build_unconditional_branch(loop_bb);
            builder.position_at_end(loop_bb);

            // cond
            if let Some(c) = cond {
                let cval = codegen_expr(ctx, builder, module, vars, c)?;
                let zero = ctx.i64_type().const_int(0, false);
                let cond_bool = builder.build_int_compare(
                    inkwell::IntPredicate::NE,
                    cval.into_int_value(),
                    zero,
                    "forcond",
                )?;
                let body_bb = ctx.append_basic_block(func, "forbody");
                let _ = builder.build_conditional_branch(cond_bool, body_bb, after_bb);
                builder.position_at_end(body_bb);
                for s in body.iter() {
                    codegen_stmt(ctx, builder, module, vars, s)?;
                }
                if let Some(p) = post {
                    codegen_stmt(ctx, builder, module, vars, p)?;
                }
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    let _ = builder.build_unconditional_branch(loop_bb);
                }
                builder.position_at_end(after_bb);
            } else {
                // infinite loop body
                for s in body.iter() {
                    codegen_stmt(ctx, builder, module, vars, s)?;
                }
                if let Some(p) = post {
                    codegen_stmt(ctx, builder, module, vars, p)?;
                }
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    let _ = builder.build_unconditional_branch(loop_bb);
                }
                builder.position_at_end(after_bb);
            }

            Ok(None)
        }
    }
}

/// Lower HIR into LLVM IR represented as a string. Minimal lowering: i64-based values.
pub fn lower(hir: &[HIRFunction]) -> Result<String, BuilderError> {
    let ctx = Context::create();
    let module = ctx.create_module("fib_module");
    let builder = ctx.create_builder();

    // Create function declarations and bodies
    for hf in hir.iter() {
        // Build function type (all params -> i64, returns i64 or void)
        let mut param_types = Vec::new();
        for (_name, ty) in hf.params.iter() {
            let llvm_ty = map_hir_type_to_llvm(ty, &ctx);
            param_types.push(llvm_ty);
        }
        let ret_llvm = map_hir_type_to_llvm(&hf.ret_type, &ctx);
        let fn_type = ret_llvm.fn_type(
            &param_types
                .iter()
                .map(|t| (*t).into())
                .collect::<Vec<BasicMetadataTypeEnum>>(),
            false,
        );
        let function = module.add_function(&hf.name, fn_type, None);

        // Create entry block
        let entry = ctx.append_basic_block(function, "entry");
        builder.position_at_end(entry);

        // Create allocas for params
        let mut vars = create_entry_allocas(&ctx, &module, &builder, function, hf);

        // Emit statements
        for s in hf.body.iter() {
            codegen_stmt(&ctx, &builder, &module, &mut vars, s)?;
        }

        // Ensure function has a return
        if builder
            .get_insert_block()
            .unwrap()
            .get_terminator()
            .is_none()
        {
            if let Type::Unit = hf.ret_type {
                let _ = builder.build_return(None);
            } else {
                let _ = builder.build_return(Some(
                    &ctx.i64_type().const_int(0, false).as_basic_value_enum(),
                ));
            }
        }
    }

    // Return LLVM IR as string.
    // Some LLVM builds (with opaque pointers) print pointer types as `ptr` which
    // older clang versions reject. For now, post-process the printed IR to
    // restore typed pointers for our simple i64-based lowering.
    let ir = module.print_to_string().to_string();
    let ir = ir.replace("ptr %", "i64* %").replace("ptr @", "i64* @");
    Ok(ir)
}
