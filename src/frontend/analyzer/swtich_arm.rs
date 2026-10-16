use crate::frontend::{ast::switch::SwitchArm, symbol_table::SymbolTable};

impl SwitchArm {
    pub fn resolve_name(&self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        let _ = symbol_table;
        todo!()
    }
}
