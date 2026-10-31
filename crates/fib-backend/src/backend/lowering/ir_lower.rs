//! Input verification boundary, replacing the old flat-IR consumer.

use crate::ir::Ir;

use super::BackendError;

/// Verification proof tied to one immutable IR borrow. No public constructor can
/// certify unchecked arena records. This does not certify target support.
#[derive(Debug)]
pub struct VerifiedIr<'ir> {
    ir: &'ir Ir,
}

impl<'ir> VerifiedIr<'ir> {
    pub fn ir(&self) -> &'ir Ir {
        self.ir
    }
}

/// Invokes `crate::ir::verify_ir` and preserves its diagnostics. No local
/// partial check, graph repair, or successful empty-module special case.
pub fn verify_input(ir: &Ir) -> Result<VerifiedIr<'_>, BackendError> {
    crate::ir::verify_ir(ir).map_err(|e| BackendError::Invalid {
        message: e.to_string(),
    })?;
    Ok(VerifiedIr { ir })
}
