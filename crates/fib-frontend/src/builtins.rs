//! Identity-based compiler declarations for memory, syscalls and build operations.

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
    pub const CORE: Self = Self {
        name: "core",
        functions: &[BuiltinFunction::Sizeof],
    };
    pub const MEMORY: Self = Self {
        name: "std::memory",
        functions: &[
            BuiltinFunction::UncheckedDeref,
            BuiltinFunction::SliceFromParts,
            BuiltinFunction::SliceData,
        ],
    };
    pub const OS: Self = Self {
        name: "std::os",
        functions: &[
            BuiltinFunction::Syscall0,
            BuiltinFunction::Syscall1,
            BuiltinFunction::Syscall2,
            BuiltinFunction::Syscall3,
            BuiltinFunction::Syscall4,
            BuiltinFunction::Syscall5,
            BuiltinFunction::Syscall6,
        ],
    };
    pub const OS_PRIVATE: Self = Self {
        name: "std::os_abi",
        functions: &[
            BuiltinFunction::PointerFromWord,
            BuiltinFunction::PointerToWord,
        ],
    };
    pub const BUILD_API: Self = Self {
        name: "build_api",
        functions: &BuiltinFunction::ALL,
    };
}

pub use fib_builtins::BuiltinFunction;

pub fn semantic_type(builtin: BuiltinFunction, types: &mut SemanticTypes) -> SemanticType {
    if builtin.is_intrinsic() {
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
