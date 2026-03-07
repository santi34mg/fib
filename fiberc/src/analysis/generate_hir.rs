use std::error::Error;

use crate::ast::ast::{
    DeclarationNode, Expression as PExpr, FunctionDeclaration, TypeDeclaration, TypeExpression,
    VariableDeclaration,
};
use crate::ast::{Ast, StatementNode};
use crate::hir::{
    CompilationUnit, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction, HIRStmt,
    HIRSymbol, HIRTypeKind, HIRVar, Scope,
};
use crate::token::Operator;
use crate::token::builtin::BuiltinType;
use crate::token::literal::Literal;

/// Perform semantic analysis on the parser AST and produce a vector of HIR functions.
pub fn analyze(ast: Ast) -> Result<CompilationUnit, Box<dyn Error>> {
    // name resolution
    let mut current_scope = Scope::new();
    let mut hir_declarations: Vec<HIRDeclaration> = Vec::new();
    for declaration in ast.declarations {
        current_scope = name_resolution(&declaration, current_scope)?;
        let hir_declaration: Option<HIRDeclaration> = match declaration {
            DeclarationNode::FunctionDeclaration(function_declaration) => Some(
                HIRDeclaration::HIRFunction(func_to_hir(function_declaration, &mut current_scope)?),
            ),
            DeclarationNode::TypeDeclaration(_) => None,
            DeclarationNode::Statement(stmt) => match stmt {
                StatementNode::VariableDeclaration(var_declaration) => Some(
                    HIRDeclaration::HIRVar(var_decl_to_hir(var_declaration, &mut current_scope)?),
                ),
                _ => todo!("analyze: statement not supported yet"),
            },
        };
        if let Some(hir_declaration) = hir_declaration {
            hir_declarations.push(hir_declaration);
        }
    }

    let compilation_unit = CompilationUnit {
        scope_root: current_scope,
        declarations: hir_declarations,
    };
    Ok(compilation_unit)
}

fn func_to_hir(
    function_declaration: FunctionDeclaration,
    current_scope: &mut Scope,
) -> Result<HIRFunction, Box<dyn Error>> {
    let mut params = Vec::new();
    for param in function_declaration.signature.parameters {
        params.push((param.parameter_name, map_type(param.parameter_type)?))
    }
    let return_type = match function_declaration.signature.return_type {
        Some(rt) => map_type(rt)?,
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };
    let mut body = Vec::new();
    for stmt in function_declaration.body.statements {
        body.push(stmt_to_hir(stmt, current_scope)?);
    }
    Ok(HIRFunction {
        name: function_declaration.signature.name,
        params,
        return_type,
        body,
    })
}

fn stmt_to_hir(stmt: StatementNode, current_scope: &mut Scope) -> Result<HIRStmt, Box<dyn Error>> {
    match stmt {
        StatementNode::VariableDeclaration(variable_declaration) => Ok(HIRStmt::Let(
            var_decl_to_hir(variable_declaration, current_scope)?,
        )),
        StatementNode::Assignment { identifier, expr } => {
            let e = expr_to_hir(expr, current_scope)?;
            Ok(HIRStmt::Assign {
                name: identifier,
                expr: e,
            })
        }
        StatementNode::ExpressionStatement(e) => Ok(HIRStmt::Expr(expr_to_hir(e, current_scope)?)),
        StatementNode::Return(opt) => Ok(HIRStmt::Return(match opt {
            Some(e) => Some(expr_to_hir(e, current_scope)?),
            None => None,
        })),
        StatementNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = expr_to_hir(condition, current_scope)?;
            let mut then_h = Vec::new();
            for s in then_branch {
                then_h.push(stmt_to_hir(s, current_scope)?);
            }
            let else_h = match else_branch {
                Some(v) => {
                    let mut ev = Vec::new();
                    for s in v {
                        ev.push(stmt_to_hir(s, current_scope)?);
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
                Some(b) => Some(Box::new(stmt_to_hir(*b, current_scope)?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_hir(e, current_scope)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_hir(*b, current_scope)?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body {
                body_h.push(stmt_to_hir(s, current_scope)?);
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

fn var_decl_to_hir(
    variable_declaration: VariableDeclaration,
    current_scope: &mut Scope,
) -> Result<HIRVar, Box<dyn Error>> {
    let mut init = match variable_declaration.expression {
        Some(e) => Some(expr_to_hir(e, current_scope)?),
        None => None,
    };
    let ty = match variable_declaration.variable_type {
        Some(t) => map_type(t)?,
        // TODO: inferred types `let x = ...` should be done below
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };
    // check that type matches
    if let Some(init) = &mut init {
        if init.inferred_type != ty {
            return Err(format!(
                r#"initalization type does not match explicit type for {}
explicit type: {}
inferred type of expressoin: {}"#,
                variable_declaration.identifier, ty, init.inferred_type
            )
            .into());
        }
    }
    let hir_var = HIRVar {
        name: variable_declaration.identifier.clone(),
        ty,
        init,
    };
    current_scope.symbols.insert(
        variable_declaration.identifier,
        HIRSymbol::Variable(hir_var.clone()),
    );
    Ok(hir_var)
}

fn expr_to_hir(expr: PExpr, current_scope: &Scope) -> Result<HIRExpression, Box<dyn Error>> {
    match expr {
        PExpr::Literal(Literal::Integer(value)) => {
            // infer type to be as small as possible
            let inferred_type: HIRTypeKind;
            if value <= (1 << 8) - 1 {
                inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt8)
            } else if 1 << 8 <= value && value <= (1 << 16) - 1 {
                inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt16)
            } else if 1 << 16 <= value && value <= (1 << 32) - 1 {
                inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt32)
            } else if 1 << 32 <= value {
                inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt64)
            } else {
                unreachable!()
            }
            Ok(HIRExpression {
                inferred_type,
                expression: HIRExpressionKind::LiteralInt { value },
            })
        }
        PExpr::Literal(Literal::Boolean(b)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Boolean),
            expression: HIRExpressionKind::LiteralBool(b),
        }),
        PExpr::Literal(Literal::Float(_)) => Err(format!(
            "expr_to_hir: Float literals are not supported in minimal semantic pass"
        )
        .into()),
        PExpr::Literal(Literal::Character(_)) => Err(format!(
            "expr_to_hir: Character literals are not supported in minimal semantic pass"
        )
        .into()),
        PExpr::Literal(Literal::String(_)) => Err(format!(
            "expr_to_hir: String literals are not supported in minimal semantic pass"
        )
        .into()),
        PExpr::Literal(Literal::Null) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Void),
            expression: HIRExpressionKind::Null,
        }),
        PExpr::Identifier(name) => Ok(HIRExpression {
            inferred_type: match current_scope.symbols.get(&name).ok_or_else(|| {
                format!(
                    "expr_to_hir: identifier {} not found in scope {:?}",
                    name, current_scope
                )
            })? {
                HIRSymbol::Variable(var) => var.ty.clone(),
                _ => {
                    return Err(
                        format!("expr_to_hir: identifier {} is not a variable", name).into(),
                    );
                }
            },
            expression: HIRExpressionKind::Variable(name),
        }),
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_hir(*left, current_scope)?;
            let r = expr_to_hir(*right, current_scope)?;
            let inferred_type = match operator {
                // TODO: support more operations than integers
                Operator::Plus
                | Operator::Minus
                | Operator::Multiply
                | Operator::Divide
                | Operator::RightShift
                | Operator::LeftShift
                | Operator::GreaterThan
                | Operator::GreaterEqual
                | Operator::LesserThan
                | Operator::LesserEqual
                | Operator::Modulo
                | Operator::Ampersand
                | Operator::Pipe
                | Operator::Caret => HIRTypeKind::Builtin(BuiltinType::UInt64),
                _ => panic!(),
            };
            Ok(HIRExpression {
                inferred_type,
                expression: HIRExpressionKind::Binary {
                    left: Box::new(l),
                    operator,
                    right: Box::new(r),
                },
            })
        }
        PExpr::Grouping(inner) => expr_to_hir(*inner, current_scope),
        PExpr::Call { callee, args } => {
            match *callee {
                PExpr::Identifier(name) => {
                    let mut hargs = Vec::new();
                    for a in args {
                        hargs.push(expr_to_hir(a, current_scope)?);
                    }
                    let inferred_type: HIRTypeKind;
                    if let HIRSymbol::Function(func) =
                        current_scope.symbols.get(&name).ok_or_else(|| {
                            format!("expr_to_hir: function {} not found in current scope", name)
                        })?
                    {
                        inferred_type = func.return_type.clone();
                    } else {
                        return Err(
                            format!("expr_to_hir: symbol {} is not a function", name).into()
                        );
                    }
                    Ok(HIRExpression {
                        inferred_type,
                        expression: HIRExpressionKind::Call {
                            callee: name.clone(),
                            args: hargs,
                        },
                    })
                }
                // Only accept identifier callees for this minimal pass
                _ => Err(format!(
                    "expr_to_hir: call target must be an identifier in this minimal semantic pass"
                )
                .into()),
            }
        }
        PExpr::Unary {
            operator,
            expression,
        } => {
            // represent unary as binary with a zero/true literal where appropriate for now
            todo!()
        }
    }
}

fn map_type(type_expression: TypeExpression) -> Result<HIRTypeKind, Box<dyn Error>> {
    let hir_typekind = match type_expression {
        TypeExpression::Builtin(builtin) => HIRTypeKind::Builtin(builtin),
        TypeExpression::Identifier(identifier) => HIRTypeKind::Identifier(identifier),
        TypeExpression::Struct { fields } => {
            todo!("map_type: struct type expression not supported yet")
        }
        TypeExpression::Function {
            argument_types,
            return_type,
        } => {
            todo!("map_type: function type expression not supported yet")
        }
        TypeExpression::Pointer {
            pointer_variant,
            pointed_type,
        } => {
            todo!("map_type: pointer type expression not supported yet")
        }
    };

    Ok(hir_typekind)
}

fn name_resolution(
    declaration: &DeclarationNode,
    current_scope: Scope,
) -> Result<Scope, Box<dyn Error>> {
    match declaration {
        DeclarationNode::TypeDeclaration(type_declaration) => {
            resolve_type_decl(&type_declaration, current_scope)
        }
        DeclarationNode::FunctionDeclaration(function_declaration) => {
            resolve_function_decl(&function_declaration, current_scope)
        }
        DeclarationNode::Statement(statement) => resolve_statement(statement, current_scope),
    }
}

fn resolve_statement(
    statement: &StatementNode,
    mut current_scope: Scope,
) -> Result<Scope, Box<dyn Error>> {
    match statement {
        StatementNode::VariableDeclaration(variable_declaration) => {
            current_scope.symbols.insert(
                variable_declaration.identifier.clone(),
                HIRSymbol::Variable(var_decl_to_hir(
                    variable_declaration.clone(),
                    &mut current_scope.clone(),
                )?),
            );
            Ok(current_scope)
        }
        StatementNode::Return(_) => Ok(current_scope),
        stmt => todo!("resolve_statement: statement {:?} not supported yet", stmt),
    }
}

fn resolve_type_decl(
    type_declaration: &TypeDeclaration,
    mut current_scope: Scope,
) -> Result<Scope, Box<dyn Error>> {
    let lhs_td = type_declaration.name.clone();
    // match the right hand side of `type <name> = <type expression>`
    match &type_declaration.type_expression {
        TypeExpression::Identifier(rhs_td) => {
            current_scope.symbols.insert(
                lhs_td,
                current_scope
                    .symbols
                    .get(&rhs_td)
                    .expect(&format!(
                        "resolve_type_decl: Type {} not found in current scope",
                        rhs_td
                    ))
                    .clone(),
            );
        }
        TypeExpression::Function {
            argument_types,
            return_type,
        } => {}
        TypeExpression::Pointer {
            pointer_variant,
            pointed_type,
        } => {}
        TypeExpression::Struct { fields } => {}
        TypeExpression::Builtin(builtin_type) => {
            current_scope.symbols.insert(
                lhs_td,
                HIRSymbol::Type(HIRTypeKind::Builtin(builtin_type.clone())),
            );
        }
    }
    Ok(current_scope)
}

fn resolve_function_decl(
    function_declaration: &FunctionDeclaration,
    mut current_scope: Scope,
) -> Result<Scope, Box<dyn Error>> {
    current_scope.symbols.insert(
        function_declaration.signature.name.clone(),
        HIRSymbol::Function(func_to_hir(
            function_declaration.clone(),
            &mut current_scope.clone(),
        )?),
    );
    let mut new_scope = Scope::new();
    for stmt in function_declaration.body.statements.iter() {
        new_scope = resolve_statement(stmt, new_scope)?;
    }
    for (name, symbol) in &current_scope.symbols {
        new_scope.symbols.insert(name.clone(), symbol.clone());
    }
    current_scope.children_scope.push(Box::new(new_scope));
    Ok(current_scope)
}
