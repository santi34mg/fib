use std::collections::HashMap;
use std::fmt;

use crate::frontend::ast::pattern::Pattern;
use crate::frontend::ast::{
    Ast, declaration::DeclarationNode, expression::Expression as PExpr, field::Field,
    function_declaration::FunctionDeclaration, statement::Statement, statement::StatementKind,
    type_expression::TypeExpression, variable_declaration::VariableDeclaration,
};
use crate::frontend::identifier::Identifier;
use crate::frontend::ir::{
    CompilationUnit, GenericFunctionTemplate, HIRBinding, HIRDeclaration, HIREnumVariant, HIRExpression, HIRExpressionKind, HIRFunction, HIRIf, HIRModule, HIRPattern, HIRReturn, HIRStatement, HIRSwitchArm, HIRSymbol, HIRTypeDeclaration, HIRTypeKind, Scope,
};
use crate::frontend::tokens::Operator;
use crate::frontend::tokens::builtin::{BuiltinFunction, BuiltinType};
use crate::frontend::tokens::literal::Literal;

#[derive(Debug)]
pub struct AnalysisError {
    pub msg: String,
    /// The source line the error originates from, if known. Filled in as the
    /// error propagates up through the statement that triggered it.
    pub line: Option<usize>,
}

impl std::error::Error for AnalysisError {}

impl fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "AnalysisError at line {}: {}", line, self.msg),
            None => write!(f, "AnalysisError: {}", self.msg),
        }
    }
}

impl From<String> for AnalysisError {
    fn from(value: String) -> Self {
        Self {
            msg: value,
            line: None,
        }
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
                import.path.iter().map(|id| id.value.clone()).collect();
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
                    .map(|a| a.value.clone())
                    .unwrap_or_else(|| import.path.last().unwrap().value.clone());
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
                import.path.iter().map(|id| id.value.clone()).collect();
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

    // Collect generic instantiations produced during analysis. Sort by
    // mangled name so the emitted declaration order is deterministic.
    let mut generic_fns: Vec<_> = generic_cache.into_iter().collect();
    generic_fns.sort_by(|a, b| a.0.cmp(&b.0));
    for (_, func) in generic_fns {
        hir_declarations.push(HIRDeclaration::HIRFunction(func));
    }

    let compilation_unit = CompilationUnit {
        scope_root: current_scope,
        declarations: hir_declarations,
        imported_declarations,
    };
    Ok::<CompilationUnit, AnalysisError>(compilation_unit)
}

/// Map an AST function signature to HIR parameter and return types.
/// Comptime (`type`-typed) parameters are excluded from the parameter list.
fn signature_to_hir(
    function_declaration: &FunctionDeclaration,
) -> Result<(Vec<(Identifier, HIRTypeKind)>, HIRTypeKind), AnalysisError> {
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
    Ok((params, return_type))
}

fn func_to_hir(
    function_declaration: FunctionDeclaration,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRFunction, AnalysisError> {
    // Only include non-comptime parameters in the HIR function signature.
    let (params, return_type) = signature_to_hir(&function_declaration)?;

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
    stmt: Statement,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRStatement, AnalysisError> {
    let line = stmt.line;
    stmt_to_hir_inner(stmt.kind, current_scope, generic_cache).map_err(|mut e| {
        e.line.get_or_insert(line);
        e
    })
}

fn stmt_to_hir_inner(
    stmt: StatementKind,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRStatement, AnalysisError> {
    match stmt {
        StatementKind::VariableDeclaration(variable_declaration) => Ok(HIRStatement::Binding(
            var_decl_to_hir(variable_declaration, current_scope, generic_cache)?,
        )),
        StatementKind::Assignment { identifier, expr } => {
            let target_ty = match current_scope.symbols.get(&identifier) {
                Some(HIRSymbol::Binding(binding)) => {
                    if !binding.mutable {
                        return Err(format!("cannot assign to constant '{}'", identifier).into());
                    }
                    binding.ty.clone()
                }
                Some(_) => {
                    return Err(format!("cannot assign to '{}': not a variable", identifier).into());
                }
                None => {
                    return Err(
                        format!("assignment to undeclared variable '{}'", identifier).into(),
                    );
                }
            };
            let e = expr_to_hir(expr, current_scope, generic_cache)?;
            let found = e.inferred_type.clone();
            let e = coerce_or_alias(e, &target_ty, current_scope).map_err(|_| {
                format!(
                    "cannot assign value of type {:?} to '{}' of type {:?}",
                    found, identifier, target_ty
                )
            })?;
            Ok(HIRStatement::Assign {
                name: identifier,
                expr: e,
            })
        }
        StatementKind::FieldAssign {
            object,
            field,
            expr,
        } => {
            // Lower the object expression — its inferred type tells us the struct
            // shape, which we use to compute the field index.
            // If the object is a plain identifier referring to an immutable
            // binding, surface a friendly error.
            if let PExpr::Identifier(id) = &object
                && let Some(HIRSymbol::Binding(b)) = current_scope.symbols.get(id)
                && !b.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", id).into());
            }
            let obj_hir = expr_to_hir(object, current_scope, generic_cache)?;
            let struct_fields = match &obj_hir.inferred_type {
                HIRTypeKind::Struct { fields } => fields.clone(),
                HIRTypeKind::Identifier(_) => {
                    resolve_struct_fields(&obj_hir.inferred_type, current_scope)?
                }
                other => {
                    return Err(format!(
                        "stmt_to_hir: FieldAssign target is not a struct: {:?}",
                        other
                    )
                    .into());
                }
            };
            let field_index = struct_fields
                .iter()
                .position(|(name, _)| name == &field.value)
                .ok_or_else(|| {
                    format!(
                        "stmt_to_hir: field {} not found in struct",
                        field.value
                    )
                })?;
            let e = expr_to_hir(expr, current_scope, generic_cache)?;
            Ok(HIRStatement::FieldAssign {
                object: obj_hir,
                field: field.value,
                field_index,
                expr: e,
            })
        }
        StatementKind::ExpressionStatement(e) => Ok(HIRStatement::Expr(expr_to_hir(
            e,
            current_scope,
            generic_cache,
        )?)),
        StatementKind::MultiAssignment { targets, values } => {
            let mut hir_targets = Vec::new();
            for target in targets {
                validate_assignment_target(&target, current_scope)?;
                hir_targets.push(expr_to_hir(target, current_scope, generic_cache)?);
            }
            let mut hir_values = Vec::new();
            for value in values {
                hir_values.push(expr_to_hir(value, current_scope, generic_cache)?);
            }
            validate_multi_assignment_shape(&hir_targets, &hir_values)?;
            Ok(HIRStatement::MultiAssign {
                targets: hir_targets,
                values: hir_values,
            })
        }
        StatementKind::MultiVariableDeclaration {
            identifiers,
            values,
        } => multi_var_decl_to_hir(identifiers, values, current_scope, generic_cache),
        StatementKind::Return(opt) => Ok(HIRStatement::Return(match opt {
            Some(exprs) => {
                let mut hir_exprs = Vec::new();
                for expr in exprs {
                    hir_exprs.push(expr_to_hir(expr, current_scope, generic_cache)?);
                }
                Some(HIRReturn { values: hir_exprs })
            }
            None => None,
        })),
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = expr_to_hir(condition, current_scope, generic_cache)?;
            // Each branch gets its own child scope so bindings declared
            // inside it don't leak into the enclosing block.
            let mut then_scope = current_scope.clone();
            let mut then_h = Vec::new();
            for s in then_branch {
                then_h.push(stmt_to_hir(s, &mut then_scope, generic_cache)?);
            }
            let else_h = match else_branch {
                Some(v) => {
                    let mut else_scope = current_scope.clone();
                    let mut ev = Vec::new();
                    for s in v {
                        ev.push(stmt_to_hir(s, &mut else_scope, generic_cache)?);
                    }
                    Some(ev)
                }
                None => None,
            };
            Ok(HIRStatement::If(HIRIf {
                cond,
                then_branch: then_h,
                else_branch: else_h,
            }))
        }
        StatementKind::For {
            initializer,
            condition,
            post_operation: increment,
            body,
        } => {
            // The loop header and body share one child scope (the init
            // binding is visible to cond/post/body) that doesn't leak out.
            let mut loop_scope = current_scope.clone();
            let init_h = match initializer {
                Some(b) => Some(Box::new(stmt_to_hir(*b, &mut loop_scope, generic_cache)?)),
                None => None,
            };
            let cond_h = match condition {
                Some(e) => Some(expr_to_hir(e, &loop_scope, generic_cache)?),
                None => None,
            };
            let post_h = match increment {
                Some(b) => Some(Box::new(stmt_to_hir(*b, &mut loop_scope, generic_cache)?)),
                None => None,
            };
            let mut body_h = Vec::new();
            for s in body {
                body_h.push(stmt_to_hir(s, &mut loop_scope, generic_cache)?);
            }
            Ok(HIRStatement::For {
                init: init_h,
                cond: cond_h,
                post: post_h,
                body: body_h,
            })
        }
        StatementKind::DerefAssign { pointer, expr } => {
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
            Ok(HIRStatement::DerefAssign {
                pointer: ptr_hir,
                expr: val_hir,
            })
        }
        StatementKind::IndexAssign {
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
            Ok(HIRStatement::IndexAssign {
                object: obj_hir,
                index: idx_hir,
                expr: val_hir,
            })
        }
        StatementKind::Break => Ok(HIRStatement::Break),
        StatementKind::Continue => Ok(HIRStatement::Continue),
        StatementKind::Defer(inner) => {
            let hir_inner = stmt_to_hir(*inner, current_scope, generic_cache)?;
            Ok(HIRStatement::Defer(Box::new(hir_inner)))
        }
        StatementKind::Switch { subject, arms } => {
            let subj_hir = expr_to_hir(subject, current_scope, generic_cache)?;
            let resolved = resolve_type_alias(subj_hir.inferred_type.clone(), current_scope);
            let variants = match &resolved {
                HIRTypeKind::Enum { variants } => variants.clone(),
                other => {
                    return Err(
                        format!("stmt_to_hir: switch subject is not an enum: {:?}", other).into(),
                    );
                }
            };
            let mut hir_arms = Vec::new();
            for arm in arms {
                let (hir_pattern, arm_binding) = match arm.pattern {
                    Pattern::Wildcard => (HIRPattern::Wildcard, None),
                    Pattern::EnumVariant { variant, binding } => {
                        let v = variants
                            .iter()
                            .find(|v| v.name == variant.value)
                            .ok_or_else(|| {
                                format!(
                                    "stmt_to_hir: enum has no variant '{}' in switch arm",
                                    variant.value
                                )
                            })?;
                        let payload_ty = v.payload.as_ref().map(|fields| HIRTypeKind::Struct {
                            fields: fields
                                .iter()
                                .map(|(n, t)| (n.clone(), Box::new(t.clone())))
                                .collect(),
                        });
                        if binding.is_some() && payload_ty.is_none() {
                            return Err(format!(
                                "switch arm: variant '{}' has no payload but a binding was supplied",
                                variant
                            )
                            .into());
                        }
                        let bind_clone = binding.clone();
                        (
                            HIRPattern::EnumVariant {
                                variant: v.name.clone(),
                                discriminant: v.discriminant,
                                binding,
                                payload_ty: payload_ty.clone(),
                            },
                            bind_clone.zip(payload_ty),
                        )
                    }
                };
                // Each arm body gets its own child scope; the pattern binding
                // (if any) only exists inside it.
                let mut arm_scope = current_scope.clone();
                if let Some((bind_id, payload_ty)) = &arm_binding {
                    arm_scope.symbols.insert(
                        bind_id.clone(),
                        HIRSymbol::Binding(HIRBinding {
                            name: bind_id.clone(),
                            ty: payload_ty.clone(),
                            init: None,
                            mutable: false,
                        }),
                    );
                }
                let mut body_h = Vec::new();
                for s in arm.body {
                    body_h.push(stmt_to_hir(s, &mut arm_scope, generic_cache)?);
                }
                hir_arms.push(HIRSwitchArm {
                    pattern: hir_pattern,
                    body: body_h,
                });
            }
            Ok(HIRStatement::Switch {
                subject: subj_hir,
                arms: hir_arms,
            })
        }
    }
}

fn validate_assignment_target(target: &PExpr, current_scope: &Scope) -> Result<(), AnalysisError> {
    match target {
        PExpr::Identifier(id) => {
            if let Some(HIRSymbol::Binding(binding)) = current_scope.symbols.get(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to constant '{}'", id).into());
            }
            Ok(())
        }
        PExpr::FieldAccess { object, .. } => {
            if let PExpr::Identifier(id) = object.as_ref()
                && let Some(HIRSymbol::Binding(binding)) = current_scope.symbols.get(id)
                && !binding.mutable
            {
                return Err(format!("cannot assign to field of constant '{}'", id).into());
            }
            Ok(())
        }
        PExpr::Dereference(_) | PExpr::IndexAccess { .. } => Ok(()),
        _ => Err("invalid multi-assignment target".to_string().into()),
    }
}

fn validate_multi_assignment_shape(
    targets: &[HIRExpression],
    values: &[HIRExpression],
) -> Result<(), AnalysisError> {
    if values.len() == targets.len() {
        return Ok(());
    }
    if values.len() == 1
        && let HIRTypeKind::Tuple { elements } = &values[0].inferred_type
        && elements.len() == targets.len()
    {
        return Ok(());
    }
    Err(format!(
        "multi-assignment arity mismatch: {} target(s), {} value expression(s)",
        targets.len(),
        values.len()
    )
    .into())
}

fn infer_multi_binding_types(
    identifiers: &[Identifier],
    values: &[HIRExpression],
) -> Result<Vec<HIRTypeKind>, AnalysisError> {
    if values.len() == identifiers.len() {
        return Ok(values
            .iter()
            .map(|value| value.inferred_type.clone())
            .collect());
    }
    if values.len() == 1
        && let HIRTypeKind::Tuple { elements } = &values[0].inferred_type
        && elements.len() == identifiers.len()
    {
        return Ok(elements.clone());
    }
    Err(format!(
        "multi-variable declaration arity mismatch: {} identifier(s), {} value expression(s)",
        identifiers.len(),
        values.len()
    )
    .into())
}

fn multi_var_decl_to_hir(
    identifiers: Vec<Identifier>,
    values: Vec<PExpr>,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRStatement, AnalysisError> {
    let mut hir_values = Vec::new();
    for value in values {
        hir_values.push(expr_to_hir(value, current_scope, generic_cache)?);
    }

    let binding_types = infer_multi_binding_types(&identifiers, &hir_values)?;
    let mut bindings = Vec::new();
    for (identifier, ty) in identifiers.into_iter().zip(binding_types) {
        let binding = HIRBinding {
            name: identifier.clone(),
            ty,
            init: None,
            mutable: true,
        };
        current_scope
            .symbols
            .insert(identifier, HIRSymbol::Binding(binding.clone()));
        bindings.push(binding);
    }

    Ok(HIRStatement::MultiBinding {
        bindings,
        values: hir_values,
    })
}

fn var_decl_to_hir(
    var_decl: VariableDeclaration,
    current_scope: &mut Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<HIRBinding, AnalysisError> {
    let declared_ty = match var_decl.constant_type {
        Some(TypeExpression::TypeKeyword) => {
            return Err("mutable type bindings (`var type`) are not yet supported; use `const type` for compile-time type aliases".to_string().into());
        }
        Some(t) => Some(map_type(t)?),
        None => None,
    };
    let init = match var_decl.expression {
        Some(expr) => Some(expr_to_hir(expr, current_scope, generic_cache)?),
        // A typed declaration without an initializer is legal: the binding
        // stays uninitialized until first assignment.
        None if declared_ty.is_some() => None,
        None => {
            return Err("inferred variable declaration requires an initializer"
                .to_string()
                .into());
        }
    };
    let ty = match (declared_ty, &init) {
        (Some(ty), _) => ty,
        (None, Some(init)) => init.inferred_type.clone(),
        (None, None) => unreachable!("rejected above"),
    };
    // check that type matches; allow struct-by-name to match struct literal type
    // `never` unifies with any type
    let init = match init {
        None => None,
        Some(mut init) => {
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
                } else if let HIRTypeKind::Builtin(_) = &ty {
                    // Numeric mismatches get a real conversion (literals are just
                    // re-typed; other expressions get a cast); anything else errors.
                    let found = init.inferred_type.clone();
                    init = coerce_expr_to_type(init, &ty).map_err(|_| {
                        format!(
                            "initialization type {:?} does not match declared type {:?} for {}",
                            found, ty, var_decl.identifier
                        )
                    })?;
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
            Some(init)
        }
    };
    let hir_var = HIRBinding {
        name: var_decl.identifier.clone(),
        ty,
        init,
        mutable: true,
    };
    current_scope
        .symbols
        .insert(var_decl.identifier, HIRSymbol::Binding(hir_var.clone()));
    Ok(hir_var)
}

fn is_integer_builtin(builtin: &BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Int1
            | BuiltinType::Int2
            | BuiltinType::Int4
            | BuiltinType::Int8
            | BuiltinType::Int16
            | BuiltinType::UInt1
            | BuiltinType::UInt2
            | BuiltinType::UInt4
            | BuiltinType::UInt8
            | BuiltinType::UInt16
            | BuiltinType::Char
    )
}

fn is_float_builtin(builtin: &BuiltinType) -> bool {
    matches!(
        builtin,
        BuiltinType::Float2 | BuiltinType::Float4 | BuiltinType::Float8 | BuiltinType::Float16
    )
}

fn is_numeric_type(ty: &HIRTypeKind) -> bool {
    matches!(
        ty,
        HIRTypeKind::Builtin(builtin) if is_integer_builtin(builtin) || is_float_builtin(builtin)
    )
}

fn is_boolean_type(ty: &HIRTypeKind) -> bool {
    matches!(ty, HIRTypeKind::Builtin(BuiltinType::Boolean))
}

fn coerce_expr_to_type(
    mut expr: HIRExpression,
    target: &HIRTypeKind,
) -> Result<HIRExpression, AnalysisError> {
    if &expr.inferred_type == target {
        return Ok(expr);
    }

    if expr.inferred_type == HIRTypeKind::Builtin(BuiltinType::Never) {
        expr.inferred_type = target.clone();
        return Ok(expr);
    }

    match (&expr.expression, &expr.inferred_type, target) {
        (
            HIRExpressionKind::LiteralInt { .. },
            HIRTypeKind::Builtin(src),
            HIRTypeKind::Builtin(dst),
        ) if is_integer_builtin(src) && is_integer_builtin(dst) => {
            expr.inferred_type = target.clone();
            Ok(expr)
        }
        (
            HIRExpressionKind::LiteralFloat { .. },
            HIRTypeKind::Builtin(src),
            HIRTypeKind::Builtin(dst),
        ) if is_float_builtin(src) && is_float_builtin(dst) => {
            expr.inferred_type = target.clone();
            Ok(expr)
        }
        (_, src, dst) if is_numeric_type(src) && is_numeric_type(dst) => Ok(HIRExpression {
            inferred_type: target.clone(),
            expression: HIRExpressionKind::Cast {
                expr: Box::new(expr),
                target_type: target.clone(),
            },
        }),
        _ => Err(format!(
            "cannot coerce expression of type {:?} to {:?}",
            expr.inferred_type, target
        )
        .into()),
    }
}

/// Like `coerce_expr_to_type`, but also accepts the case where the target is
/// a named type alias of the expression's structural type (or vice versa),
/// in which case the expression is just re-annotated with the target type.
fn coerce_or_alias(
    expr: HIRExpression,
    target: &HIRTypeKind,
    scope: &Scope,
) -> Result<HIRExpression, AnalysisError> {
    if &expr.inferred_type == target {
        return Ok(expr);
    }
    if resolve_type_alias(target.clone(), scope)
        == resolve_type_alias(expr.inferred_type.clone(), scope)
    {
        let mut e = expr;
        e.inferred_type = target.clone();
        return Ok(e);
    }
    coerce_expr_to_type(expr, target)
}

fn check_call_args(
    func_name: &str,
    params: &[(Identifier, HIRTypeKind)],
    is_variadic: bool,
    args: Vec<PExpr>,
    current_scope: &Scope,
    generic_cache: &mut HashMap<String, HIRFunction>,
) -> Result<Vec<HIRExpression>, AnalysisError> {
    if is_variadic {
        if args.len() < params.len() {
            return Err(format!(
                "{} expects at least {} argument(s), but {} were given",
                func_name,
                params.len(),
                args.len()
            )
            .into());
        }
    } else if args.len() != params.len() {
        return Err(format!(
            "{} expects {} argument(s), but {} were given",
            func_name,
            params.len(),
            args.len()
        )
        .into());
    }

    let mut hargs = Vec::with_capacity(args.len());
    for (i, arg) in args.into_iter().enumerate() {
        let harg = expr_to_hir(arg, current_scope, generic_cache)?;
        let harg = match params.get(i) {
            Some((param_name, param_type)) => {
                let found_type = harg.inferred_type.clone();
                coerce_expr_to_type(harg, param_type).map_err(|_| {
                    format!(
                        "{}: argument '{}' expects type {:?}, but found {:?}",
                        func_name, param_name.value, param_type, found_type
                    )
                })?
            }
            // Extra arguments to a variadic function (e.g. printf) aren't type-checked.
            None => harg,
        };
        hargs.push(harg);
    }
    Ok(hargs)
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
        // `null` unifies with any type (like `never`); it is concretized by
        // the context it is used in (declaration type, comparison operand, ...).
        PExpr::Literal(Literal::Null) => Ok(HIRExpression {
            inferred_type: HIRTypeKind::Builtin(BuiltinType::Never),
            expression: HIRExpressionKind::Null,
        }),
        PExpr::BuiltinCall { builtin, args } => {
            // Every string builtin takes `@string` arguments; only the arity and
            // return type differ.
            let (arity, ret) = match builtin {
                BuiltinFunction::StrLen => (1usize, HIRTypeKind::Builtin(BuiltinType::UInt8)),
                BuiltinFunction::Concat => (2, HIRTypeKind::Builtin(BuiltinType::String)),
                BuiltinFunction::StrEq => (2, HIRTypeKind::Builtin(BuiltinType::Boolean)),
            };
            if args.len() != arity {
                return Err(format!(
                    "{} expects {} argument(s), but {} were given",
                    builtin,
                    arity,
                    args.len()
                )
                .into());
            }
            let string_ty = HIRTypeKind::Builtin(BuiltinType::String);
            let mut hargs = Vec::with_capacity(args.len());
            for (i, arg) in args.into_iter().enumerate() {
                let harg = expr_to_hir(arg, current_scope, generic_cache)?;
                let found = harg.inferred_type.clone();
                let harg = coerce_expr_to_type(harg, &string_ty).map_err(|_| -> AnalysisError {
                    format!(
                        "{}: argument {} expects type @string, but found {:?}",
                        builtin,
                        i + 1,
                        found
                    )
                    .into()
                })?;
                hargs.push(harg);
            }
            Ok(HIRExpression {
                inferred_type: ret,
                expression: HIRExpressionKind::BuiltinCall {
                    builtin,
                    args: hargs,
                },
            })
        }
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
        PExpr::EnumVariantConstruct {
            type_name,
            variant,
            fields,
        } => {
            let sym = current_scope.symbols.get(&type_name).ok_or_else(|| {
                format!(
                    "expr_to_hir: type '{}' not found for enum variant construction",
                    type_name
                )
            })?;
            let HIRSymbol::Type(enum_ty) = sym else {
                return Err(format!("expr_to_hir: '{}' is not a type", type_name).into());
            };
            let resolved = resolve_type_alias(enum_ty.clone(), current_scope);
            let HIRTypeKind::Enum { variants } = &resolved else {
                return Err(format!("expr_to_hir: '{}' is not an enum type", type_name).into());
            };
            let v = variants
                .iter()
                .find(|v| v.name == variant.value)
                .ok_or_else(|| {
                    format!(
                        "expr_to_hir: enum '{}' has no variant '{}'",
                        type_name, variant
                    )
                })?
                .clone();
            let Some(payload_spec) = v.payload.clone() else {
                return Err(format!(
                    "expr_to_hir: variant '{}.{}' carries no payload",
                    type_name, variant
                )
                .into());
            };
            let mut hir_fields: Vec<(String, HIRExpression)> = Vec::new();
            for (fname, fval) in fields {
                let Some((_, spec_ty)) = payload_spec.iter().find(|(n, _)| n == &fname.value)
                else {
                    return Err(format!(
                        "expr_to_hir: variant '{}.{}' has no payload field '{}'",
                        type_name, variant, fname.value
                    )
                    .into());
                };
                let fval_hir = expr_to_hir(fval, current_scope, generic_cache)?;
                let fval_hir = coerce_or_alias(fval_hir, spec_ty, current_scope).map_err(|_| {
                    format!(
                        "expr_to_hir: field '{}' of variant '{}.{}' expects type {:?}",
                        fname.value, type_name, variant, spec_ty
                    )
                })?;
                hir_fields.push((fname.value.clone(), fval_hir));
            }
            // Verify each declared field is supplied (order-insensitive).
            for (n, _) in &payload_spec {
                if !hir_fields.iter().any(|(fname, _)| fname == n) {
                    return Err(format!(
                        "expr_to_hir: missing field '{}' in variant '{}.{}' payload",
                        n, type_name, variant
                    )
                    .into());
                }
            }
            Ok(HIRExpression {
                inferred_type: HIRTypeKind::Identifier(type_name.clone()),
                expression: HIRExpressionKind::EnumVariantConstruct {
                    type_name: type_name.value.clone(),
                    variant: v.name.clone(),
                    discriminant: v.discriminant,
                    fields: hir_fields,
                    enum_type: Box::new(resolved),
                },
            })
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
            let r = expr_to_hir(*right, current_scope, generic_cache)?;
            let (inferred_type, l, r) = match operator {
                // Arithmetic/bitwise: result type = LHS type.
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
                    // For pointer arithmetic, keep pointer type; don't coerce RHS.
                    if matches!(l.inferred_type, HIRTypeKind::Pointer(_)) {
                        (l.inferred_type.clone(), l, r)
                    } else if is_numeric_type(&l.inferred_type) {
                        let r = coerce_expr_to_type(r, &l.inferred_type)?;
                        (l.inferred_type.clone(), l, r)
                    } else {
                        return Err(format!(
                            "operator {:?} requires numeric operands, got {:?}",
                            operator, l.inferred_type
                        )
                        .into());
                    }
                }
                Operator::GreaterThan
                | Operator::GreaterEqual
                | Operator::LesserThan
                | Operator::LesserEqual => {
                    if !is_numeric_type(&l.inferred_type) {
                        return Err(format!(
                            "operator {:?} requires numeric operands, got {:?}",
                            operator, l.inferred_type
                        )
                        .into());
                    }
                    let r = coerce_expr_to_type(r, &l.inferred_type)?;
                    (HIRTypeKind::Builtin(BuiltinType::Boolean), l, r)
                }
                Operator::DoubleEquals | Operator::Different => {
                    // Coerce whichever side is more flexible: `null == p`
                    // needs the LHS adapted to the RHS pointer type.
                    if l.inferred_type == HIRTypeKind::Builtin(BuiltinType::Never)
                        && r.inferred_type != HIRTypeKind::Builtin(BuiltinType::Never)
                    {
                        let target = r.inferred_type.clone();
                        let l = coerce_expr_to_type(l, &target)?;
                        (HIRTypeKind::Builtin(BuiltinType::Boolean), l, r)
                    } else {
                        let r = coerce_expr_to_type(r, &l.inferred_type)?;
                        (HIRTypeKind::Builtin(BuiltinType::Boolean), l, r)
                    }
                }
                Operator::LogicalAnd | Operator::LogicalOr => {
                    if !is_boolean_type(&l.inferred_type) || !is_boolean_type(&r.inferred_type) {
                        return Err(format!(
                            "operator {:?} requires bool operands, got {:?} and {:?}",
                            operator, l.inferred_type, r.inferred_type
                        )
                        .into());
                    }
                    (HIRTypeKind::Builtin(BuiltinType::Boolean), l, r)
                }
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
                                    callee: Identifier { value: mangled_name },
                                    args: hargs,
                                },
                            })
                        }
                        Some(HIRSymbol::Function(func)) => {
                            let inferred_type = func.return_type.clone();
                            let hargs = check_call_args(
                                &name.value,
                                &func.params,
                                func.is_variadic,
                                args,
                                current_scope,
                                generic_cache,
                            )?;
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
                    let module_alias = module.value.clone();
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
                            value: format!("{}__{}", module_alias, member.value),
                        }
                    };
                    let hargs = check_call_args(
                        &format!("{}::{}", module_alias, member.value),
                        &func.params,
                        func.is_variadic,
                        args,
                        current_scope,
                        generic_cache,
                    )?;
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
            // Special case: `TypeName.VariantName` on an enum produces an EnumLiteral.
            if let PExpr::Identifier(type_name) = &*object
                && let Some(HIRSymbol::Type(ty)) = current_scope.symbols.get(type_name)
            {
                let resolved = resolve_type_alias(ty.clone(), current_scope);
                if let HIRTypeKind::Enum { variants } = &resolved {
                    let v = variants
                        .iter()
                        .find(|v| v.name == field.value)
                        .ok_or_else(|| {
                            format!(
                                "expr_to_hir: enum {} has no variant {}",
                                type_name, field.value
                            )
                        })?;
                    return Ok(HIRExpression {
                        inferred_type: HIRTypeKind::Identifier(type_name.clone()),
                        expression: HIRExpressionKind::EnumLiteral {
                            type_name: type_name.value.clone(),
                            variant: v.name.clone(),
                            discriminant: v.discriminant,
                        },
                    });
                }
            }
            let obj_hir = expr_to_hir(*object, current_scope, generic_cache)?;
            let struct_fields = resolve_struct_fields(&obj_hir.inferred_type, current_scope)?;
            let field_index = struct_fields
                .iter()
                .position(|(name, _)| name == &field.value)
                .ok_or_else(|| {
                    format!(
                        "expr_to_hir: field {} not found in struct type {:?}",
                        field.value, obj_hir.inferred_type
                    )
                })?;
            let field_ty = *struct_fields[field_index].1.clone();
            Ok(HIRExpression {
                inferred_type: field_ty,
                expression: HIRExpressionKind::FieldAccess {
                    object: Box::new(obj_hir),
                    field: field.value,
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
            // Lower the provided fields, verifying each exists in the struct.
            let mut provided: Vec<(String, HIRExpression)> = Vec::new();
            for (fname, fexpr) in fields {
                if !struct_fields
                    .iter()
                    .any(|(name, _)| name == &fname.value)
                {
                    return Err(format!(
                        "expr_to_hir: field {} not found in struct {}",
                        fname.value, type_name
                    )
                    .into());
                }
                if provided.iter().any(|(n, _)| n == &fname.value) {
                    return Err(format!(
                        "expr_to_hir: duplicate field {} in construction of {}",
                        fname.value, type_name
                    )
                    .into());
                }
                let fval = expr_to_hir(fexpr, current_scope, generic_cache)?;
                provided.push((fname.value, fval));
            }
            // Re-emit the fields in *declared* order (lowering stores them
            // positionally) and require every field to be present.
            let mut hir_fields = Vec::new();
            for (fname, fty) in &struct_fields {
                let pos = provided
                    .iter()
                    .position(|(n, _)| n == fname)
                    .ok_or_else(|| {
                        format!(
                            "expr_to_hir: missing field {} in construction of struct {}",
                            fname, type_name
                        )
                    })?;
                let (n, fval) = provided.remove(pos);
                let fval = coerce_or_alias(fval, fty.as_ref(), current_scope).map_err(|_| {
                    format!(
                        "expr_to_hir: field {} of struct {} expects type {:?}",
                        fname, type_name, fty
                    )
                })?;
                hir_fields.push((n, fval));
            }
            Ok(HIRExpression {
                inferred_type: struct_ty,
                expression: HIRExpressionKind::StructConstruct {
                    type_name: type_name.value,
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
            let inner_hir = expr_to_hir(*expr, current_scope, generic_cache)?;
            let hir_target = map_type(target_type)?;
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
                    &inner.inferred_type,
                    HIRTypeKind::Builtin(b) if is_float_builtin(b)
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
            let module_alias = module.value.clone();
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
/// Walk through `Identifier` aliases until reaching a concrete type.
fn resolve_type_alias(ty: HIRTypeKind, scope: &Scope) -> HIRTypeKind {
    let mut current = ty;
    loop {
        match &current {
            HIRTypeKind::Identifier(id) => match scope.symbols.get(id) {
                Some(HIRSymbol::Type(inner)) => current = inner.clone(),
                _ => return current,
            },
            _ => return current,
        }
    }
}

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
                HIRSymbol::Type(inner_ty) => resolve_struct_fields(&inner_ty, current_scope),
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
                hir_fields.push((f.label.value.clone(), Box::new(map_type(f.type_id)?)));
            }
            HIRTypeKind::Struct { fields: hir_fields }
        }
        TypeExpression::Enum { variants } => {
            let mut hir_variants = Vec::new();
            for (idx, v) in variants.into_iter().enumerate() {
                let payload = match v.payload {
                    Some(fields) => {
                        let mut p = Vec::new();
                        for f in fields {
                            p.push((f.label.value.clone(), map_type(f.type_id)?));
                        }
                        Some(p)
                    }
                    None => None,
                };
                hir_variants.push(HIREnumVariant {
                    name: v.name.value.clone(),
                    discriminant: idx as u32,
                    payload,
                });
            }
            HIRTypeKind::Enum {
                variants: hir_variants,
            }
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
        TypeExpression::Tuple { elements } => {
            let mut mapped_elements = Vec::new();
            for element in elements {
                mapped_elements.push(map_type(element)?);
            }
            HIRTypeKind::Tuple {
                elements: mapped_elements,
            }
        }
        TypeExpression::Pointer {
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
            module: module.value.clone(),
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
        TypeExpression::Identifier(id) => id.value.clone(),
        TypeExpression::Pointer { pointed_type, .. } => {
            format!("ptr_{}", mangle_type_expr(pointed_type))
        }
        TypeExpression::Array { element_type, size } => {
            format!("arr{}_{}", size, mangle_type_expr(element_type))
        }
        TypeExpression::Struct { .. } => "struct".to_string(),
        TypeExpression::Enum { .. } => "enum".to_string(),
        TypeExpression::Function { .. } => "fn".to_string(),
        TypeExpression::Tuple { elements } => format!(
            "tuple_{}",
            elements
                .iter()
                .map(mangle_type_expr)
                .collect::<Vec<_>>()
                .join("_")
        ),
        TypeExpression::QualifiedIdentifier { module, name } => {
            format!("{}__{}", module.value, name.value)
        }
        TypeExpression::TypeKeyword => "type".to_string(),
    }
}

/// Substitute all occurrences of type identifiers in `subs` within a TypeExpression.
fn substitute_type(te: &TypeExpression, subs: &HashMap<String, TypeExpression>) -> TypeExpression {
    match te {
        TypeExpression::Identifier(id) => {
            if let Some(replacement) = subs.get(&id.value) {
                replacement.clone()
            } else {
                te.clone()
            }
        }
        TypeExpression::Pointer {
            pointed_type,
        } => TypeExpression::Pointer {
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
        TypeExpression::Tuple { elements } => TypeExpression::Tuple {
            elements: elements.iter().map(|t| substitute_type(t, subs)).collect(),
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

fn substitute_in_stmt(stmt: &mut StatementKind, subs: &HashMap<String, TypeExpression>) {
    match stmt {
        StatementKind::VariableDeclaration(decl) => {
            if let Some(t) = &decl.constant_type {
                decl.constant_type = Some(substitute_type(t, subs));
            }
            if let Some(e) = &mut decl.expression {
                substitute_in_expr(e, subs);
            }
        }
        StatementKind::Return(Some(exprs)) => {
            for expr in exprs {
                substitute_in_expr(expr, subs);
            }
        }
        StatementKind::ExpressionStatement(expr) => substitute_in_expr(expr, subs),
        StatementKind::Assignment { expr, .. } => substitute_in_expr(expr, subs),
        StatementKind::MultiAssignment { targets, values } => {
            for target in targets {
                substitute_in_expr(target, subs);
            }
            for value in values {
                substitute_in_expr(value, subs);
            }
        }
        StatementKind::MultiVariableDeclaration { values, .. } => {
            for value in values {
                substitute_in_expr(value, subs);
            }
        }
        StatementKind::FieldAssign { expr, .. } => substitute_in_expr(expr, subs),
        StatementKind::DerefAssign { pointer, expr } => {
            substitute_in_expr(pointer, subs);
            substitute_in_expr(expr, subs);
        }
        StatementKind::IndexAssign {
            object,
            index,
            expr,
        } => {
            substitute_in_expr(object, subs);
            substitute_in_expr(index, subs);
            substitute_in_expr(expr, subs);
        }
        StatementKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_in_expr(condition, subs);
            for s in then_branch {
                substitute_in_stmt(&mut s.kind, subs);
            }
            if let Some(eb) = else_branch {
                for s in eb {
                    substitute_in_stmt(&mut s.kind, subs);
                }
            }
        }
        StatementKind::For {
            initializer,
            condition,
            post_operation,
            body,
        } => {
            if let Some(init) = initializer {
                substitute_in_stmt(&mut init.kind, subs);
            }
            if let Some(cond) = condition {
                substitute_in_expr(cond, subs);
            }
            if let Some(post) = post_operation {
                substitute_in_stmt(&mut post.kind, subs);
            }
            for s in body {
                substitute_in_stmt(&mut s.kind, subs);
            }
        }
        StatementKind::Defer(inner) => substitute_in_stmt(&mut inner.kind, subs),
        StatementKind::Switch { subject, arms } => {
            substitute_in_expr(subject, subs);
            for arm in arms {
                for s in &mut arm.body {
                    substitute_in_stmt(&mut s.kind, subs);
                }
            }
        }
        StatementKind::Break | StatementKind::Continue | StatementKind::Return(None) => {}
    }
}

/// Monomorphize a generic function with the given type arguments.
/// Returns the mangled name and the instantiated return type.
/// The instantiated HIRFunction is stored in `generic_cache`.
fn instantiate_generic(
    template: &GenericFunctionTemplate,
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
            .value
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
        template.name.value,
        type_arg_names.join("__")
    );

    // Check cache — deduplication
    if let Some(cached) = generic_cache.get(&mangled) {
        return Ok((mangled, cached.return_type.clone()));
    }

    // Clone and substitute the AST function declaration
    let mut substituted = template.ast_decl.clone();
    substituted.signature.name = Identifier {
        value: mangled.clone(),
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
            substitute_in_stmt(&mut stmt.kind, &subs);
        }
    }

    // Pre-insert a signature-only placeholder so a recursive call inside the
    // body hits the cache instead of re-instantiating forever. It is replaced
    // with the fully analyzed function below.
    let (placeholder_params, placeholder_ret) = signature_to_hir(&substituted)?;
    generic_cache.insert(
        mangled.clone(),
        HIRFunction {
            name: substituted.signature.name.clone(),
            params: placeholder_params,
            return_type: placeholder_ret,
            body: Vec::new(),
            is_extern: substituted.is_extern,
            is_variadic: substituted.is_variadic,
        },
    );

    // Analyze the substituted function
    let mut func_scope = scope.clone();
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
            HIRSymbol::GenericFunction(GenericFunctionTemplate {
                name: function_declaration.signature.name.clone(),
                ast_decl: function_declaration.clone(),
                comptime_params,
            }),
        );
        return Ok(current_scope);
    }

    // Register the signature only. The body is analyzed exactly once, in
    // `analyze()`, *after* this symbol is in scope — which is what makes
    // recursive calls resolve.
    let (params, return_type) = signature_to_hir(function_declaration)?;
    current_scope.symbols.insert(
        function_declaration.signature.name.clone(),
        HIRSymbol::Function(HIRFunction {
            name: function_declaration.signature.name.clone(),
            params,
            return_type,
            body: Vec::new(),
            is_extern: function_declaration.is_extern,
            is_variadic: function_declaration.is_variadic,
        }),
    );
    Ok(current_scope)
}

mod test;
