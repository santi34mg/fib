use crate::frontend::ast::type_declaration::TypeDeclaration;

impl TypeDeclaration {
    pub fn resolve_name(&self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        let _ = symbol_table;
        todo!()
    }
}
