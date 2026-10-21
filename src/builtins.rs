//! Builtins available while analyzing a project build file.

use crate::frontend::{
    semantic_type::{FunctionType, SemanticTypes},
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
}

impl BuiltinFunction {
    pub const ALL: [Self; 3] = [Self::AddSource, Self::SetEntry, Self::SetOutput];

    pub const fn name(self) -> &'static str {
        match self {
            Self::AddSource => "add_source",
            Self::SetEntry => "set_entry",
            Self::SetOutput => "set_output",
        }
    }

    pub fn semantic_type(self, types: &mut SemanticTypes) -> FunctionType {
        let string = types.builtin(BuiltinType::String);
        let void = types.builtin(BuiltinType::Void);
        FunctionType {
            is_comp: false,
            parameters: vec![string],
            return_type: void,
        }
    }
}
