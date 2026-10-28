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

/// Stable identity for functions supplied by a synthetic compiler module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFunction {
    Sizeof,
    AddSource,
    SetEntry,
    SetOutput,
    AddCSource,
    AddLinkLib,
    AddLibraryPath,
    UncheckedDeref,
    SliceFromParts,
    SliceData,
    Syscall0,
    Syscall1,
    Syscall2,
    Syscall3,
    Syscall4,
    Syscall5,
    Syscall6,
    PointerFromWord,
    PointerToWord,
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
            Self::Sizeof => "sizeof",
            Self::AddSource => "add_source",
            Self::SetEntry => "set_entry",
            Self::SetOutput => "set_output",
            Self::AddCSource => "add_c_source",
            Self::AddLinkLib => "add_link_lib",
            Self::AddLibraryPath => "add_library_path",
            Self::UncheckedDeref => "unchecked_deref",
            Self::SliceFromParts => "slice_from_parts",
            Self::SliceData => "slice_data",
            Self::Syscall0 => "syscall0",
            Self::Syscall1 => "syscall1",
            Self::Syscall2 => "syscall2",
            Self::Syscall3 => "syscall3",
            Self::Syscall4 => "syscall4",
            Self::Syscall5 => "syscall5",
            Self::Syscall6 => "syscall6",
            Self::PointerFromWord => "pointer_from_word",
            Self::PointerToWord => "pointer_to_word",
        }
    }

    pub fn is_intrinsic(self) -> bool {
        matches!(
            self,
            Self::Sizeof
                | Self::UncheckedDeref
                | Self::SliceFromParts
                | Self::SliceData
                | Self::Syscall0
                | Self::Syscall1
                | Self::Syscall2
                | Self::Syscall3
                | Self::Syscall4
                | Self::Syscall5
                | Self::Syscall6
                | Self::PointerFromWord
                | Self::PointerToWord
        )
    }

    pub fn syscall_arity(self) -> Option<usize> {
        match self {
            Self::Syscall0 => Some(0),
            Self::Syscall1 => Some(1),
            Self::Syscall2 => Some(2),
            Self::Syscall3 => Some(3),
            Self::Syscall4 => Some(4),
            Self::Syscall5 => Some(5),
            Self::Syscall6 => Some(6),
            _ => None,
        }
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
