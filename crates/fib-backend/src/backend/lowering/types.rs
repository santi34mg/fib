//! Explicit target selection for LLVM emission.

/// Requested target, not validated target data. There is deliberately no host
/// default. LLVM's target machinery must derive and validate its data layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSpec {
    pub triple: String,
    pub cpu: String,
    /// Target feature spellings must be checked, not silently ignored.
    pub features: Vec<String>,
}
