//! Multi-block scalar LLVM emission and per-function ABI classification.
//!
//! `llvm_lower` emits verified SSA; `abi` classifies each function signature.
//! Remaining phase-plan APIs fail explicitly. `next/` is inactive legacy code.

pub mod abi;
pub mod context;
pub mod error;
pub mod expressions;
pub mod ir_lower;
#[cfg(feature = "llvm")]
mod llvm_abi;
#[cfg(feature = "llvm")]
mod llvm_enum;
pub mod llvm_lower;
#[cfg(feature = "llvm")]
mod llvm_scalar;
pub mod statements;
pub mod types;

#[cfg(test)]
mod test;

pub use error::BackendError;
pub use llvm_lower::{LlvmArtifact, emit_llvm};
pub use types::TargetSpec;

#[cfg(all(test, feature = "llvm"))]
mod cfg_test;
