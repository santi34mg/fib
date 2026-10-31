//! Compiler facade and executable pipeline orchestration.
pub use fib_backend as backend;
pub use fib_frontend::{builtins, diagnostics, frontend, standard_library};
pub mod cli;
pub mod driver;
pub mod ir {
    pub use fib_ir::*;
    pub use fib_lowering::lowering;
}
pub mod ir_interpreter;
pub mod plan;
pub mod symbol_export;
pub use diagnostics::CompilerError;
