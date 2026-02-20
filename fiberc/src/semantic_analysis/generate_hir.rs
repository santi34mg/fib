use crate::hir;
use crate::parser::{Ast, Expression, Statement, TypeIdentifier, function::FunctionBody};

fn map_type(ty: &TypeIdentifier) -> Result<hir::Type, String> {
    use crate::parser::TypeIdentifier;
    match ty {
        TypeIdentifier::Integer => Ok(hir::Type::Int),
        TypeIdentifier::Boolean => Ok(hir::Type::Bool),
        TypeIdentifier::Unit => Ok(hir::Type::Unit),
        TypeIdentifier::Function {
            argument_types,
            return_type,
        } => {
            let mut args = Vec::new();
            for a in argument_types.iter() {
                args.push(map_type(a)?);
            }
            let ret = Box::new(map_type(return_type)?);
            Ok(hir::Type::Function { args, ret })
        }
        _ => Err(format!("unsupported type in semantic analysis: {:?}", ty)),
    }
}

fn expr_to_hir(expr: &Expression) -> Result<hir::HIRExpr, String> {
    use crate::parser::expression::Expression as PExpr;
    use crate::token::literal::Literal;

    match expr {
        PExpr::Literal(Literal::Integer(i)) => Ok(hir::HIRExpr::LiteralInt(*i)),
        PExpr::Literal(Literal::Boolean(b)) => Ok(hir::HIRExpr::LiteralBool(*b)),
        PExpr::Literal(Literal::Float(_)) => {
            Err("Float literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::Character(_)) => {
            Err("Character literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::String(_)) => {
            Err("String literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::Null) => Ok(hir::HIRExpr::Null),
        PExpr::Identifier(name) => Ok(hir::HIRExpr::Var(name.clone())),
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_hir(left)?;
            let r = expr_to_hir(right)?;
            let op = format!("{:?}", operator);
            Ok(hir::HIRExpr::Binary {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
        }
        PExpr::Grouping(inner) => expr_to_hir(inner),
        PExpr::Call { callee, args } => {
            // Only accept identifier callees for this minimal pass
            match &**callee {
                PExpr::Identifier(name) => {
                    let mut hargs = Vec::new();
                    for a in args.iter() {
                        hargs.push(expr_to_hir(a)?);
                    }
                    Ok(hir::HIRExpr::Call {
                        callee: name.clone(),
                        args: hargs,
                    })
                }
                _ => Err(
                    "call target must be an identifier in this minimal semantic pass".to_string(),
                ),
            }
        }
        PExpr::Unary {
            operator,
            expression,
        } => {
            // represent unary as binary with a zero/true literal where appropriate for now
            let inner = expr_to_hir(expression)?;
            let op = format!("{:?}", operator);
            Ok(hir::HIRExpr::Binary {
                left: Box::new(inner.clone()),
                op,
                right: Box::new(hir::HIRExpr::LiteralInt(0)),
            })
        }
    }
}

fn stmt_to_hir(stmt: &Statement) -> Result<hir::HIRStmt, String> {
    use crate::parser::statement::Statement as S;

    match stmt {
        S::VariableDeclaration(var) => {
            let init = match &var.expression {
                Some(e) => Some(expr_to_hir(e)?),
                None => None,
            };
            let ty = match &var.variable_type {
                Some(t) => Some(map_type(t)?),
                None => None,
            };
            Ok(hir::HIRStmt::Let {
                name: var.identifier.clone(),
                ty,
                init,
            })
        }
        S::Assignment { identifier, expr } => {
            let e = expr_to_hir(expr)?;
            Ok(hir::HIRStmt::Assign {
                name: identifier.clone(),
                expr: e,
            })
        }
        S::Expression(e) => Ok(hir::HIRStmt::Expr(expr_to_hir(e)?)),
        S::Return(opt) => Ok(hir::HIRStmt::Return(match opt {
            Some(e) => Some(expr_to_hir(e)?),
            None => None,
        })),
        S::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = expr_to_hir(condition)?;
            let mut then_h = Vec::new();
            for s in then_branch.iter() {
                then_h.push(stmt_to_hir(s)?);
            }
            let else_h = match else_branch {
                Some(v) => {
                    let mut ev = Vec::new();
                    for s in v.iter() {
                        ev.push(stmt_to_hir(s)?);
                    }
                    Some(ev)
                }
                None => None,
            };
            Ok(hir::HIRStmt::If {
                cond,
                then_branch: then_h,
                else_branch: else_h,
            })
        }
        S::For {
            initializer,
            condition,
            increment,
            body,
        } => {
            let init_h = match initializer {
                Some(b) => Some(Box::new(stmt_to_hir(b)?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_hir(e)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_hir(b)?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body.iter() {
                body_h.push(stmt_to_hir(s)?);
            }
            Ok(hir::HIRStmt::For {
                init: init_h,
                cond: cond_h,
                post: post_h,
                body: body_h,
            })
        }
        S::FunctionDeclaration(_) => Err(
            "nested function declarations are not supported in minimal semantic pass".to_string(),
        ),
    }
}

/// Perform semantic analysis on the parser AST and produce a vector of HIR functions.
pub fn analyze(ast: &Ast) -> Result<Vec<hir::HIRFunction>, String> {
    let mut result: Vec<hir::HIRFunction> = Vec::new();

    for stmt in ast.statements.iter() {
        match stmt {
            Statement::FunctionDeclaration(f) => {
                // Map signature
                let sig = &f.signature;
                let mut params: Vec<(String, hir::Type)> = Vec::new();
                for p in sig.parameters.iter() {
                    let pty = map_type(&p.parameter_type)?;
                    params.push((p.parameter_name.clone(), pty));
                }
                let ret_type = map_type(&sig.return_type)?;

                // Map body statements (only if FunctionBody::Statements)
                let mut body_h: Vec<hir::HIRStmt> = Vec::new();
                match &f.body {
                    FunctionBody::Statements(stmts) => {
                        for s in stmts.iter() {
                            body_h.push(stmt_to_hir(s)?);
                        }
                    }
                    FunctionBody::Empty => {}
                }

                result.push(hir::HIRFunction {
                    name: sig.name.clone(),
                    params,
                    ret_type,
                    body: body_h,
                });
            }
            other => {
                return Err(format!(
                    "only top-level functions supported in minimal semantic pass: found {:?}",
                    other
                ));
            }
        }
    }

    Ok(result)
}
