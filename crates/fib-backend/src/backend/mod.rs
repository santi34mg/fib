//! LLVM emission of the initial scalar subset, consuming only verified SSA IR.
//!
//! ABI classification is per function; broader representations remain unfinished.

pub mod lowering;

pub use lowering::{BackendError, LlvmArtifact, TargetSpec, emit_llvm};
