use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};

use super::error::LowerError;
use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::typed_ast::{SymbolTable, TypedFunction, TypedStatement};

#[derive(Clone, Copy)]
pub(super) struct LoopContext<'ctx> {
    pub(super) break_bb: BasicBlock<'ctx>,
    pub(super) continue_bb: BasicBlock<'ctx>,
    /// Depth of the deferred-statement stack at loop entry. `break` and
    /// `continue` run the deferred frames above this depth (those opened
    /// inside the loop) before jumping.
    pub(super) deferred_depth: usize,
}

pub(super) struct CodegenCtx<'ctx, 'r> {
    pub(super) ctx: &'ctx Context,
    pub(super) module: &'r Module<'ctx>,
    pub(super) builder: &'r Builder<'ctx>,
}

/// Per-function lowering state. This is the designated owner for the
/// `llvm_lower.rs` split (phase 4): `types.rs` keeps `map_type_to_llvm`,
/// `expr.rs` provides `codegen_expr`, `stmt.rs` provides `codegen_stmt`, and
/// all of them take `&FunctionLowering` instead of the current
/// `(ctx, vars, scope, loop_ctx, deferred_stack)` tuple. New helpers
/// (`insert_block`, `parent_function`, `emit_frames_from`, ...) build on it
/// instead of adding more free-function params.
pub(super) struct FunctionLowering<'ctx, 'r> {
    pub(super) ctx: &'r CodegenCtx<'ctx, 'r>,
    function: FunctionValue<'ctx>,
    pub(super) vars: HashMap<Identifier, PointerValue<'ctx>>,
    pub(super) scope: SymbolTable,
    pub(super) deferred_stack: Vec<Vec<TypedStatement>>,
    loop_ctx: Option<LoopContext<'ctx>>,
    /// Debug runtime bounds checks for `arr.[i]` / `arr.[a..b]`.
    /// True unless `--release` was passed; compile-time checks always run.
    pub(super) bounds_checks: bool,
}

impl<'ctx, 'r> FunctionLowering<'ctx, 'r> {
    pub(super) fn new(
        ctx: &'r CodegenCtx<'ctx, 'r>,
        function: FunctionValue<'ctx>,
        vars: HashMap<Identifier, PointerValue<'ctx>>,
        scope: SymbolTable,
        bounds_checks: bool,
    ) -> Self {
        Self {
            ctx,
            function,
            vars,
            scope,
            deferred_stack: vec![Vec::new()],
            loop_ctx: None,
            bounds_checks,
        }
    }

    /// Current insert block with context (replaces `get_insert_block().unwrap()`).
    /// `LowerError::MissingBlock` carries the site (`what`), the enclosing
    /// function name, and an optional line.
    pub(super) fn insert_block(&self, what: &str) -> Result<BasicBlock<'ctx>, LowerError> {
        self.ctx.builder.get_insert_block().ok_or_else(|| {
            LowerError::missing_block(
                what,
                Some(self.function.get_name().to_string_lossy().into_owned()),
                None,
            )
        })
    }

    /// Enclosing function with context (replaces `get_parent().unwrap()`).
    pub(super) fn parent_function(&self, what: &str) -> Result<FunctionValue<'ctx>, LowerError> {
        self.insert_block(what)?.get_parent().ok_or_else(|| {
            LowerError::missing_block(
                what,
                Some(self.function.get_name().to_string_lossy().into_owned()),
                None,
            )
        })
    }

    /// Loop context for `break`/`continue`, if currently inside a loop.
    pub(super) fn loop_ctx(&self) -> Option<&LoopContext<'ctx>> {
        self.loop_ctx.as_ref()
    }

    pub(super) fn enter_loop(&mut self, loop_ctx: LoopContext<'ctx>) -> Option<LoopContext<'ctx>> {
        self.loop_ctx.replace(loop_ctx)
    }

    pub(super) fn exit_loop(&mut self, previous: Option<LoopContext<'ctx>>) {
        self.loop_ctx = previous;
    }

    /// Emit one deferred frame in reverse order (LIFO). Deferred statements
    /// run with a fresh empty deferred stack and no loop context, mirroring
    /// the previous free-function behavior.
    pub(super) fn emit_deferred_frame(
        &mut self,
        frame: &[TypedStatement],
    ) -> Result<(), LowerError> {
        let saved_stack = std::mem::take(&mut self.deferred_stack);
        let saved_loop = self.loop_ctx.take();
        let res = (|| {
            for stmt in frame.iter().rev() {
                self.codegen_stmt(stmt)?;
            }
            Ok(())
        })();
        self.deferred_stack = saved_stack;
        self.loop_ctx = saved_loop;
        res
    }

    /// Emit the deferred frames at `stack[from..]`, innermost frame first. Used
    /// on early exits: `return` unwinds everything (`from = 0`), `break` /
    /// `continue` unwind the frames opened inside the loop.
    pub(super) fn emit_frames_from(
        &mut self,
        stack: Vec<Vec<TypedStatement>>,
        from: usize,
    ) -> Result<(), LowerError> {
        for frame in stack[from.min(stack.len())..].iter().rev() {
            self.emit_deferred_frame(frame)?;
        }
        Ok(())
    }
}

/// Current insert block with context (replaces `get_insert_block().unwrap()`).
/// Kept as a free function for call sites that build blocks without a
/// `FunctionLowering` (the IR lowering path and unit tests).
pub(super) fn insert_block<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    what: &str,
) -> Result<BasicBlock<'ctx>, LowerError> {
    ctx.builder
        .get_insert_block()
        .ok_or_else(|| LowerError::missing_block(what, None, None))
}

/// Lower Typed into LLVM IR represented as a string.
/// Coerce an LLVM int value to `target_type`, widening with `zext` when
/// the source Fib value is `@uint*` and `sext` otherwise (mirrors the
/// `Cast` int->int rule keyed off `inner.inferred_type`).
pub(super) fn coerce_int_to_llvm_type<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    value: BasicValueEnum<'ctx>,
    target_type: BasicTypeEnum<'ctx>,
    is_unsigned: bool,
) -> Result<BasicValueEnum<'ctx>, LowerError> {
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

/// Create an alloca for each parameter in the entry block
pub(super) fn create_entry_allocas<'ctx>(
    ctx: &'ctx Context,
    function: FunctionValue<'ctx>,
    typed_fn: TypedFunction,
    current_scope: SymbolTable,
) -> Result<HashMap<Identifier, PointerValue<'ctx>>, LowerError> {
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
