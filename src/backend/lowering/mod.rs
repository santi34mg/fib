//! Fallible phase boundaries for `Ir -> LLVM IR text`.
//!
//! These APIs are available without the `llvm` feature because they contain no
//! LLVM objects. Every unfinished operation returns `BackendError::NotImplemented`.
//! The preserved `next/` directory is not part of this module tree.

pub mod abi;
pub mod context;
pub mod error;
pub mod expressions;
pub mod ir_lower;
pub mod llvm_lower;
pub mod statements;
pub mod types;

#[cfg(test)]
mod test;

pub use error::BackendError;
pub use llvm_lower::{LlvmArtifact, emit_llvm};
pub use types::TargetSpec;
