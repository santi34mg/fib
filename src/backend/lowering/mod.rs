#[cfg(feature = "llvm")]
mod context;
#[cfg(feature = "llvm")]
pub mod error;
#[cfg(feature = "llvm")]
mod expressions;
#[cfg(feature = "llvm")]
mod ir_lower;
#[cfg(feature = "llvm")]
pub mod llvm_lower;
#[cfg(feature = "llvm")]
mod statements;
#[cfg(all(test, feature = "llvm"))]
mod test;
#[cfg(feature = "llvm")]
pub mod types;

#[cfg(feature = "llvm")]
pub use error::LowerError;
#[cfg(feature = "llvm")]
pub use ir_lower::lower_ir;
#[cfg(feature = "llvm")]
pub use llvm_lower::lower;
