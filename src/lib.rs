pub mod backend;
pub mod builtins;
pub mod cli;
pub mod diagnostics;
pub mod driver;
pub mod frontend;
pub mod ir;
pub mod ir_interpreter;
pub(crate) mod numeric;
pub mod plan;

pub use diagnostics::CompilerError;
