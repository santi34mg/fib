use crate::ast::ast::{DeclarationNode, Expression as PExpr, FunctionBody, TypeIdentifier};
use crate::ast::{Ast, StatementNode};
use crate::hir::{HIRExpr, HIRFunction, HIRStmt, Type};
use crate::token::literal::Literal;

fn map_type(ty: &TypeIdentifier) -> Result<Type, String> {
    match ty {
        TypeIdentifier::Integer => Ok(Type::Int),
        TypeIdentifier::Boolean => Ok(Type::Bool),
        TypeIdentifier::Unit => Ok(Type::Unit),
        TypeIdentifier::Function {
            argument_types,
            return_type,
        } => {
            let mut args = Vec::new();
            for a in argument_types.iter() {
                args.push(map_type(a)?);
            }
            let ret = Box::new(map_type(return_type)?);
            Ok(Type::Function { args, ret })
        }
        _ => Err(format!("unsupported type in semantic analysis: {:?}", ty)),
    }
}

fn expr_to_hir(expr: &PExpr) -> Result<HIRExpr, String> {
    match expr {
        PExpr::Literal(Literal::Integer(i)) => Ok(HIRExpr::LiteralInt(*i)),
        PExpr::Literal(Literal::Boolean(b)) => Ok(HIRExpr::LiteralBool(*b)),
        PExpr::Literal(Literal::Float(_)) => {
            Err("Float literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::Character(_)) => {
            Err("Character literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::String(_)) => {
            Err("String literals are not supported in minimal semantic pass".to_string())
        }
        PExpr::Literal(Literal::Null) => Ok(HIRExpr::Null),
        PExpr::Identifier(name) => Ok(HIRExpr::Var(name.clone())),
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_hir(left)?;
            let r = expr_to_hir(right)?;
            let op = format!("{:?}", operator);
            Ok(HIRExpr::Binary {
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
                    Ok(HIRExpr::Call {
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
            Ok(HIRExpr::Binary {
                left: Box::new(inner.clone()),
                op,
                right: Box::new(HIRExpr::LiteralInt(0)),
            })
        }
    }
}

fn stmt_to_hir(stmt: &StatementNode) -> Result<HIRStmt, String> {
    match stmt {
        StatementNode::VariableDeclaration(var) => {
            let init = match &var.expression {
                Some(e) => Some(expr_to_hir(e)?),
                None => None,
            };
            let ty = match &var.variable_type {
                Some(t) => Some(map_type(t)?),
                None => None,
            };
            Ok(HIRStmt::Let {
                name: var.identifier.clone(),
                ty,
                init,
            })
        }
        StatementNode::Assignment { identifier, expr } => {
            let e = expr_to_hir(expr)?;
            Ok(HIRStmt::Assign {
                name: identifier.clone(),
                expr: e,
            })
        }
        StatementNode::ExpressionStatement(e) => Ok(HIRStmt::Expr(expr_to_hir(e)?)),
        StatementNode::Return(opt) => Ok(HIRStmt::Return(match opt {
            Some(e) => Some(expr_to_hir(e)?),
            None => None,
        })),
        StatementNode::If {
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
            Ok(HIRStmt::If {
                cond,
                then_branch: then_h,
                else_branch: else_h,
            })
        }
        StatementNode::For {
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
            Ok(HIRStmt::For {
                init: init_h,
                cond: cond_h,
                post: post_h,
                body: body_h,
            })
        }
    }
}

/// Perform semantic analysis on the parser AST and produce a vector of HIR functions.
pub fn analyze(ast: &Ast) -> Result<Vec<HIRFunction>, String> {
    let mut result: Vec<HIRFunction> = Vec::new();

    // AST now groups declarations under modules. Iterate modules and their declarations.
    for module in ast.program.modules.iter() {
        for decl in module.declarations.iter() {
            match decl {
                DeclarationNode::FunctionDeclaration(f) => {
                    // Map signature
                    let sig = &f.signature;
                    let mut params: Vec<(String, Type)> = Vec::new();
                    for p in sig.parameters.iter() {
                        let pty = map_type(&p.parameter_type)?;
                        params.push((p.parameter_name.clone(), pty));
                    }
                    let ret_type = match &sig.return_type {
                        Some(rt) => map_type(rt)?,
                        None => Type::Unit,
                    };

                    // Map body statements (only if FunctionBody::Statements)
                    let mut body_h: Vec<HIRStmt> = Vec::new();
                    match &f.body {
                        Some(FunctionBody::Statements(stmts)) => {
                            for s in stmts.iter() {
                                body_h.push(stmt_to_hir(s)?);
                            }
                        }
                        None => {
                            // No body, treat as empty function for now
                        }
                    }

                    result.push(HIRFunction {
                        name: sig.name.clone(),
                        params,
                        ret_type,
                        body: body_h,
                    });
                }
                DeclarationNode::ModuleDeclaration(_m) => {
                    // Ignore module declarations for this minimal pass, but could be used in future passes for error checking or namespacing
                }
                other => {
                    return Err(format!(
                        "only top-level functions supported in minimal semantic pass: found {:?}",
                        other
                    ));
                }
            }
        }
    }

    Ok(result)
}
