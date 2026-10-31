//! Multi-block scalar LLVM emission and per-function ABI classification.
//!
//! `llvm_lower` emits verified SSA; `abi` classifies each function signature.

pub mod abi;
pub mod error;
pub mod ir_lower;
#[cfg(feature = "llvm")]
mod llvm_abi;
#[cfg(feature = "llvm")]
mod llvm_data;
#[cfg(feature = "llvm")]
mod llvm_enum;
pub mod llvm_lower;
#[cfg(feature = "llvm")]
mod llvm_scalar;
#[cfg(feature = "llvm")]
mod llvm_syscall;
pub mod types;

#[cfg(test)]
mod test;

pub use error::BackendError;
pub use llvm_lower::{LlvmArtifact, emit_llvm};
pub use types::TargetSpec;

#[cfg(all(test, feature = "llvm"))]
mod cfg_test;

mod c_abi;
