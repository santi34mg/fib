use std::collections::HashMap;
use std::error::Error;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};

use super::statements::codegen_stmt;
use super::types::map_type_to_llvm;
use crate::frontend::identifier::Identifier;
use crate::frontend::typed_ast::{SymbolTable, TypedFunction, TypedStatement};

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
/// `expr.rs` gets `codegen_expr`, `stmt.rs` gets `codegen_stmt`, and all of
/// them take `&FunctionLowering` instead of the current
/// `(ctx, vars, scope, loop_ctx, deferred_stack)` tuple. Introduced now so
/// new helpers (`insert_block`, `emit_if`, IR consumer) build on it instead
/// of adding more free-function params.
#[allow(dead_code)]
pub(super) struct FunctionLowering<'ctx, 'r> {
    pub(super) ctx: &'r CodegenCtx<'ctx, 'r>,
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
pub(super) fn insert_block<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    what: &str,
) -> Result<BasicBlock<'ctx>, Box<dyn Error>> {
    ctx.builder
        .get_insert_block()
        .ok_or_else(|| format!("lowering '{}': no insert block", what).into())
}

/// Enclosing function with context (replaces `get_parent().unwrap()`).
pub(super) fn parent_function<'ctx, 'r>(
    ctx: &CodegenCtx<'ctx, 'r>,
    what: &str,
) -> Result<FunctionValue<'ctx>, Box<dyn Error>> {
    Ok(insert_block(ctx, what)?
        .get_parent()
        .ok_or_else(|| format!("lowering '{}': insert block has no parent function", what))?)
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

/// Create an alloca for each parameter in the entry block
pub(super) fn create_entry_allocas<'ctx>(
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

pub(super) fn emit_deferred_frame<'ctx, 'r>(
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
pub(super) fn emit_frames_from<'ctx, 'r>(
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
