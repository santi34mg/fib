use crate::frontend::{
    ast::{expression::Expression, statement::StatementKind},
    parser::ParseResult,
    tokens::Token,
};

use super::Parser;
impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Token>,
{
    /// Build an assignment statement node from a parsed LHS expression and a
    /// computed RHS expression. Errors if the LHS isn't a valid lvalue.
    pub fn lhs_to_assignment(
        &self,
        lhs: Expression,
        rhs: Expression,
    ) -> ParseResult<StatementKind> {
        match lhs {
            Expression::Identifier(id) => Ok(StatementKind::Assignment {
                identifier: id,
                expr: rhs,
            }),
            Expression::FieldAccess { object, field } => Ok(StatementKind::FieldAssign {
                object: *object,
                field,
                expr: rhs,
            }),
            Expression::Dereference(inner) => Ok(StatementKind::DerefAssign {
                pointer: *inner,
                expr: rhs,
            }),
            Expression::IndexAccess { object, index } => Ok(StatementKind::IndexAssign {
                object: *object,
                index: *index,
                expr: rhs,
            }),
            _ => {
                let (line, column) = self.last_pos;
                Err(self.error("invalid assignment target", line, column))
            }
        }
    }
}
