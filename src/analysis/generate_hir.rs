use std::collections::HashMap;
use std::fmt;

use crate::ast::{Ast, StatementNode};
use crate::ast::{
    DeclarationNode, Expression as PExpr, Field, FunctionDeclaration,
    StatementNode as ASTStatementNode, TypeExpression, VariableDeclaration,
};
use crate::hir::{
    CompilationUnit, HIRBinding, HIRDeclaration, HIRExpression, HIRExpressionKind, HIRFunction,
    HIRIf, HIRModule, HIRStmt, HIRSymbol, HIRTypeDeclaration, HIRTypeKind, Scope,
};
use crate::tokens::Operator;
use crate::tokens::builtin::BuiltinType;
use crate::tokens::identifier::Identifier;
use crate::tokens::literal::Literal;

#[derive(Debug)]
pub struct AnalysisError {
    pub msg: String,
}

impl std::error::Error for AnalysisError {}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AnalysisError: {}", self.msg)
    }
}

impl From<String> for AnalysisError {
    fn from(value: String) -> Self {
        Self { msg: value }
    }
}

/// Perform semantic analysis on the parser AST and produce a vector of HIR functions.
pub fn analyze(
    ast: Ast,
    resolved_modules: &std::collections::HashMap<Vec<String>, HIRModule>,
) -> Result<CompilationUnit, AnalysisError> {
    // name resolution
    let mut current_scope = Scope::new();

    // Import resolution: process all import declarations first so that module
    // symbols are available when resolving subsequent declarations.
    for declaration in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = declaration {
            let path_strs: Vec<String> =
                import.path.iter().map(|id| id.identifier.clone()).collect();
            let module = resolved_modules
                .get(&path_strs)
                .ok_or_else(|| format!("module '{}' not found", path_strs.join("::")))?;

            if let Some(selected) = &import.selective {
                // Copy selected symbols directly into local scope
                for name in selected {
                    let sym = module.exports.get(name).ok_or_else(|| {
                        format!("'{}' not found in module '{}'", name, path_strs.join("::"))
                    })?;
                    current_scope.symbols.insert(name.clone(), sym.clone());
                }
            } else {
                // Register as a named module in scope
                let local_name = import
                    .alias
                    .as_ref()
                    .map(|a| a.identifier.clone())
                    .unwrap_or_else(|| import.path.last().unwrap().identifier.clone());
                current_scope.modules.insert(local_name, module.clone());
            }
        }
    }

    // Collect imported declarations for lowering
    let mut imported_declarations: Vec<HIRDeclaration> = Vec::new();
    for module in current_scope.modules.values() {
        imported_declarations.extend(module.declarations.clone());
    }
    // Also collect selectively imported declarations
    for declaration in &ast.declarations {
        if let DeclarationNode::ImportDeclaration(import) = declaration
            && import.selective.is_some()
        {
            let path_strs: Vec<String> =
                import.path.iter().map(|id| id.identifier.clone()).collect();
            if let Some(module) = resolved_modules.get(&path_strs) {
                imported_declarations.extend(module.declarations.clone());
            }
        }
    }

    let mut hir_declarations: Vec<HIRDeclaration> = Vec::new();
    // Cache of instantiated generic functions keyed by mangled name.
    let mut generic_cache: HashMap<String, HIRFunction> = HashMap::new();

    for declaration in ast.declarations {
        current_scope = resolve_declaration(&declaration, current_scope)?;
        let hir_declaration: Option<HIRDeclaration> = match declaration {
            DeclarationNode::ImportDeclaration(_) => None,
            DeclarationNode::FunctionDeclaration(function_declaration) => {
                // Generic functions are stored as templates; they produce no direct HIR declaration.
                if is_generic_function(&function_declaration) {
                    None
                } else {
                    Some(HIRDeclaration::HIRFunction(func_to_hir(
                        function_declaration,
                        &mut current_scope,
                        &mut generic_cache,
                    )?))
                }
            }
            DeclarationNode::TypeDeclaration(ty_decl) => match ty_decl.expression {
                TypeExpression::TypeKeyword => None,
                expr => {
                    let ty = map_type(expr)?;
                    Some(HIRDeclaration::HIRType(HIRTypeDeclaration {
                        name: ty_decl.name,
                        ty,
                    }))
                }
            },
        };
        if let Some(hir_declaration) = hir_declaration {
            hir_declarations.push(hir_declaration);
        }
    }

    // Collect generic instantiations produced during analysis
    for (_, func) in generic_cache {
        hir_declarations.push(HIRDeclaration::HIRFunction(func));
    }

    let compilation_unit = CompilationUnit {
        scope_root: current_scope,
        declarations: hir_declarations,
        imported_declarations,
    };
    Ok::<CompilationUnit, AnalysisError>(compilation_unit)
}

fn func_to_hir(
    function_declaration: FunctionDeclaration,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRFunction, AnalysisError> {
    // Only include non-comptime parameters in the HIR function signature.
    let mut params = Vec::new();
    for param in &function_declaration.signature.parameters {
        let ty = map_type(param.parameter_type.clone())?;
        if ty != HIRTypeKind::Type {
            params.push((param.parameter_name.clone(), ty));
        }
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
        let ty = map_type(param.parameter_type.clone())?;
        if ty != HIRTypeKind::Type {
            body_scope.symbols.insert(
                param.parameter_name.clone(),
                HIRSymbol::Binding(HIRBinding {
                    name: param.parameter_name.clone(),
                    ty,
                    init: None,
                    mutable: true,
                }),
            );
        }
    }

    let mut body = Vec::new();
    if let Some(fb) = function_declaration.body {
        for stmt in fb.statements {
            body.push(stmt_to_hir(stmt, &mut body_scope, generic_cache)?);
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

fn stmt_to_hir(
    stmt: StatementNode,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRStmt, AnalysisError> {
    match stmt {
        StatementNode::VariableDeclaration(variable_declaration) => Ok(HIRStmt::Binding(
            var_decl_to_hir(variable_declaration, current_scope, generic_cache)?,
        )),
        StatementNode::Assignment { identifier, expr } => {
            if let Some(HIRSymbol::Binding(binding)) = current_scope.symbols.get(&identifier)
                && !binding.mutable
            {
                return Err(format!("cannot assign to constant '{}'", identifier).into());
            }
            let e = expr_to_hir(expr, current_scope, generic_cache)?;
            Ok(HIRStmt::Assign {
                name: identifier,
                expr: e,
            })
        }
        StatementNode::FieldAssign {
            object,
            field,
            expr,
        } => {
            // Look up the object's type to find the field index
            let obj_ty =
                match current_scope.symbols.get(&object).ok_or_else(|| {
                    format!("stmt_to_hir: identifier {} not found in scope", object)
                })? {
                    HIRSymbol::Binding(b) => {
                        if !b.mutable {
                            return Err(
                                format!("cannot assign to field of constant '{}'", object).into()
                            );
                        }
                        b.ty.clone()
                    }
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
                    format!(
                        "stmt_to_hir: field {} not found in struct {}",
                        field.identifier, object
                    )
                })?;
            let e = expr_to_hir(expr, current_scope, generic_cache)?;
            Ok(HIRStmt::FieldAssign {
                object,
                field: field.identifier,
                field_index,
                expr: e,
            })
        }
        StatementNode::ExpressionStatement(e) => {
            Ok(HIRStmt::Expr(expr_to_hir(e, current_scope, generic_cache)?))
        }
        StatementNode::Return(opt) => Ok(HIRStmt::Return(match opt {
            Some(e) => Some(expr_to_hir(e, current_scope, generic_cache)?),
            None => None,
        })),
        StatementNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = expr_to_hir(condition, current_scope, generic_cache)?;
            let mut then_h = Vec::new();
            for s in then_branch {
                then_h.push(stmt_to_hir(s, current_scope, generic_cache)?);
            }
            let else_h = match else_branch {
                Some(v) => {
                    let mut ev = Vec::new();
                    for s in v {
                        ev.push(stmt_to_hir(s, current_scope, generic_cache)?);
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
                Some(b) => Some(Box::new(stmt_to_hir(*b, current_scope, generic_cache)?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_hir(e, current_scope, generic_cache)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_hir(*b, current_scope, generic_cache)?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body {
                body_h.push(stmt_to_hir(s, current_scope, generic_cache)?);
            }
            Ok(HIRStmt::For {
                init: init_h,
                cond: cond_h,
                post: post_h,
                body: body_h,
            })
        }
        StatementNode::DerefAssign { pointer, expr } => {
            let ptr_hir = expr_to_hir(pointer, current_scope, generic_cache)?;
            let pointee_ty = match &ptr_hir.inferred_type {
                HIRTypeKind::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "stmt_to_hir: DerefAssign pointer expression has non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            let val_hir = expr_to_hir(expr, current_scope, generic_cache)?;
            // Allow coercion of integer literals (which default to Int32) to the pointee type
            let _val_ty = &val_hir.inferred_type;
            let _ = pointee_ty; // type already verified above
            Ok(HIRStmt::DerefAssign {
                pointer: ptr_hir,
                expr: val_hir,
            })
        }
        StatementNode::IndexAssign {
            object,
            index,
            expr,
        } => {
            let obj_hir = expr_to_hir(object, current_scope, generic_cache)?;
            match &obj_hir.inferred_type {
                HIRTypeKind::Pointer(_) | HIRTypeKind::Array { .. } => {}
                other => {
                    return Err(format!(
                        "stmt_to_hir: IndexAssign on non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            }
            let idx_hir = expr_to_hir(index, current_scope, generic_cache)?;
            let val_hir = expr_to_hir(expr, current_scope, generic_cache)?;
            Ok(HIRStmt::IndexAssign {
                object: obj_hir,
                index: idx_hir,
                expr: val_hir,
            })
        }
        StatementNode::Break => Ok(HIRStmt::Break),
        StatementNode::Continue => Ok(HIRStmt::Continue),
        StatementNode::Defer(inner) => {
            let hir_inner = stmt_to_hir(*inner, current_scope, generic_cache)?;
            Ok(HIRStmt::Defer(Box::new(hir_inner)))
        }
    }
}

fn var_decl_to_hir(
    var_decl: VariableDeclaration,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRBinding, AnalysisError> {
    let ty = match var_decl.constant_type {
        Some(TypeExpression::TypeKeyword) => {
            return Err("mutable type bindings (`var type`) are not yet supported; use `const type` for compile-time type aliases".to_string().into());
        }
        Some(t) => map_type(t)?,
        None => HIRTypeKind::Builtin(BuiltinType::Void),
    };
    let mut init = if let Some(expr) = var_decl.expression {
        expr_to_hir(expr, current_scope, generic_cache)?
    } else {
        // Zero-initialize based on declared type when no initializer is present
        match &ty {
            HIRTypeKind::Builtin(BuiltinType::Boolean) => HIRExpression {
                inferred_type: HIRTypeKind::Builtin(BuiltinType::Boolean),
                expression: HIRExpressionKind::LiteralBool(false),
            },
            HIRTypeKind::Builtin(
                BuiltinType::Int1
                | BuiltinType::Int2
                | BuiltinType::Int4
                | BuiltinType::Int8
                | BuiltinType::Int16
                | BuiltinType::UInt1
                | BuiltinType::UInt2
                | BuiltinType::UInt4
                | BuiltinType::UInt8
                | BuiltinType::UInt16,
            ) => HIRExpression {
                inferred_type: ty.clone(),
                expression: HIRExpressionKind::LiteralInt { value: 0 },
            },
            HIRTypeKind::Pointer(_) => HIRExpression {
                inferred_type: ty.clone(),
                expression: HIRExpressionKind::Null,
            },
            _ => {
                return Err("var declaration requires type or initializer"
                    .to_string()
                    .into());
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
            HIRTypeKind::Identifier(id) => match current_scope.symbols.get(id) {
                Some(HIRSymbol::Type(inner)) => *inner == init.inferred_type,
                _ => false,
            },
            _ => false,
        };
        if resolved_ty_match {
            // Replace the init's inferred type with the declared identifier type
            // so that subsequent lookups (e.g. binding type in codegen) return
            // the identifier-keyed type.
            init.inferred_type = ty.clone();
        } else if let (
            HIRTypeKind::Array {
                element_type: decl_elem,
                size: decl_size,
            },
            HIRTypeKind::Array {
                size: init_size, ..
            },
        ) = (&ty, &init.inferred_type)
        {
            if *decl_size != *init_size {
                return Err(format!(
                    "array size mismatch for {}: declared size {} but initializer has {} elements",
                    var_decl.identifier, decl_size, init_size
                )
                .into());
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
                BuiltinType::Int1 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int1),
                BuiltinType::Int2 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int2),
                BuiltinType::Int4 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int4),
                BuiltinType::Int8 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int8),
                BuiltinType::Int16 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::Int16),
                BuiltinType::UInt1 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt1),
                BuiltinType::UInt2 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt2),
                BuiltinType::UInt4 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt4),
                BuiltinType::UInt8 => init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt8),
                BuiltinType::UInt16 => {
                    init.inferred_type = HIRTypeKind::Builtin(BuiltinType::UInt16)
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
        mutable: true,
    };
    current_scope
        .symbols
        .insert(var_decl.identifier, HIRSymbol::Binding(hir_var.clone()));
    Ok(hir_var)
}

fn expr_to_hir(
    expr: PExpr,
    current_scope: &Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRExpression, AnalysisError> {
    match expr {
        PExpr::Literal(Literal::Integer(value)) => {
            // Default to Int32 (i32); explicit type annotations coerce as needed.
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Builtin(BuiltinType::Int4),
                expression: HIRExpressionKind::LiteralInt { value },
            })
        }
        PExpr::Literal(Literal::Boolean(b)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Boolean),
            expression: HIRExpressionKind::LiteralBool(b),
        }),
        PExpr::Literal(Literal::Float(f)) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Float8),
            expression: HIRExpressionKind::LiteralFloat { value: f },
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
        PExpr::Identifier(name) => {
            let sym = current_scope
                .symbols
                .get(&name)
                .ok_or_else(|| format!("expr_to_hir: identifier {} not found in scope", name))?;
            match sym {
                HIRSymbol::Binding(var) => Ok(HIRExpression {
                    inferred_type: var.ty.clone(),
                    expression: HIRExpressionKind::Identifier(name),
                }),
                HIRSymbol::Type(ty) => {
                    // A type name used in expression position is a comptime type value.
                    Ok(HIRExpression {
                        inferred_type: HIRTypeKind::Type,
                        expression: HIRExpressionKind::ComptimeType(ty.clone()),
                    })
                }
                HIRSymbol::GenericFunction(_) | HIRSymbol::Function(_) => {
                    Err(format!("expr_to_hir: identifier {} is not a variable", name).into())
                }
            }
        }
        PExpr::TypeValue(te) => {
            let hir_type = map_type(te)?;
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Type,
                expression: HIRExpressionKind::ComptimeType(hir_type),
            })
        }
        PExpr::Binary {
            left,
            operator,
            right,
        } => {
            let l = expr_to_hir(*left, current_scope, generic_cache)?;
            let mut r = expr_to_hir(*right, current_scope, generic_cache)?;
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
                op => return Err(format!("unsupported binary operator {:?}", op).into()),
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
        PExpr::Grouping(inner) => expr_to_hir(*inner, current_scope, generic_cache),
        PExpr::Call { callee, args } => {
            match *callee {
                PExpr::Identifier(name) => {
                    match current_scope.symbols.get(&name).cloned() {
                        Some(HIRSymbol::GenericFunction(template)) => {
                            // Generic call: extract comptime type args and instantiate.
                            let (mangled_name, return_type) = instantiate_generic(
                                &template,
                                &args,
                                current_scope,
                                generic_cache,
                            )?;
                            // Build HIR args for runtime (non-comptime) parameters only.
                            let mut hargs = Vec::new();
                            for (i, arg) in args.into_iter().enumerate() {
                                if !template.comptime_params.contains(&i) {
                                    hargs.push(expr_to_hir(arg, current_scope, generic_cache)?);
                                }
                            }
                            Ok(HIRExpression {
                                inferred_type: return_type,
                                expression: HIRExpressionKind::Call {
                                    callee: Identifier { identifier: mangled_name },
                                    args: hargs,
                                },
                            })
                        }
                        Some(HIRSymbol::Function(func)) => {
                            let inferred_type = func.return_type.clone();
                            let mut hargs = Vec::new();
                            for a in args {
                                hargs.push(expr_to_hir(a, current_scope, generic_cache)?);
                            }
                            Ok(HIRExpression {
                                inferred_type,
                                expression: HIRExpressionKind::Call {
                                    callee: name.clone(),
                                    args: hargs,
                                },
                            })
                        }
                        Some(_) => Err(format!(
                            "expr_to_hir: symbol {} is not a function",
                            name
                        ).into()),
                        None => Err(format!(
                            "expr_to_hir: unknown function '{}' — did you forget an extern declaration?",
                            name
                        ).into()),
                    }
                }
                PExpr::QualifiedAccess { module, member } => {
                    let module_alias = module.identifier.clone();
                    let hir_module = current_scope
                        .modules
                        .get(&module_alias)
                        .ok_or_else(|| format!("unknown module '{}'", module_alias))?;
                    let func = match hir_module.exports.get(&member) {
                        Some(HIRSymbol::Function(f)) => f.clone(),
                        Some(_) => {
                            return Err(format!(
                                "'{}' in module '{}' is not a function",
                                member, module_alias
                            )
                            .into());
                        }
                        None => {
                            return Err(format!(
                                "'{}' not found in module '{}'",
                                member, module_alias
                            )
                            .into());
                        }
                    };
                    // Extern functions keep their C name; user functions get a mangled name.
                    let callee_name = if func.is_extern {
                        member.clone()
                    } else {
                        Identifier {
                            identifier: format!("{}__{}", module_alias, member.identifier),
                        }
                    };
                    let mut hargs = Vec::new();
                    for a in args {
                        hargs.push(expr_to_hir(a, current_scope, generic_cache)?);
                    }
                    Ok(HIRExpression {
                        inferred_type: func.return_type.clone(),
                        expression: HIRExpressionKind::Call {
                            callee: callee_name,
                            args: hargs,
                        },
                    })
                }
                // Only accept identifier/qualified callees
                _ => Err(
                    "expr_to_hir: call target must be an identifier or qualified access"
                        .to_string()
                        .into(),
                ),
            }
        }
        PExpr::FieldAccess { object, field } => {
            let obj_hir = expr_to_hir(*object, current_scope, generic_cache)?;
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
            let struct_ty = match current_scope
                .symbols
                .get(&type_name)
                .ok_or_else(|| format!("expr_to_hir: type {} not found in scope", type_name))?
            {
                HIRSymbol::Type(ty) => ty.clone(),
                _ => return Err(format!("expr_to_hir: {} is not a type", type_name).into()),
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
                let fval = expr_to_hir(fexpr, current_scope, generic_cache)?;
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
            let inner_hir = expr_to_hir(*inner, current_scope, generic_cache)?;
            let ptr_ty = HIRTypeKind::Pointer(Box::new(inner_hir.inferred_type.clone()));
            Ok(HIRExpression {
                inferred_type: ptr_ty,
                expression: HIRExpressionKind::AddressOf(Box::new(inner_hir)),
            })
        }
        PExpr::Dereference(inner) => {
            let inner_hir = expr_to_hir(*inner, current_scope, generic_cache)?;
            let pointee_ty = match &inner_hir.inferred_type {
                HIRTypeKind::Pointer(pointee) => *pointee.clone(),
                other => {
                    return Err(format!(
                        "expr_to_hir: dereference of non-pointer type {:?}",
                        other
                    )
                    .into());
                }
            };
            Ok(HIRExpression {
                inferred_type: pointee_ty,
                expression: HIRExpressionKind::Deref(Box::new(inner_hir)),
            })
        }
        PExpr::Cast { expr, target_type } => {
            let mut inner_hir = expr_to_hir(*expr, current_scope, generic_cache)?;
            let hir_target = map_type(target_type)?;
            // If the inner expression is a function call to an unknown external function,
            // propagate the cast target type so the auto-declaration uses the right return type.
            if let HIRExpressionKind::Call { .. } = &inner_hir.expression
                && inner_hir.inferred_type == HIRTypeKind::Builtin(BuiltinType::Int4)
            {
                inner_hir.inferred_type = hir_target.clone();
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
            let obj_hir = expr_to_hir(*object, current_scope, generic_cache)?;
            let idx_hir = expr_to_hir(*index, current_scope, generic_cache)?;
            let pointee_ty = match &obj_hir.inferred_type {
                HIRTypeKind::Pointer(inner) => *inner.clone(),
                HIRTypeKind::Array { element_type, .. } => *element_type.clone(),
                other => {
                    return Err(format!(
                        "expr_to_hir: index access on non-pointer type {:?}",
                        other
                    )
                    .into());
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
                .map(|e| expr_to_hir(e, current_scope, generic_cache))
                .collect::<Result<_, _>>()?;
            if hir_elements.is_empty() {
                return Err("array literal must have at least one element"
                    .to_string()
                    .into());
            }
            let elem_ty = hir_elements[0].inferred_type.clone();
            for (i, e) in hir_elements.iter().enumerate() {
                if e.inferred_type != elem_ty {
                    return Err(format!(
                        "array literal: element {} has type {:?}, expected {:?}",
                        i, e.inferred_type, elem_ty
                    )
                    .into());
                }
            }
            let size = hir_elements.len() as u64;
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Array {
                    element_type: Box::new(elem_ty),
                    size,
                },
                expression: HIRExpressionKind::ArrayLiteral {
                    elements: hir_elements,
                },
            })
        }
        PExpr::Unary {
            operator,
            expression,
        } => match operator {
            Operator::Minus => {
                let inner = expr_to_hir(*expression, current_scope, generic_cache)?;
                let is_float = matches!(
                    inner.inferred_type,
                    HIRTypeKind::Builtin(BuiltinType::Float4 | BuiltinType::Float8)
                );
                let zero = HIRExpression {
                    inferred_type: inner.inferred_type.clone(),
                    expression: if is_float {
                        HIRExpressionKind::LiteralFloat { value: 0.0 }
                    } else {
                        HIRExpressionKind::LiteralInt { value: 0 }
                    },
                };
                Ok(HIRExpression {
                    inferred_type: inner.inferred_type.clone(),
                    expression: HIRExpressionKind::Binary {
                        left: Box::new(zero),
                        operator: Operator::Minus,
                        right: Box::new(inner),
                    },
                })
            }
            Operator::LogicalNot => expr_to_hir(
                PExpr::Binary {
                    left: expression,
                    operator: Operator::DoubleEquals,
                    right: Box::new(PExpr::Literal(Literal::Boolean(false))),
                },
                current_scope,
                generic_cache,
            ),
            Operator::Tilde => {
                // Desugar ~x to x ^ (-1) which in two's complement flips all bits
                let inner = expr_to_hir(*expression, current_scope, generic_cache)?;
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
        PExpr::QualifiedAccess { module, member } => {
            let module_alias = module.identifier.clone();
            let hir_module = current_scope
                .modules
                .get(&module_alias)
                .ok_or_else(|| format!("unknown module '{}'", module_alias))?;
            let sym = hir_module
                .exports
                .get(&member)
                .ok_or_else(|| format!("'{}' not found in module '{}'", member, module_alias))?;
            let inferred_type = match sym {
                HIRSymbol::Binding(b) => b.ty.clone(),
                HIRSymbol::Function(f) => HIRTypeKind::Function {
                    argument_types: f.params.iter().map(|(_, t)| t.clone()).collect(),
                    return_type: Box::new(f.return_type.clone()),
                },
                HIRSymbol::Type(t) => t.clone(),
                HIRSymbol::GenericFunction(_) => {
                    return Err(format!(
                        "'{}::{}' is a generic function and cannot be used as a value",
                        module_alias, member
                    )
                    .into());
                }
            };
            Ok(HIRExpression {
                inferred_type,
                expression: HIRExpressionKind::QualifiedAccess {
                    module: module_alias,
                    name: member,
                },
            })
        }
    }
}

/// Resolve a HIRTypeKind (possibly an Identifier alias) to its struct fields.
fn resolve_struct_fields(
    ty: &HIRTypeKind,
    current_scope: &Scope,
) -> Result<Vec<(String, Box<HIRTypeKind>)>, AnalysisError> {
    match ty {
        HIRTypeKind::Struct { fields } => Ok(fields.clone()),
        HIRTypeKind::Identifier(id) => {
            let symbol = current_scope
                .symbols
                .get(id)
                .ok_or_else(|| format!("resolve_struct_fields: type {} not found in scope", id))?;
            match symbol {
                HIRSymbol::Type(inner_ty) => resolve_struct_fields(inner_ty, current_scope),
                _ => Err(format!("resolve_struct_fields: {} is not a type", id).into()),
            }
        }
        _ => Err(format!("resolve_struct_fields: {:?} is not a struct type", ty).into()),
    }
}

fn map_type(type_expression: TypeExpression) -> Result<HIRTypeKind, AnalysisError> {
    let hir_typekind = match type_expression {
        TypeExpression::TypeKeyword => HIRTypeKind::Type,
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
            let mut mapped_arguments = Vec::new();
            for argument in argument_types {
                mapped_arguments.push(map_type(argument)?);
            }
            HIRTypeKind::Function {
                argument_types: mapped_arguments,
                return_type: Box::new(map_type(*return_type)?),
            }
        }
        TypeExpression::Pointer {
            pointer_variant: _,
            pointed_type,
        } => {
            let inner = map_type(*pointed_type)?;
            HIRTypeKind::Pointer(Box::new(inner))
        }
        TypeExpression::Array { element_type, size } => HIRTypeKind::Array {
            element_type: Box::new(map_type(*element_type)?),
            size,
        },
        TypeExpression::QualifiedIdentifier { module, name } => HIRTypeKind::QualifiedIdentifier {
            module: module.identifier.clone(),
            name,
        },
    };

    Ok(hir_typekind)
}

fn resolve_declaration(
    declaration: &DeclarationNode,
    current_scope: Scope,
) -> Result<Scope, AnalysisError> {
    match declaration {
        DeclarationNode::ImportDeclaration(_) => Ok(current_scope), // handled in analyze()
        DeclarationNode::FunctionDeclaration(function_declaration) => {
            resolve_function_decl(function_declaration, current_scope)
        }
        DeclarationNode::TypeDeclaration(type_declaration) => {
            let ty = map_type(type_declaration.expression.clone())?;
            let mut new_scope = current_scope;
            new_scope
                .symbols
                .insert(type_declaration.name.clone(), HIRSymbol::Type(ty));
            Ok(new_scope)
        }
    }
}

fn resolve_statement(
    statement: &StatementNode,
    mut current_scope: Scope,
) -> Result<Scope, AnalysisError> {
    match statement {
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
                    mutable: true,
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
            }
            if let Some(po) = post_operation {
                for_scope = resolve_statement(po, for_scope)?;
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
    }
}

/// Returns true if the function has at least one `type`-typed parameter (making it generic).
fn is_generic_function(fn_decl: &FunctionDeclaration) -> bool {
    fn_decl
        .signature
        .parameters
        .iter()
        .any(|p| matches!(p.parameter_type, TypeExpression::TypeKeyword))
}

/// Compute a stable mangle string for a type expression (used in generic function name mangling).
fn mangle_type_expr(te: &TypeExpression) -> String {
    match te {
        TypeExpression::Builtin(bt) => format!("{}", bt),
        TypeExpression::Identifier(id) => id.identifier.clone(),
        TypeExpression::Pointer { pointed_type, .. } => {
            format!("ptr_{}", mangle_type_expr(pointed_type))
        }
        TypeExpression::Array { element_type, size } => {
            format!("arr{}_{}", size, mangle_type_expr(element_type))
        }
        TypeExpression::Struct { .. } => "struct".to_string(),
        TypeExpression::Function { .. } => "fn".to_string(),
        TypeExpression::QualifiedIdentifier { module, name } => {
            format!("{}__{}", module.identifier, name.identifier)
        }
        TypeExpression::TypeKeyword => "type".to_string(),
    }
}

/// Substitute all occurrences of type identifiers in `subs` within a TypeExpression.
fn substitute_type(te: &TypeExpression, subs: &HashMap<String, TypeExpression>) -> TypeExpression {
    match te {
        TypeExpression::Identifier(id) => {
            if let Some(replacement) = subs.get(&id.identifier) {
                replacement.clone()
            } else {
                te.clone()
            }
        }
        TypeExpression::Pointer {
            pointer_variant,
            pointed_type,
        } => TypeExpression::Pointer {
            pointer_variant: pointer_variant.clone(),
            pointed_type: Box::new(substitute_type(pointed_type, subs)),
        },
        TypeExpression::Array { element_type, size } => TypeExpression::Array {
            element_type: Box::new(substitute_type(element_type, subs)),
            size: *size,
        },
        TypeExpression::Struct { fields } => TypeExpression::Struct {
            fields: fields
                .iter()
                .map(|f| Field {
                    label: f.label.clone(),
                    type_id: substitute_type(&f.type_id, subs),
                })
                .collect(),
        },
        TypeExpression::Function {
            argument_types,
            return_type,
        } => TypeExpression::Function {
            argument_types: argument_types
                .iter()
                .map(|t| substitute_type(t, subs))
                .collect(),
            return_type: Box::new(substitute_type(return_type, subs)),
        },
        // Builtins, QualifiedIdentifier, TypeKeyword contain no substitutable identifiers
        _ => te.clone(),
    }
}

fn substitute_in_expr(expr: &mut PExpr, subs: &HashMap<String, TypeExpression>) {
    match expr {
        PExpr::Cast {
            target_type,
            expr: inner,
        } => {
            *target_type = substitute_type(target_type, subs);
            substitute_in_expr(inner, subs);
        }
        PExpr::Binary { left, right, .. } => {
            substitute_in_expr(left, subs);
            substitute_in_expr(right, subs);
        }
        PExpr::Unary { expression, .. } => substitute_in_expr(expression, subs),
        PExpr::Call { callee, args } => {
            substitute_in_expr(callee, subs);
            for arg in args {
                substitute_in_expr(arg, subs);
            }
        }
        PExpr::FieldAccess { object, .. } => substitute_in_expr(object, subs),
        PExpr::AddressOf(inner) | PExpr::Dereference(inner) | PExpr::Grouping(inner) => {
            substitute_in_expr(inner, subs);
        }
        PExpr::IndexAccess { object, index } => {
            substitute_in_expr(object, subs);
            substitute_in_expr(index, subs);
        }
        PExpr::ArrayLiteral { elements } => {
            for e in elements {
                substitute_in_expr(e, subs);
            }
        }
        PExpr::StructConstruct { fields, .. } => {
            for (_, val) in fields {
                substitute_in_expr(val, subs);
            }
        }
        // Literals, Identifiers, QualifiedAccess, TypeValue — no substitution
        _ => {}
    }
}

fn substitute_in_stmt(stmt: &mut ASTStatementNode, subs: &HashMap<String, TypeExpression>) {
    match stmt {
        ASTStatementNode::VariableDeclaration(decl) => {
            if let Some(t) = &decl.constant_type {
                decl.constant_type = Some(substitute_type(t, subs));
            }
            if let Some(e) = &mut decl.expression {
                substitute_in_expr(e, subs);
            }
        }
        ASTStatementNode::Return(Some(expr)) => substitute_in_expr(expr, subs),
        ASTStatementNode::ExpressionStatement(expr) => substitute_in_expr(expr, subs),
        ASTStatementNode::Assignment { expr, .. } => substitute_in_expr(expr, subs),
        ASTStatementNode::FieldAssign { expr, .. } => substitute_in_expr(expr, subs),
        ASTStatementNode::DerefAssign { pointer, expr } => {
            substitute_in_expr(pointer, subs);
            substitute_in_expr(expr, subs);
        }
        ASTStatementNode::IndexAssign {
            object,
            index,
            expr,
        } => {
            substitute_in_expr(object, subs);
            substitute_in_expr(index, subs);
            substitute_in_expr(expr, subs);
        }
        ASTStatementNode::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_in_expr(condition, subs);
            for s in then_branch {
                substitute_in_stmt(s, subs);
            }
            if let Some(eb) = else_branch {
                for s in eb {
                    substitute_in_stmt(s, subs);
                }
            }
        }
        ASTStatementNode::For {
            initializer,
            condition,
            post_operation,
            body,
        } => {
            if let Some(init) = initializer {
                substitute_in_stmt(init, subs);
            }
            if let Some(cond) = condition {
                substitute_in_expr(cond, subs);
            }
            if let Some(post) = post_operation {
                substitute_in_stmt(post, subs);
            }
            for s in body {
                substitute_in_stmt(s, subs);
            }
        }
        ASTStatementNode::Defer(inner) => substitute_in_stmt(inner, subs),
        ASTStatementNode::Break | ASTStatementNode::Continue | ASTStatementNode::Return(None) => {}
    }
}

/// Monomorphize a generic function with the given type arguments.
/// Returns the mangled name and the instantiated return type.
/// The instantiated HIRFunction is stored in `generic_cache`.
fn instantiate_generic(
    template: &crate::hir::GenericFunctionTemplate,
    call_args: &[PExpr],
    scope: &Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<(String, HIRTypeKind), AnalysisError> {
    // Build type substitution map: param_name -> TypeExpression
    let mut subs: HashMap<String, TypeExpression> = HashMap::new();
    let mut type_arg_names: Vec<String> = Vec::new();

    for &param_idx in &template.comptime_params {
        let param_name = template.ast_decl.signature.parameters[param_idx]
            .parameter_name
            .identifier
            .clone();
        let arg = call_args.get(param_idx).ok_or_else(|| {
            format!(
                "generic call to '{}': missing argument for comptime param '{}'",
                template.name, param_name
            )
        })?;
        let type_expr = match arg {
            PExpr::TypeValue(te) => te.clone(),
            PExpr::Identifier(id) => {
                // A user-defined type name passed as argument
                match scope.symbols.get(id) {
                    Some(HIRSymbol::Type(_)) => TypeExpression::Identifier(id.clone()),
                    _ => return Err(format!(
                        "generic call to '{}': argument for comptime param '{}' must be a type, got identifier '{}'",
                        template.name, param_name, id
                    ).into()),
                }
            }
            _ => return Err(format!(
                "generic call to '{}': argument for comptime param '{}' must be a type expression",
                template.name, param_name
            )
            .into()),
        };
        type_arg_names.push(mangle_type_expr(&type_expr));
        subs.insert(param_name, type_expr);
    }

    // Build mangled name: funcname__type1__type2
    let mangled = format!(
        "{}__{}",
        template.name.identifier,
        type_arg_names.join("__")
    );

    // Check cache — deduplication
    if let Some(cached) = generic_cache.get(&mangled) {
        return Ok((mangled, cached.return_type.clone()));
    }

    // Clone and substitute the AST function declaration
    let mut substituted = template.ast_decl.clone();
    substituted.signature.name = Identifier {
        identifier: mangled.clone(),
    };

    // Apply type substitution to parameter types (skip comptime params — they will be removed)
    for param in &mut substituted.signature.parameters {
        param.parameter_type = substitute_type(&param.parameter_type, &subs);
    }
    // Remove comptime parameters from the signature
    let runtime_params: Vec<_> = substituted
        .signature
        .parameters
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !template.comptime_params.contains(i))
        .map(|(_, p)| p)
        .collect();
    substituted.signature.parameters = runtime_params;

    // Substitute return type
    if let Some(rt) = &substituted.signature.return_type {
        substituted.signature.return_type = Some(substitute_type(rt, &subs));
    }
    // Substitute in body statements
    if let Some(ref mut body) = substituted.body {
        for stmt in &mut body.statements {
            substitute_in_stmt(stmt, &subs);
        }
    }

    // Analyze the substituted function
    let mut func_scope = scope.clone();
    // Insert self-reference for recursion
    // (will be replaced after analysis with the real function)
    let hir_func = func_to_hir(substituted, &mut func_scope, generic_cache)?;
    let return_type = hir_func.return_type.clone();

    // Cache the instantiation
    generic_cache.insert(mangled.clone(), hir_func);

    Ok((mangled, return_type))
}

fn process_escape_sequences(raw: &str) -> Result<String, AnalysisError> {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars
            .next()
            .ok_or("trailing backslash in string literal".to_string())?
        {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' => out.push('\0'),
            'x' => {
                let h1 = chars
                    .next()
                    .ok_or("expected hex digit after \\x".to_string())?;
                let h2 = chars
                    .next()
                    .ok_or("expected two hex digits after \\x".to_string())?;
                let byte = u8::from_str_radix(&format!("{}{}", h1, h2), 16)
                    .map_err(|_| format!("invalid hex escape \\x{}{}", h1, h2))?;
                out.push(byte as char);
            }
            'u' => {
                if chars.next() != Some('{') {
                    return Err("expected '{{' after \\u".to_string().into());
                }
                let mut hex = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(d) => hex.push(d),
                        None => return Err("unterminated \\u{{...}} escape".to_string().into()),
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
) -> Result<Scope, AnalysisError> {
    // Generic functions (those with `type`-typed parameters) are stored as templates.
    // They are instantiated lazily when called with concrete type arguments.
    if is_generic_function(function_declaration) {
        let comptime_params: Vec<usize> = function_declaration
            .signature
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p.parameter_type, TypeExpression::TypeKeyword))
            .map(|(i, _)| i)
            .collect();
        current_scope.symbols.insert(
            function_declaration.signature.name.clone(),
            HIRSymbol::GenericFunction(crate::hir::GenericFunctionTemplate {
                name: function_declaration.signature.name.clone(),
                ast_decl: function_declaration.clone(),
                comptime_params,
            }),
        );
        return Ok(current_scope);
    }

    // Start the function's child scope with all symbols already visible at
    // module level so that body expressions can reference previously-declared
    // functions (e.g. calls to other top-level fns).
    let mut new_scope = Scope::new();

    // Copy module-level symbols and imported modules so they are available during body resolution.
    for (name, symbol) in &current_scope.symbols {
        new_scope.symbols.insert(name.clone(), symbol.clone());
    }
    for (name, module) in &current_scope.modules {
        new_scope.modules.insert(name.clone(), module.clone());
    }

    // Parameters — inserted after the module-level copy so they shadow any
    // hypothetical module-level name collision (though that should not occur
    // in practice given the language semantics).
    for param in function_declaration.signature.parameters.clone() {
        let ty = map_type(param.parameter_type)?;
        if ty != HIRTypeKind::Type {
            new_scope.symbols.insert(
                param.parameter_name.clone(),
                HIRSymbol::Binding(HIRBinding {
                    name: param.parameter_name,
                    ty,
                    init: None,
                    mutable: true,
                }),
            );
        }
    }

    // Resolve all statements in the body so local bindings are registered.
    if let Some(fb) = &function_declaration.body {
        for stmt in fb.statements.iter() {
            new_scope = resolve_statement(stmt, new_scope)?;
        }
    }

    // Add new scope to current scope
    let mut temp_cache = HashMap::new();
    current_scope.symbols.insert(
        function_declaration.signature.name.clone(),
        HIRSymbol::Function(func_to_hir(
            function_declaration.clone(),
            &mut new_scope.clone(),
            &mut temp_cache,
        )?),
    );
    current_scope.children_scope.push(Box::new(new_scope));
    Ok(current_scope)
}
