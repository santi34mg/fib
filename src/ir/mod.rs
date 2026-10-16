//! Dragon-style three-address IR (middle-end).
//!
//! Boundary: the frontend produces `typed_ast::TypedAst` (trees +
//! `If`/`For`/`Switch`, nested expressions). This module is the first
//! flat representation: at most one operation per instruction, control
//! flow only via labels and jumps, values in temporaries.
//!
//! Status: `lower_typed_program` implements locals, integer/bool/float
//! arithmetic, calls, `if`/`for`/`break`/`continue`, `defer` (inline
//! duplication), `return`, plus assignment forms (`MultiAssign` /
//! `MultiBinding` / `FieldAssign` / `DerefAssign` / `IndexAssign`),
//! plain-enum `switch` (no payload bindings), and module-level `const`
//! globals. Aggregate / pointer / enum-payload / qualified nodes are
//! modeled as explicit `Instruction` variants; complex shapes still return
//! `LowerError::Unsupported` so the driver can fall back to the direct
//! `TypedAst -> LLVM` path. `driver::lower_to_llvm_ir` tries this
//! import first, then the `ir_lower` consumer; on any `Unsupported` (or
//! consumer gap) it falls back, so `IrProgram` is on the production path
//! for every program that fits the core subset.

// FIXME: can this be avoided?
/// Generic binary operation, shared with the frontend (`TypedExprKind::Binary`).
/// Signedness / float-ness is carried by the `ty` field on
/// `Instruction::Binary` so the backend can pick `SDiv` vs `UDiv`, `SLT` vs
/// `ULT`, `LShr` vs `AShr`, etc.
use crate::{
    frontend::typed_ast::{
        ty::Ty, typed_ast::TypedAst, typed_binding::TypedBinding, typed_declaration::TypedDecl,
        typed_symbol_id::NameId,
    },
    ir::{builder::BasicBlockBuilder, expressions::lower_expr, instruction::Instruction},
};
use statements::lower_function;

mod builder;
mod display;
mod expressions;
mod instruction;
mod statements;
mod test;

#[derive(Debug, Clone, Default)]
pub struct Ir {
    blocks: Vec<BasicBlock>,
}

impl Ir {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// A jump target. Labels are patched after emission (Dragon backpatching),
/// so jumps refer to a `Label` id, never to an `Instruction` by reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub u32);

/// A compiler-generated temporary holding one flat value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Temp(pub u32);

/// An operand: a temporary, a resolved binding, or a constant.
#[derive(Debug, Clone)]
pub enum Operand {
    Temp(Temp),
    Symbol(NameId),
    ConstInt { value: u64, ty: Ty },
    ConstFloat { value: f64, ty: Ty },
    ConstBool(bool),
    StringLit(String),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub label: Label,
    pub instrs: Vec<Instruction>,
}

// ---------------------------------------------------------------------------
// Predicates shared with the backend (single source of truth for signedness)
// ---------------------------------------------------------------------------

/// True for `@uint*` types. The backend uses this to pick `UDiv`/`URem`,
/// `ULT/UGT/...`, logical `LShr`, and `zext`/`uitofp` over signed forms.
pub fn ty_is_unsigned(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::UInt1
                | BuiltinType::UInt2
                | BuiltinType::UInt4
                | BuiltinType::UInt8
                | BuiltinType::UInt16
                | BuiltinType::Usize
        )
    )
}

pub fn ty_is_float(ty: &Ty) -> bool {
    use crate::frontend::tokens::builtin::BuiltinType;
    matches!(
        ty,
        Ty::Builtin(
            BuiltinType::Float2 | BuiltinType::Float4 | BuiltinType::Float8 | BuiltinType::Float16
        )
    )
}

/// Lower a typed program to flat IR.
///
/// Handles locals, arithmetic, calls, `if`/`for`/`break`/`continue`,
/// `defer`, `return`, assignment forms, plain-enum `switch`, and
/// module-level `const` globals. Anything else returns
/// `LowerError::Unsupported` with the source construct named so the
/// driver can fall back to the direct path.
pub fn lower_typed_ast<'tast>(typed_ast: TypedAst) -> anyhow::Result<Ir> {
    let mut ir = Ir::default();
    for decl in typed_ast.declarations {
        match decl {
            TypedDecl::Function(f) => {
                ir.blocks.extend(lower_function(f)?);
            }
            TypedDecl::Const(binding) => {}
            TypedDecl::Type => {
                // Types live in the scope; no code to emit.
            }
        }
    }
    Ok(ir)
}

/// Lower a module-level `const` to an `IrConst` global.
///
/// Only constant initializers (no emitted instructions, constant operand)
/// are accepted; anything requiring runtime code returns `Unsupported`.
fn lower_const(binding: &TypedBinding) -> anyhow::Result<BasicBlock> {
    let mut b = BasicBlockBuilder::new();
    let alloca = Instruction::Alloca {
        sym: binding.symbol,
        name: binding.symbol.name,
        ty: binding.ty,
    };
    if let Some(init) = binding.init {
        let instr = lower_expr(&mut b, &init);
        Ok(BasicBlock {
            label: todo!(),
            instrs: vec![alloca],
        })
    } else {
        Ok(BasicBlock {
            label: todo!(),
            instrs: vec![alloca],
        })
    }
    // let init_expr = b.init.as_ref().ok_or_else(|| {
    //     LowerError::Unsupported(format!(
    //         "module-level const '{}' has no initializer",
    //         b.symbol
    //     ))
    // })?;
    // // Lower with a throwaway builder: const inits must not emit code.
    // let mut tmp =
    //     builder::FunctionBuilder::new("__const_init".to_string(), b.ty.clone(), true, false);
    // let operand = expressions::lower_expr(&mut tmp, init_expr)?;
    // if tmp.emitted_any() {
    //     return Err(LowerError::Unsupported(format!(
    //         "module-level const '{}' needs runtime lowering",
    //         b.symbol
    //     )));
    // }
    // // Accept only constant operands; temps/symbols need runtime slots.
    // let is_const = matches!(
    //     operand,
    //     Operand::ConstInt { .. }
    //         | Operand::ConstFloat { .. }
    //         | Operand::ConstBool(_)
    //         | Operand::StringLit(_)
    //         | Operand::Null
    // );
    // if !is_const {
    //     // Void-typed const inits (e.g. calls) lower to a dummy bool; still
    //     // not a compile-time constant.
    //     let _ = BuiltinType::Void;
    //     return Err(LowerError::Unsupported(format!(
    //         "module-level const '{}' needs runtime lowering",
    //         b.symbol
    //     )));
    // }
    // Ok(IrConst {
    //     name: b.symbol.raw_str.clone(),
    //     ty: b.ty.clone(),
    //     init: operand,
    // })
}
