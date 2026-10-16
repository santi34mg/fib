use crate::{
    frontend::{ast::Ast, scope_kind::ScopeKind, symbol_table::SymbolTable, tokens::name::Name},
    ir::Ir,
};

impl Ast {
    pub fn resolve_name(
        &mut self,
        mut symbol_table: &mut SymbolTable,
        module_name: Name,
    ) -> anyhow::Result<()> {
        symbol_table.enter_scope(ScopeKind::Module(module_name));
        for stmt in &mut self.statements {
            stmt.resolve_name(&mut symbol_table)?;
        }
        symbol_table.exit_scope();
        Ok(())
    }

    pub fn resolve_type(&mut self) -> anyhow::Result<Ir> {
        todo!()
    }
}
