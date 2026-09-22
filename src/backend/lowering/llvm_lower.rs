use std::collections::HashMap;

use super::context::{CodegenCtx, FunctionLowering, create_entry_allocas};
use super::error::LowerError;
use super::types::map_type_to_llvm;
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::{
    ScopeKind, Ty, TypedBinding, TypedDecl, TypedProgram, TypedSymbol,
};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, FunctionType};

pub fn lower(
    compilation_unit: TypedProgram,
    module_name: &str,
    emit_bounds_checks: bool,
) -> Result<String, LowerError> {
    let ctx = Context::create();
    let module: Module<'_> = ctx.create_module(module_name);
    let builder: Builder<'_> = ctx.create_builder();

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
                // rather than creating a duplicate with a mangled name.
                // Duplicate definitions cannot reach lowering: the driver
                // merges imported + local declarations with
                // `dedupe_declarations`, so each symbol is emitted exactly once.
                let function = match module.get_function(&function_name) {
                    Some(f) => f,
                    None => module.add_function(&function_name, fn_ty, None),
                };
                let entry = ctx.append_basic_block(function, "entry");
                builder.position_at_end(entry);
                let entry_vars = create_entry_allocas(
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
                let mut fl = FunctionLowering::new(
                    &codegen_ctx,
                    function,
                    entry_vars,
                    fn_scope,
                    emit_bounds_checks,
                );
                for stmt in typed_function.body.iter() {
                    fl.codegen_stmt(stmt)?;
                }
                // For void functions, if the current block at the end of
                // codegen has no terminator (i.e. the function falls off the
                // end without an explicit `return`), emit `ret void` so the
                // function returns cleanly.
                if let Ty::Builtin(BuiltinType::Void) = typed_function.return_type
                    && let Some(cur_bb) = builder.get_insert_block()
                    && cur_bb.get_terminator().is_none()
                {
                    let stack = fl.deferred_stack.clone();
                    fl.emit_frames_from(stack, 0)?;
                    let _ = builder.build_return(None);
                }
                fl.scope.exit_scope();
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
                let function = builder
                    .get_insert_block()
                    .expect("checked above")
                    .get_parent()
                    .expect("insert block inside a function body has a parent");
                let mut fl = FunctionLowering::new(
                    &codegen_ctx,
                    function,
                    HashMap::new(),
                    compilation_unit.symbol_table.clone(),
                    emit_bounds_checks,
                );
                let ty = map_type_to_llvm(
                    &typed_binding.ty,
                    &ctx,
                    compilation_unit.symbol_table.clone(),
                )?;
                let alloca = match fl
                    .ctx
                    .builder
                    .build_alloca(ty, &format!("{}_addr", typed_binding.name))
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
                let init = typed_binding
                    .init
                    .as_ref()
                    .ok_or_else(|| "no init for binding".to_string())?;
                // store the value into the alloca
                let _ = fl.ctx.builder.build_store(alloca, fl.codegen_expr(init)?);
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
