//! Backend architecture sketches consuming only the self-owned SSA IR.
//!
//! No LLVM emission or fallback compiler is implemented. See README.md.

pub mod lowering;

pub use lowering::{BackendError, LlvmArtifact, TargetSpec, emit_llvm};
