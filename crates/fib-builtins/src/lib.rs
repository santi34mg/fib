//! Shared compiler builtin identities, independent of syntax and LLVM.
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
}
