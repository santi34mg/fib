//! Lower validated frontend analysis into verified SSA.
use fib_frontend::{builtins, diagnostics, frontend, numeric, plan};
use fib_ir as ir;
pub mod lowering;
pub use lowering::{lower, lower_with_entry};
