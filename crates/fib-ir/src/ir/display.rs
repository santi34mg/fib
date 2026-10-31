//! Diagnostic formatting only; not a stable serialization or validation API.

use std::fmt;

use super::{BlockId, ConstantId, FunctionId, GlobalId, Ir, Operand, StackSlotId, TypeId, ValueId};

macro_rules! display_id {
    ($($id:ident => $prefix:literal),+ $(,)?) => {
        $(impl fmt::Display for $id {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        })+
    };
}

display_id!(
    FunctionId => "fn", GlobalId => "global", TypeId => "type",
    ConstantId => "const", BlockId => "bb", ValueId => "%v", StackSlotId => "slot",
);

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(id) => id.fmt(f),
            Self::Constant(id) => id.fmt(f),
        }
    }
}

impl fmt::Display for Ir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ir (unverified architecture sketch) {{")?;
        for (i, ty) in self.types.iter().enumerate() {
            writeln!(f, "  {} = {ty:?}", TypeId(i))?;
        }
        for (i, constant) in self.constants.iter().enumerate() {
            writeln!(f, "  {} = {constant:?}", ConstantId(i))?;
        }
        for (i, global) in self.globals.iter().enumerate() {
            writeln!(f, "  {} = {global:?}", GlobalId(i))?;
        }
        for (i, function) in self.functions.iter().enumerate() {
            writeln!(f, "  {} = {function:?}", FunctionId(i))?;
        }
        for (i, slot) in self.stack_slots.iter().enumerate() {
            writeln!(f, "  {} = {slot:?}", StackSlotId(i))?;
        }
        for (i, value) in self.values.iter().enumerate() {
            writeln!(f, "  {} = {value:?}", ValueId(i))?;
        }
        for (i, block) in self.blocks.iter().enumerate() {
            writeln!(
                f,
                "  {} (owner {}, params {:?}) {{",
                BlockId(i),
                block.owner,
                block.params
            )?;
            for instruction in &block.instructions {
                writeln!(f, "    {instruction:?}")?;
            }
            writeln!(f, "    {:?}", block.terminator)?;
            writeln!(f, "  }}")?;
        }
        writeln!(f, "}}")
    }
}
