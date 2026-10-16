use crate::frontend::{
    ast::function_declaration::FunctionDeclaration, scope_kind::ScopeKind,
    symbol_table::SymbolTable,
};

impl FunctionDeclaration {
    pub fn resolve_name(&mut self, symbol_table: &mut SymbolTable) -> anyhow::Result<()> {
        self.signature.resolve_name(symbol_table)?;
        if let Some(ref mut body) = self.body {
            symbol_table.enter_scope(ScopeKind::Function);
            for stmt in &mut body.statements {
                stmt.resolve_name(symbol_table)?;
            }
            symbol_table.exit_scope();
        }
        Ok(())
    }
}

// impl<'t, 'symtbl> Analyzable<'t, 'symtbl, TypedDecl<'t>> for FunctionDeclaration<'t> {
//     // TODO: analyze no tiene en cuenta el cuerpo, mirar funciones de mas abajo
//     fn analyze(
//         &self,
//         symbol_table: &'symtbl mut SymbolTable<'symtbl>,
//         _generics_cache: &mut HashMap<&'t str, TypedFunction<'t>>,
//     ) -> anyhow::Result<TypedDecl<'t>> {
//         // Generic functions (those with `type`-typed parameters) are stored as templates.
//         // They are instantiated lazily when called with concrete type arguments.
//         if self.is_generic() {
//             let comptime_params: Vec<usize> = self
//                 .signature
//                 .parameters
//                 .iter()
//                 .enumerate()
//                 .filter(|(_, p)| matches!(&p.parameter_type.kind, TypeExpressionKind::TypeKeyword))
//                 .map(|(i, _)| i)
//                 .collect();
//             symbol_table.insert(
//                 self.signature.identifier.id,
//                 TypedSymbol::GenericFunction(GenericFunctionTemplate {
//                     name: self.signature.identifier,
//                     func_decl: *self,
//                     comptime_params: &comptime_params,
//                 }),
//             );
//             return Ok(TypedDecl::Function(TypedFunction {
//                 symbol: todo!(),
//                 params: todo!(),
//                 return_type: todo!(),
//                 body: todo!(),
//                 is_extern: todo!(),
//                 is_variadic: todo!(),
//             }));
//         }
//
//         // Register the signature only. The body is analyzed exactly once, in
//         // `analyze()`, *after* this symbol is in scope — which is what makes
//         // recursive calls resolve.
//         let fn_line = fn_body_line(self);
//         let (params, return_type) = signature_to_typed(self)?;
//         symbol_table.insert(
//             self.signature.identifier.id.clone(),
//             TypedSymbol::Function(TypedFunction {
//                 symbol: self.signature.identifier.clone(),
//                 params: &params,
//                 return_type,
//                 body: &[],
//                 is_extern: self.is_extern,
//                 is_variadic: self.is_variadic,
//             }),
//         );
//         Ok(TypedDecl::Function(TypedFunction {
//             symbol: todo!(),
//             params: todo!(),
//             return_type: todo!(),
//             body: todo!(),
//             is_extern: todo!(),
//             is_variadic: todo!(),
//         }))
//     }
// }
//
// /// Fallback line for signature errors: the function's first body statement,
// /// if any. Signature types carry no spans in the AST, so a bad parameter or
// /// return type would otherwise report `line: None`. Body errors already carry
// /// precise lines via `stmt_to_typed`, which always win (`with_line` keeps
// /// the inner line).
// fn fn_body_line(function_declaration: &FunctionDeclaration) -> Option<usize> {
//     function_declaration
//         .body
//         .as_ref()
//         .and_then(|b| b.statements.first().map(|s| s.span.line))
// }
//
// /// Attach `line` when known, pass the error through otherwise.
// fn with_opt_line(err: AnalysisError, line: Option<usize>) -> AnalysisError {
//     match line {
//         Some(l) => err.with_line(l),
//         None => err,
//     }
// }
//
// /// Map an AST function signature to Typed parameter and return types.
// /// Comptime (`type`-typed) parameters are excluded from the parameter list.
// pub fn signature_to_typed<'t>(
//     function_declaration: &'t FunctionDeclaration,
// ) -> anyhow::Result<(Vec<(Symbol, Ty<'t>)>, Ty<'t>)> {
//     let mut params = Vec::new();
//     for param in function_declaration.signature.parameters {
//         let ty = map_type(param.parameter_type.clone())?;
//         if ty != Ty::Type {
//             params.push((param.parameter_name.clone(), ty));
//         }
//     }
//     let return_type = map_type(function_declaration.signature.return_type.clone())?;
//     Ok((params, return_type))
// }
//
// pub fn func_to_typed<'t>(
//     function_declaration: FunctionDeclaration,
//     symbol_table: &mut SymbolTable,
//     generics_cache: &mut HashMap<&str, TypedFunction>,
// ) -> anyhow::Result<TypedFunction<'t>> {
//     // Signature errors have no span of their own; fall back to the first
//     // body line so they point at the function instead of nowhere.
//     let fn_line = fn_body_line(&function_declaration);
//     // Only include non-comptime parameters in the Typed function signature.
//     let (params, return_type) = signature_to_typed(&function_declaration)?;
//
//     // Enter a function scope holding the runtime parameters. Exited before
//     // returning so params never leak into the enclosing (global) table.
//     symbol_table.enter_scope(ScopeKind::Function);
//     for param in function_declaration.signature.parameters {
//         let ty = map_type(param.parameter_type.clone()).map_err(|e| with_opt_line(e, fn_line))?;
//         if ty != Ty::Type {
//             symbol_table.insert(
//                 param.parameter_name.id.into(),
//                 TypedSymbol::Binding(TypedBinding {
//                     symbol: param.parameter_name.clone(),
//                     ty,
//                     init: None,
//                     mutable: true,
//                 }),
//             );
//         }
//     }
//
//     let mut body = Vec::new();
//     if let Some(fb) = function_declaration.body {
//         for stmt in fb.statements {
//             body.push(stmt.analyze(symbol_table, generics_cache)?);
//         }
//     }
//     validate_return_types(&body, &return_type, symbol_table).map_err(|e| {
//         // Body errors already carry precise lines via `stmt_to_typed`.
//         with_opt_line(e, fn_line)
//     })?;
//     symbol_table.exit_scope();
//     Ok(TypedFunction {
//         symbol: function_declaration.signature.identifier,
//         params: &params,
//         return_type,
//         body: &body,
//         is_extern: function_declaration.is_extern,
//         is_variadic: function_declaration.is_variadic,
//     })
// }
//
// fn check_one(
//     return_type: &Ty,
//     value: &TypedExpr,
//     symbol_table: &SymbolTable,
// ) -> Result<(), AnalysisError> {
//     let found = value.inferred_type;
//     check_assignable(
//         return_type,
//         &value,
//         symbol_table,
//         &format!("return value of type {:?}", return_type),
//     )
//     .map(|_| ())
//     .map_err(|_| {
//         format!(
//             "return type mismatch: function returns {:?} but this returns {:?}",
//             return_type, found
//         )
//         .into()
//     })
// }
//
// /// Check every `return` in `body` against the declared return type, using the
// /// same `check_assignable` rule as assignments so strictness is identical.
// /// Multi-value returns (`return a, b`) must match a tuple return type element
// /// by element; a single value must coerce to the return type. `return` with no
// /// value is only legal in `@void` functions.
// fn validate_return_types(
//     body: &[TypedStatement],
//     return_type: &Ty,
//     symbol_table: &SymbolTable,
// ) -> Result<(), AnalysisError> {
//     for stmt in body {
//         match stmt {
//             TypedStatement::Return(Some(ret)) => {
//                 if ret.values.is_empty() {
//                     if *return_type != Ty::Builtin(BuiltinType::Void) {
//                         return Err(format!(
//                             "return type mismatch: function returns {:?} but `return` has no value",
//                             return_type
//                         )
//                         .into());
//                     }
//                     continue;
//                 }
//                 match return_type {
//                     Ty::Tuple { elements } => {
//                         if ret.values.len() != elements.len() {
//                             return Err(format!(
//                                 "return type mismatch: function returns {} value(s) but this returns {}",
//                                 elements.len(),
//                                 ret.values.len()
//                             )
//                             .into());
//                         }
//                         for (value, elem_ty) in ret.values.iter().zip(elements.iter()) {
//                             check_one(elem_ty, value, symbol_table)?;
//                         }
//                     }
//                     single => {
//                         if ret.values.len() != 1 {
//                             return Err(format!(
//                                 "return type mismatch: function returns {:?} but this returns {} values",
//                                 single,
//                                 ret.values.len()
//                             )
//                             .into());
//                         }
//                         check_one(single, &ret.values[0], symbol_table)?;
//                     }
//                 }
//             }
//             TypedStatement::Return(None) => {
//                 if *return_type != Ty::Builtin(BuiltinType::Void) {
//                     return Err(format!(
//                         "return type mismatch: function returns {:?} but `return` has no value",
//                         return_type
//                     )
//                     .into());
//                 }
//             }
//             TypedStatement::If(TypedIf {
//                 then_branch,
//                 else_branch,
//                 ..
//             }) => {
//                 validate_return_types(then_branch, return_type, symbol_table)?;
//                 if let Some(else_branch) = else_branch {
//                     validate_return_types(else_branch, return_type, symbol_table)?;
//                 }
//             }
//             TypedStatement::While { body, .. } => {
//                 validate_return_types(body, return_type, symbol_table)?;
//             }
//             TypedStatement::Defer(inner) => {
//                 validate_return_types(std::slice::from_ref(inner), return_type, symbol_table)?;
//             }
//             TypedStatement::Switch { arms, .. } => {
//                 for TypedSwitchArm { body, .. } in *arms {
//                     validate_return_types(body, return_type, symbol_table)?;
//                 }
//             }
//             _ => {}
//         }
//     }
//     Ok(())
// }
