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
                let _ = hir_function.body.iter().for_each(|stmt| {
                    let _ = codegen_stmt(
                        &ctx,
                        &builder,
                        &module,
                        &mut entry_vars,
                        &mut compilation_unit.scope_root,
                        &stmt,
                    );
                });
            }
            HIRDeclaration::HIRConst(hir_var) => {
                let ty = if let HIRSymbol::Constant(var) = compilation_unit
                    .scope_root
                    .symbols
                    .get(&hir_var.name)
                    .ok_or_else(|| format!("didnt find type for name {}", hir_var.name))?
                {
                    map_type_to_llvm(&var.ty, &ctx, compilation_unit.scope_root.clone())?
                } else {
                    return Err(format!("codegen_expr: {} is not a variable", hir_var.name).into());
                };
                let alloca = match builder.build_alloca(ty, &format!("{}_addr", hir_var.name)) {
                    Ok(a) => a,
                    Err(e) => {
                        eprintln!(
                            "Failed to create alloca for parameter '{}': {}",
                            hir_var.name, e
                        );
                        continue;
                    }
                };
                // store the param value into the alloca
                if let Some(expr) = hir_var.init {
                    let _ = builder.build_store(
                        alloca,
                        codegen_expr(
                            &ctx,
                            &builder,
                            &module,
                            &mut vars,
                            &mut compilation_unit.scope_root.clone(),
                            &expr,
                        )?,
                    );
                }
                vars.insert(hir_var.name, alloca);
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
        HIRExpressionKind::Identifier(name) => {
            let ty = if let HIRSymbol::Constant(var) = current_scope
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
                Operator::Multiply => Ok(builder
                    .build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?
                    .as_basic_value_enum()),
                Operator::Divide => Ok(builder
                    .build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?
                    .as_basic_value_enum()),
                Operator::StrictlyEquals => Ok(builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        l.into_int_value(),
                        r.into_int_value(),
                        "eqtmp",
                    )?
                    .as_basic_value_enum()),
                Operator::GreaterThan => Ok(builder
                    .build_int_compare(
                        IntPredicate::UGT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "gttmp",
                    )?
                    .as_basic_value_enum()),
                Operator::GreaterEqual => Ok(builder
                    .build_int_compare(
                        IntPredicate::UGE,
                        l.into_int_value(),
                        r.into_int_value(),
                        "getmp",
                    )?
                    .as_basic_value_enum()),
                Operator::LesserThan => Ok(builder
                    .build_int_compare(
                        IntPredicate::ULT,
                        l.into_int_value(),
                        r.into_int_value(),
                        "lttmp",
                    )?
                    .as_basic_value_enum()),
                Operator::LesserEqual => Ok(builder
                    .build_int_compare(
                        IntPredicate::ULE,
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
            // lookup function by name
            if let Some(fnval) = module.get_function(&callee.identifier) {
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
        HIRStmt::Const(hir_var) => {
            let ty = if let HIRSymbol::Constant(var) = current_scope
                .symbols
                .get(&hir_var.name)
                .ok_or_else(|| format!("didnt find type for name {}", hir_var.name))?
            {
                map_type_to_llvm(&var.ty, &ctx, current_scope.clone())?
            } else {
                return Err(format!("codegen_expr: {} is not a variable", hir_var.name).into());
            };
            let alloca = match builder.build_alloca(ty, &format!("{}_addr", hir_var.name)) {
                Ok(a) => a,
                Err(e) => {
                    return Err(format!(
                        "Failed to create alloca for parameter '{}': {}",
                        hir_var.name, e
                    )
                    .into());
                }
            };
            // store the param value into the alloca
            if let Some(expr) = &hir_var.init {
                let _ = builder.build_store(
                    alloca,
                    codegen_expr(
                        &ctx,
                        &builder,
                        &module,
                        &mut vars,
                        &mut current_scope.clone(),
                        &expr,
                    )?,
                )?;
            }
            vars.insert(hir_var.name.clone(), alloca);
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
                let _ = builder.build_return(Some(&v));
            } else {
                let _ = builder.build_return(None);
            }
            Ok(None)
        }

        HIRStmt::If(hir_if) => {
            // create blocks
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let then_bb = ctx.append_basic_block(func, "then");
            let else_bb = ctx.append_basic_block(func, "else");
            let merge_bb = if !hir_if.then_branch_terminates() || !hir_if.else_branch_terminates() {
                Some(ctx.append_basic_block(func, "ifcont"))
            } else {
                None
            };

            let cond_v = codegen_expr(ctx, builder, module, vars, current_scope, &hir_if.cond)?;
            let _ = builder.build_conditional_branch(cond_v.into_int_value(), then_bb, else_bb);

            // then
            builder.position_at_end(then_bb);
            for s in hir_if.then_branch.iter() {
                codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
            }
            if let Some(mbb) = merge_bb {
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    let _ = builder.build_unconditional_branch(mbb);
                }
            }

            // else
            builder.position_at_end(else_bb);
            if let Some(eb) = &hir_if.else_branch {
                for s in eb.iter() {
                    codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
                }
            }
            if let Some(mbb) = merge_bb {
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    let _ = builder.build_unconditional_branch(mbb);
                }
            }

            // continue
            if let Some(mbb) = merge_bb {
                builder.position_at_end(mbb);
            }
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
                codegen_stmt(ctx, builder, module, vars, current_scope, i)?;
            }
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let loop_bb = ctx.append_basic_block(func, "loop");
            let after_bb = ctx.append_basic_block(func, "afterloop");

            let _ = builder.build_unconditional_branch(loop_bb);
            builder.position_at_end(loop_bb);

            // cond
            if let Some(c) = cond {
                let cval = codegen_expr(ctx, builder, module, vars, current_scope, c)?;
                // TODO: some other way?
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
                    codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
                }
                if let Some(p) = post {
                    codegen_stmt(ctx, builder, module, vars, current_scope, p)?;
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
                    codegen_stmt(ctx, builder, module, vars, current_scope, s)?;
                }
                if let Some(p) = post {
                    codegen_stmt(ctx, builder, module, vars, current_scope, p)?;
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
