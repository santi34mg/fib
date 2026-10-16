use crate::frontend::ast::Ast;
use crate::frontend::symbol_table::SymbolTable;
use crate::ir::Ir;

/// `Analyzer` is the struct responsible for bothe lowering into and
/// analyzing IR.
pub struct Analyzer {
    ast: Ast,
    ir: Ir,
    symbol_table: SymbolTable,
}

impl Analyzer {
    pub fn new(ast: Ast) -> Self {
        Self {
            ast,
            ir: Ir::new(),
            symbol_table: SymbolTable::new(),
        }
    }

    /// `analyze` is the public facing analysis method.
    /// Performs all passes of the analysis phase, namely:
    /// * name resolution
    /// * type resolution
    pub fn analyze(mut self) -> anyhow::Result<Ir> {
        self.ast.resolve_name(&mut self.symbol_table)?;
        self.ast.resolve_type()
    }
}

pub mod ast;
pub mod expression;
pub mod function_declaration;
pub mod function_signature;
pub mod generics;
pub mod statement;
pub mod swtich_arm;
pub mod test;
pub mod type_declaration;
pub mod type_expression;
pub mod types;
pub mod variable_declaration;
