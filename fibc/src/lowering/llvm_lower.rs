use std::collections::HashMap;

use crate::hir::{
    CompilationUnit, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction, HIRStmt,
    HIRSymbol, HIRTypeKind, Scope,
};
use crate::token::Operator;
use crate::token::builtin::BuiltinType;
use crate::token::identifier::Identifier;
use inkwell::{FloatPredicate, IntPredicate};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, FunctionType};
use inkwell::basic_block::BasicBlock;
use inkwell::values::BasicMetadataValueEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use std::error::Error;

struct LoopContext<'ctx> {
    break_bb: BasicBlock<'ctx>,
    continue_bb: BasicBlock<'ctx>,
}

/// Lower HIR into LLVM IR represented as a string.
pub fn lower(
    compilation_unit: CompilationUnit,
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
                    fn_ty = ctx.void_type().fn_type(&fn_params, hir_function.is_variadic);
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
                    module.add_function(
                        &function_name,
                        fn_ty,
                        Some(inkwell::module::Linkage::External),
                    );
                    continue;
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
                let mut fn_deferred: Vec<crate::hir::HIRStmt> = Vec::new();
                for stmt in hir_function.body.iter() {
                    codegen_stmt(
                        &ctx,
                        &builder,
                        &module,
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
                if let HIRTypeKind::Builtin(BuiltinType::Void) = hir_function.return_type {
                    if let Some(cur_bb) = builder.get_insert_block() {
                        if cur_bb.get_terminator().is_none() {
                            emit_deferred(&ctx, &builder, &module, &mut entry_vars, &mut fn_scope, &fn_deferred)?;
                            let _ = builder.build_return(None);
                        }
                    }
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
        HIRExpressionKind::LiteralFloat { value } => {
            if let BasicTypeEnum::FloatType(ty) =
                map_type_to_llvm(&expr.inferred_type, ctx, Scope::new())?
            {
                Ok(ty.const_float(*value).as_basic_value_enum())
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
        HIRExpressionKind::Null => {
            Ok(ctx.ptr_type(inkwell::AddressSpace::default()).const_null().as_basic_value_enum())
        }
        HIRExpressionKind::Binary {
            left,
            operator,
            right,
        } => {
            let l = codegen_expr(ctx, builder, module, vars, current_scope, &left)?;
            let r = codegen_expr(ctx, builder, module, vars, current_scope, &right)?;
            let is_float = matches!(
                left.inferred_type,
                HIRTypeKind::Builtin(
                    BuiltinType::Float16
                    | BuiltinType::Float32
                    | BuiltinType::Float64
                    | BuiltinType::Float128
                )
            );
            let is_unsigned = matches!(
                left.inferred_type,
                HIRTypeKind::Builtin(
                    BuiltinType::UInt8
                    | BuiltinType::UInt16
                    | BuiltinType::UInt32
                    | BuiltinType::UInt64
                )
            );
            match operator {
                Operator::Plus => {
                    if is_float {
                        Ok(builder.build_float_add(l.into_float_value(), r.into_float_value(), "faddtmp")?.as_basic_value_enum())
                    } else if let HIRTypeKind::Pointer(inner_ty) = &left.inferred_type {
                        let elem_ty = map_type_to_llvm(inner_ty, ctx, current_scope.clone())?;
                        let gep = unsafe {
                            builder.build_gep(
                                elem_ty,
                                l.into_pointer_value(),
                                &[r.into_int_value()],
                                "ptr_add",
                            )?
                        };
                        Ok(gep.as_basic_value_enum())
                    } else {
                        Ok(builder
                            .build_int_add(l.into_int_value(), r.into_int_value(), "addtmp")?
                            .as_basic_value_enum())
                    }
                }
                Operator::Minus => {
                    if is_float {
                        Ok(builder.build_float_sub(l.into_float_value(), r.into_float_value(), "fsubtmp")?.as_basic_value_enum())
                    } else if let HIRTypeKind::Pointer(inner_ty) = &left.inferred_type {
                        let elem_ty = map_type_to_llvm(inner_ty, ctx, current_scope.clone())?;
                        let neg_idx = builder.build_int_neg(r.into_int_value(), "neg_idx")?;
                        let gep = unsafe {
                            builder.build_gep(
                                elem_ty,
                                l.into_pointer_value(),
                                &[neg_idx],
                                "ptr_sub",
                            )?
                        };
                        Ok(gep.as_basic_value_enum())
                    } else {
                        Ok(builder
                            .build_int_sub(l.into_int_value(), r.into_int_value(), "subtmp")?
                            .as_basic_value_enum())
                    }
                }
                Operator::Star => {
                    if is_float {
                        Ok(builder.build_float_mul(l.into_float_value(), r.into_float_value(), "fmultmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_mul(l.into_int_value(), r.into_int_value(), "multmp")?.as_basic_value_enum())
                    }
                }
                Operator::Slash => {
                    if is_float {
                        Ok(builder.build_float_div(l.into_float_value(), r.into_float_value(), "fdivtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_unsigned_div(l.into_int_value(), r.into_int_value(), "udivtmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_signed_div(l.into_int_value(), r.into_int_value(), "divtmp")?.as_basic_value_enum())
                    }
                }
                Operator::Percent => {
                    if is_float {
                        Ok(builder.build_float_rem(l.into_float_value(), r.into_float_value(), "fremtmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_unsigned_rem(l.into_int_value(), r.into_int_value(), "uremtmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_signed_rem(l.into_int_value(), r.into_int_value(), "remtmp")?.as_basic_value_enum())
                    }
                }
                Operator::GreaterThan => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::OGT, l.into_float_value(), r.into_float_value(), "fgttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_compare(IntPredicate::UGT, l.into_int_value(), r.into_int_value(), "ugttmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::SGT, l.into_int_value(), r.into_int_value(), "gttmp")?.as_basic_value_enum())
                    }
                }
                Operator::GreaterEqual => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::OGE, l.into_float_value(), r.into_float_value(), "fgetmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_compare(IntPredicate::UGE, l.into_int_value(), r.into_int_value(), "ugetmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::SGE, l.into_int_value(), r.into_int_value(), "getmp")?.as_basic_value_enum())
                    }
                }
                Operator::LesserThan => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::OLT, l.into_float_value(), r.into_float_value(), "flttmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_compare(IntPredicate::ULT, l.into_int_value(), r.into_int_value(), "ulttmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::SLT, l.into_int_value(), r.into_int_value(), "lttmp")?.as_basic_value_enum())
                    }
                }
                Operator::LesserEqual => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::OLE, l.into_float_value(), r.into_float_value(), "fletmp")?.as_basic_value_enum())
                    } else if is_unsigned {
                        Ok(builder.build_int_compare(IntPredicate::ULE, l.into_int_value(), r.into_int_value(), "uletmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::SLE, l.into_int_value(), r.into_int_value(), "letmp")?.as_basic_value_enum())
                    }
                }
                Operator::DoubleEquals => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::OEQ, l.into_float_value(), r.into_float_value(), "feqtmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::EQ, l.into_int_value(), r.into_int_value(), "eqtmp")?.as_basic_value_enum())
                    }
                }
                Operator::Different => {
                    if is_float {
                        Ok(builder.build_float_compare(FloatPredicate::ONE, l.into_float_value(), r.into_float_value(), "fnetmp")?.as_basic_value_enum())
                    } else {
                        Ok(builder.build_int_compare(IntPredicate::NE, l.into_int_value(), r.into_int_value(), "netmp")?.as_basic_value_enum())
                    }
                }
                Operator::LogicalAnd => Ok(builder
                    .build_and(l.into_int_value(), r.into_int_value(), "andtmp")?
                    .as_basic_value_enum()),
                Operator::LogicalOr => Ok(builder
                    .build_or(l.into_int_value(), r.into_int_value(), "ortmp")?
                    .as_basic_value_enum()),
                Operator::LeftShift => Ok(builder
                    .build_left_shift(l.into_int_value(), r.into_int_value(), "shltmp")?
                    .as_basic_value_enum()),
                Operator::RightShift => Ok(builder
                    .build_right_shift(l.into_int_value(), r.into_int_value(), false, "shrtmp")?
                    .as_basic_value_enum()),
                Operator::Ampersand => Ok(builder
                    .build_and(l.into_int_value(), r.into_int_value(), "bandtmp")?
                    .as_basic_value_enum()),
                Operator::Pipe => Ok(builder
                    .build_or(l.into_int_value(), r.into_int_value(), "bortmp")?
                    .as_basic_value_enum()),
                Operator::Caret => Ok(builder
                    .build_xor(l.into_int_value(), r.into_int_value(), "xortmp")?
                    .as_basic_value_enum()),
                op => Err(format!("unsupported binary operator in codegen: {:?}", op).into()),
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
                    let ret_ty = map_type_to_llvm(&expr.inferred_type, ctx, current_scope.clone())?;
                    let fn_ty = match ret_ty {
                        BasicTypeEnum::IntType(it) => it.fn_type(&param_types, false),
                        BasicTypeEnum::PointerType(pt) => pt.fn_type(&param_types, false),
                        BasicTypeEnum::FloatType(ft) => ft.fn_type(&param_types, false),
                        BasicTypeEnum::StructType(st) => st.fn_type(&param_types, false),
                        other => return Err(format!("unsupported return type for auto-declared function: {:?}", other).into()),
                    };
                    module.add_function(&callee.identifier, fn_ty, None)
                }
            };
            let md_args: Vec<BasicMetadataValueEnum> =
                arg_values.into_iter().map(|v| v.into()).collect();
            let call_site = builder.build_call(fnval, &md_args, "calltmp")?;
            match call_site.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => Ok(v),
                inkwell::values::ValueKind::Instruction(_) => {
                    // void return — return a dummy i32 zero (the value won't be used)
                    Ok(ctx.i32_type().const_int(0, false).as_basic_value_enum())
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
                    let obj_val = codegen_expr(ctx, builder, module, vars, current_scope, object)?;
                    let tmp = builder.build_alloca(obj_val.get_type(), "struct_tmp")?;
                    builder.build_store(tmp, obj_val)?;
                    tmp
                }
            };
            let struct_ty = map_type_to_llvm(&object.inferred_type, ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = struct_ty else {
                return Err(format!("codegen_expr: FieldAccess on non-struct type").into());
            };
            let gep = builder.build_struct_gep(st, struct_ptr, *field_index as u32, "fieldptr")?;
            let field_ty = st
                .get_field_type_at_index(*field_index as u32)
                .ok_or_else(|| format!("codegen_expr: no field at index {}", field_index))?;
            let loaded = builder.build_load(field_ty, gep, "fieldload")?;
            Ok(loaded)
        }
        HIRExpressionKind::StructConstruct { type_name: _, fields } => {
            // Allocate a struct, fill each field, then load the whole value.
            let struct_ty = map_type_to_llvm(&expr.inferred_type, ctx, current_scope.clone())?;
            let BasicTypeEnum::StructType(st) = struct_ty else {
                return Err(format!("codegen_expr: StructConstruct on non-struct type").into());
            };
            let alloca = builder.build_alloca(st, "structtmp")?;
            for (idx, (_, field_expr)) in fields.iter().enumerate() {
                let val = codegen_expr(ctx, builder, module, vars, current_scope, field_expr)?;
                let gep = builder.build_struct_gep(st, alloca, idx as u32, "fieldptr")?;
                builder.build_store(gep, val)?;
            }
            let loaded = builder.build_load(st, alloca, "structload")?;
            Ok(loaded)
        }
        HIRExpressionKind::AddressOf(inner) => {
            // The inner expression must be an identifier; return its alloca directly.
            match &inner.expression {
                HIRExpressionKind::Identifier(name) => {
                    let ptr = *vars.get(name).ok_or_else(|| {
                        format!("codegen_expr: AddressOf: no alloca for identifier {}", name)
                    })?;
                    Ok(ptr.as_basic_value_enum())
                }
                _ => Err("codegen_expr: AddressOf requires an lvalue (identifier)".into()),
            }
        }
        HIRExpressionKind::Deref(inner) => {
            // Codegen the pointer expression, then load through it.
            let ptr_val = codegen_expr(ctx, builder, module, vars, current_scope, inner)?;
            let pointee_llvm_ty = map_type_to_llvm(&expr.inferred_type, ctx, current_scope.clone())?;
            let loaded = builder.build_load(pointee_llvm_ty, ptr_val.into_pointer_value(), "deref")?;
            Ok(loaded)
        }
        HIRExpressionKind::Cast { expr: inner, target_type } => {
            let src = codegen_expr(ctx, builder, module, vars, current_scope, inner)?;
            let dst_ty = map_type_to_llvm(target_type, ctx, current_scope.clone())?;
            match (src, dst_ty) {
                // int -> int
                (BasicValueEnum::IntValue(iv), BasicTypeEnum::IntType(it)) => {
                    let src_bits = iv.get_type().get_bit_width();
                    let dst_bits = it.get_bit_width();
                    if src_bits > dst_bits {
                        Ok(builder.build_int_truncate(iv, it, "cast_trunc")?.as_basic_value_enum())
                    } else if src_bits < dst_bits {
                        // Use signed extend for signed types, zero-extend otherwise
                        let signed = matches!(
                            inner.inferred_type,
                            HIRTypeKind::Builtin(
                                BuiltinType::Int8
                                | BuiltinType::Int16
                                | BuiltinType::Int32
                                | BuiltinType::Int64
                            )
                        );
                        if signed {
                            Ok(builder.build_int_s_extend(iv, it, "cast_sext")?.as_basic_value_enum())
                        } else {
                            Ok(builder.build_int_z_extend(iv, it, "cast_zext")?.as_basic_value_enum())
                        }
                    } else {
                        Ok(iv.as_basic_value_enum())
                    }
                }
                // int -> ptr
                (BasicValueEnum::IntValue(iv), BasicTypeEnum::PointerType(pt)) => {
                    Ok(builder.build_int_to_ptr(iv, pt, "cast_itoptr")?.as_basic_value_enum())
                }
                // ptr -> int
                (BasicValueEnum::PointerValue(pv), BasicTypeEnum::IntType(it)) => {
                    Ok(builder.build_ptr_to_int(pv, it, "cast_ptrtoi")?.as_basic_value_enum())
                }
                // ptr -> ptr (opaque pointers: no-op)
                (BasicValueEnum::PointerValue(pv), BasicTypeEnum::PointerType(_)) => {
                    Ok(pv.as_basic_value_enum())
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
            let arr_ty = map_type_to_llvm(&expr.inferred_type, ctx, current_scope.clone())?;
            let elem_ty = if let HIRTypeKind::Array { element_type, .. } = &expr.inferred_type {
                map_type_to_llvm(element_type, ctx, current_scope.clone())?
            } else {
                unreachable!("ArrayLiteral inferred_type must be Array")
            };
            let alloca = builder.build_alloca(arr_ty, "arrtmp")?;
            let i32_zero = ctx.i32_type().const_int(0, false);
            for (i, elem) in elements.iter().enumerate() {
                let val = codegen_expr(ctx, builder, module, vars, current_scope, elem)?;
                let idx = ctx.i32_type().const_int(i as u64, false);
                let elem_ptr = unsafe {
                    builder.build_gep(
                        arr_ty,
                        alloca,
                        &[i32_zero, idx],
                        "arr_elem_ptr",
                    )?
                };
                builder.build_store(elem_ptr, val)?;
            }
            let loaded = builder.build_load(arr_ty, alloca, "arrload")?;
            Ok(loaded)
        }
        HIRExpressionKind::IndexAccess { object, index } => {
            let idx_val = codegen_expr(ctx, builder, module, vars, current_scope, index)?;
            let elem_ty = map_type_to_llvm(&expr.inferred_type, ctx, current_scope.clone())?;
            match &object.inferred_type {
                HIRTypeKind::Array { .. } => {
                    let arr_ty = map_type_to_llvm(&object.inferred_type, ctx, current_scope.clone())?;
                    // Need a pointer to the array for GEP — store to temp alloca
                    let arr_val = codegen_expr(ctx, builder, module, vars, current_scope, object)?;
                    let alloca = builder.build_alloca(arr_ty, "arridxtmp")?;
                    builder.build_store(alloca, arr_val)?;
                    let i32_zero = ctx.i32_type().const_int(0, false);
                    let gep = unsafe {
                        builder.build_gep(
                            arr_ty,
                            alloca,
                            &[i32_zero, idx_val.into_int_value()],
                            "arr_idx_ptr",
                        )?
                    };
                    let loaded = builder.build_load(elem_ty, gep, "arr_idx_load")?;
                    Ok(loaded)
                }
                _ => {
                    let ptr_val = codegen_expr(ctx, builder, module, vars, current_scope, object)?;
                    let gep = unsafe {
                        builder.build_gep(
                            elem_ty,
                            ptr_val.into_pointer_value(),
                            &[idx_val.into_int_value()],
                            "idx_ptr",
                        )?
                    };
                    let loaded = builder.build_load(elem_ty, gep, "idx_load")?;
                    Ok(loaded)
                }
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
                BuiltinType::Int8 => BasicTypeEnum::IntType(ctx.i8_type()),
                BuiltinType::Int16 => BasicTypeEnum::IntType(ctx.i16_type()),
                BuiltinType::Int32 => BasicTypeEnum::IntType(ctx.i32_type()),
                BuiltinType::Int64 => BasicTypeEnum::IntType(ctx.i64_type()),
                BuiltinType::Float16 => BasicTypeEnum::FloatType(ctx.f16_type()),
                BuiltinType::Float32 => BasicTypeEnum::FloatType(ctx.f32_type()),
                BuiltinType::Float64 => BasicTypeEnum::FloatType(ctx.f64_type()),
                BuiltinType::Float128 => BasicTypeEnum::FloatType(ctx.f128_type()),
                BuiltinType::String => {
                    BasicTypeEnum::PointerType(ctx.ptr_type(inkwell::AddressSpace::default()))
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
                .get(&identifier)
                .ok_or_else(|| format!("identifier {} not found in current scope", identifier))?;
            if let HIRSymbol::Type(ty) = symbol {
                return Ok(map_type_to_llvm(ty, ctx, current_scope.clone())?);
            } else {
                return Err(format!("symbol {:?} is not a type", symbol).into());
            }
        }
        HIRTypeKind::Struct { fields } => {
            let field_types: Vec<BasicTypeEnum> = fields
                .iter()
                .map(|(_, ty)| map_type_to_llvm(ty, ctx, current_scope.clone()))
                .collect::<Result<_, _>>()?;
            Ok(ctx.struct_type(&field_types, false).into())
        }
        HIRTypeKind::Pointer(_) => {
            Ok(ctx.ptr_type(inkwell::AddressSpace::default()).into())
        }
        HIRTypeKind::Array { element_type, size } => {
            let elem_ty = map_type_to_llvm(element_type, ctx, current_scope)?;
            Ok(elem_ty.array_type(*size as u32).into())
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

fn emit_deferred<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    deferred: &[HIRStmt],
) -> Result<(), Box<dyn Error>> {
    // Emit deferred statements in reverse order (LIFO)
    for stmt in deferred.iter().rev() {
        codegen_stmt(ctx, builder, module, vars, current_scope, stmt, None, &mut vec![])?;
    }
    Ok(())
}

fn codegen_stmt<'ctx>(
    ctx: &'ctx Context,
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    mut vars: &mut HashMap<Identifier, PointerValue<'ctx>>,
    current_scope: &mut Scope,
    stmt: &HIRStmt,
    loop_ctx: Option<&LoopContext<'ctx>>,
    deferred: &mut Vec<HIRStmt>,
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

        HIRStmt::FieldAssign { object, field: _, field_index, expr } => {
            let struct_ptr = *vars.get(object).ok_or_else(|| {
                format!("codegen_stmt: no alloca for struct variable {}", object)
            })?;
            // Get the struct type from the binding in scope
            let obj_ty = if let Some(HIRSymbol::Binding(b)) = current_scope.symbols.get(object) {
                map_type_to_llvm(&b.ty, ctx, current_scope.clone())?
            } else {
                return Err(format!("codegen_stmt: {} not found in scope", object).into());
            };
            let BasicTypeEnum::StructType(st) = obj_ty else {
                return Err(format!("codegen_stmt: {} is not a struct", object).into());
            };
            let gep =
                builder.build_struct_gep(st, struct_ptr, *field_index as u32, "fieldassignptr")?;
            let val = codegen_expr(ctx, builder, module, vars, current_scope, expr)?;
            builder.build_store(gep, val)?;
            Ok(None)
        }

        HIRStmt::DerefAssign { pointer, expr } => {
            let ptr_val = codegen_expr(ctx, builder, module, vars, current_scope, pointer)?;
            let val = codegen_expr(ctx, builder, module, vars, current_scope, expr)?;
            builder.build_store(ptr_val.into_pointer_value(), val)?;
            Ok(None)
        }

        HIRStmt::IndexAssign { object, index, expr } => {
            let idx_val = codegen_expr(ctx, builder, module, vars, current_scope, index)?;
            let val = codegen_expr(ctx, builder, module, vars, current_scope, expr)?;
            match &object.inferred_type {
                HIRTypeKind::Array { element_type, .. } => {
                    let arr_ty = map_type_to_llvm(&object.inferred_type, ctx, current_scope.clone())?;
                    let elem_ty = map_type_to_llvm(element_type, ctx, current_scope.clone())?;
                    // Get the alloca for the array identifier directly
                    let arr_ptr = if let HIRExpressionKind::Identifier(name) = &object.expression {
                        *vars.get(name).ok_or_else(|| format!("IndexAssign: array {} not found", name))?
                    } else {
                        let arr_val = codegen_expr(ctx, builder, module, vars, current_scope, object)?;
                        let alloca = builder.build_alloca(arr_ty, "idxassigntmp")?;
                        builder.build_store(alloca, arr_val)?;
                        alloca
                    };
                    let i32_zero = ctx.i32_type().const_int(0, false);
                    let gep = unsafe {
                        builder.build_gep(
                            arr_ty,
                            arr_ptr,
                            &[i32_zero, idx_val.into_int_value()],
                            "arr_assign_ptr",
                        )?
                    };
                    builder.build_store(gep, val)?;
                    Ok(None)
                }
                HIRTypeKind::Pointer(inner) => {
                    let ptr_val = codegen_expr(ctx, builder, module, vars, current_scope, object)?;
                    let elem_ty = map_type_to_llvm(inner, ctx, current_scope.clone())?;
                    let gep = unsafe {
                        builder.build_gep(
                            elem_ty,
                            ptr_val.into_pointer_value(),
                            &[idx_val.into_int_value()],
                            "idx_assign_ptr",
                        )?
                    };
                    builder.build_store(gep, val)?;
                    Ok(None)
                }
                other => Err(format!("codegen_stmt: IndexAssign on non-pointer/array {:?}", other).into()),
            }
        }

        HIRStmt::Expr(e) => {
            let _ = codegen_expr(ctx, builder, module, vars, current_scope, e)?;
            Ok(None)
        }

        HIRStmt::Defer(inner) => {
            // Push onto the deferred stack for later emission
            deferred.push(*inner.clone());
            Ok(None)
        }

        HIRStmt::Return(opt) => {
            // Emit deferred statements before returning (in reverse order)
            emit_deferred(ctx, builder, module, vars, current_scope, deferred)?;
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
            let mut then_deferred: Vec<crate::hir::HIRStmt> = Vec::new();
            for s in hir_if.then_branch.iter() {
                codegen_stmt(ctx, builder, module, vars, current_scope, s, loop_ctx, &mut then_deferred)?;
            }
            // Only emit the fallthrough branch if the block has no terminator
            // yet (i.e. the branch body did not end with `return`).
            if builder
                .get_insert_block()
                .unwrap()
                .get_terminator()
                .is_none()
            {
                emit_deferred(ctx, builder, module, vars, current_scope, &then_deferred)?;
                let _ = builder.build_unconditional_branch(merge_bb);
            }

            // --- else branch (only when one exists) ---
            if let Some(eb) = &hir_if.else_branch {
                builder.position_at_end(else_bb);
                let mut else_deferred: Vec<crate::hir::HIRStmt> = Vec::new();
                for s in eb.iter() {
                    codegen_stmt(ctx, builder, module, vars, current_scope, s, loop_ctx, &mut else_deferred)?;
                }
                if builder
                    .get_insert_block()
                    .unwrap()
                    .get_terminator()
                    .is_none()
                {
                    emit_deferred(ctx, builder, module, vars, current_scope, &else_deferred)?;
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
            // init (no loop context yet)
            if let Some(i) = init {
                codegen_stmt(ctx, builder, module, vars, current_scope, i, None, deferred)?;
            }

            let func = builder.get_insert_block().unwrap().get_parent().unwrap();

            let cond_bb = ctx.append_basic_block(func, "forcond");
            let body_bb = ctx.append_basic_block(func, "forbody");
            let post_bb = ctx.append_basic_block(func, "forpost");
            let after_bb = ctx.append_basic_block(func, "afterloop");

            // continue jumps to post_bb if there is a post-op, otherwise to cond_bb
            let continue_target = if post.is_some() { post_bb } else { cond_bb };
            let for_loop_ctx = LoopContext {
                break_bb: after_bb,
                continue_bb: continue_target,
            };

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
            let mut body_deferred: Vec<crate::hir::HIRStmt> = Vec::new();
            for s in body.iter() {
                codegen_stmt(ctx, builder, module, vars, current_scope, s, Some(&for_loop_ctx), &mut body_deferred)?;
            }
            // fall through to post if no terminator
            if builder.get_insert_block().unwrap().get_terminator().is_none() {
                emit_deferred(ctx, builder, module, vars, current_scope, &body_deferred)?;
                builder.build_unconditional_branch(post_bb)?;
            }

            // post
            builder.position_at_end(post_bb);
            if let Some(p) = post {
                codegen_stmt(ctx, builder, module, vars, current_scope, p, Some(&for_loop_ctx), deferred)?;
            }
            // jump back to condition if block didn't terminate
            if builder.get_insert_block().unwrap().get_terminator().is_none() {
                builder.build_unconditional_branch(cond_bb)?;
            }

            // continue here after loop
            builder.position_at_end(after_bb);

            Ok(None)
        }
        HIRStmt::Break => {
            let lc = loop_ctx.ok_or("break outside of loop")?;
            emit_deferred(ctx, builder, module, vars, current_scope, deferred)?;
            builder.build_unconditional_branch(lc.break_bb)?;
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let dead_bb = ctx.append_basic_block(func, "dead");
            builder.position_at_end(dead_bb);
            Ok(None)
        }
        HIRStmt::Continue => {
            let lc = loop_ctx.ok_or("continue outside of loop")?;
            emit_deferred(ctx, builder, module, vars, current_scope, deferred)?;
            builder.build_unconditional_branch(lc.continue_bb)?;
            let func = builder.get_insert_block().unwrap().get_parent().unwrap();
            let dead_bb = ctx.append_basic_block(func, "dead");
            builder.position_at_end(dead_bb);
            Ok(None)
        }
    }
}
