use anyhow::bail;

use super::builder::{BasicBlockBuilder, LoopTarget};
use super::expressions::lower_expr;
use super::{Instruction, Operand, NameId, UnOp};
use crate::frontend::tokens::builtin::BuiltinType;
use crate::frontend::typed_ast::ty::Ty;
use crate::frontend::typed_ast::typed_statement::TypedStatement;
use crate::frontend::typed_ast::{TypedExpr, TypedExpressionKind, TypedFunction};
use crate::ir;

pub(super) fn lower_function(f: TypedFunction) -> anyhow::Result<Vec<ir::BasicBlock>> {
    let mut b = BasicBlockBuilder::new();
    // Params are allocas like locals so `Store`/`Load` stay uniform.
    // Extern functions get no body: record params but emit nothing.
    for (name, ty) in &f.params {
        let sym = b.fresh_symbol(name.raw_str.clone(), ty.clone());
        b.params.push((sym, name.raw_str.clone(), ty.clone()));
        if !f.is_extern {
            b.emit(Instruction::Alloca {
                sym,
                name: &name.raw_str,
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
    b: &mut BasicBlockBuilder,
    from_depth: usize,
) -> anyhow::Result<()> {
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

pub(super) fn lower_stmt(b: &mut BasicBlockBuilder, stmt: &TypedStatement) -> anyhow::Result<()> {
    match stmt {
        TypedStatement::Binding(binding) => {
            if matches!(binding.ty, Ty::Type) {
                bail!("comptime `type` binding in runtime code")
            }
            let sym = b.fresh_symbol(binding.symbol.name.clone(), binding.ty.clone());
            b.emit(Instruction::Alloca {
                sym,
                name: binding.symbol.name.clone(),
                ty: binding.ty.clone(),
            });
            if let Some(init) = &binding.init {
                let v = lower_expr(b, init)?;
                b.emit(Instruction::Store { dst: sym, src: v });
            }
            Ok(())
        }
        TypedStatement::Assign { symbol, expr } => {
            let sym = b
                .lookup(&symbol.name).co
                ntext(format!("assign to unknown variable '{}'", symbol))?;
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
                ty: Ty::Builtin(BuiltinType::Boolean),
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
        TypedStatement::While { cond, body } => {
            b.enter_scope();
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
                    ty: Ty::Builtin(BuiltinType::Boolean),
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
            b.emit(Instruction::Goto { target: cond_label });
            b.emit(Instruction::LabelDef(after_label));
            b.exit_scope();
            Ok(())
        }
        TypedStatement::Break => {
            let target = b.loops.last().context("`break` outside loop")?;
            let (lbl, depth) = (target.break_label, target.deferred_depth);
            lower_deferred_from(b, depth)?;
            b.emit(Instruction::Goto { target: lbl });
            Ok(())
        }
        TypedStatement::Continue => {
            let target = b.loops.last().context("`continue` outside loop")?;
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
        TypedStatement::MultiAssign { targets, values } => lower_multi_assign(b, targets, values),
        TypedStatement::MultiBinding { bindings, values } => {
            lower_multi_binding(b, bindings, values)
        }
        TypedStatement::FieldAssign {
            object,
            field_index,
            expr,
            ..
        } => {
            // Simple case: `ident.field = expr` where the base is a named
            // place. Nested bases (`a.b.c = …`) stay `Unsupported` so the
            // driver falls back to the direct path.
            let base_sym = match &object.expression {
                TypedExpressionKind::Identifier(name) => {
                    b.lookup(&name.raw_str)
                        .context("field-assign base unknown '{}'")?;
                }
                _ => {
                    bail!("field-assign of non-identifier base (fallback)".to_string(),)
                }
            };
            let val = lower_expr(b, expr)?;
            let ptr_tmp = b.fresh_temp();
            b.emit(Instruction::AddrOf {
                dst: ptr_tmp,
                src: base_sym,
            });
            b.emit(Instruction::FieldStore {
                base: Operand::Temp(ptr_tmp),
                field_index: *field_index,
                src: val,
            });
            Ok(())
        }
        TypedStatement::DerefAssign { pointer, expr } => {
            let ptr = lower_expr(b, pointer)?;
            let val = lower_expr(b, expr)?;
            b.emit(Instruction::StorePtr { ptr, src: val });
            Ok(())
        }
        TypedStatement::IndexAssign {
            object,
            index,
            expr,
        } => lower_index_assign(b, object, index, expr),
        TypedStatement::Switch { subject, arms } => lower_switch(b, subject, arms),
    }
}

fn lower_lvalue_store(
    b: &mut BasicBlockBuilder,
    target: &TypedExpr,
    value: Operand,
) -> anyhow::Result<()> {
    match &target.expression {
        TypedExpressionKind::Identifier(name) => {
            let sym = b
                .lookup(&name.raw_str)
                .context("assign to unknown variable '{}'")?;
            b.emit(Instruction::Store {
                dst: sym,
                src: value,
            });
            Ok(())
        }
        TypedExpressionKind::Deref(inner) => {
            // `*p = v`: lower the pointer, then store through it. The inner
            // pointer expr is usually an identifier (already supported).
            let ptr = lower_expr(b, inner)?;
            b.emit(Instruction::StorePtr { ptr, src: value });
            Ok(())
        }
        TypedExpressionKind::FieldAccess {
            object,
            field_index,
            ..
        } => {
            // Only `ident.field = v` for now; nested chains fall back.
            let base_sym = match &object.expression {
                TypedExpressionKind::Identifier(name) => {
                    b.lookup(&name.raw_str)
                        .context("field-assign base unknown '{}'")?;
                }
                _ => {
                    bail!("multi-assign field target of non-identifier base (fallback)")
                }
            };
            let ptr_tmp = b.fresh_temp();
            b.emit(Instruction::AddrOf {
                dst: ptr_tmp,
                src: base_sym,
            });
            b.emit(Instruction::FieldStore {
                base: Operand::Temp(ptr_tmp),
                field_index: *field_index,
                src: value,
            });
            Ok(())
        }
        TypedExpressionKind::IndexAccess { object, index } => {
            lower_index_store_operand(b, object, index, value)
        }
        _ => bail!("multi-assign to non-place target (fallback)"),
    }
}

fn lower_index_store_operand(
    b: &mut BasicBlockBuilder,
    object: &TypedExpr,
    index: &TypedExpr,
    value: Operand,
) -> anyhow::Result<()> {
    let index_op = lower_expr(b, index)?;
    // Array-typed identifier: address-of the slot; pointer-typed: load it.
    // Anything else falls back to the direct path.
    match &object.expression {
        TypedExpressionKind::Identifier(name) => {
            let sym: NameId = b
                .lookup(&name.raw_str)
                .context("index-assign base unknown '{}'")?;
            match &object.inferred_type {
                Ty::Array { .. } => {
                    let ptr_tmp = b.fresh_temp();
                    b.emit(Instruction::AddrOf {
                        dst: ptr_tmp,
                        src: sym,
                    });
                    b.emit(Instruction::IndexStore {
                        base: Operand::Temp(ptr_tmp),
                        index: index_op,
                        src: value,
                    });
                    Ok(())
                }
                Ty::Pointer(_) => {
                    let ptr = lower_expr(b, object)?;
                    b.emit(Instruction::IndexStore {
                        base: ptr,
                        index: index_op,
                        src: value,
                    });
                    Ok(())
                }
                _ => bail!("index-assign on non-array/non-pointer (fallback)"),
            }
        }
        _ => bail!("index-assign of non-identifier base (fallback)"),
    }
}

fn lower_index_assign(
    b: &mut BasicBlockBuilder,
    object: &TypedExpr,
    index: &TypedExpr,
    expr: &TypedExpr,
) -> anyhow::Result<()> {
    let val = lower_expr(b, expr)?;
    lower_index_store_operand(b, object, index, val)
}

fn lower_multi_assign(
    b: &mut BasicBlockBuilder,
    targets: &[TypedExpr],
    values: &[TypedExpr],
) -> anyhow::Result<()> {
    // Tuple unpack: `a, b = f()` where `f` returns a tuple.
    if values.len() == 1 && matches!(values[0].inferred_type, Ty::Tuple { .. }) {
        let tuple_op = lower_expr(b, &values[0])?;
        for (i, target) in targets.iter().enumerate() {
            let dst = b.fresh_temp();
            b.emit(Instruction::TupleExtract {
                dst,
                src: tuple_op.clone(),
                index: i,
                ty: target.inferred_type.clone(),
            });
            lower_lvalue_store(b, target, Operand::Temp(dst))?;
        }
        return Ok(());
    }
    if targets.len() != values.len() {
        bail!(
            "multi-assign arity {} vs {} (fallback)",
            targets.len(),
            values.len()
        )
    }
    let mut lowered = Vec::with_capacity(values.len());
    for v in values {
        lowered.push(lower_expr(b, v)?);
    }
    for (target, val) in targets.iter().zip(lowered) {
        lower_lvalue_store(b, target, val)?;
    }
    Ok(())
}

fn lower_multi_binding(
    b: &mut BasicBlockBuilder,
    bindings: &[crate::frontend::typed_ast::TypedBinding],
    values: &[TypedExpr],
) -> anyhow::Result<()> {
    if values.len() == 1 && matches!(values[0].inferred_type, Ty::Tuple { .. }) {
        let tuple_op = lower_expr(b, &values[0])?;
        for (i, binding) in bindings.iter().enumerate() {
            if matches!(binding.ty, Ty::Type) {
                bail!("comptime `type` binding in runtime code")
            }
            let sym = b.fresh_symbol(binding.symbol.raw_str.clone(), binding.ty.clone());
            b.emit(Instruction::Alloca {
                sym,
                name: binding.symbol.raw_str.clone(),
                ty: binding.ty.clone(),
            });
            let dst = b.fresh_temp();
            b.emit(Instruction::TupleExtract {
                dst,
                src: tuple_op.clone(),
                index: i,
                ty: binding.ty.clone(),
            });
            b.emit(Instruction::Store {
                dst: sym,
                src: Operand::Temp(dst),
            });
        }
        return Ok(());
    }
    if bindings.len() != values.len() {
        bail!(
            "multi-binding arity {} vs {} (fallback)",
            bindings.len(),
            values.len()
        )
    }
    let mut lowered = Vec::with_capacity(values.len());
    for v in values {
        lowered.push(lower_expr(b, v)?);
    }
    for (binding, val) in bindings.iter().zip(lowered) {
        if matches!(binding.ty, Ty::Type) {
            bail!("comptime `type` binding in runtime code")
        }
        let sym = b.fresh_symbol(binding.symbol.raw_str.clone(), binding.ty.clone());
        b.emit(Instruction::Alloca {
            sym,
            name: binding.symbol.raw_str.clone(),
            ty: binding.ty.clone(),
        });
        b.emit(Instruction::Store { dst: sym, src: val });
    }
    Ok(())
}

fn lower_switch(
    b: &mut BasicBlockBuilder,
    subject: &TypedExpr,
    arms: &[crate::frontend::typed_ast::TypedSwitchArm],
) -> anyhow::Result<()> {
    use crate::frontend::typed_ast::TypedPattern;
    // Plain enums only: any payload binding needs the direct path.
    for arm in arms {
        if let TypedPattern::EnumVariant {
            binding: Some(_), ..
        } = &arm.pattern
        {
            bail!("switch arm with payload binding (fallback)")
        }
    }
    let subj = lower_expr(b, subject)?;
    let merge_label = b.fresh_label();
    let mut cases: Vec<(u32, super::Label)> = Vec::new();
    let mut arm_labels: Vec<super::Label> = Vec::new();
    let mut default_label = merge_label;
    let mut has_default = false;
    for arm in arms {
        match &arm.pattern {
            TypedPattern::EnumVariant { discriminant, .. } => {
                let lbl = b.fresh_label();
                cases.push((*discriminant, lbl));
                arm_labels.push(lbl);
            }
            TypedPattern::Wildcard => {
                let lbl = b.fresh_label();
                default_label = lbl;
                has_default = true;
                arm_labels.push(lbl);
            }
        }
    }
    b.emit(Instruction::SwitchDispatch {
        subject: subj,
        cases,
        default: default_label,
    });
    for (arm, lbl) in arms.iter().zip(arm_labels) {
        b.emit(Instruction::LabelDef(lbl));
        b.enter_scope();
        b.deferred.push(Vec::new());
        for s in &arm.body {
            lower_stmt(b, s)?;
        }
        b.deferred.pop();
        b.exit_scope();
        b.emit(Instruction::Goto {
            target: merge_label,
        });
    }
    // No wildcard: default falls straight through to merge.
    if !has_default {
        debug_assert_eq!(default_label, merge_label);
    }
    b.emit(Instruction::LabelDef(merge_label));
    Ok(())
}
