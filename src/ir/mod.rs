//! Self-owned SSA IR with transactional construction and typed memory/SSA verification.
//!
//! The boundary is `AnalyzedProject -> ir::lowering::lower -> Ir`; the driver
//! owns pipeline orchestration. All runtime types, identities and constants here are
//! independent of the frontend and of any backend. See README.md for invariants
//! and the currently verified subset. Public records are deliberately raw,
//! unverified data; neither construction nor formatting certifies validity.

mod builder;
mod display;
mod expressions;
mod instruction;
pub mod lowering;
mod statements;
#[cfg(test)]
mod test;
mod types;
mod verify;

pub use builder::FunctionBuilder;
pub use expressions::{Address, Constant, ConstantKind, Operand, Value, ValueDefinition};
pub use instruction::{BinaryOp, CastKind, Instruction, InstructionKind, UnaryOp};
pub use statements::{BasicBlock, Edge, SwitchCase, Terminator, TrapReason};
pub use types::{CallingConvention, EnumVariant, FloatWidth, IntegerWidth, IrType, Signature};
pub use verify::verify_ir;

macro_rules! ids {
    // santi: this is black magic
    ($($name:ident),+ $(,)?) => {
        $(
            /// Index into the corresponding arena of one `Ir`. Not portable
            /// between IR instances; indices must remain stable while in use.
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub struct $name(pub usize);
        )+
    };
}

ids!(
    FunctionId,
    GlobalId,
    TypeId,
    ConstantId,
    BlockId,
    ValueId,
    StackSlotId
);

/// Arena indices are IR-wide, including those for function-local entities.
/// Owner metadata and body membership must agree. No deletion/compaction API
/// is provided: remapping live IDs is a future transformation's responsibility.
#[derive(Debug, Clone, Default)]
pub struct Ir {
    pub functions: Vec<Function>,
    /// Interpreter intrinsics keyed by canonical IR function identity.
    pub builtin_functions: std::collections::HashMap<FunctionId, crate::builtins::BuiltinFunction>,
    pub globals: Vec<Global>,
    pub types: Vec<IrType>,
    pub constants: Vec<Constant>,
    pub blocks: Vec<BasicBlock>,
    pub values: Vec<Value>,
    pub stack_slots: Vec<StackSlot>,
}

impl Ir {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether all arenas are empty, including declaration-only modules.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
            && self.builtin_functions.is_empty()
            && self.globals.is_empty()
            && self.types.is_empty()
            && self.constants.is_empty()
            && self.blocks.is_empty()
            && self.values.is_empty()
            && self.stack_slots.is_empty()
    }

    pub fn get_function(&self, id: FunctionId) -> Option<&Function> {
        self.functions.get(id.0)
    }

    pub fn get_global(&self, id: GlobalId) -> Option<&Global> {
        self.globals.get(id.0)
    }

    pub fn get_type(&self, id: TypeId) -> Option<&IrType> {
        self.types.get(id.0)
    }

    pub fn get_constant(&self, id: ConstantId) -> Option<&Constant> {
        self.constants.get(id.0)
    }

    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(id.0)
    }

    pub fn get_value(&self, id: ValueId) -> Option<&Value> {
        self.values.get(id.0)
    }

    pub fn get_stack_slot(&self, id: StackSlotId) -> Option<&StackSlot> {
        self.stack_slots.get(id.0)
    }

    /// Only resolves the recorded type; does not check ownership or validity.
    pub fn operand_type(&self, operand: Operand) -> Option<TypeId> {
        match operand {
            Operand::Value(id) => self.get_value(id).map(|value| value.ty),
            Operand::Constant(id) => self.get_constant(id).map(|constant| constant.ty),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Function {
    /// Owned, canonical linker name, not a source-name lookup key.
    pub identifier: String,
    pub signature: Signature,
    pub linkage: Linkage,
    pub body: FunctionBody,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    /// External declaration: retains the signature but owns no local entities.
    Declaration,
    Definition {
        /// Entry parameters are the function arguments, in signature order.
        /// No control-flow edge may target the entry block.
        entry: BlockId,
        /// Includes entry; order is for traversal, never implicit fallthrough.
        blocks: Vec<BlockId>,
        stack_slots: Vec<StackSlotId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Linkage {
    Internal,
    External,
}

#[derive(Debug, Clone)]
pub struct Global {
    pub name: String,
    /// Stored value type, not the type of its address.
    pub ty: TypeId,
    pub mutable: bool,
    pub linkage: Linkage,
    pub initializer: GlobalInitializer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalInitializer {
    /// Storage is supplied outside this module.
    Declaration,
    /// An explicitly typed constant, including explicit zero initializers.
    Constant(ConstantId),
}

/// Mutable storage is not an SSA value. Taking its address produces a pointer
/// value; only explicit loads/stores read/write its contents.
#[derive(Debug, Clone)]
pub struct StackSlot {
    pub owner: FunctionId,
    pub ty: TypeId,
    pub debug_name: Option<String>,
}

/// Invalid inputs and unsupported verification families fail explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrError {
    Invalid { message: &'static str },
    NotImplemented { operation: &'static str },
}

pub type IrResult<T> = Result<T, IrError>;

impl std::fmt::Display for IrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid { message } => write!(f, "invalid IR: {message}"),
            Self::NotImplemented { operation } => write!(f, "{operation} is not implemented"),
        }
    }
}

impl std::error::Error for IrError {}
