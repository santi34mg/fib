//! Utility and unfinished typing contracts. Entity-focused name-resolution
//! specifications are grouped in the inline `name_resolution` module below.

use std::collections::HashMap;

use crate::{
    build_plan::FileId,
    frontend::{
        analysis_annotation::{
            BindingAnnotation, ExpressionAnnotation, ExpressionInfo, LabelAnnotation,
            MemberAnnotation, NameAnnotation, TypeAnnotation, ValueCategory,
        },
        analyzer::LocalIdAllocator,
        ast::{
            Ast,
            expression::{Expression, ExpressionKind},
        },
        identifier::{DeclId, Entity, LabelId, LocalId, ResolvedName},
        namespace::ProjectSymbols,
        scope_kind::ScopeKind,
        semantic_type::{SemanticTypes, TypeId},
        symbol_table::{LocalBinding, SymbolTable},
    },
};

use super::{
    AnalyzedProject, Analyzer, ResolveContext, TypeContext,
    expression::resolve_member,
    generics::{GenericInstanceKey, resolve_generic_instance},
    resolve_types,
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
        file_id: FileId(1),
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
            file_id: FileId(2),
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
fn unfinished_analysis_and_typing_never_report_fake_success() {
    let analyzer = Analyzer {
        units: Vec::new(),
        symbols: symbols(),
        local_ids: LocalIdAllocator::default(),
        paths: HashMap::new(),
        phase: super::NamePhase::New,
    };
    assert_unimplemented(analyzer.analyze());

    let registry = symbols();
    let mut units = Vec::new();
    let mut types = SemanticTypes::default();
    assert_unimplemented(resolve_types(&mut units, &registry, &mut types));
    assert_unimplemented(validate_analysis(&AnalyzedProject {
        units,
        symbols: registry,
        types,
    }));
}

#[test]
fn empty_ast_name_resolution_preserves_scopes() {
    let mut registry = symbols();
    let mut context = ResolveContext {
        file_id: FileId(0),
        symbols: &mut registry,
        lexical: SymbolTable::new(),
        local_ids: &mut LocalIdAllocator::default(),
    };
    let mut ast = Ast::new();
    ast.resolve_name(&mut context).unwrap();
    assert!(ast.statements.is_empty());
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
    assert_unimplemented(Analyzer::resolve_type_expression(
        &mut expression,
        &mut context,
        None,
    ));
    let key = GenericInstanceKey {
        template: DeclId {
            file_id: FileId(0),
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

mod name_resolution {
    //! Executable specification of name resolution and its explicit semantic boundary.
    //! Run one entity phase with `cargo test --no-default-features name_resolution::phase_01`.

    use std::{collections::HashMap, path::Path};

    use super::{Analyzer, LocalIdAllocator, ResolveContext};
    use crate::{
        build_plan::{BuildPlan, FileId, SourceFile},
        diagnostics::Span,
        driver::CompilationUnit,
        frontend::{
            analysis_annotation::{BindingAnnotation, BindingId, NameAnnotation},
            ast::{
                Ast,
                expression::{Expression, ExpressionKind},
                pattern::Pattern,
                qualified_name::QualifiedIdentifier,
                statement::{Statement, StatementKind},
                type_expression::{TypeExpression, TypeExpressionKind},
                variable_declaration::VariableDeclaration,
            },
            identifier::{DeclId, Entity, LocalId, ResolvedName},
            namespace::ProjectSymbols,
            parser::Parser,
            scope_kind::ScopeKind,
            symbol_table::{LocalBinding, SymbolTable},
            tokenizer::Tokenizer,
        },
    };

    mod phase_01_files {
        //! File registration and declaration identities; no name/type pass required.
        use super::*;
        use crate::frontend::namespace::Visibility;

        #[test]
        fn named_units_register_without_reading_paths() {
            let (plan, units) = inputs(&[("main", ""), ("pkg::util", "")]);
            let analyzer = Analyzer::new(plan, units).unwrap();
            assert_eq!(analyzer.symbols.files_by_name.len(), 2);
            assert_eq!(
                analyzer
                    .symbols
                    .files_by_name
                    .get(&"pkg::util".to_string().into()),
                Some(&FileId(1))
            );
            assert!(analyzer.symbols.namespaces.is_empty());
        }

        #[test]
        fn project_registration_uses_only_plan_files() {
            let (plan, _) = inputs(&[("main", ""), ("pkg::util", "")]);
            let symbols =
                ProjectSymbols::from_plan(&plan).expect("register named files without disk access");
            assert_eq!(symbols.files_by_name.len(), 2);
            assert_eq!(symbols.files_by_name[&"main".to_string().into()], FileId(0));
            reject(
                symbols.resolve_path(FileId(0), &["unlisted".into()]),
                &["unlisted"],
            );
        }

        #[test]
        fn duplicate_plan_ids_are_rejected() {
            let (mut plan, units) = inputs(&[("main", ""), ("util", "")]);
            plan.files[1].id = FileId(0);
            reject(Analyzer::new(plan, units), &["duplicate", "file"]);
        }
        #[test]
        fn duplicate_unit_ids_are_rejected() {
            let (plan, mut units) = inputs(&[("main", ""), ("util", "")]);
            units[1].source_file_id = FileId(0);
            reject(Analyzer::new(plan, units), &["duplicate", "file"]);
        }
        #[test]
        fn missing_units_are_rejected() {
            let (plan, mut units) = inputs(&[("main", ""), ("util", "")]);
            units.pop();
            reject(Analyzer::new(plan, units), &["file", "match"]);
        }
        #[test]
        fn extra_units_are_rejected() {
            let (plan, mut units) = inputs(&[("main", "")]);
            units.push(CompilationUnit {
                source_file_id: FileId(99),
                ast: parse(""),
            });
            reject(Analyzer::new(plan, units), &["file", "match"]);
        }
        #[test]
        fn mismatched_unit_ids_are_rejected() {
            let (plan, mut units) = inputs(&[("main", "")]);
            units[0].source_file_id = FileId(9);
            reject(Analyzer::new(plan, units), &["file", "match"]);
        }
        #[test]
        fn duplicate_file_names_are_rejected() {
            let (plan, units) = inputs(&[("main", ""), ("main", "")]);
            reject(Analyzer::new(plan, units), &["main"]);
        }
        #[test]
        fn registration_rejects_duplicate_ids() {
            let (mut plan, _) = inputs(&[("main", ""), ("util", "")]);
            plan.files[1].id = FileId(0);
            reject(ProjectSymbols::from_plan(&plan), &["duplicate"]);
        }
        #[test]
        fn registration_rejects_ambiguous_names() {
            let (plan, _) = inputs(&[("main", ""), ("main", "")]);
            reject(ProjectSymbols::from_plan(&plan), &["main"]);
        }

        const DECLARATIONS: &str = "value := 1; pub const limit := 2; fn callable() {} comp Type: type = struct { field: int32 }; pub left, const right := 3, 4;";
        #[test]
        fn declarations_have_ordered_namespace_slots_and_independent_visibility() {
            let analyzer = collected(&[("main", DECLARATIONS)]);
            let ns = &analyzer.symbols.namespaces[&FileId(0)];
            assert_eq!(ns.file_id, FileId(0));
            let expected = ["value", "limit", "callable", "Type", "left", "right"];
            assert_eq!(ns.declarations.len(), expected.len());
            assert_eq!(ns.names.len(), expected.len());
            for (i, name) in expected.iter().enumerate() {
                assert_eq!(ns.names[*name], decl(0, i as u32));
                assert_eq!(ns.declarations[i].name, *name);
                assert_eq!(
                    ns.declarations[i].visibility,
                    if [1, 4].contains(&i) {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    }
                );
                assert_eq!(ns.declarations[i].alias_target, None);
            }
            for (statement, indices) in analyzer.units[0].ast.statements.iter().zip([
                vec![0],
                vec![1],
                vec![2],
                vec![3],
                vec![4, 5],
            ]) {
                for index in indices {
                    assert_eq!(ns.declarations[index].span, statement.span);
                }
            }
        }
        #[test]
        fn collection_annotates_every_binding_without_resolving_initializers() {
            let analyzer = collected(&[("main", DECLARATIONS)]);
            let observed = observe(&analyzer.units[0].ast);
            for (i, (_, binding)) in observed.bindings.iter().enumerate() {
                assert_eq!(*binding, Some(BindingId::Declaration(decl(0, i as u32))));
            }
            let mut analyzer = collected(&[("main", "a := later; later := 1;")]);
            assert_eq!(
                observe(&analyzer.units[0].ast).names,
                vec![("later".into(), None)]
            );
            reject(
                crate::frontend::analyzer::validate_names(&mut analyzer.units[0].ast),
                &["pending", "later"],
            );
            analyzer.resolve_names().unwrap();
            crate::frontend::analyzer::validate_names(&mut analyzer.units[0].ast).unwrap();
            if let StatementKind::VariableDeclaration(var) =
                &mut analyzer.units[0].ast.statements[0].kind
            {
                var.annotation.binding = None;
            }
            reject(
                crate::frontend::analyzer::validate_names(&mut analyzer.units[0].ast),
                &["pending binding", "a"],
            );
        }
        #[test]
        fn file_namespaces_are_independent_and_ids_follow_file_identity() {
            let analyzer = collected(&[("first", "same := 1;"), ("second", "same := 2;")]);
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(0)].names["same"],
                decl(0, 0)
            );
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(1)].names["same"],
                decl(1, 0)
            );
        }
        #[test]
        fn unit_order_does_not_change_declaring_file_identity() {
            let (plan, mut units) = inputs(&[("first", "a := 1;"), ("second", "b := 2;")]);
            units.reverse();
            let mut analyzer = Analyzer::new(plan, units).unwrap();
            analyzer.collect_declarations().unwrap();
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(1)].names["b"],
                decl(1, 0)
            );
        }

        macro_rules! duplicate {
            ($name:ident, $source:literal) => {
                #[test]
                fn $name() {
                    let (plan, units) = inputs(&[("main", $source)]);
                    let mut analyzer = Analyzer::new(plan, units).unwrap();
                    reject(analyzer.collect_declarations(), &["duplicate", "same"]);
                }
            };
        }
        duplicate!(duplicate_variables, "same := 1; same := 2;");
        duplicate!(duplicate_functions, "fn same() {} fn same() {}");
        duplicate!(function_variable_collision, "fn same() {} same := 1;");
        duplicate!(variable_function_collision, "same := 1; fn same() {}");
        duplicate!(type_value_collision, "comp same: type = int32; same := 1;");
        duplicate!(duplicate_inside_multi_binding, "same, same := 1, 2;");
        duplicate!(
            multi_binding_collision_with_prior_declaration,
            "same := 0; other, same := 1, 2;"
        );

        #[test]
        fn invalid_top_level_statement_is_rejected() {
            // Construct directly: this is a collector contract, not a grammar extension.
            let (plan, mut units) = inputs(&[("main", "")]);
            units[0].ast.statements.push(Statement {
                kind: StatementKind::Return(None),
                span: Span::new(2, 3),
            });
            let mut analyzer = Analyzer::new(plan, units).unwrap();
            reject(analyzer.collect_declarations(), &["top-level"]);
        }

        #[test]
        fn public_function_flag_is_collected_from_ast() {
            // `pub fn` is not accepted by the parser; the AST already represents it.
            let (plan, mut units) = inputs(&[("main", "fn exported();")]);
            let StatementKind::FunctionDeclaration(fun) = &mut units[0].ast.statements[0].kind
            else {
                unreachable!()
            };
            fun.is_pub = true;
            let mut analyzer = Analyzer::new(plan, units).unwrap();
            analyzer.collect_declarations().unwrap();
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(0)].declarations[0].visibility,
                Visibility::Public
            );
        }

        #[test]
        fn staged_api_rejects_invalid_order_and_repeated_calls() {
            let (plan, units) = inputs(&[("main", "x := 1;")]);
            let mut analyzer = Analyzer::new(plan, units).unwrap();
            reject(analyzer.resolve_names(), &["collection"]);
            analyzer.collect_declarations().unwrap();
            reject(analyzer.collect_declarations(), &["already"]);
            analyzer.resolve_names().unwrap();
            let original = analyzer.units.clone();
            reject(analyzer.resolve_names(), &["once"]);
            assert_eq!(analyzer.units, original);
        }

        #[test]
        fn failed_resolution_cannot_be_retried() {
            let mut analyzer = collected(&[("main", "fn f() { x := 1; absent; }")]);
            reject(analyzer.resolve_names(), &["absent"]);
            reject(analyzer.resolve_names(), &["once"]);
        }

        #[test]
        fn name_errors_preserve_category_file_and_location() {
            let mut analyzer = collected(&[(
                "main",
                "fn f() {
    absent;
}",
            )]);
            let error = analyzer
                .resolve_names()
                .unwrap_err()
                .downcast::<crate::diagnostics::CompilerError>()
                .unwrap();
            assert_eq!(error.kind, crate::diagnostics::ErrorKind::Name);
            assert_eq!(error.span, Some(Span::new(2, 5)));
            assert_eq!(error.filename, Some("main.fib".into()));
        }
    }
    mod phase_02_locals {
        //! Parameters, runtime local bindings, declaration order, and lexical lifetimes.
        use super::*;

        #[test]
        fn parameter_and_use_share_identity() {
            let analyzer = resolved(&[("main", "fn f(value: int32) { value; }")]);
            names(
                &analyzer,
                0,
                &[(
                    "value",
                    ResolvedName::Local(local(&analyzer, 0, "value", 0)),
                )],
            );
        }
        #[test]
        fn all_local_binding_forms_annotate_uses() {
            let analyzer = resolved(&[(
                "main",
                "fn f() { inferred := 1; typed: int32 = 2; const fixed := 3; empty: int32; a, b := 4, 5; inferred; typed; fixed; empty; a; b; }",
            )]);
            let expected = ["inferred", "typed", "fixed", "empty", "a", "b"]
                .map(|name| (name, ResolvedName::Local(local(&analyzer, 0, name, 0))));
            names(&analyzer, 0, &expected);
            let unique: std::collections::HashSet<_> = expected.iter().map(|(_, id)| id).collect();
            assert_eq!(unique.len(), expected.len());
        }
        #[test]
        fn ids_are_project_unique_across_functions_and_files() {
            let analyzer = resolved(&[
                (
                    "one",
                    "fn a(p: int32) { x := p; } fn b(p: int32) { x := p; }",
                ),
                ("two", "fn a(p: int32) { x := p; }"),
            ]);
            let ids = [
                local(&analyzer, 0, "p", 0),
                local(&analyzer, 0, "x", 0),
                local(&analyzer, 0, "p", 1),
                local(&analyzer, 0, "x", 1),
                local(&analyzer, 1, "p", 0),
                local(&analyzer, 1, "x", 0),
            ];
            assert_eq!(
                ids.into_iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                6
            );
        }
        #[test]
        fn initializer_precedes_new_local_and_later_uses_see_local() {
            let analyzer = resolved(&[("main", "x := 10; fn f() { x := x + 1; x; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("x", target(0, 0)),
                    ("x", ResolvedName::Local(local(&analyzer, 0, "x", 1))),
                ],
            );
        }
        #[test]
        fn all_multi_initializers_precede_all_new_bindings() {
            let analyzer =
                resolved(&[("main", "a := 10; b := 20; fn f() { a, b := b, a; a; b; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("b", target(0, 1)),
                    ("a", target(0, 0)),
                    ("a", ResolvedName::Local(local(&analyzer, 0, "a", 1))),
                    ("b", ResolvedName::Local(local(&analyzer, 0, "b", 1))),
                ],
            );
        }
        #[test]
        fn sibling_blocks_can_reuse_spelling_with_distinct_ids() {
            let analyzer = resolved(&[(
                "main",
                "fn f() { if true { item := 1; item; } else { item := 2; item; }; }",
            )]);
            let first = local(&analyzer, 0, "item", 0);
            let second = local(&analyzer, 0, "item", 1);
            assert_ne!(first, second);
            names(
                &analyzer,
                0,
                &[
                    ("item", ResolvedName::Local(first)),
                    ("item", ResolvedName::Local(second)),
                ],
            );
        }
        #[test]
        fn parameter_can_share_file_declaration_spelling() {
            let analyzer = resolved(&[("main", "item := 1; fn f(item: int32) { item; }")]);
            names(
                &analyzer,
                0,
                &[("item", ResolvedName::Local(local(&analyzer, 0, "item", 1)))],
            );
        }
        #[test]
        fn nested_blocks_can_read_enclosing_local() {
            let analyzer = resolved(&[(
                "main",
                "fn f() { x := 1; if true { while (true) { x; }; }; }",
            )]);
            names(
                &analyzer,
                0,
                &[("x", ResolvedName::Local(local(&analyzer, 0, "x", 0)))],
            );
        }

        macro_rules! invalid {
            ($name:ident, $source:literal, $identifier:literal) => {
                #[test]
                fn $name() {
                    reject_source($source, &[$identifier]);
                }
            };
        }
        invalid!(
            self_reference_without_existing_binding,
            "fn f() { x := x; }",
            "x"
        );
        invalid!(
            multi_initializer_cannot_see_earlier_new_binding,
            "fn f() { a, b := 1, a; }",
            "a"
        );
        invalid!(
            multi_initializer_cannot_see_later_new_binding,
            "fn f() { a, b := b, 1; }",
            "b"
        );
        invalid!(
            multi_initializer_cannot_see_own_new_binding,
            "fn f() { a, b := a, 1; }",
            "a"
        );
        invalid!(
            later_local_is_not_forward_visible,
            "fn f() { x; x := 1; }",
            "x"
        );
        invalid!(unknown_local, "fn f() { absent; }", "absent");
        invalid!(duplicate_parameters, "fn f(p: int32, p: int32) {}", "p");
        invalid!(duplicate_local, "fn f() { x := 1; x := 2; }", "x");
        invalid!(
            local_cannot_redeclare_parameter,
            "fn f(p: int32) { p := 1; }",
            "p"
        );
        invalid!(
            nested_local_cannot_redeclare_parameter,
            "fn f(p: int32) { if true { p := 1; }; }",
            "p"
        );
        invalid!(
            nested_local_cannot_redeclare_local,
            "fn f() { x := 1; if true { x := 2; }; }",
            "x"
        );
        invalid!(duplicate_multi_locals, "fn f() { x, x := 1, 2; }", "x");
        invalid!(
            branch_local_does_not_escape,
            "fn f() { if true { x := 1; }; x; }",
            "x"
        );
        invalid!(
            branch_local_not_visible_in_sibling,
            "fn f() { if true { x := 1; } else { x; }; }",
            "x"
        );
        invalid!(
            loop_local_does_not_escape,
            "fn f() { while (true) { x := 1; }; x; }",
            "x"
        );
        invalid!(
            locals_do_not_leak_between_functions,
            "fn f() { x := 1; } fn g() { x; }",
            "x"
        );
        invalid!(
            parameters_do_not_leak_between_functions,
            "fn f(p: int32) {} fn g() { p; }",
            "p"
        );

        #[test]
        fn isolated_local_initializer_is_resolved_before_insertion() {
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            let mut statement = body("fresh := fresh;").remove(0);
            reject(
                Analyzer::resolve_name_statement(&mut statement, &mut context),
                &["fresh"],
            );
            assert_eq!(
                context.lexical.resolve_lexical("fresh"),
                None,
                "failed initializer must not publish its binding"
            );
        }
        #[test]
        fn isolated_local_binding_annotation_matches_uses() {
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            let mut statement = body("fresh := x;").remove(0);
            Analyzer::resolve_name_statement(&mut statement, &mut context).unwrap();
            let StatementKind::VariableDeclaration(var) = &statement.kind else {
                unreachable!()
            };
            let Some(BindingId::Local(id)) = var.annotation.binding else {
                panic!("local binding annotation missing")
            };
            assert_eq!(
                context.lexical.resolve_lexical("fresh"),
                Some(ResolvedName::Local(id))
            );
            let mut observed = Observed::default();
            observed.statement(&mut statement);
            assert_eq!(
                observed.names,
                vec![("x".into(), Some(ResolvedName::Local(LocalId(100))))]
            );
        }

        #[test]
        fn expired_block_local_exposes_file_binding_again() {
            let analyzer = resolved(&[("main", "x := 10; fn f() { if true { x := 1; x; }; x; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("x", ResolvedName::Local(local(&analyzer, 0, "x", 1))),
                    ("x", target(0, 0)),
                ],
            );
        }
        #[test]
        fn typed_multi_binding_without_values_registers_each_local() {
            let analyzer = resolved(&[("main", "fn f() { a: int32, b: int32; a; b; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("a", ResolvedName::Local(local(&analyzer, 0, "a", 0))),
                    ("b", ResolvedName::Local(local(&analyzer, 0, "b", 0))),
                ],
            );
        }
        #[test]
        fn existing_parameter_is_visible_in_later_local_initializer() {
            let analyzer = resolved(&[("main", "fn f(p: int32) { value := p; value; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("p", ResolvedName::Local(local(&analyzer, 0, "p", 0))),
                    (
                        "value",
                        ResolvedName::Local(local(&analyzer, 0, "value", 0)),
                    ),
                ],
            );
        }
        #[test]
        fn shared_call_initializer_is_resolved_before_multi_bindings() {
            let analyzer =
                resolved(&[("main", "fn produce(); fn f() { a, b := produce(); a; b; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("produce", target(0, 0)),
                    ("a", ResolvedName::Local(local(&analyzer, 0, "a", 0))),
                    ("b", ResolvedName::Local(local(&analyzer, 0, "b", 0))),
                ],
            );
        }
        #[test]
        fn isolated_multi_initializers_cannot_see_any_new_bindings() {
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            let mut statement = body("fresh, other := 1, fresh;").remove(0);
            reject(
                Analyzer::resolve_name_statement(&mut statement, &mut context),
                &["fresh"],
            );
            assert_eq!(context.lexical.resolve_lexical("fresh"), None);
            assert_eq!(context.lexical.resolve_lexical("other"), None);
        }
    }
    mod phase_03_references {
        //! File fallback, functions, qualified paths, and visibility.
        use super::*;

        #[test]
        fn file_initializer_resolves_earlier_declaration() {
            let analyzer = resolved(&[("main", "first := 1; second := first;")]);
            names(&analyzer, 0, &[("first", target(0, 0))]);
        }
        #[test]
        fn forward_file_reference_is_visible_before_initialization_validation() {
            let analyzer = resolved(&[("main", "first := later; later := 1;")]);
            names(&analyzer, 0, &[("later", target(0, 1))]);
        }
        #[test]
        fn file_multi_initializers_resolve_predeclared_names() {
            let analyzer = resolved(&[("main", "first, second := later, first; later := 1;")]);
            names(
                &analyzer,
                0,
                &[("later", target(0, 2)), ("first", target(0, 0))],
            );
        }
        #[test]
        fn self_recursive_function_resolves_its_file_identity() {
            let analyzer = resolved(&[("main", "fn recurse() { recurse(); }")]);
            names(&analyzer, 0, &[("recurse", target(0, 0))]);
        }
        #[test]
        fn mutually_recursive_functions_resolve_forward_identities() {
            let analyzer =
                resolved(&[("main", "fn first() { second(); } fn second() { first(); }")]);
            names(
                &analyzer,
                0,
                &[("second", target(0, 1)), ("first", target(0, 0))],
            );
        }
        #[test]
        fn bodyless_function_types_and_parameters_are_annotated() {
            let analyzer = resolved(&[("main", "comp T: type = int32; fn declared(value: T) T;")]);
            local(&analyzer, 0, "value", 0);
            names(&analyzer, 0, &[("T", target(0, 0)), ("T", target(0, 0))]);
            assert_eq!(
                observe(&analyzer.units[0].ast).bindings[1].1,
                Some(BindingId::Declaration(decl(0, 1)))
            );
        }
        #[test]
        fn qualified_current_file_bypasses_same_spelled_parameter() {
            let analyzer =
                resolved(&[("main", "item := 1; fn f(item: int32) { item; main::item; }")]);
            names(
                &analyzer,
                0,
                &[
                    ("item", ResolvedName::Local(local(&analyzer, 0, "item", 1))),
                    ("main::item", target(0, 0)),
                ],
            );
        }
        #[test]
        fn current_file_can_select_its_private_declaration() {
            let analyzer = resolved(&[("main", "hidden := 1; result := main::hidden;")]);
            names(&analyzer, 0, &[("main::hidden", target(0, 0))]);
        }
        #[test]
        fn public_cross_file_declaration_is_annotated() {
            let analyzer = resolved(&[
                ("main", "result := util::value;"),
                ("util", "pub value := 1;"),
            ]);
            names(&analyzer, 0, &[("util::value", target(1, 0))]);
        }
        #[test]
        fn nested_plan_name_selects_canonical_file() {
            let analyzer = resolved(&[
                ("main", "result := pkg::math::value;"),
                ("pkg::math", "pub value := 1;"),
            ]);
            names(&analyzer, 0, &[("pkg::math::value", target(1, 0))]);
        }
        #[test]
        fn same_declaration_selected_by_local_file_spelling_and_path_has_one_identity() {
            let analyzer = resolved(&[("main", "value := 1; a := value; b := main::value;")]);
            names(
                &analyzer,
                0,
                &[("value", target(0, 0)), ("main::value", target(0, 0))],
            );
        }

        macro_rules! bad_path {
            ($name:ident, $source:literal, $other:literal, $fragment:literal) => {
                #[test]
                fn $name() {
                    let mut analyzer = collected(&[("main", $source), ("util", $other)]);
                    reject(analyzer.resolve_names(), &[$fragment]);
                }
            };
        }
        bad_path!(
            private_cross_file_target,
            "result := util::hidden;",
            "hidden := 1;",
            "hidden"
        );
        bad_path!(
            missing_qualified_declaration,
            "result := util::absent;",
            "pub value := 1;",
            "absent"
        );
        bad_path!(
            missing_qualified_file,
            "result := unlisted::value;",
            "pub value := 1;",
            "unlisted"
        );
        bad_path!(
            variable_is_not_intermediate_namespace,
            "result := util::value::member;",
            "pub value := 1;",
            "value"
        );
        #[test]
        fn function_is_not_intermediate_namespace() {
            // Public function syntax is not parsed yet; exercise its existing AST flag.
            let (plan, mut units) = inputs(&[
                ("main", "result := util::callable::member;"),
                ("util", "fn callable();"),
            ]);
            let StatementKind::FunctionDeclaration(function) = &mut units[1].ast.statements[0].kind
            else {
                unreachable!()
            };
            function.is_pub = true;
            let mut analyzer = Analyzer::new(plan, units).unwrap();
            analyzer.collect_declarations().unwrap();
            reject(analyzer.resolve_names(), &["callable"]);
        }

        #[test]
        fn isolated_file_fallback_annotates_reference_and_preserves_span() {
            let analyzer = collected(&[("main", "known := 1;")]);
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&analyzer.symbols, &mut ids);
            let mut expression = named("known");
            let span = expression.span;
            Analyzer::resolve_name_expression(&mut expression, &mut context).unwrap();
            let ExpressionKind::QualifiedIdentifier(name) = expression.kind else {
                unreachable!()
            };
            assert_eq!(name.annotation.resolved, Some(target(0, 0)));
            assert_eq!(name.segments, ["known"]);
            assert_eq!(expression.span, span);
        }
        #[test]
        fn isolated_local_lookup_takes_precedence_over_file_declaration() {
            let analyzer = collected(&[("main", "x := 1;")]);
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&analyzer.symbols, &mut ids);
            let mut expression = named("x");
            Analyzer::resolve_name_expression(&mut expression, &mut context).unwrap();
            let ExpressionKind::QualifiedIdentifier(name) = expression.kind else {
                unreachable!()
            };
            assert_eq!(
                name.annotation.resolved,
                Some(ResolvedName::Local(LocalId(100)))
            );
        }

        #[test]
        fn canonical_file_path_bypasses_same_spelled_runtime_local() {
            let analyzer = resolved(&[
                ("main", "fn f(util: int32) { util::value; }"),
                ("util", "pub value := 1;"),
            ]);
            names(&analyzer, 0, &[("util::value", target(1, 0))]);
        }

        #[test]
        fn longest_registered_file_prefix_is_selected() {
            let analyzer = resolved(&[
                ("main", "result := pkg::math::value;"),
                ("pkg", "unused := 1;"),
                ("pkg::math", "pub value := 2;"),
            ]);
            names(&analyzer, 0, &[("pkg::math::value", target(2, 0))]);
        }
    }
    mod phase_04_aliases {
        //! Compile-time entities and aliases retain canonical target identities.
        use super::*;

        #[test]
        fn file_alias_can_be_used_as_qualified_namespace() {
            let analyzer = resolved(&[
                ("main", "comp Library := util; result := Library::value;"),
                ("util", "pub value := 1;"),
            ]);
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(0)].declarations[0].alias_target,
                Some(Entity::File(FileId(1)))
            );
            names(
                &analyzer,
                0,
                &[
                    ("util", ResolvedName::Entity(Entity::File(FileId(1)))),
                    ("Library::value", target(1, 0)),
                ],
            );
        }
        #[test]
        fn aliases_to_mutable_file_variable_share_target_without_new_storage_identity() {
            let analyzer = resolved(&[(
                "main",
                "counter := 0; comp first := counter; comp second := first; result := second;",
            )]);
            let ns = &analyzer.symbols.namespaces[&FileId(0)];
            assert_eq!(
                ns.declarations[1].alias_target,
                Some(Entity::Declaration(decl(0, 0)))
            );
            assert_eq!(
                ns.declarations[2].alias_target,
                Some(Entity::Declaration(decl(0, 0)))
            );
            names(
                &analyzer,
                0,
                &[
                    ("counter", target(0, 0)),
                    ("first", target(0, 0)),
                    ("second", target(0, 0)),
                ],
            );
        }
        #[test]
        fn function_alias_calls_use_original_function_identity() {
            let analyzer = resolved(&[(
                "main",
                "fn original(); comp alias := original; result := alias();",
            )]);
            names(
                &analyzer,
                0,
                &[("original", target(0, 0)), ("alias", target(0, 0))],
            );
        }
        #[test]
        fn type_alias_references_use_original_type_binding() {
            let analyzer = resolved(&[(
                "main",
                "comp Original: type = int32; comp Alias := Original; value: Alias;",
            )]);
            names(
                &analyzer,
                0,
                &[("Original", target(0, 0)), ("Alias", target(0, 0))],
            );
        }
        #[test]
        fn forward_alias_chain_resolves_to_final_entity() {
            let analyzer = resolved(&[(
                "main",
                "comp first := second; comp second := original; original := 1; result := first;",
            )]);
            names(
                &analyzer,
                0,
                &[
                    ("second", target(0, 2)),
                    ("original", target(0, 2)),
                    ("first", target(0, 2)),
                ],
            );
        }
        #[test]
        fn multiple_aliases_share_one_target() {
            let analyzer = resolved(&[(
                "main",
                "original := 1; comp a := original; comp b := original; result := a + b;",
            )]);
            names(
                &analyzer,
                0,
                &[
                    ("original", target(0, 0)),
                    ("original", target(0, 0)),
                    ("a", target(0, 0)),
                    ("b", target(0, 0)),
                ],
            );
        }
        #[test]
        fn const_value_binding_has_its_own_identity() {
            let analyzer = resolved(&[(
                "main",
                "original := 1; const snapshot := original; result := snapshot;",
            )]);
            names(
                &analyzer,
                0,
                &[("original", target(0, 0)), ("snapshot", target(0, 1))],
            );
            assert_eq!(
                analyzer.symbols.namespaces[&FileId(0)].declarations[1].alias_target,
                None
            );
        }
        #[test]
        fn local_comp_alias_can_target_file_declaration() {
            let analyzer = resolved(&[(
                "main",
                "original := 1; fn f() { comp alias := original; alias; }",
            )]);
            names(
                &analyzer,
                0,
                &[("original", target(0, 0)), ("alias", target(0, 0))],
            );
        }
        #[test]
        fn alias_to_public_cross_file_target_preserves_identity() {
            let analyzer = resolved(&[
                ("main", "comp alias := util::value; result := alias;"),
                ("util", "pub value := 1;"),
            ]);
            names(
                &analyzer,
                0,
                &[("util::value", target(1, 0)), ("alias", target(1, 0))],
            );
        }
        #[test]
        fn public_reexport_of_public_target_keeps_canonical_identity() {
            let analyzer = resolved(&[
                ("main", "result := facade::exposed;"),
                ("facade", "pub comp exposed := util::value;"),
                ("util", "pub value := 1;"),
            ]);
            names(&analyzer, 0, &[("facade::exposed", target(2, 0))]);
        }

        macro_rules! invalid {
            ($name:ident, $source:literal, $identifier:literal) => {
                #[test]
                fn $name() {
                    reject_source($source, &[$identifier]);
                }
            };
        }
        invalid!(missing_alias_target, "comp alias := absent;", "absent");
        invalid!(self_alias_cycle, "comp looped := looped;", "cycle");
        invalid!(
            two_alias_cycle,
            "comp first := second; comp second := first;",
            "cycle"
        );
        invalid!(
            long_alias_cycle,
            "comp first := second; comp second := third; comp third := first;",
            "cycle"
        );
        invalid!(
            alias_cannot_target_parameter,
            "fn f(p: int32) { comp alias := p; }",
            "p"
        );
        invalid!(
            alias_cannot_target_runtime_local,
            "fn f() { value := 1; comp alias := value; }",
            "value"
        );
        #[test]
        fn alias_cannot_access_private_cross_file_target() {
            let mut analyzer = collected(&[
                ("main", "comp alias := util::hidden;"),
                ("util", "hidden := 1;"),
            ]);
            reject(analyzer.resolve_names(), &["hidden"]);
        }
        #[test]
        fn private_alias_cannot_be_selected_from_another_file() {
            let mut analyzer = collected(&[
                ("main", "result := util::alias;"),
                ("util", "pub value := 1; comp alias := value;"),
            ]);
            reject(analyzer.resolve_names(), &["alias"]);
        }

        #[test]
        fn registry_path_follows_alias_targets_without_source_rewriting() {
            // Isolate path traversal from the still-unfinished source alias pass.
            let mut analyzer = collected(&[
                ("main", "pub comp Library := util;"),
                ("util", "pub original := 1; pub comp alias := original;"),
            ]);
            analyzer
                .symbols
                .namespaces
                .get_mut(&FileId(0))
                .unwrap()
                .declarations[0]
                .alias_target = Some(Entity::File(FileId(1)));
            analyzer
                .symbols
                .namespaces
                .get_mut(&FileId(1))
                .unwrap()
                .declarations[1]
                .alias_target = Some(Entity::Declaration(decl(1, 0)));
            assert_eq!(
                analyzer
                    .symbols
                    .resolve_path(
                        FileId(0),
                        &["main".into(), "Library".into(), "alias".into()]
                    )
                    .unwrap(),
                Entity::Declaration(decl(1, 0))
            );
        }
        #[test]
        fn registry_path_rejects_alias_only_cycle() {
            let mut analyzer =
                collected(&[("main", "comp first := second; comp second := first;")]);
            let ns = analyzer.symbols.namespaces.get_mut(&FileId(0)).unwrap();
            ns.declarations[0].alias_target = Some(Entity::Declaration(decl(0, 1)));
            ns.declarations[1].alias_target = Some(Entity::Declaration(decl(0, 0)));
            reject(
                analyzer
                    .symbols
                    .resolve_path(FileId(0), &["main".into(), "first".into()]),
                &["cycle"],
            );
        }

        #[test]
        fn aliases_can_cross_multiple_files() {
            let analyzer = resolved(&[
                ("main", "comp alias := facade::exposed; result := alias;"),
                ("facade", "pub comp exposed := util::original;"),
                ("util", "pub original := 1;"),
            ]);
            names(
                &analyzer,
                0,
                &[("facade::exposed", target(2, 0)), ("alias", target(2, 0))],
            );
        }
        #[test]
        fn alias_cycle_across_files_is_rejected() {
            let mut analyzer = collected(&[
                ("main", "pub comp first := util::second;"),
                ("util", "pub comp second := main::first;"),
            ]);
            let error = analyzer
                .resolve_names()
                .unwrap_err()
                .downcast::<crate::diagnostics::CompilerError>()
                .unwrap();
            assert_eq!(error.filename, Some("main.fib".into()));
            assert_eq!(error.span, Some(Span::new(1, 19)));
            reject::<()>(Err(error.into()), &["cycle", "main::first", "util::second"]);
        }
        #[test]
        fn local_file_alias_is_scoped_and_resolves_qualified_uses() {
            let analyzer = resolved(&[
                ("main", "fn f() { comp Library := util; Library::value; }"),
                ("util", "pub value := 1;"),
            ]);
            names(
                &analyzer,
                0,
                &[
                    ("util", ResolvedName::Entity(Entity::File(FileId(1)))),
                    ("Library::value", target(1, 0)),
                ],
            );
        }
        #[test]
        fn local_alias_cannot_escape_its_branch() {
            reject_source(
                "original := 1; fn f() { if true { comp alias := original; }; alias; }",
                &["alias"],
            );
        }

        #[test]
        fn local_alias_obeys_declaration_order() {
            reject_source(
                "original := 1; fn f() { alias; comp alias := original; }",
                &["alias"],
            );
            reject_source(
                "original := 1; fn f() { comp first := second; comp second := original; }",
                &["second"],
            );
        }

        #[test]
        fn local_alias_cannot_reuse_file_name_or_registered_root() {
            for file in ["util", "util::math"] {
                let mut analyzer = collected(&[
                    ("main", "fn f() { comp util := main; }"),
                    (file, "pub value := 1;"),
                ]);
                reject(analyzer.resolve_names(), &["local alias", "util", "file"]);
            }
        }

        #[test]
        fn private_alias_step_cannot_be_bypassed_by_public_target() {
            let mut analyzer = collected(&[
                ("main", "value := facade::Library::value;"),
                ("facade", "comp Library := util;"),
                ("util", "pub value := 1;"),
            ]);
            reject(analyzer.resolve_names(), &["Library", "private"]);
        }

        #[test]
        fn invalid_comp_initializers_have_source_diagnostics() {
            for source in ["comp alias := 1 + 2;", "comp alias: int32;"] {
                let mut analyzer = collected(&[("main", source)]);
                let error = analyzer
                    .resolve_names()
                    .unwrap_err()
                    .downcast::<crate::diagnostics::CompilerError>()
                    .unwrap();
                assert_eq!(error.kind, crate::diagnostics::ErrorKind::Name);
                assert_eq!(error.filename, Some("main.fib".into()));
                assert!(error.span.is_some());
            }
        }

        #[test]
        fn cycle_diagnostic_contains_dependency_chain() {
            let mut analyzer = collected(&[(
                "main",
                "comp first := second; comp second := third; comp third := first;",
            )]);
            reject(
                analyzer.resolve_names(),
                &["cycle", "first", "second", "third", "->"],
            );
        }

        #[test]
        fn long_alias_chain_resolves_without_stack_overflow() {
            const CHILD_MARKER: &str = "FIBC_TEST_LONG_ALIAS_CHAIN_CHILD";
            const COMPLETED: &str = "long alias chain identities verified";

            // Stack overflow aborts rather than unwinds. Isolate the fixture so
            // a regression fails this test without aborting the entire suite.
            if std::env::var_os(CHILD_MARKER).is_none() {
                let module = module_path!().split_once("::").unwrap().1;
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .arg("--exact")
                    .arg(format!(
                        "{module}::long_alias_chain_resolves_without_stack_overflow"
                    ))
                    .arg("--nocapture")
                    .env(CHILD_MARKER, "1")
                    .env("RUST_MIN_STACK", (2 * 1024 * 1024).to_string())
                    .output()
                    .expect("run the alias-chain regression in a child test process");
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                assert!(
                    output.status.success(),
                    "alias-chain child failed: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    output.status
                );
                assert!(
                    stdout.contains(COMPLETED),
                    "child did not execute the fixture\nstdout:\n{stdout}\nstderr:\n{stderr}"
                );
                return;
            }

            const ALIAS_COUNT: u32 = 8_000;
            let mut source = String::new();
            for i in 0..ALIAS_COUNT {
                source.push_str(&format!("comp a{i} := a{};\n", i + 1));
            }
            source.push_str(&format!("a{ALIAS_COUNT} := 1;"));
            let mut analyzer = collected(&[("main", &source)]);
            analyzer
                .resolve_names()
                .expect("a long acyclic alias chain must resolve");

            let declarations = &analyzer.symbols.namespaces[&FileId(0)].declarations;
            let terminal = Entity::Declaration(decl(0, ALIAS_COUNT));
            for declaration in &declarations[..ALIAS_COUNT as usize] {
                assert_eq!(
                    declaration.alias_target,
                    Some(terminal),
                    "alias '{}' must retain the terminal declaration identity",
                    declaration.name
                );
            }
            println!("{COMPLETED}");
        }
    }
    mod phase_05_types {
        //! Name lookup inside type syntax, with semantic types/members still pending.
        use super::*;

        fn type_fixture(source: &str) -> TypeExpression {
            let ast = parse(&format!("probe: {source};"));
            let StatementKind::VariableDeclaration(var) = &ast.statements[0].kind else {
                unreachable!()
            };
            var.ty.clone().unwrap()
        }
        fn check_type(source: &str, count: usize) {
            let mut ty = type_fixture(source);
            let original = ty.clone();
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            ty.resolve_name(&mut context)
                .expect("type names resolve without type checking");
            let mut observed = Observed::default();
            observed.ty(&mut ty);
            assert_eq!(
                observed.names,
                vec![("T".into(), Some(ResolvedName::Local(LocalId(103)))); count]
            );
            assert_eq!(ty, original, "only name annotations may change");
            assert_eq!(context.lexical.resolve_lexical("field"), None);
            assert_eq!(context.lexical.resolve_lexical("Variant"), None);
            assert_eq!(context.lexical.resolve_lexical("payload"), None);
            assert_eq!(
                context.lexical.resolve_lexical("x"),
                Some(ResolvedName::Local(LocalId(100)))
            );
        }
        macro_rules! type_ok {
            ($name:ident, $source:literal, $count:literal) => {
                #[test]
                fn $name() {
                    check_type($source, $count);
                }
            };
        }
        type_ok!(builtin_has_no_name_to_resolve, "int32", 0);
        type_ok!(named_type, "T", 1);
        type_ok!(pointer_pointee, "*T", 1);
        type_ok!(array_element, "T[4]", 1);
        type_ok!(slice_element, "T[]", 1);
        type_ok!(tuple_elements, "(T, T)", 2);
        type_ok!(function_arguments_and_return, "fn(T, *T) -> T", 3);
        type_ok!(
            struct_field_types_are_not_lexical_bindings,
            "struct { field: T, x: *T }",
            2
        );
        type_ok!(
            enum_payload_types_are_not_lexical_bindings,
            "enum { Variant { payload: T }, Empty }",
            1
        );
        type_ok!(nested_composite_type, "*(T, fn(T[]) -> *T)[3]", 3);

        macro_rules! type_bad {
            ($name:ident, $source:literal) => {
                #[test]
                fn $name() {
                    let mut ty = type_fixture($source);
                    let symbols = empty_symbols();
                    let mut ids = LocalIdAllocator::default();
                    reject(
                        ty.resolve_name(&mut context(&symbols, &mut ids)),
                        &["Missing"],
                    );
                }
            };
        }
        type_bad!(missing_named_type, "Missing");
        type_bad!(missing_pointer_pointee, "*Missing");
        type_bad!(missing_array_element, "Missing[2]");
        type_bad!(missing_slice_element, "Missing[]");
        type_bad!(missing_second_tuple_element, "(T, Missing)");
        type_bad!(missing_function_argument, "fn(T, Missing) -> T");
        type_bad!(missing_function_return, "fn(T) -> Missing");
        type_bad!(missing_struct_field_type, "struct { field: Missing }");
        type_bad!(
            missing_enum_payload_type,
            "enum { Variant { payload: Missing } }"
        );

        #[test]
        fn file_binding_types_and_function_signatures_resolve_forward_names() {
            let analyzer = resolved(&[(
                "main",
                "value: Later; fn f(p: Later) Later { return p; } comp Later: type = int32;",
            )]);
            names(
                &analyzer,
                0,
                &[
                    ("Later", target(0, 2)),
                    ("Later", target(0, 2)),
                    ("Later", target(0, 2)),
                    ("p", ResolvedName::Local(local(&analyzer, 0, "p", 0))),
                ],
            );
        }
        #[test]
        fn local_type_value_can_be_referenced_by_later_local() {
            let analyzer = resolved(&[(
                "main",
                "fn f() { comp Local: type = struct { field: int32 }; value: Local; }",
            )]);
            names(&analyzer, 0, &[("Local", target(0, 1))]);
        }
        #[test]
        fn qualified_and_aliased_type_names_resolve_canonically() {
            let analyzer = resolved(&[
                (
                    "main",
                    "comp Alias := util::Type; first: util::Type; second: Alias;",
                ),
                ("util", "pub comp Type: type = int32;"),
            ]);
            names(
                &analyzer,
                0,
                &[
                    ("util::Type", target(1, 0)),
                    ("util::Type", target(1, 0)),
                    ("Alias", target(1, 0)),
                ],
            );
        }
        #[test]
        fn cast_resolves_operand_and_target_name() {
            let analyzer = resolved(&[(
                "main",
                "comp Type: type = int32; source := 1; result := source as Type;",
            )]);
            names(
                &analyzer,
                0,
                &[("source", target(0, 1)), ("Type", target(0, 0))],
            );
        }
        #[test]
        fn struct_construction_resolves_owner_and_values_but_not_field_spelling() {
            let analyzer = resolved(&[(
                "main",
                "comp Owner: type = struct { field: int32 }; value := 1; result := Owner { field: value };",
            )]);
            names(
                &analyzer,
                0,
                &[("Owner", target(0, 0)), ("value", target(0, 1))],
            );
            assert!(
                !analyzer.symbols.namespaces[&FileId(0)]
                    .names
                    .contains_key("field")
            );
        }
        #[test]
        fn member_spelling_is_deferred_even_when_no_lexical_binding_exists() {
            let analyzer = resolved(&[("main", "object := 1; result := object.unknown;")]);
            names(&analyzer, 0, &[("object", target(0, 0))]);
            assert!(
                !analyzer.symbols.namespaces[&FileId(0)]
                    .names
                    .contains_key("unknown")
            );
        }
        #[test]
        fn field_definitions_cannot_supply_lexical_names() {
            reject_source(
                "comp Owner: type = struct { field: int32 }; result := field;",
                &["field"],
            );
        }
        #[test]
        fn variant_definitions_cannot_supply_lexical_names() {
            reject_source(
                "comp Owner: type = enum { Variant }; result := Variant;",
                &["Variant"],
            );
        }

        #[test]
        fn type_bindings_require_explicit_comp() {
            for source in [
                "Point: type = struct { x: int32 };",
                "Number: type = int32;",
                "Number := int32;",
                "fn f() { Number: type = int32; }",
                "comp Original: type = int32; Alias: type = Original;",
                "comp Original: type = int32; const Alias := Original;",
                "comp Original: type = int32; fn f() { Alias := Original; }",
                "comp Original: type = int32; Alias, value := Original, 1;",
            ] {
                reject_source(source, &["requires explicit comp"]);
            }
        }

        #[test]
        fn local_type_aliases_share_declaration_identity_without_exporting_names() {
            let analyzer = resolved(&[(
                "main",
                "fn f() { comp T: type = struct { x: int32 }; comp U := T; value: U; }",
            )]);
            names(&analyzer, 0, &[("T", target(0, 1)), ("U", target(0, 1))]);
            let ns = &analyzer.symbols.namespaces[&FileId(0)];
            assert_eq!(ns.names.len(), 1);
            assert!(!ns.names.contains_key("T"));
            reject(
                analyzer
                    .symbols
                    .resolve_path(FileId(0), &["main".into(), "T".into()]),
                &["T"],
            );
        }

        #[test]
        fn inline_type_multi_bindings_get_independent_identities() {
            let analyzer = resolved(&[(
                "main",
                "comp A, comp B := int32, int64; fn f() { comp C, comp D := int32, int64; c: C; d: D; }",
            )]);
            names(&analyzer, 0, &[("C", target(0, 3)), ("D", target(0, 4))]);
            assert_eq!(analyzer.symbols.namespaces[&FileId(0)].names.len(), 3);
        }

        #[test]
        fn local_types_are_ordered_and_scoped() {
            reject_source("fn f() { value: T; comp T: type = int32; }", &["T"]);
            reject_source(
                "fn f() { if true { comp T: type = int32; }; value: T; }",
                &["T"],
            );
            reject_source("fn f() { comp T: type = struct { next: *T }; }", &["T"]);
        }

        #[test]
        fn recursive_file_type_names_resolve_without_layout_checking() {
            let analyzer = resolved(&[("main", "comp Node: type = struct { next: *Node };")]);
            names(&analyzer, 0, &[("Node", target(0, 0))]);
        }
    }
    mod phase_06_traversal {
        //! Each name-bearing child is exercised directly as well as through file traversal.
        use super::*;
        use crate::frontend::identifier::LabelId;

        fn expected_names(expected: &[&str]) -> Vec<(String, Option<ResolvedName>)> {
            expected
                .iter()
                .map(|name| {
                    let index = ["x", "y", "z", "T"]
                        .iter()
                        .position(|candidate| candidate == name)
                        .unwrap();
                    (
                        (*name).into(),
                        Some(ResolvedName::Local(LocalId(100 + index as u32))),
                    )
                })
                .collect()
        }
        fn expression_ok(source: &str, expected: &[&str]) {
            let mut expression = initializer(source);
            let original = expression.clone();
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            Analyzer::resolve_name_expression(&mut expression, &mut context)
                .expect("traverse expression");
            let mut observed = Observed::default();
            observed.expr(&mut expression);
            assert_eq!(observed.names, expected_names(expected));
            assert!(observed.bindings.is_empty());
            assert_eq!(expression, original);
            assert_eq!(context.lexical.depth(), 2);
        }
        macro_rules! expr_ok {
    ($name:ident, $source:literal, [$($expected:literal),*]) => { #[test] fn $name() { expression_ok($source, &[$($expected),*]); } };
}
        macro_rules! expr_bad {
            ($name:ident, $source:literal) => {
                #[test]
                fn $name() {
                    let mut expression = initializer($source);
                    let symbols = empty_symbols();
                    let mut ids = LocalIdAllocator::default();
                    reject(
                        Analyzer::resolve_name_expression(
                            &mut expression,
                            &mut context(&symbols, &mut ids),
                        ),
                        &["Missing"],
                    );
                }
            };
        }
        expr_ok!(literal_leaf, "42", []);
        expr_ok!(binary_children, "x + y", ["x", "y"]);
        expr_ok!(unary_operand, "!x", ["x"]);
        expr_ok!(call_callee_and_arguments, "x(y, z)", ["x", "y", "z"]);
        expr_ok!(zero_argument_call, "x()", ["x"]);
        expr_ok!(builtin_arguments, "@concat(x, y)", ["x", "y"]);
        expr_ok!(member_receiver_only, "x.unknown", ["x"]);
        expr_ok!(nested_member_receivers, "x.first.second", ["x"]);
        expr_ok!(
            struct_owner_and_field_values,
            "T { field: x, other: y }",
            ["T", "x", "y"]
        );
        expr_ok!(empty_struct_constructor, "T {}", ["T"]);
        expr_ok!(cast_value_and_type, "x as T", ["x", "T"]);
        expr_ok!(index_receiver_and_index, "x[y]", ["x", "y"]);
        expr_ok!(slice_receiver_and_bounds, "x[y..z]", ["x", "y", "z"]);
        expr_ok!(slice_inclusive_end, "x[y.=z]", ["x", "y", "z"]);
        expr_ok!(slice_without_start, "x[..z]", ["x", "z"]);
        expr_ok!(slice_without_end, "x[y..]", ["x", "y"]);
        expr_ok!(slice_without_bounds, "x[..]", ["x"]);
        expr_ok!(array_elements, "[x, y, z]", ["x", "y", "z"]);
        expr_ok!(empty_array, "[]", []);
        expr_ok!(type_value_nested_names, "*T", ["T"]);

        expr_bad!(missing_binary_left, "Missing + y");
        expr_bad!(missing_binary_right, "x + Missing");
        expr_bad!(missing_unary_operand, "!Missing");
        expr_bad!(missing_call_callee, "Missing(y, z)");
        expr_bad!(missing_first_call_argument, "x(Missing, z)");
        expr_bad!(missing_last_call_argument, "x(y, Missing)");
        expr_bad!(missing_first_builtin_argument, "@concat(Missing, y)");
        expr_bad!(missing_last_builtin_argument, "@concat(x, Missing)");
        expr_bad!(missing_member_receiver, "Missing.field");
        expr_bad!(missing_struct_owner, "Missing { field: x }");
        expr_bad!(
            missing_first_struct_field_value,
            "T { field: Missing, other: y }"
        );
        expr_bad!(
            missing_last_struct_field_value,
            "T { field: x, other: Missing }"
        );
        expr_bad!(missing_cast_operand, "Missing as T");
        expr_bad!(missing_cast_target, "x as Missing");
        expr_bad!(missing_index_receiver, "Missing[y]");
        expr_bad!(missing_index_value, "x[Missing]");
        expr_bad!(missing_slice_receiver, "Missing[y..z]");
        expr_bad!(missing_slice_start, "x[Missing..z]");
        expr_bad!(missing_slice_end, "x[y..Missing]");
        expr_bad!(missing_first_array_element, "[Missing, y]");
        expr_bad!(missing_last_array_element, "[x, Missing]");
        expr_bad!(missing_type_value_reference, "*Missing");

        fn statement_ok(source: &str, expected: &[&str]) {
            let mut statements = body(source);
            let original = statements.clone();
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            for statement in &mut statements {
                Analyzer::resolve_name_statement(statement, &mut context)
                    .expect("traverse statement");
            }
            let mut observed = Observed::default();
            observed.statements(&mut statements);
            assert_eq!(observed.names, expected_names(expected));
            assert_eq!(statements, original);
            assert_eq!(context.lexical.depth(), 2);
        }
        macro_rules! stmt_ok {
    ($name:ident, $source:literal, [$($expected:literal),*]) => { #[test] fn $name() { statement_ok($source, &[$($expected),*]); } };
}
        macro_rules! stmt_bad {
            ($name:ident, $source:literal) => {
                #[test]
                fn $name() {
                    let mut statements = body($source);
                    let symbols = empty_symbols();
                    let mut ids = LocalIdAllocator::default();
                    let mut context = context(&symbols, &mut ids);
                    // Single container statement per fixture keeps failures at the intended child.
                    assert_eq!(statements.len(), 1);
                    reject(
                        Analyzer::resolve_name_statement(&mut statements[0], &mut context),
                        &["Missing"],
                    );
                }
            };
        }
        stmt_ok!(expression_statement, "x;", ["x"]);
        stmt_ok!(assignment_target_and_value, "x = y;", ["x", "y"]);
        stmt_ok!(index_assignment_children, "x[y] = z;", ["x", "y", "z"]);
        stmt_ok!(
            member_assignment_receiver_and_value,
            "x.field = y;",
            ["x", "y"]
        );
        stmt_ok!(return_values, "return x, y;", ["x", "y"]);
        stmt_ok!(empty_return, "return;", []);
        stmt_ok!(
            if_condition_and_both_branches,
            "if x { y; } else { z; };",
            ["x", "y", "z"]
        );
        stmt_ok!(if_without_else, "if x { y; };", ["x", "y"]);
        stmt_ok!(while_condition_and_body, "while (x) { y; };", ["x", "y"]);
        stmt_ok!(while_without_condition, "while () { x; };", ["x"]);
        stmt_ok!(
            loop_break_and_continue,
            "while () { break; continue; };",
            []
        );
        stmt_ok!(deferred_statement, "defer x(y);", ["x", "y"]);
        stmt_ok!(
            switch_subject_and_bodies,
            "switch (x) { when .Ready { y; } when else { z; } };",
            ["x", "y", "z"]
        );
        stmt_ok!(empty_switch, "switch (x) {};", ["x"]);
        stmt_ok!(empty_branches, "if true {} else {};", []);

        stmt_bad!(missing_assignment_target, "Missing = y;");
        stmt_bad!(missing_assignment_value, "x = Missing;");
        stmt_bad!(missing_index_assignment_receiver, "Missing[y] = z;");
        stmt_bad!(missing_index_assignment_index, "x[Missing] = z;");
        stmt_bad!(missing_index_assignment_value, "x[y] = Missing;");
        stmt_bad!(missing_first_return_value, "return Missing, y;");
        stmt_bad!(missing_last_return_value, "return x, Missing;");
        stmt_bad!(missing_if_condition, "if Missing {};");
        stmt_bad!(missing_then_reference, "if x { Missing; };");
        stmt_bad!(missing_else_reference, "if x {} else { Missing; };");
        stmt_bad!(missing_while_condition, "while (Missing) {};");
        stmt_bad!(missing_while_body_reference, "while (x) { Missing; };");
        stmt_bad!(missing_deferred_reference, "defer Missing();");
        stmt_bad!(missing_switch_subject, "switch (Missing) {};");
        stmt_bad!(
            missing_first_switch_arm_reference,
            "switch (x) { when .Ready { Missing; } when else {} };"
        );
        stmt_bad!(
            missing_last_switch_arm_reference,
            "switch (x) { when .Ready {} when else { Missing; } };"
        );

        #[test]
        fn void_expression_has_no_names_or_scope_effects() {
            let mut expression = Expression::at(ExpressionKind::VoidExpression, Span::new(7, 9));
            let original = expression.clone();
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            Analyzer::resolve_name_expression(&mut expression, &mut context).unwrap();
            assert_eq!(expression, original);
            assert_eq!(context.lexical.depth(), 2);
        }

        #[test]
        fn switch_payloads_have_arm_local_distinct_identities() {
            let analyzer = resolved(&[(
                "main",
                "fn f(subject: int32) { switch (subject) { when .First(payload) { payload.field; } when .Second(payload) { payload.field; } }; }",
            )]);
            let first = local(&analyzer, 0, "payload", 0);
            let second = local(&analyzer, 0, "payload", 1);
            assert_ne!(first, second);
            names(
                &analyzer,
                0,
                &[
                    (
                        "subject",
                        ResolvedName::Local(local(&analyzer, 0, "subject", 0)),
                    ),
                    ("payload", ResolvedName::Local(first)),
                    ("payload", ResolvedName::Local(second)),
                ],
            );
        }
        #[test]
        fn switch_payload_does_not_escape() {
            reject_source(
                "fn f(subject: int32) { switch (subject) { when .Ready(payload) {} }; payload; }",
                &["payload"],
            );
        }
        #[test]
        fn switch_payload_not_visible_in_other_arm() {
            reject_source(
                "fn f(subject: int32) { switch (subject) { when .Ready(payload) {} when else { payload; } }; }",
                &["payload"],
            );
        }
        #[test]
        fn switch_payload_cannot_redeclare_enclosing_local() {
            reject_source(
                "fn f(subject: int32) { payload := 1; switch (subject) { when .Ready(payload) {} }; }",
                &["payload"],
            );
        }
        #[test]
        fn switch_payload_cannot_redeclare_parameter() {
            reject_source(
                "fn f(payload: int32) { switch (payload) { when .Ready(payload) {} }; }",
                &["payload"],
            );
        }

        // These variants are not currently emitted by the parser. Exercise their AST
        // contracts directly rather than introducing unsupported source syntax.
        fn direct_statement(kind: StatementKind, expected: &[&str], bad: bool) {
            let mut statement = Statement {
                kind,
                span: Span::new(4, 2),
            };
            let original = statement.clone();
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            let result = Analyzer::resolve_name_statement(&mut statement, &mut context);
            if bad {
                reject(result, &["Missing"]);
            } else {
                result.unwrap();
                let mut observed = Observed::default();
                observed.statement(&mut statement);
                assert_eq!(observed.names, expected_names(expected));
                assert_eq!(statement, original);
            }
        }
        #[test]
        fn direct_deref_assignment() {
            direct_statement(
                StatementKind::DerefAssign {
                    pointer: named("x"),
                    expr: named("y"),
                },
                &["x", "y"],
                false,
            );
        }
        #[test]
        fn direct_deref_assignment_missing_pointer() {
            direct_statement(
                StatementKind::DerefAssign {
                    pointer: named("Missing"),
                    expr: named("y"),
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_deref_assignment_missing_value() {
            direct_statement(
                StatementKind::DerefAssign {
                    pointer: named("x"),
                    expr: named("Missing"),
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_index_assignment() {
            direct_statement(
                StatementKind::IndexAssign {
                    object: named("x"),
                    index: named("y"),
                    expr: named("z"),
                },
                &["x", "y", "z"],
                false,
            );
        }
        #[test]
        fn direct_index_assignment_missing_object() {
            direct_statement(
                StatementKind::IndexAssign {
                    object: named("Missing"),
                    index: named("y"),
                    expr: named("z"),
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_index_assignment_missing_index() {
            direct_statement(
                StatementKind::IndexAssign {
                    object: named("x"),
                    index: named("Missing"),
                    expr: named("z"),
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_index_assignment_missing_value() {
            direct_statement(
                StatementKind::IndexAssign {
                    object: named("x"),
                    index: named("y"),
                    expr: named("Missing"),
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_multi_assignment() {
            direct_statement(
                StatementKind::MultiAssignment {
                    targets: vec![named("x"), named("y")],
                    values: vec![named("z")],
                },
                &["x", "y", "z"],
                false,
            );
        }
        #[test]
        fn direct_multi_assignment_missing_first_target() {
            direct_statement(
                StatementKind::MultiAssignment {
                    targets: vec![named("Missing"), named("y")],
                    values: vec![named("z")],
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_multi_assignment_missing_last_target() {
            direct_statement(
                StatementKind::MultiAssignment {
                    targets: vec![named("x"), named("Missing")],
                    values: vec![named("z")],
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_multi_assignment_missing_first_value() {
            direct_statement(
                StatementKind::MultiAssignment {
                    targets: vec![named("x")],
                    values: vec![named("Missing"), named("z")],
                },
                &[],
                true,
            );
        }
        #[test]
        fn direct_multi_assignment_missing_last_value() {
            direct_statement(
                StatementKind::MultiAssignment {
                    targets: vec![named("x")],
                    values: vec![named("y"), named("Missing")],
                },
                &[],
                true,
            );
        }

        #[test]
        fn labels_lookup_nearest_and_restore_outer_identity_on_exit() {
            let mut table = SymbolTable::new();
            table.insert_label("outer".into(), LabelId(1)).unwrap();
            table.enter_scope(ScopeKind::Block);
            assert_eq!(table.resolve_label("outer"), Some(LabelId(1)));
            table.insert_label("outer".into(), LabelId(2)).unwrap();
            reject(
                table.insert_label("outer".into(), LabelId(3)),
                &["duplicate", "outer"],
            );
            assert_eq!(table.resolve_label("outer"), Some(LabelId(2)));
            table.insert_label("inner".into(), LabelId(4)).unwrap();
            table.exit_scope().unwrap();
            assert_eq!(table.resolve_label("outer"), Some(LabelId(1)));
            assert_eq!(table.resolve_label("inner"), None);
        }
        #[test]
        fn label_lookup_cannot_resolve_value_bindings_and_vice_versa() {
            let mut table = SymbolTable::new();
            table.insert_label("label".into(), LabelId(1)).unwrap();
            table
                .insert_binding("value".into(), LocalBinding::Local(LocalId(1)))
                .unwrap();
            assert_eq!(table.resolve_lexical("label"), None);
            assert_eq!(table.resolve_label("value"), None);
        }

        #[test]
        fn failed_block_traversal_restores_scope_and_discards_bindings() {
            let mut statements = body("if true { temporary := 1; Missing; };");
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            reject(
                Analyzer::resolve_name_statement(&mut statements[0], &mut context),
                &["Missing"],
            );
            assert_eq!(context.lexical.depth(), 2);
            assert_eq!(context.lexical.resolve_lexical("temporary"), None);
        }

        #[test]
        fn failed_function_signature_restores_function_scope() {
            let mut ast = parse("fn f(p: int32, p: int32) {}");
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            reject(ast.resolve_name(&mut context), &["p"]);
            assert_eq!(context.lexical.depth(), 2);
            assert_eq!(context.lexical.resolve_lexical("p"), None);
        }

        #[test]
        fn unsupported_nodes_return_errors_without_panics() {
            let mut ast = parse("fn f() {}");
            let StatementKind::FunctionDeclaration(fun) = &mut ast.statements[0].kind else {
                unreachable!()
            };
            fun.signature.generic_parameters.push("T".into());
            let symbols = empty_symbols();
            let mut ids = LocalIdAllocator::default();
            let mut context = context(&symbols, &mut ids);
            reject(ast.resolve_name(&mut context), &["generic"]);
            assert_eq!(context.lexical.depth(), 2);
            let mut expr = Expression::at(
                ExpressionKind::QualifiedAccess {
                    segments: vec!["x".into()],
                },
                Span::new(1, 1),
            );
            reject(
                Analyzer::resolve_name_expression(&mut expr, &mut context),
                &["legacy"],
            );
            let mut statement = parse("fn nested() {}").statements.remove(0);
            reject(
                Analyzer::resolve_name_statement(&mut statement, &mut context),
                &["nested"],
            );
        }
    }

    fn parse(source: &str) -> Ast {
        Parser::with_source(Tokenizer::new(source), Path::new("resolution.fib"), source)
            .parse()
            .unwrap_or_else(|error| panic!("invalid test fixture:\n{source}\n{error:#}"))
    }

    fn inputs(files: &[(&str, &str)]) -> (BuildPlan, Vec<CompilationUnit>) {
        let plan = BuildPlan {
            files: files
                .iter()
                .enumerate()
                .map(|(i, (name, _))| SourceFile {
                    id: FileId(i as u32),
                    name: (*name).to_owned().into(),
                    path: format!("{name}.fib").into(),
                })
                .collect(),
            entry: FileId(0),
            output: "unused-output".into(),
        };
        let units = files
            .iter()
            .enumerate()
            .map(|(i, (_, source))| CompilationUnit {
                source_file_id: FileId(i as u32),
                ast: parse(source),
            })
            .collect();
        (plan, units)
    }

    fn collected(files: &[(&str, &str)]) -> Analyzer {
        let (plan, units) = inputs(files);
        let mut analyzer = Analyzer::new(plan, units).expect("valid named units");
        analyzer
            .collect_declarations()
            .expect("declaration collection");
        analyzer
    }

    fn resolved(files: &[(&str, &str)]) -> Analyzer {
        let mut analyzer = collected(files);
        let original = analyzer.units.clone();
        analyzer
            .resolve_names()
            .expect("name resolution must succeed");
        for (unit, before) in analyzer.units.iter().zip(original) {
            assert_source_preserved(&before.ast, &unit.ast);
        }
        analyzer
    }

    fn reject<T>(result: anyhow::Result<T>, fragments: &[&str]) {
        let error = result.err().expect("expected a resolution error");
        let message = format!("{error:#}").to_lowercase();
        assert!(
            !message.contains("not implemented"),
            "stub error is not a semantic rejection: {message}"
        );
        for fragment in fragments {
            assert!(
                message.contains(&fragment.to_lowercase()),
                "expected {fragment:?} in {message:?}"
            );
        }
    }

    fn reject_source(source: &str, fragments: &[&str]) {
        let mut analyzer = collected(&[("main", source)]);
        reject(analyzer.resolve_names(), fragments);
    }

    fn decl(file: u32, index: u32) -> DeclId {
        DeclId {
            file_id: FileId(file),
            index,
        }
    }
    fn target(file: u32, index: u32) -> ResolvedName {
        ResolvedName::Entity(Entity::Declaration(decl(file, index)))
    }

    #[derive(Default, Debug)]
    struct Observed {
        names: Vec<(String, Option<ResolvedName>)>,
        bindings: Vec<(String, Option<BindingId>)>,
    }

    // This visitor inspects and clears only name-pass annotations on a cloned AST.
    // Comparing that clone to its input also checks spelling/spans and ensures the
    // name pass did not fill type, expression, or member annotations.
    impl Observed {
        fn name(&mut self, name: &mut QualifiedIdentifier) {
            self.names
                .push((name.segments.join("::"), name.annotation.resolved));
            name.annotation = NameAnnotation::default();
        }
        fn binding(&mut self, name: &str, annotation: &mut BindingAnnotation) {
            self.bindings.push((name.into(), annotation.binding));
            *annotation = BindingAnnotation::default();
        }
        fn variable(&mut self, var: &mut VariableDeclaration) {
            self.binding(&var.identifier, &mut var.annotation);
            if let Some(ty) = &mut var.ty {
                self.ty(ty);
            }
            if let Some(expr) = &mut var.expression {
                self.expr(expr);
            }
        }
        fn ty(&mut self, ty: &mut TypeExpression) {
            match &mut ty.kind {
                TypeExpressionKind::Builtin(_) => {}
                TypeExpressionKind::QualifiedIdentifier(name) => self.name(name),
                TypeExpressionKind::Function {
                    argument_types,
                    return_type,
                } => {
                    for ty in argument_types {
                        self.ty(ty);
                    }
                    self.ty(return_type);
                }
                TypeExpressionKind::Tuple { elements } => {
                    for ty in elements {
                        self.ty(ty);
                    }
                }
                TypeExpressionKind::Pointer { pointed_type } => self.ty(pointed_type),
                TypeExpressionKind::Array { element_type, .. }
                | TypeExpressionKind::Slice { element_type } => self.ty(element_type),
                TypeExpressionKind::Struct { fields } => {
                    for field in fields {
                        self.expr(&mut field.expression);
                    }
                }
                TypeExpressionKind::Enum { variants } => {
                    for variant in variants {
                        if let Some(fields) = &mut variant.payload {
                            for field in fields {
                                self.expr(&mut field.expression);
                            }
                        }
                    }
                }
            }
        }
        fn expr(&mut self, expr: &mut Expression) {
            match &mut expr.kind {
                ExpressionKind::QualifiedIdentifier(name) => self.name(name),
                ExpressionKind::BinaryExpresison(binary) => {
                    self.expr(&mut binary.left);
                    self.expr(&mut binary.right);
                }
                ExpressionKind::UnaryExpression(unary) => self.expr(&mut unary.expression),
                ExpressionKind::Call { callee, args } => {
                    self.expr(callee);
                    for arg in args {
                        self.expr(arg);
                    }
                }
                ExpressionKind::CallToBuiltin { args, .. } => {
                    for arg in args {
                        self.expr(arg);
                    }
                }
                ExpressionKind::ExprAcess { expr, .. } => self.expr(expr),
                ExpressionKind::StructConstruct {
                    struct_identifier,
                    fields,
                } => {
                    self.name(struct_identifier);
                    for field in fields {
                        self.expr(&mut field.expression);
                    }
                }
                ExpressionKind::Cast { expr, target_type } => {
                    self.expr(expr);
                    self.ty(target_type);
                }
                ExpressionKind::IndexAccess { object, index } => {
                    self.expr(object);
                    self.expr(index);
                }
                ExpressionKind::SliceLiteral {
                    object, start, end, ..
                } => {
                    self.expr(object);
                    if let Some(start) = start {
                        self.expr(start);
                    }
                    if let Some(end) = end {
                        self.expr(end);
                    }
                }
                ExpressionKind::ArrayLiteral { elements } => {
                    for element in elements {
                        self.expr(element);
                    }
                }
                ExpressionKind::TypeValue(ty) => self.ty(ty),
                ExpressionKind::Literal(_)
                | ExpressionKind::VoidExpression
                | ExpressionKind::QualifiedAccess { .. } => {}
            }
        }
        fn statements(&mut self, statements: &mut [Statement]) {
            for stmt in statements {
                self.statement(stmt);
            }
        }
        fn statement(&mut self, stmt: &mut Statement) {
            match &mut stmt.kind {
                StatementKind::FunctionDeclaration(fun) => {
                    let sig = &mut fun.signature;
                    self.binding(&sig.identifier, &mut sig.annotation);
                    for param in &mut sig.parameters {
                        self.binding(&param.parameter_identifier, &mut param.annotation);
                        self.ty(&mut param.parameter_type);
                    }
                    self.ty(&mut sig.return_type);
                    if let Some(body) = &mut fun.body {
                        self.statements(&mut body.statements);
                    }
                }
                StatementKind::VariableDeclaration(var) => self.variable(var),
                StatementKind::MultiVariableDeclaration { bindings, values } => {
                    for binding in bindings {
                        self.variable(binding);
                    }
                    for value in values {
                        self.expr(value);
                    }
                }
                StatementKind::ExpressionStatement(expr) => self.expr(expr),
                StatementKind::Assignment { left, right } => {
                    self.expr(left);
                    self.expr(right);
                }
                StatementKind::DerefAssign { pointer, expr } => {
                    self.expr(pointer);
                    self.expr(expr);
                }
                StatementKind::IndexAssign {
                    object,
                    index,
                    expr,
                } => {
                    self.expr(object);
                    self.expr(index);
                    self.expr(expr);
                }
                StatementKind::Return(values) => {
                    if let Some(values) = values {
                        for value in values {
                            self.expr(value);
                        }
                    }
                }
                StatementKind::MultiAssignment { targets, values } => {
                    for expr in targets.iter_mut().chain(values) {
                        self.expr(expr);
                    }
                }
                StatementKind::If {
                    condition,
                    then_branch,
                    else_branch,
                } => {
                    self.expr(condition);
                    self.statements(then_branch);
                    if let Some(branch) = else_branch {
                        self.statements(branch);
                    }
                }
                StatementKind::While { condition, body } => {
                    if let Some(condition) = condition {
                        self.expr(condition);
                    }
                    self.statements(body);
                }
                StatementKind::Defer(stmt) => self.statement(stmt),
                StatementKind::Switch { subject, arms } => {
                    self.expr(subject);
                    for arm in arms {
                        if let Pattern::EnumVariant {
                            binding: Some(binding),
                            ..
                        } = &mut arm.pattern
                        {
                            self.binding(&binding.identifier, &mut binding.annotation);
                        }
                        self.statements(&mut arm.body);
                    }
                }
                StatementKind::Break | StatementKind::Continue => {}
            }
        }
    }

    fn observe(ast: &Ast) -> Observed {
        let mut observed = Observed::default();
        observed.statements(&mut ast.clone().statements);
        observed
    }
    fn assert_source_preserved(before: &Ast, after: &Ast) {
        let mut cleared = after.clone();
        Observed::default().statements(&mut cleared.statements);
        let mut before = before.clone();
        Observed::default().statements(&mut before.statements);
        assert_eq!(
            cleared, before,
            "name resolution changed source or later-pass annotations"
        );
    }
    fn names(analyzer: &Analyzer, file: usize, expected: &[(&str, ResolvedName)]) {
        assert_eq!(
            observe(&analyzer.units[file].ast).names,
            expected
                .iter()
                .map(|(name, id)| ((*name).into(), Some(*id)))
                .collect::<Vec<_>>()
        );
    }
    fn local(analyzer: &Analyzer, file: usize, spelling: &str, occurrence: usize) -> LocalId {
        let bindings = observe(&analyzer.units[file].ast).bindings;
        let (_, binding) = bindings
            .iter()
            .filter(|(name, _)| name == spelling)
            .nth(occurrence)
            .expect("binding site exists");
        match binding {
            Some(BindingId::Local(id)) => *id,
            other => panic!("expected annotated local {spelling}: {other:?}"),
        }
    }
    fn initializer(source: &str) -> Expression {
        let ast = parse(&format!("probe := {source};"));
        let StatementKind::VariableDeclaration(var) = &ast.statements[0].kind else {
            panic!("expected variable")
        };
        var.expression.clone().unwrap()
    }
    fn body(source: &str) -> Vec<Statement> {
        let ast = parse(&format!("fn fixture() {{ {source} }}"));
        let StatementKind::FunctionDeclaration(fun) = &ast.statements[0].kind else {
            panic!("expected function")
        };
        fun.body.as_ref().unwrap().statements.clone()
    }
    fn context<'a>(
        symbols: &'a ProjectSymbols,
        ids: &'a mut LocalIdAllocator,
    ) -> ResolveContext<'a> {
        let mut lexical = SymbolTable::new();
        lexical.enter_scope(ScopeKind::Function);
        for (i, name) in ["x", "y", "z", "T"].iter().enumerate() {
            lexical
                .insert_binding((*name).into(), LocalBinding::Local(LocalId(100 + i as u32)))
                .unwrap();
        }
        ResolveContext {
            file_id: FileId(0),
            lexical,
            symbols,
            local_ids: ids,
        }
    }
    fn empty_symbols() -> ProjectSymbols {
        ProjectSymbols {
            files_by_name: HashMap::new(),
            namespaces: HashMap::new(),
        }
    }
    fn named(name: &str) -> Expression {
        Expression::at(
            ExpressionKind::QualifiedIdentifier(QualifiedIdentifier::new(vec![name.into()])),
            Span::new(1, 1),
        )
    }
}
