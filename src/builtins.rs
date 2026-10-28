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
    AddCSource,
    AddLinkLib,
    AddLibraryPath,
    UncheckedDeref,
}

impl BuiltinFunction {
    pub const ALL: [Self; 6] = [
        Self::AddSource,
        Self::SetEntry,
        Self::SetOutput,
        Self::AddCSource,
        Self::AddLinkLib,
        Self::AddLibraryPath,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::AddSource => "add_source",
            Self::SetEntry => "set_entry",
            Self::SetOutput => "set_output",
            Self::AddCSource => "add_c_source",
            Self::AddLinkLib => "add_link_lib",
            Self::AddLibraryPath => "add_library_path",
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
        let element = types.builtin(BuiltinType::UInt8);
        let slice = types.intern(SemanticType::Slice(element));
        let string = types.intern(SemanticType::Readonly(slice));
        let void = types.builtin(BuiltinType::Void);
        SemanticType::Function(FunctionType {
            calling_convention: crate::frontend::semantic_type::CallConvention::Fib,
            is_comp: false,
            parameters: vec![string],
            return_type: void,
        })
    }
}
