use std::error::Error;

use crate::ast::ast::{
    ConstantDeclaration, DeclarationNode, Expression as PExpr, FunctionDeclaration,
    TypeDeclaration, TypeExpression, VariableDeclaration,
};
use crate::ast::{Ast, StatementNode};
use crate::hir::{
    CompilationUnit, HIRBinding, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction,
    HIRIf, HIRStmt, HIRSymbol, HIRTypeKind, Scope,
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
        current_scope = resolve_declaration(&declaration, current_scope)?;
        let hir_declaration: Option<HIRDeclaration> = match declaration {
            DeclarationNode::FunctionDeclaration(function_declaration) => Some(
                HIRDeclaration::HIRFunction(func_to_hir(function_declaration, &mut current_scope)?),
            ),
            DeclarationNode::TypeDeclaration(_) => None,
            DeclarationNode::Statement(stmt) => match stmt {
                StatementNode::ConstantDeclaration(var_declaration) => {
                    Some(HIRDeclaration::HIRConst(const_decl_to_hir(
                        var_declaration,
                        &mut current_scope,
                    )?))
                }
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
    for param in &function_declaration.signature.parameters {
        params.push((param.parameter_name.clone(), map_type(param.parameter_type.clone())?))
    }
    let return_type = match function_declaration.signature.return_type.clone() {
        Some(rt) => map_type(rt)?,
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };

    // Build a body-level scope that inherits all module-level symbols and also
    // includes the function's own parameters.  This ensures that both other
    // functions (for call resolution) and the parameter bindings are visible
    // when we lower each statement in the body.
    let mut body_scope = current_scope.clone();
    for param in &function_declaration.signature.parameters {
        body_scope.symbols.insert(
            param.parameter_name.clone(),
            HIRSymbol::Binding(HIRBinding {
                name: param.parameter_name.clone(),
                ty: map_type(param.parameter_type.clone())?,
                init: None,
            }),
        );
    }

    let mut body = Vec::new();
    if let Some(fb) = function_declaration.body {
        for stmt in fb.statements {
            body.push(stmt_to_hir(stmt, &mut body_scope)?);
        }
    }
    Ok(HIRFunction {
        name: function_declaration.signature.name,
        params,
        return_type,
        body,
        is_extern: function_declaration.is_extern,
        is_variadic: function_declaration.is_variadic,
    })
}

fn stmt_to_hir(stmt: StatementNode, current_scope: &mut Scope) -> Result<HIRStmt, Box<dyn Error>> {
    match stmt {
        StatementNode::ConstantDeclaration(constant_declaration) => Ok(HIRStmt::Binding(
            const_decl_to_hir(constant_declaration, current_scope)?,
        )),
        StatementNode::VariableDeclaration(variable_declaration) => Ok(HIRStmt::Binding(
            var_decl_to_hir(variable_declaration, current_scope)?,
        )),
        StatementNode::Assignment { identifier, expr } => {
            let e = expr_to_hir(expr, current_scope)?;
            Ok(HIRStmt::Assign {
                name: identifier,
                expr: e,
            })
        }
        StatementNode::FieldAssign { object, field, expr } => {
            // Look up the object's type to find the field index
            let obj_ty = match current_scope.symbols.get(&object).ok_or_else(|| {
                format!("stmt_to_hir: identifier {} not found in scope", object)
            })? {
                HIRSymbol::Binding(b) => b.ty.clone(),
                _ => return Err(format!("stmt_to_hir: {} is not a variable", object).into()),
            };
            let struct_fields = match &obj_ty {
                HIRTypeKind::Struct { fields } => fields.clone(),
                HIRTypeKind::Identifier(_) => {
                    // Resolve through type alias
                    resolve_struct_fields(&obj_ty, current_scope)?
                }
                _ => return Err(format!("stmt_to_hir: {} is not a struct", object).into()),
            };
            let field_index = struct_fields
                .iter()
                .position(|(name, _)| name == &field.identifier)
                .ok_or_else(|| {
                    format!("stmt_to_hir: field {} not found in struct {}", field.identifier, object)
                })?;
            let e = expr_to_hir(expr, current_scope)?;
            Ok(HIRStmt::FieldAssign {
                object,
                field: field.identifier,
                field_index,
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
            Ok(HIRStmt::If(HIRIf {
                cond,
                then_branch: then_h,
                else_branch: else_h,
            }))
        }
        StatementNode::For {
            initializer,
            condition,
            post_operation: increment,
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
        StatementNode::DerefAssign { pointer, expr } => {
            let ptr_hir = expr_to_hir(pointer, current_scope)?;
            let pointee_ty = match &ptr_hir.inferred_type {
                HIRTypeKind::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "stmt_to_hir: DerefAssign pointer expression has non-pointer type {:?}",
                        other
                    )
                    .into())
                }
            };
            let val_hir = expr_to_hir(expr, current_scope)?;
            // Allow coercion of integer literals (which default to Int32) to the pointee type
            let _val_ty = &val_hir.inferred_type;
            let _ = pointee_ty; // type already verified above
            Ok(HIRStmt::DerefAssign {
                pointer: ptr_hir,
                expr: val_hir,
            })
        }
        StatementNode::IndexAssign { object, index, expr } => {
            let obj_hir = expr_to_hir(object, current_scope)?;
            match &obj_hir.inferred_type {
                HIRTypeKind::Pointer(_) | HIRTypeKind::Array { .. } => {}
                other => {
                    return Err(format!(
                        "stmt_to_hir: IndexAssign on non-pointer type {:?}",
                        other
                    )
                    .into())
                }
            }
            let idx_hir = expr_to_hir(index, current_scope)?;
            let val_hir = expr_to_hir(expr, current_scope)?;
            Ok(HIRStmt::IndexAssign {
                object: obj_hir,
                index: idx_hir,
                expr: val_hir,
            })
        }
        StatementNode::Break => Ok(HIRStmt::Break),
        StatementNode::Continue => Ok(HIRStmt::Continue),
        StatementNode::Defer(inner) => {
            let hir_inner = stmt_to_hir(*inner, current_scope)?;
            Ok(HIRStmt::Defer(Box::new(hir_inner)))
        }
    }
}

fn const_decl_to_hir(
    const_decl: ConstantDeclaration,
    current_scope: &mut Scope,
) -> Result<HIRBinding, Box<dyn Error>> {
    let mut init = expr_to_hir(const_decl.expression, &current_scope)?;
    let ty = match const_decl.constant_type {
        Some(t) => map_type(t)?,
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };
    // check that type matches; allow struct-by-name to match struct literal type
    if init.inferred_type == HIRTypeKind::Builtin(BuiltinType::Never) {
        init.inferred_type = ty.clone();
    }
    if init.inferred_type != ty {
        let resolved_ty_match = match &ty {
            HIRTypeKind::Identifier(id) => match current_scope.symbols.get(id) {
                Some(HIRSymbol::Type(inner)) => *inner == init.inferred_type,
                _ => false,
            },
            _ => false,
        };
        if resolved_ty_match {
            init.inferred_type = ty.clone();
        } else if let HIRTypeKind::Builtin(_) = &ty {
            // Coerce integer/numeric literals to the declared builtin type
            // (mirrors the same coercion already present in var_decl_to_hir).
            init.inferred_type = ty.clone();
        } else {
            return Err(format!(
                r#"initalization type does not match explicit type for {}
explicit type: {}
inferred type of expression: {}"#,
                const_decl.identifier, ty, init.inferred_type
            )
            .into());
        }
    }
    let hir_bind = HIRBinding {
        name: const_decl.identifier.clone(),
        ty,
        init: Some(init),
    };
    current_scope
        .symbols
        .insert(const_decl.identifier, HIRSymbol::Binding(hir_bind.clone()));
    Ok(hir_bind)
}

fn var_decl_to_hir(
    var_decl: VariableDeclaration,
    current_scope: &mut Scope,
) -> Result<HIRBinding, Box<dyn Error>> {
    let ty = match var_decl.constant_type {
        Some(t) => map_type(t)?,
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };
    let mut init = if let Some(expr) = var_decl.expression {
        expr_to_hir(expr, &current_scope)?
    } else {
        // Zero-initialize based on declared type when no initializer is present
        match &ty {
            HIRTypeKind::Builtin(BuiltinType::Boolean) => HIRExpression {
                inferred_type: HIRTypeKind::Builtin(BuiltinType::Boolean),
                expression: HIRExpressionKind::LiteralBool(false),
            },
            HIRTypeKind::Builtin(
                BuiltinType::Int8
                | BuiltinType::Int16
                | BuiltinType::Int32
                | BuiltinType::Int64
                | BuiltinType::UInt8
                | BuiltinType::UInt16
                | BuiltinType::UInt32
                | BuiltinType::UInt64,
            ) => HIRExpression {
                inferred_type: ty.clone(),
                expression: HIRExpressionKind::LiteralInt { value: 0 },
            },
            HIRTypeKind::Pointer(_) => HIRExpression {
                inferred_type: ty.clone(),
                expression: HIRExpressionKind::Null,
            },
            _ => {
                return Err("var declaration requires type or initializer".into())
            }
        }
    };
    // check that type matches; allow struct-by-name to match struct literal type
    // `never` unifies with any type
    if init.inferred_type == HIRTypeKind::Builtin(BuiltinType::Never) {
        init.inferred_type = ty.clone();
    }
    if init.inferred_type != ty {
        // When the declared type is an identifier (e.g. `Point`) and the init
        // expression is a StructConstruct, the inferred type is the resolved
        // HIRTypeKind::Struct directly.  In that case, accept the match by
        // annotating the init expression with the identifier type so the rest
        // of the pipeline sees a consistent declared type.
        let resolved_ty_match = match &ty {
            HIRTypeKind::Identifier(id) => {
                match current_scope.symbols.get(id) {
                    Some(HIRSymbol::Type(inner)) => *inner == init.inferred_type,
                    _ => false,
                }
            }
            _ => false,
        };
        if resolved_ty_match {
            // Replace the init's inferred type with the declared identifier type
            // so that subsequent lookups (e.g. binding type in codegen) return
            // the identifier-keyed type.
            init.inferred_type = ty.clone();
        } else if let (HIRTypeKind::Array { element_type: decl_elem, size: decl_size }, HIRTypeKind::Array { size: init_size, .. }) = (&ty, &init.inferred_type) {
            if *decl_size != *init_size {
                return Err(format!(
                    "array size mismatch for {}: declared size {} but initializer has {} elements",
                    var_decl.identifier, decl_size, init_size
                ).into());
            }
            // Coerce element types in the initializer
            if let HIRExpressionKind::ArrayLiteral { elements } = &mut init.expression {
                for elem in elements.iter_mut() {
                    elem.inferred_type = *decl_elem.clone();
                }
            }
            init.inferred_type = ty.clone();
        } else if let HIRTypeKind::Builtin(builtin) = &ty {
            // FIXME: this should also check that the inferred type is kinda safe to cast
            match builtin {
                BuiltinType::Int8 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int8),
                BuiltinType::Int16 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int16)
                }
                BuiltinType::Int32 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int32)
                }
                BuiltinType::Int64 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int64)
                }
                BuiltinType::UInt8 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt8),
                BuiltinType::UInt16 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt16)
                }
                BuiltinType::UInt32 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt32)
                }
                BuiltinType::UInt64 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt64)
                }
                _ => {}
            }
        } else {
            return Err(format!(
                r#"initalization type does not match explicit type for {}
explicit type: {:?}
inferred type of expression: {:?}"#,
                var_decl.identifier, ty, init.inferred_type
            )
            .into());
        }
    }
    let hir_var = HIRBinding {
        name: var_decl.identifier.clone(),
        ty,
        init: Some(init),
    };
    current_scope
        .symbols
        .insert(var_decl.identifier, HIRSymbol::Binding(hir_var.clone()));
    Ok(hir_var)
}

fn expr_to_hir(expr: PExpr, current_scope: &Scope) -> Result<HIRExpression, Box<dyn Error>> {
    match expr {
        PExpr::Literal(Literal::Integer(value)) => {
            // Default to Int32 (i32); explicit type annotations coerce as needed.
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Builtin(BuiltinType::Int32),
                expression: HIRExpressionKind::LiteralInt { value },
            })
        }
        PExpr::Literal(Literal::Boolean(b)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Boolean),
            expression: HIRExpressionKind::LiteralBool(b),
        }),
        PExpr::Literal(Literal::Float(f)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Float64),
            expression: HIRExpressionKind::LiteralFloat { value: f as f64 },
        }),
        PExpr::Literal(Literal::Character(c)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Char),
            expression: HIRExpressionKind::LiteralInt { value: c as u64 },
        }),
        PExpr::Literal(Literal::String(raw)) => {
            let value = process_escape_sequences(&raw)?;
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Builtin(BuiltinType::String),
                expression: HIRExpressionKind::LiteralString { value },
            })
        }
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
                HIRSymbol::Binding(var) => var.ty.clone(),
                _ => {
                    return Err(
                        format!("expr_to_hir: identifier {} is not a variable", name).into(),
                    );
                }
            },
            expression: HIRExpressionKind::Identifier(name),
        }),
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_hir(*left, current_scope)?;
            let mut r = expr_to_hir(*right, current_scope)?;
            let inferred_type = match operator {
                // Arithmetic/bitwise: result type = LHS type
                Operator::Plus
                | Operator::Minus
                | Operator::Star
                | Operator::Slash
                | Operator::Percent
                | Operator::RightShift
                | Operator::LeftShift
                | Operator::Ampersand
                | Operator::Pipe
                | Operator::Caret => {
                    // For pointer arithmetic, keep pointer type; don't coerce RHS
                    if !matches!(l.inferred_type, HIRTypeKind::Pointer(_)) {
                        r.inferred_type = l.inferred_type.clone();
                    }
                    l.inferred_type.clone()
                }
                // Comparison/logical: result type = bool
                Operator::DoubleEquals
                | Operator::Different
                | Operator::GreaterThan
                | Operator::GreaterEqual
                | Operator::LesserThan
                | Operator::LesserEqual
                | Operator::LogicalAnd
                | Operator::LogicalOr => HIRTypeKind::Builtin(BuiltinType::Boolean),
                op => {
                    return Err(format!("unsupported binary operator {:?}", op).into())
                }
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
                    // If the function is declared in scope use its return type;
                    // otherwise return an error (use extern declarations for external functions).
                    let inferred_type = match current_scope.symbols.get(&name) {
                        Some(HIRSymbol::Function(func)) => func.return_type.clone(),
                        Some(_) => {
                            return Err(format!(
                                "expr_to_hir: symbol {} is not a function",
                                name
                            )
                            .into())
                        }
                        None => {
                            return Err(format!(
                                "expr_to_hir: unknown function '{}' — did you forget an extern declaration?",
                                name
                            )
                            .into())
                        }
                    };
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
        PExpr::FieldAccess { object, field } => {
            let obj_hir = expr_to_hir(*object, current_scope)?;
            let struct_fields = resolve_struct_fields(&obj_hir.inferred_type, current_scope)?;
            let field_index = struct_fields
                .iter()
                .position(|(name, _)| name == &field.identifier)
                .ok_or_else(|| {
                    format!(
                        "expr_to_hir: field {} not found in struct type {:?}",
                        field.identifier, obj_hir.inferred_type
                    )
                })?;
            let field_ty = *struct_fields[field_index].1.clone();
            Ok(HIRExpression {
                inferred_type: field_ty,
                expression: HIRExpressionKind::FieldAccess {
                    object: Box::new(obj_hir),
                    field: field.identifier,
                    field_index,
                },
            })
        }
        PExpr::StructConstruct { type_name, fields } => {
            // Look up the struct type in scope
            let struct_ty = match current_scope.symbols.get(&type_name).ok_or_else(|| {
                format!("expr_to_hir: type {} not found in scope", type_name)
            })? {
                HIRSymbol::Type(ty) => ty.clone(),
                _ => {
                    return Err(
                        format!("expr_to_hir: {} is not a type", type_name).into(),
                    )
                }
            };
            let struct_fields = resolve_struct_fields(&struct_ty, current_scope)?;
            let mut hir_fields = Vec::new();
            for (fname, fexpr) in fields {
                // verify the field exists
                let _field_idx = struct_fields
                    .iter()
                    .position(|(name, _)| name == &fname.identifier)
                    .ok_or_else(|| {
                        format!(
                            "expr_to_hir: field {} not found in struct {}",
                            fname.identifier, type_name
                        )
                    })?;
                let fval = expr_to_hir(fexpr, current_scope)?;
                hir_fields.push((fname.identifier, fval));
            }
            Ok(HIRExpression {
                inferred_type: struct_ty,
                expression: HIRExpressionKind::StructConstruct {
                    type_name: type_name.identifier,
                    fields: hir_fields,
                },
            })
        }
        PExpr::AddressOf(inner) => {
            let inner_hir = expr_to_hir(*inner, current_scope)?;
            let ptr_ty = HIRTypeKind::Pointer(Box::new(inner_hir.inferred_type.clone()));
            Ok(HIRExpression {
                inferred_type: ptr_ty,
                expression: HIRExpressionKind::AddressOf(Box::new(inner_hir)),
            })
        }
        PExpr::Dereference(inner) => {
            let inner_hir = expr_to_hir(*inner, current_scope)?;
            let pointee_ty = match &inner_hir.inferred_type {
                HIRTypeKind::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "expr_to_hir: dereference of non-pointer type {:?}",
                        other
                    )
                    .into())
                }
            };
            Ok(HIRExpression {
                inferred_type: pointee_ty,
                expression: HIRExpressionKind::Deref(Box::new(inner_hir)),
            })
        }
        PExpr::Cast { expr, target_type } => {
            let mut inner_hir = expr_to_hir(*expr, current_scope)?;
            let hir_target = map_type(target_type)?;
            // If the inner expression is a function call to an unknown external function,
            // propagate the cast target type so the auto-declaration uses the right return type.
            if let HIRExpressionKind::Call { .. } = &inner_hir.expression {
                if inner_hir.inferred_type == HIRTypeKind::Builtin(BuiltinType::Int32) {
                    inner_hir.inferred_type = hir_target.clone();
                }
            }
            Ok(HIRExpression {
                inferred_type: hir_target.clone(),
                expression: HIRExpressionKind::Cast {
                    expr: Box::new(inner_hir),
                    target_type: hir_target,
                },
            })
        }
        PExpr::IndexAccess { object, index } => {
            let obj_hir = expr_to_hir(*object, current_scope)?;
            let idx_hir = expr_to_hir(*index, current_scope)?;
            let pointee_ty = match &obj_hir.inferred_type {
                HIRTypeKind::Pointer(inner) => *inner.clone(),
                HIRTypeKind::Array { element_type, .. } => *element_type.clone(),
                other => {
                    return Err(format!(
                        "expr_to_hir: index access on non-pointer type {:?}",
                        other
                    )
                    .into())
                }
            };
            Ok(HIRExpression {
                inferred_type: pointee_ty,
                expression: HIRExpressionKind::IndexAccess {
                    object: Box::new(obj_hir),
                    index: Box::new(idx_hir),
                },
            })
        }
        PExpr::ArrayLiteral { elements } => {
            let hir_elements: Vec<HIRExpression> = elements
                .into_iter()
                .map(|e| expr_to_hir(e, current_scope))
                .collect::<Result<_, _>>()?;
            if hir_elements.is_empty() {
                return Err("array literal must have at least one element".into());
            }
            let elem_ty = hir_elements[0].inferred_type.clone();
            for (i, e) in hir_elements.iter().enumerate() {
                if e.inferred_type != elem_ty {
                    return Err(format!(
                        "array literal: element {} has type {:?}, expected {:?}",
                        i, e.inferred_type, elem_ty
                    ).into());
                }
            }
            let size = hir_elements.len() as u64;
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Array { element_type: Box::new(elem_ty), size },
                expression: HIRExpressionKind::ArrayLiteral { elements: hir_elements },
            })
        }
        PExpr::Unary {
            operator,
            expression,
        } => match operator {
            Operator::Minus => expr_to_hir(
                PExpr::Binary {
                    left: Box::new(PExpr::Literal(Literal::Integer(0))),
                    operator: Operator::Minus,
                    right: expression,
                },
                current_scope,
            ),
            Operator::LogicalNot => expr_to_hir(
                PExpr::Binary {
                    left: expression,
                    operator: Operator::DoubleEquals,
                    right: Box::new(PExpr::Literal(Literal::Boolean(false))),
                },
                current_scope,
            ),
            Operator::Tilde => {
                // Desugar ~x to x ^ (-1) which in two's complement flips all bits
                let inner = expr_to_hir(*expression, current_scope)?;
                let minus_one = HIRExpression {
                    inferred_type: inner.inferred_type.clone(),
                    expression: HIRExpressionKind::LiteralInt { value: u64::MAX },
                };
                Ok(HIRExpression {
                    inferred_type: inner.inferred_type.clone(),
                    expression: HIRExpressionKind::Binary {
                        left: Box::new(inner),
                        operator: Operator::Caret,
                        right: Box::new(minus_one),
                    },
                })
            }
            op => Err(format!("unsupported unary operator {:?}", op).into()),
        },
    }
}

/// Resolve a HIRTypeKind (possibly an Identifier alias) to its struct fields.
fn resolve_struct_fields(
    ty: &HIRTypeKind,
    current_scope: &Scope,
) -> Result<Vec<(String, Box<HIRTypeKind>)>, Box<dyn Error>> {
    match ty {
        HIRTypeKind::Struct { fields } => Ok(fields.clone()),
        HIRTypeKind::Identifier(id) => {
            let symbol = current_scope.symbols.get(id).ok_or_else(|| {
                format!("resolve_struct_fields: type {} not found in scope", id)
            })?;
            match symbol {
                HIRSymbol::Type(inner_ty) => resolve_struct_fields(inner_ty, current_scope),
                _ => Err(format!("resolve_struct_fields: {} is not a type", id).into()),
            }
        }
        _ => Err(format!("resolve_struct_fields: {:?} is not a struct type", ty).into()),
    }
}

fn map_type(type_expression: TypeExpression) -> Result<HIRTypeKind, Box<dyn Error>> {
    let hir_typekind = match type_expression {
        TypeExpression::Builtin(builtin) => HIRTypeKind::Builtin(builtin),
        TypeExpression::Identifier(identifier) => HIRTypeKind::Identifier(identifier),
        TypeExpression::Struct { fields } => {
            let mut hir_fields = Vec::new();
            for f in fields {
                hir_fields.push((f.label.identifier.clone(), Box::new(map_type(f.type_id)?)));
            }
            HIRTypeKind::Struct { fields: hir_fields }
        }
        TypeExpression::Function {
            argument_types,
            return_type,
        } => {
            todo!("map_type: function type expression not supported yet")
        }
        TypeExpression::Pointer {
            pointer_variant: _,
            pointed_type,
        } => {
            let inner = map_type(*pointed_type)?;
            HIRTypeKind::Pointer(Box::new(inner))
        }
        TypeExpression::Array { element_type, size } => {
            HIRTypeKind::Array { element_type: Box::new(map_type(*element_type)?), size }
        }
    };

    Ok(hir_typekind)
}

fn resolve_declaration(
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
        StatementNode::ConstantDeclaration(constant_declaration) => {
            // Resolve pass: register the binding with its declared type only.
            // Full init evaluation happens in stmt_to_hir; doing it here would
            // fail when the init expression references outer-scope variables
            // that aren't visible in the narrow local scope used for resolve.
            let ty = match &constant_declaration.constant_type {
                Some(t) => map_type(t.clone())?,
                None => HIRTypeKind::Builtin(BuiltinType::Void),
            };
            current_scope.symbols.insert(
                constant_declaration.identifier.clone(),
                HIRSymbol::Binding(HIRBinding {
                    name: constant_declaration.identifier.clone(),
                    ty,
                    init: None,
                }),
            );
            Ok(current_scope)
        }
        StatementNode::VariableDeclaration(variable_declaration) => {
            // Resolve pass: register the binding with its declared type only.
            let ty = match &variable_declaration.constant_type {
                Some(t) => map_type(t.clone())?,
                None => HIRTypeKind::Builtin(BuiltinType::Void),
            };
            current_scope.symbols.insert(
                variable_declaration.identifier.clone(),
                HIRSymbol::Binding(HIRBinding {
                    name: variable_declaration.identifier.clone(),
                    ty,
                    init: None,
                }),
            );
            Ok(current_scope)
        }
        StatementNode::Return(_) => Ok(current_scope),
        StatementNode::Break => Ok(current_scope),
        StatementNode::Continue => Ok(current_scope),
        StatementNode::If {
            condition: _c,
            then_branch,
            else_branch,
        } => {
            let mut then_scope = Scope::new();
            for stmt in then_branch {
                then_scope = resolve_statement(stmt, then_scope)?;
            }
            current_scope.children_scope.push(Box::new(then_scope));
            if let Some(else_branch) = else_branch {
                let mut else_scope = Scope::new();
                for stmt in else_branch {
                    else_scope = resolve_statement(stmt, else_scope)?;
                }
                current_scope.children_scope.push(Box::new(else_scope));
            }
            Ok(current_scope)
        }
        StatementNode::For {
            initializer,
            condition: _c,
            post_operation,
            body,
        } => {
            let mut for_scope = Scope::new();
            if let Some(init) = initializer {
                for_scope = resolve_statement(init, for_scope)?;
            }
            for stmt in body {
                for_scope = resolve_statement(stmt, for_scope)?;
                if let Some(po) = post_operation {
                    for_scope = resolve_statement(po, for_scope)?;
                }
            }
            current_scope.children_scope.push(Box::new(for_scope));
            Ok(current_scope)
        }
        StatementNode::Assignment {
            identifier: _i,
            expr: _e,
        } => {
            // TODO: for now i wont do anything, perhaps do type checking in the future
            Ok(current_scope)
        }
        StatementNode::FieldAssign { .. } => Ok(current_scope),
        StatementNode::DerefAssign { .. } => Ok(current_scope),
        StatementNode::IndexAssign { .. } => Ok(current_scope),
        StatementNode::ExpressionStatement(_) => Ok(current_scope),
        StatementNode::Defer(_) => Ok(current_scope),
        stmt => todo!("resolve_statement: statement {:?}", stmt),
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
        TypeExpression::Struct { fields } => {
            let hir_type = map_type(TypeExpression::Struct { fields: fields.clone() })?;
            current_scope.symbols.insert(lhs_td, HIRSymbol::Type(hir_type));
        }
        TypeExpression::Builtin(builtin_type) => {
            current_scope.symbols.insert(
                lhs_td,
                HIRSymbol::Type(HIRTypeKind::Builtin(builtin_type.clone())),
            );
        }
        TypeExpression::Array { .. } => {
            let hir_type = map_type(type_declaration.type_expression.clone())?;
            current_scope.symbols.insert(lhs_td, HIRSymbol::Type(hir_type));
        }
    }
    Ok(current_scope)
}

fn process_escape_sequences(raw: &str) -> Result<String, Box<dyn Error>> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next().ok_or("trailing backslash in string literal")? {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' => out.push('\0'),
            'x' => {
                let h1 = chars.next().ok_or("expected hex digit after \\x")?;
                let h2 = chars.next().ok_or("expected two hex digits after \\x")?;
                let byte = u8::from_str_radix(&format!("{}{}", h1, h2), 16)
                    .map_err(|_| format!("invalid hex escape \\x{}{}", h1, h2))?;
                out.push(byte as char);
            }
            'u' => {
                if chars.next() != Some('{') {
                    return Err("expected '{' after \\u".into());
                }
                let mut hex = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(d) => hex.push(d),
                        None => return Err("unterminated \\u{...} escape".into()),
                    }
                }
                let codepoint = u32::from_str_radix(&hex, 16)
                    .map_err(|_| format!("invalid unicode escape \\u{{{}}}", hex))?;
                let ch = char::from_u32(codepoint)
                    .ok_or_else(|| format!("invalid unicode scalar \\u{{{}}}", hex))?;
                out.push(ch);
            }
            other => return Err(format!("unknown escape sequence \\{}", other).into()),
        }
    }
    Ok(out)
}

fn resolve_function_decl(
    function_declaration: &FunctionDeclaration,
    mut current_scope: Scope,
) -> Result<Scope, Box<dyn Error>> {
    // Start the function's child scope with all symbols already visible at
    // module level so that body expressions can reference previously-declared
    // functions (e.g. calls to other top-level fns).
    let mut new_scope = Scope::new();

    // Copy module-level symbols first so they are available during body
    // resolution below.
    for (name, symbol) in &current_scope.symbols {
        new_scope.symbols.insert(name.clone(), symbol.clone());
    }

    // Parameters — inserted after the module-level copy so they shadow any
    // hypothetical module-level name collision (though that should not occur
    // in practice given the language semantics).
    for param in function_declaration.signature.parameters.clone() {
        new_scope.symbols.insert(
            param.parameter_name.clone(),
            HIRSymbol::Binding(HIRBinding {
                name: param.parameter_name,
                ty: map_type(param.parameter_type)?,
                init: None,
            }),
        );
    }

    // Resolve all statements in the body so local bindings are registered.
    if let Some(fb) = &function_declaration.body {
        for stmt in fb.statements.iter() {
            new_scope = resolve_statement(stmt, new_scope)?;
        }
    }

    // Add new scope to current scope
    current_scope.symbols.insert(
        function_declaration.signature.name.clone(),
        HIRSymbol::Function(func_to_hir(
            function_declaration.clone(),
            &mut new_scope.clone(),
        )?),
    );
    current_scope.children_scope.push(Box::new(new_scope));
    Ok(current_scope)
}
