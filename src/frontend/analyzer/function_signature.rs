use crate::frontend::{ast::function_signature::FunctionSignature, symbol_table::SymbolTable};

impl FunctionSignature {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        symbol_table.insert_name(&mut self.funcition_name_symbol);
        Ok(())
    }
}
