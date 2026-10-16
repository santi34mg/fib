use crate::frontend::{
    ast::statement::{Statement, StatementKind},
    symbol_table::SymbolTable,
};

impl Statement {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        match self.kind {
            StatementKind::VariableDeclaration(ref mut variable_declaration) => {
                variable_declaration.resolve_name(symbol_table)?
            }
            StatementKind::ExpressionStatement(ref mut expression) => {
                expression.resolve_name(symbol_table)?
            }
            StatementKind::Assignment {
                ref mut left,
                ref mut right,
            } => {
                left.resolve_name(symbol_table)?;
                right.resolve_name(symbol_table)?;
            }
            StatementKind::DerefAssign {
                ref mut pointer,
                ref mut expr,
            } => {
                pointer.resolve_name(symbol_table)?;
                expr.resolve_name(symbol_table)?;
            }
            StatementKind::IndexAssign {
                ref mut object,
                ref mut index,
                ref mut expr,
            } => {
                object.resolve_name(symbol_table)?;
                index.resolve_name(symbol_table)?;
                expr.resolve_name(symbol_table)?;
            }
            StatementKind::Return(ref mut expressions) => {
                if let Some(expressions) = expressions {
                    for expr in expressions {
                        expr.resolve_name(symbol_table)?;
                    }
                }
            }
            StatementKind::MultiAssignment {
                ref mut targets,
                ref mut values,
            } => {
                for t in targets {
                    t.resolve_name(symbol_table)?;
                }
                for v in values {
                    v.resolve_name(symbol_table)?;
                }
            }
            StatementKind::MultiVariableDeclaration {
                ref mut bindings,
                ref mut values,
            } => {
                todo!()
            }
            StatementKind::If {
                ref mut condition,
                ref mut then_branch,
                ref mut else_branch,
            } => {
                condition.resolve_name(symbol_table)?;
                for stmt in then_branch {
                    stmt.resolve_name(symbol_table)?;
                }
                if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        stmt.resolve_name(symbol_table)?;
                    }
                }
            }
            StatementKind::While {
                ref mut condition,
                ref mut body,
            } => {
                if let Some(condition) = condition {
                    condition.resolve_name(symbol_table)?;
                }
                for stmt in body {
                    stmt.resolve_name(symbol_table)?;
                }
            }
            StatementKind::Break => {}
            StatementKind::Continue => {}
            StatementKind::Defer(ref mut statement) => {
                statement.resolve_name(symbol_table)?;
            }
            StatementKind::Switch {
                ref mut subject,
                ref mut arms,
            } => {
                subject.resolve_name(symbol_table)?;
                for arm in arms {
                    arm.resolve_name(symbol_table)?;
                }
            }
            StatementKind::FunctionDeclaration(function_declaration) => todo!(),
            StatementKind::TypeDeclaration(type_declaration) => todo!(),
        }
        Ok(())
    }
}
