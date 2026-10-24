//! Identity-based compiler namespaces for memory intrinsics and build operations.

use crate::frontend::{
    semantic_type::{FunctionType, SemanticType, SemanticTypes},
    tokens::builtin::BuiltinType,
};

/// A compiler-provided namespace whose declarations participate in ordinary
/// name and type resolution without requiring a source AST.
#[derive(Debug, Clone, Copy)]
pub struct SyntheticModule {
    pub name: &'static str,
    pub functions: &'static [BuiltinFunction],
}

impl SyntheticModule {
    pub const MEMORY: Self = Self {
        name: "memory",
        functions: &[BuiltinFunction::UncheckedDeref],
    };
    pub const BUILD_API: Self = Self {
        name: "build_api",
        functions: &BuiltinFunction::ALL,
    };
}

/// Stable identity for functions supplied by a synthetic compiler module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunction {
    AddSource,
    SetEntry,
    SetOutput,
    UncheckedDeref,
}

impl BuiltinFunction {
    pub const ALL: [Self; 3] = [Self::AddSource, Self::SetEntry, Self::SetOutput];

    pub const fn name(self) -> &'static str {
        match self {
            Self::AddSource => "add_source",
            Self::SetEntry => "set_entry",
            Self::SetOutput => "set_output",
            Self::UncheckedDeref => "unchecked_deref",
        }
    }

    pub fn is_intrinsic(self) -> bool {
        self == Self::UncheckedDeref
    }

    pub fn semantic_type(self, types: &mut SemanticTypes) -> SemanticType {
        if self.is_intrinsic() {
            return SemanticType::IntrinsicFunction;
        }
        let string = types.builtin(BuiltinType::String);
        let void = types.builtin(BuiltinType::Void);
        SemanticType::Function(FunctionType {
            is_comp: false,
            parameters: vec![string],
            return_type: void,
        })
    }
}
