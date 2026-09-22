use crate::frontend::ast::{
    declaration::DeclarationNode,
    type_expression::{TypeExpression, TypeExpressionKind},
};
use crate::frontend::typed_ast::{SymbolTable, Ty, TypedEnumVariant, TypedSymbol};

use super::AnalysisError;
use super::functions::resolve_function_decl;

/// Resolve a Ty (possibly an Identifier alias) to its struct fields.
/// Walk through `Identifier` aliases until reaching a concrete type.
pub(super) fn resolve_type_alias(ty: Ty, scope: &SymbolTable) -> Ty {
    let mut current = ty;
    loop {
        match &current {
            Ty::Identifier(id) => match scope.lookup(id) {
                Some(TypedSymbol::Type(inner)) => current = inner.clone(),
                _ => return current,
            },
            _ => return current,
        }
    }
}

pub(super) fn resolve_struct_fields(
    ty: &Ty,
    current_scope: &SymbolTable,
) -> Result<Vec<(String, Box<Ty>)>, AnalysisError> {
    match ty {
        Ty::Struct { fields } => Ok(fields.clone()),
        Ty::Identifier(id) => {
            let symbol = current_scope
                .lookup(id)
                .ok_or_else(|| format!("resolve_struct_fields: type {} not found in scope", id))?;
            match symbol {
                TypedSymbol::Type(inner_ty) => resolve_struct_fields(inner_ty, current_scope),
                _ => Err(format!("resolve_struct_fields: {} is not a type", id).into()),
            }
        }
        _ => Err(format!("resolve_struct_fields: {:?} is not a struct type", ty).into()),
    }
}

pub(super) fn map_type(type_expression: TypeExpression) -> Result<Ty, AnalysisError> {
    let span = type_expression.span;
    map_type_inner(type_expression).map_err(|e| e.with_span_fallback(span))
}

fn map_type_inner(type_expression: TypeExpression) -> Result<Ty, AnalysisError> {
    let typed_typekind = match type_expression.kind {
        TypeExpressionKind::TypeKeyword => Ty::Type,
        TypeExpressionKind::Builtin(builtin) => Ty::Builtin(builtin),
        TypeExpressionKind::Identifier(identifier) => Ty::Identifier(identifier),
        TypeExpressionKind::Struct { fields } => {
            let mut typed_fields = Vec::new();
            for f in fields {
                typed_fields.push((f.label.value.clone(), Box::new(map_type(f.type_id)?)));
            }
            Ty::Struct {
                fields: typed_fields,
            }
        }
        TypeExpressionKind::Enum { variants } => {
            let mut typed_variants = Vec::new();
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
                typed_variants.push(TypedEnumVariant {
                    name: v.name.value.clone(),
                    discriminant: idx as u32,
                    payload,
                });
            }
            Ty::Enum {
                variants: typed_variants,
            }
        }
        TypeExpressionKind::Function {
            argument_types,
            return_type,
        } => {
            let mut mapped_arguments = Vec::new();
            for argument in argument_types {
                mapped_arguments.push(map_type(argument)?);
            }
            Ty::Function {
                argument_types: mapped_arguments,
                return_type: Box::new(map_type(*return_type)?),
            }
        }
        TypeExpressionKind::Tuple { elements } => {
            let mut mapped_elements = Vec::new();
            for element in elements {
                mapped_elements.push(map_type(element)?);
            }
            Ty::Tuple {
                elements: mapped_elements,
            }
        }
        TypeExpressionKind::Pointer { pointed_type } => {
            let inner = map_type(*pointed_type)?;
            Ty::Pointer(Box::new(inner))
        }
        TypeExpressionKind::Array { element_type, size } => Ty::Array {
            element_type: Box::new(map_type(*element_type)?),
            size,
        },
        TypeExpressionKind::Slice { element_type } => Ty::Slice(Box::new(map_type(*element_type)?)),
        TypeExpressionKind::QualifiedIdentifier { module, name } => Ty::QualifiedIdentifier {
            module: module.value.clone(),
            name,
        },
    };

    Ok(typed_typekind)
}

pub(super) fn resolve_declaration(
    declaration: &DeclarationNode,
    current_scope: &mut SymbolTable,
) -> Result<(), AnalysisError> {
    match declaration {
        DeclarationNode::ImportDeclaration(_) => Ok(()), // handled in analyze()
        DeclarationNode::FunctionDeclaration(function_declaration) => {
            resolve_function_decl(function_declaration, current_scope)
        }
        DeclarationNode::TypeDeclaration(type_declaration) => {
            let ty = map_type(type_declaration.expression.clone())?;
            current_scope.insert(type_declaration.name.clone(), TypedSymbol::Type(ty));
            Ok(())
        }
    }
}
