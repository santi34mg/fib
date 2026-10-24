//! Explicit SSA control flow, not AST statement lowering.

use super::{BlockId, ConstantId, FunctionId, Instruction, Operand, ValueId};

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub owner: FunctionId,
    /// Defines SSA values. Incoming edges supply positional arguments; entry
    /// parameters instead receive arguments from the function's caller.
    pub params: Vec<ValueId>,
    pub instructions: Vec<Instruction>,
    /// Always explicit, including return/unreachable. No implicit fallthrough.
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub target: BlockId,
    /// Must match the target block's parameter count and types.
    pub args: Vec<Operand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchCase {
    /// Typed integer/bool constant matching the switch subject's type.
    pub value: ConstantId,
    pub edge: Edge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jump(Edge),
    Branch {
        condition: Operand,
        then_edge: Edge,
        else_edge: Edge,
    },
    Switch {
        subject: Operand,
        cases: Vec<SwitchCase>,
        default: Edge,
    },
    Return {
        values: Vec<Operand>,
    },
    Trap(TrapReason),
    Unreachable,
}

/// Defined failures of scalar operations, independent of backend implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapReason {
    DivisionByZero,
    InvalidShift,
    InvalidFloatToInteger,
}
