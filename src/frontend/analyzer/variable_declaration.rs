use crate::frontend::{ast::variable_declaration::VariableDeclaration, symbol_table::SymbolTable};

impl VariableDeclaration {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        let _ = symbol_table;
        todo!()
    }
}
