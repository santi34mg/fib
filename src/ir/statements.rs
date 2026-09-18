use super::builder::{FunctionBuilder, LoopTarget};
use super::expressions::lower_expr;
use super::{Instruction, IrFunction, LowerError, Operand, UnOp};
use crate::frontend::typed_ast::{Ty, TypedStatement};

pub(super) fn lower_function(
    f: &crate::frontend::typed_ast::TypedFunction,
) -> Result<IrFunction, LowerError> {
    let mut b = FunctionBuilder::new(
        f.name.value.clone(),
        f.return_type.clone(),
        f.is_extern,
        f.is_variadic,
    );
    // Params are allocas like locals so `Store`/`Load` stay uniform.
    // Extern functions get no body: record params but emit nothing.
    for (name, ty) in &f.params {
        let sym = b.fresh_symbol(name.value.clone(), ty.clone());
        b.params.push((sym, name.value.clone(), ty.clone()));
        if !f.is_extern {
            b.emit(Instruction::Alloca {
                sym,
                name: name.value.clone(),
                ty: ty.clone(),
            });
        }
    }
    if f.is_extern {
        // Drop the param symbols from the block stream: externs lower to a
        // declaration only. Clear symbols' allocas by returning a function
        // with no blocks.
        let mut func = b.finish();
        func.blocks.clear();
        func.symbols.clear();
        return Ok(func);
    }
    b.enter_scope();
    // Re-bind params into the function scope frame (fresh_symbol above put
    // them in the outer frame; mirror them here so shadowing works).
    // Note: fresh_symbol already inserted into scopes[0]; re-insert the same
    // ids into the new frame without allocating new slots.
    for (sym, name, _) in b.params.clone() {
        if let Some(frame) = b.scopes.last_mut() {
            frame.insert(name, sym);
        } else {
            debug_assert!(false, "lower_function: no function scope frame");
        }
    }
    for stmt in &f.body {
        lower_stmt(&mut b, stmt)?;
    }
    // Void fallthrough: run deferred frame 0 then return.
    // (Non-void functions must return explicitly; the backend seals the rest
    // with `unreachable`, same as today.)
    use crate::frontend::tokens::builtin::BuiltinType;
    if matches!(f.return_type, Ty::Builtin(BuiltinType::Void)) {
        let needs_ret = !matches!(
            b.instrs.last(),
            Some(Instruction::Return { .. }) | Some(Instruction::Unreachable)
        );
        if needs_ret {
            let deferred: Vec<TypedStatement> = b.deferred.first().cloned().unwrap_or_default();
            for d in deferred.iter().rev() {
                lower_stmt(&mut b, d)?;
            }
            b.emit(Instruction::Return { values: vec![] });
        }
    }
    b.exit_scope();
    Ok(b.finish())
}

pub(super) fn lower_deferred_from(
    b: &mut FunctionBuilder,
    from_depth: usize,
) -> Result<(), LowerError> {
    // Clone the frames above `from_depth` so we can recursively lower them
    // while the stack is borrowed immutably.
    let frames: Vec<Vec<TypedStatement>> = b.deferred[from_depth..].to_vec();
    for frame in frames.iter().rev() {
        for stmt in frame.iter().rev() {
            lower_stmt(b, stmt)?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

pub(super) fn lower_stmt(b: &mut FunctionBuilder, stmt: &TypedStatement) -> Result<(), LowerError> {
    match stmt {
        TypedStatement::Binding(binding) => {
            if matches!(binding.ty, Ty::Type) {
                return Err(LowerError::Unsupported(
                    "comptime `type` binding in runtime code".to_string(),
                ));
            }
            let sym = b.fresh_symbol(binding.name.value.clone(), binding.ty.clone());
            b.emit(Instruction::Alloca {
                sym,
                name: binding.name.value.clone(),
                ty: binding.ty.clone(),
            });
            if let Some(init) = &binding.init {
                let v = lower_expr(b, init)?;
                b.emit(Instruction::Store { dst: sym, src: v });
            }
            Ok(())
        }
        TypedStatement::Assign { name, expr } => {
            let sym = b.lookup(&name.value).ok_or_else(|| {
                LowerError::Unsupported(format!("assign to unknown variable '{}'", name))
            })?;
            let v = lower_expr(b, expr)?;
            b.emit(Instruction::Store { dst: sym, src: v });
            Ok(())
        }
        TypedStatement::Expr(e) => {
            let _ = lower_expr(b, e)?;
            Ok(())
        }
        TypedStatement::Return(ret) => {
            lower_deferred_from(b, 0)?;
            let values = match ret {
                None => vec![],
                Some(r) => {
                    let mut vs = Vec::with_capacity(r.values.len());
                    for v in &r.values {
                        vs.push(lower_expr(b, v)?);
                    }
                    vs
                }
            };
            b.emit(Instruction::Return { values });
            Ok(())
        }
        TypedStatement::If(tif) => {
            let cond = lower_expr(b, &tif.cond)?;
            let else_label = b.fresh_label();
            let merge_label = b.fresh_label();
            // if !cond goto else
            // (backend lowers IfGoto as cond_br cond then fallthrough else target;
            // to express "else" we need an explicit negated temp)
            let ntmp = b.fresh_temp();
            b.emit(Instruction::Unary {
                dst: ntmp,
                op: UnOp::Not,
                src: cond,
                ty: crate::frontend::typed_ast::Ty::Builtin(
                    crate::frontend::tokens::builtin::BuiltinType::Boolean,
                ),
            });
            b.emit(Instruction::IfGoto {
                cond: Operand::Temp(ntmp),
                target: else_label,
            });
            b.enter_scope();
            b.deferred.push(Vec::new());
            for s in &tif.then_branch {
                lower_stmt(b, s)?;
            }
            b.deferred.pop();
            b.exit_scope();
            b.emit(Instruction::Goto {
                target: merge_label,
            });
            b.emit(Instruction::LabelDef(else_label));
            if let Some(else_branch) = &tif.else_branch {
                b.enter_scope();
                b.deferred.push(Vec::new());
                for s in else_branch {
                    lower_stmt(b, s)?;
                }
                b.deferred.pop();
                b.exit_scope();
            }
            b.emit(Instruction::LabelDef(merge_label));
            Ok(())
        }
        TypedStatement::For {
            init,
            cond,
            post,
            body,
        } => {
            b.enter_scope();
            if let Some(init_stmt) = init {
                lower_stmt(b, init_stmt)?;
            }
            let cond_label = b.fresh_label();
            let body_label = b.fresh_label();
            let post_label = b.fresh_label();
            let after_label = b.fresh_label();
            b.emit(Instruction::LabelDef(cond_label));
            if let Some(c) = cond {
                let cv = lower_expr(b, c)?;
                let ntmp = b.fresh_temp();
                b.emit(Instruction::Unary {
                    dst: ntmp,
                    op: UnOp::Not,
                    src: cv,
                    ty: crate::frontend::typed_ast::Ty::Builtin(
                        crate::frontend::tokens::builtin::BuiltinType::Boolean,
                    ),
                });
                b.emit(Instruction::IfGoto {
                    cond: Operand::Temp(ntmp),
                    target: after_label,
                });
            }
            b.emit(Instruction::Goto { target: body_label });
            b.emit(Instruction::LabelDef(body_label));
            let depth = b.deferred.len();
            b.loops.push(LoopTarget {
                break_label: after_label,
                continue_label: post_label,
                deferred_depth: depth,
            });
            b.enter_scope();
            b.deferred.push(Vec::new());
            for s in body {
                lower_stmt(b, s)?;
            }
            b.deferred.pop();
            b.exit_scope();
            b.loops.pop();
            b.emit(Instruction::LabelDef(post_label));
            if let Some(post_stmt) = post {
                lower_stmt(b, post_stmt)?;
            }
            b.emit(Instruction::Goto { target: cond_label });
            b.emit(Instruction::LabelDef(after_label));
            b.exit_scope();
            Ok(())
        }
        TypedStatement::Break => {
            let target = b
                .loops
                .last()
                .ok_or_else(|| LowerError::Unsupported("`break` outside loop".to_string()))?;
            let (lbl, depth) = (target.break_label, target.deferred_depth);
            lower_deferred_from(b, depth)?;
            b.emit(Instruction::Goto { target: lbl });
            Ok(())
        }
        TypedStatement::Continue => {
            let target = b
                .loops
                .last()
                .ok_or_else(|| LowerError::Unsupported("`continue` outside loop".to_string()))?;
            let (lbl, depth) = (target.continue_label, target.deferred_depth);
            lower_deferred_from(b, depth)?;
            b.emit(Instruction::Goto { target: lbl });
            Ok(())
        }
        TypedStatement::Defer(inner) => {
            match b.deferred.last_mut() {
                Some(frame) => frame.push((**inner).clone()),
                None => debug_assert!(false, "defer outside of function body"),
            }
            Ok(())
        }
        TypedStatement::MultiAssign { .. } => Err(LowerError::Unsupported(
            "multi-assign (tuple unpack in phase 2b)".to_string(),
        )),
        TypedStatement::MultiBinding { .. } => Err(LowerError::Unsupported(
            "multi-binding (tuple unpack in phase 2b)".to_string(),
        )),
        TypedStatement::FieldAssign { .. } => Err(LowerError::Unsupported(
            "field-assign (phase 2b: FieldStore)".to_string(),
        )),
        TypedStatement::DerefAssign { .. } => Err(LowerError::Unsupported(
            "deref-assign (phase 2b: StorePtr)".to_string(),
        )),
        TypedStatement::IndexAssign { .. } => Err(LowerError::Unsupported(
            "index-assign (phase 2b: IndexStore)".to_string(),
        )),
        TypedStatement::Switch { .. } => Err(LowerError::Unsupported(
            "switch (phase 2b: SwitchDispatch)".to_string(),
        )),
    }
}
