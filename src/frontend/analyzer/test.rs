//! Contract tests only. The former typed-AST behavior suite no longer matches
//! the analysis boundary; these tests do not assert unimplemented language rules.

use std::collections::HashMap;

use crate::{
    build_plan::{BuildPlan, FileId},
    frontend::{
        analysis_annotation::{
            BindingAnnotation, ExpressionAnnotation, ExpressionInfo, LabelAnnotation,
            MemberAnnotation, NameAnnotation, TypeAnnotation, ValueCategory,
        },
        ast::{Ast, expression::Expression, expression::ExpressionKind},
        identifier::{DeclId, Entity, LabelId, LocalId, ResolvedName},
        namespace::ProjectSymbols,
        scope_kind::ScopeKind,
        semantic_type::{SemanticTypes, TypeId},
        symbol_table::{LocalBinding, SymbolTable},
    },
};

use super::{
    AnalyzedProject, Analyzer, ResolveContext, TypeContext, collect_declarations,
    expression::resolve_member,
    generics::{GenericInstanceKey, resolve_generic_instance},
    resolve_names, resolve_types,
    statement::{resolve_multi_value_types, validate_assignment_target},
    types::check_assignable,
    validate_analysis,
};

fn symbols() -> ProjectSymbols {
    ProjectSymbols {
        files_by_name: HashMap::new(),
        namespaces: HashMap::new(),
    }
}

fn assert_unimplemented<T>(result: anyhow::Result<T>) {
    assert!(result.is_err());
    let Err(error) = result else { return };
    assert!(error.to_string().contains("not implemented"));
}

#[test]
fn default_and_new_keep_the_root_scope() {
    for mut table in [SymbolTable::new(), SymbolTable::default()] {
        assert_eq!(table.depth(), 1);
        assert_eq!(table.get_curr_scope_kind(), ScopeKind::Module);
        assert!(table.exit_scope().is_err());
        assert_eq!(table.depth(), 1);
        table
            .insert_binding("root".into(), LocalBinding::Local(LocalId(0)))
            .unwrap();
        assert_eq!(
            table.resolve_lexical("root"),
            Some(ResolvedName::Local(LocalId(0)))
        );
    }
}

#[test]
fn scope_exit_removes_bindings_and_duplicate_insertion_preserves_identity() {
    let mut table = SymbolTable::new();
    table.enter_scope(ScopeKind::Function);
    table
        .insert_binding("parameter".into(), LocalBinding::Local(LocalId(1)))
        .unwrap();
    let duplicate = table.insert_binding("parameter".into(), LocalBinding::Local(LocalId(2)));
    assert!(duplicate.is_err());
    assert!(duplicate.unwrap_err().to_string().contains("duplicate"));
    table.enter_scope(ScopeKind::Block);
    assert_eq!(table.depth(), 3);
    assert_eq!(table.get_curr_scope_kind(), ScopeKind::Block);
    assert_eq!(
        table.resolve_lexical("parameter"),
        Some(ResolvedName::Local(LocalId(1)))
    );
    table
        .insert_binding("temporary".into(), LocalBinding::Local(LocalId(3)))
        .unwrap();
    table.exit_scope().unwrap();
    assert_eq!(table.resolve_lexical("temporary"), None);
    table.exit_scope().unwrap();
    assert_eq!(table.resolve_lexical("parameter"), None);
    // A new function can reuse the spelling, but receives a different identity.
    table.enter_scope(ScopeKind::Function);
    table
        .insert_binding("parameter".into(), LocalBinding::Local(LocalId(4)))
        .unwrap();
    assert_eq!(
        table.resolve_lexical("parameter"),
        Some(ResolvedName::Local(LocalId(4)))
    );
}

#[test]
fn labels_use_a_separate_scoped_namespace() {
    let mut table = SymbolTable::new();
    table.enter_scope(ScopeKind::Block);
    table
        .insert_binding("outer".into(), LocalBinding::Local(LocalId(1)))
        .unwrap();
    table.insert_label("outer".into(), LabelId(2)).unwrap();
    assert!(table.insert_label("outer".into(), LabelId(3)).is_err());
    assert_eq!(table.resolve_label("outer"), Some(LabelId(2)));
    assert_eq!(
        table.resolve_lexical("outer"),
        Some(ResolvedName::Local(LocalId(1)))
    );
    table.exit_scope().unwrap();
    assert_eq!(table.resolve_label("outer"), None);
    assert_eq!(table.resolve_lexical("outer"), None);
}

#[test]
fn aliases_share_entities_not_spellings_or_storage() {
    let id = DeclId {
        file: FileId(1),
        index: 0,
    };
    let entity = Entity::Declaration(id);
    let mut table = SymbolTable::new();
    for spelling in ["original", "alias"] {
        table
            .insert_binding(spelling.into(), LocalBinding::CompAlias(entity))
            .unwrap();
        assert_eq!(
            table.resolve_lexical(spelling),
            Some(ResolvedName::Entity(entity))
        );
    }
    assert_ne!(
        id,
        DeclId {
            file: FileId(2),
            index: 0
        }
    );
}

#[test]
fn annotation_defaults_are_explicitly_pending() {
    assert_eq!(NameAnnotation::default().resolved, None);
    assert_eq!(BindingAnnotation::default().binding, None);
    assert_eq!(LabelAnnotation::default().target, None);
    assert_eq!(TypeAnnotation::default().resolved, None);
    assert_eq!(ExpressionAnnotation::default().resolved, None);
    assert_eq!(MemberAnnotation::default().resolved, None);
}

#[test]
fn project_entrypoints_never_report_fake_success_even_for_empty_input() {
    let plan = BuildPlan {
        files: Vec::new(),
        entry: FileId(0),
        output: "out".into(),
    };
    assert_unimplemented(ProjectSymbols::from_plan(&plan));
    assert_unimplemented(Analyzer::new(&plan, Vec::new()));
    assert_unimplemented(
        Analyzer {
            units: Vec::new(),
            symbols: symbols(),
        }
        .analyze(),
    );

    let mut registry = symbols();
    let mut units = Vec::new();
    let mut types = SemanticTypes::default();
    assert_unimplemented(collect_declarations(&units, &mut registry));
    assert_unimplemented(resolve_names(&mut units, &mut registry));
    assert_unimplemented(resolve_types(&mut units, &registry, &mut types));
    assert_unimplemented(registry.resolve_path(FileId(0), &["file".into(), "name".into()]));
    assert_unimplemented(validate_analysis(&AnalyzedProject {
        units,
        symbols: registry,
        types,
    }));
}

#[test]
fn name_sketches_do_not_mutate_ast_or_scopes() {
    let mut registry = symbols();
    let mut context = ResolveContext {
        file: FileId(0),
        symbols: &mut registry,
        lexical: SymbolTable::new(),
    };
    let mut ast = Ast::new();
    let mut expression = Expression::at(
        ExpressionKind::VoidExpression,
        crate::diagnostics::Span::new(1, 1),
    );
    let original = expression.clone();
    assert_unimplemented(ast.resolve_name(&mut context));
    assert_unimplemented(expression.resolve_name(&mut context));
    assert!(ast.statements.is_empty());
    assert_eq!(expression, original);
    assert_eq!(context.lexical.depth(), 1);
    assert!(context.symbols.namespaces.is_empty());
}

#[test]
fn typing_member_and_generic_sketches_fail_without_fabricating_annotations() {
    let registry = symbols();
    let mut types = SemanticTypes::default();
    let mut context = TypeContext {
        file: FileId(0),
        symbols: &registry,
        types: &mut types,
    };
    assert_unimplemented(Ast::new().resolve_type(&mut context));
    let mut expression = Expression::at(
        ExpressionKind::VoidExpression,
        crate::diagnostics::Span::new(1, 1),
    );
    assert_unimplemented(expression.resolve_type(&mut context, None));
    let key = GenericInstanceKey {
        template: DeclId {
            file: FileId(0),
            index: 0,
        },
        arguments: vec![TypeId(0)],
    };
    assert_unimplemented(resolve_generic_instance(&key, &mut context));
    assert_unimplemented(resolve_member(TypeId(0), "field", context.types));
    assert_unimplemented(check_assignable(TypeId(0), TypeId(0), context.types));
    assert_unimplemented(resolve_multi_value_types(0, &[], context.types));
    assert_unimplemented(validate_assignment_target(&ExpressionInfo {
        ty: TypeId(0),
        category: ValueCategory::MutablePlace,
        type_value: None,
    }));
    assert!(context.types.types.is_empty());
    assert!(context.types.declarations.is_empty());
    assert!(context.types.locals.is_empty());
}
