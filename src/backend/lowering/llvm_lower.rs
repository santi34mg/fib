//! Public emission boundary. This module deliberately creates no LLVM objects.

use crate::ir::Ir;

use super::{BackendError, TargetSpec};

/// Owned textual LLVM IR, not an object file, executable, path, or borrowed
/// LLVM module. Only a future successful, verified emitter may construct it.
/// The text must include the resolved target triple and data layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlvmArtifact {
    ir_text: String,
    target_triple: String,
    data_layout: String,
}

impl LlvmArtifact {
    pub fn ir_text(&self) -> &str {
        &self.ir_text
    }

    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }

    pub fn data_layout(&self) -> &str {
        &self.data_layout
    }

    pub fn into_ir_text(self) -> String {
        self.ir_text
    }
}

/// Planned order: verify input; resolve target and types; classify ABIs;
/// predeclare symbols; set up blocks/PHIs; emit instructions/terminators;
/// patch edges; verify LLVM; serialize. No phase executes in this sketch.
/// File I/O, object generation, optimization and linking are separate concerns.
pub fn emit_llvm(_ir: &Ir, _target: &TargetSpec) -> Result<LlvmArtifact, BackendError> {
    Err(BackendError::NotImplemented {
        operation: "emit_llvm",
    })
}
