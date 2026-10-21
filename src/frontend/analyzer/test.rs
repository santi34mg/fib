//! Utility and unfinished typing contracts. Entity-focused name-resolution
//! specifications are grouped in the inline `name_resolution` module below.

use std::collections::HashMap;

use crate::{
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
    plan::FileId,
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
fn complete_analysis_validates_an_empty_type_pass() {
    let analyzer = Analyzer {
        units: Vec::new(),
        symbols: symbols(),
        local_ids: LocalIdAllocator::default(),
        paths: HashMap::new(),
        synthetic_modules: Vec::new(),
        builtins: HashMap::new(),
        phase: super::NamePhase::New,
    };
    analyzer.analyze().unwrap();

    let registry = symbols();
    let mut units = Vec::new();
    let mut types = SemanticTypes::default();
    resolve_types(&mut units, &registry, &mut types).unwrap();
    assert!(resolve_types(&mut units, &registry, &mut types).is_err());
    validate_analysis(&AnalyzedProject {
        units,
        symbols: registry,
        types,
        builtins: HashMap::new(),
        source_paths: HashMap::new(),
    })
    .unwrap();
}

#[test]
fn synthetic_builtins_resolve_type_and_survive_analysis_only_when_configured() {
    use crate::{
        builtins::{BuiltinFunction, SyntheticModule},
        driver::CompilationUnit,
        frontend::{
            analysis_annotation::BindingId, parser::Parser, semantic_type::SemanticType,
            tokenizer::Tokenizer, tokens::builtin::BuiltinType,
        },
        plan::{BuildPlan, FileName, SourceFile},
    };
    use std::path::Path;

    fn input(source: &str) -> (BuildPlan, Vec<CompilationUnit>) {
        let ast = Parser::with_source(Tokenizer::new(source), Path::new("build.fib"), source)
            .parse()
            .unwrap();
        (
            BuildPlan {
                files: vec![SourceFile {
                    id: FileId(0),
                    name: FileName("build".into()),
                    path: "build.fib".into(),
                }],
                entry: FileId(0),
                output: "unused".into(),
            },
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )
    }

    let source = "fn configure() { build_api::add_source(\"x\"); }";
    let (plan, units) = input(source);
    let normal_error = Analyzer::new(plan, units).unwrap().analyze().unwrap_err();
    assert!(format!("{normal_error:#}").contains("add_source"));

    let (plan, units) = input(source);
    let project = Analyzer::new(plan, units)
        .unwrap()
        .with_synthetic_module(SyntheticModule::BUILD_API)
        .unwrap()
        .analyze()
        .unwrap();
    let (&id, _) = project
        .builtins
        .iter()
        .find(|(_, builtin)| **builtin == BuiltinFunction::AddSource)
        .expect("add_source builtin identity");
    let info = project.types.bindings[&BindingId::Declaration(id)];
    let SemanticType::Function(signature) = project.types.get(info.ty).unwrap() else {
        panic!("builtin must have a function type")
    };
    assert_eq!(signature.parameters.len(), 1);
    assert!(matches!(
        project.types.get(signature.parameters[0]).unwrap(),
        SemanticType::Builtin(BuiltinType::String)
    ));
    assert!(matches!(
        project.types.get(signature.return_type).unwrap(),
        SemanticType::Builtin(BuiltinType::Void)
    ));
    assert_eq!(project.builtins.len(), BuiltinFunction::ALL.len());
}

#[test]
fn synthetic_builtin_namespace_does_not_collide_with_source_names() {
    use crate::{
        builtins::{BuiltinFunction, SyntheticModule},
        driver::CompilationUnit,
        frontend::{parser::Parser, tokenizer::Tokenizer},
        plan::{BuildPlan, FileName, SourceFile},
    };
    use std::path::Path;

    let source = "fn add_source() {} fn configure() { build_api::add_source(\"x\"); }";
    let ast = Parser::with_source(Tokenizer::new(source), Path::new("build.fib"), source)
        .parse()
        .unwrap();
    let plan = BuildPlan {
        files: vec![SourceFile {
            id: FileId(0),
            name: FileName("build".into()),
            path: "build.fib".into(),
        }],
        entry: FileId(0),
        output: "unused".into(),
    };
    let project = Analyzer::new(
        plan,
        vec![CompilationUnit {
            source_file_id: FileId(0),
            ast,
        }],
    )
    .unwrap()
    .with_synthetic_module(SyntheticModule::BUILD_API)
    .unwrap()
    .analyze()
    .unwrap();
    assert_eq!(project.builtins.len(), BuiltinFunction::ALL.len());
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
fn standalone_typing_and_invalid_generic_keys_are_checked() {
    let registry = symbols();
    let mut types = SemanticTypes::default();
    let mut context = TypeContext::new(FileId(0), &registry, &mut types);
    Ast::new().resolve_type(&mut context).unwrap();
    let mut expression = Expression::at(
        ExpressionKind::VoidExpression,
        crate::diagnostics::Span::new(1, 1),
    );
    let unit = Analyzer::resolve_type_expression(&mut expression, &mut context, None).unwrap();
    assert_eq!(expression.annotation.resolved, Some(unit));
    let key = GenericInstanceKey {
        template: DeclId {
            file_id: FileId(0),
            index: 0,
        },
        arguments: vec![TypeId(0)],
    };
    assert!(resolve_generic_instance(&key, &mut context).is_err());
    assert!(resolve_member(unit.ty, "field", context.types).is_err());
    check_assignable(unit.ty, unit.ty, context.types).unwrap();
    assert!(
        resolve_multi_value_types(0, &[], context.types)
            .unwrap()
            .is_empty()
    );
    validate_assignment_target(&ExpressionInfo {
        ty: TypeId(0),
        category: ValueCategory::MutablePlace,
        type_value: None,
    })
    .unwrap();
    assert!(context.types.declarations.is_empty());
    assert!(context.types.locals.is_empty());
}

mod name_resolution {
    //! Executable specification of name resolution and its explicit semantic boundary.
    //! Run one entity phase with `cargo test --no-default-features name_resolution::phase_01`.

    use std::{collections::HashMap, path::Path};

    use super::{Analyzer, LocalIdAllocator, ResolveContext};
    use crate::{
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
        plan::{BuildPlan, FileId, SourceFile},
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
                    let index = ["x", "y", "z", "T", "concat"]
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
        expr_ok!(
            ordinary_concat_arguments,
            "concat(x, y)",
            ["concat", "x", "y"]
        );
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
        expr_bad!(missing_first_ordinary_argument, "x(Missing, y)");
        expr_bad!(missing_last_ordinary_argument, "x(y, Missing)");
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
                    ..
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
                TypeExpressionKind::Readonly(inner) => self.ty(inner),
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
        for (i, name) in ["x", "y", "z", "T", "concat"].iter().enumerate() {
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

/// Acceptance tests are written against the original AST and public pass seam.
mod type_resolution {
    use super::*;
    use crate::{
        diagnostics::{CompilerError, ErrorKind},
        driver::CompilationUnit,
        frontend::{
            ast::statement::StatementKind, parser::Parser, semantic_type::SemanticType,
            tokenizer::Tokenizer, tokens::builtin::BuiltinType,
        },
        plan::{BuildPlan, SourceFile},
    };
    use std::path::Path;

    fn analyzer(source: &str) -> Analyzer {
        let ast = Parser::with_source(Tokenizer::new(source), Path::new("types.fib"), source)
            .parse()
            .unwrap_or_else(|e| panic!("fixture did not parse: {source}\n{e:#}"));
        Analyzer::new(
            BuildPlan {
                files: vec![SourceFile {
                    id: FileId(0),
                    name: "main".to_string().into(),
                    path: "types.fib".into(),
                }],
                entry: FileId(0),
                output: "unused".into(),
            },
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )
        .unwrap()
    }
    fn run(source: &str) -> anyhow::Result<(Analyzer, SemanticTypes)> {
        let mut a = analyzer(source);
        a.collect_declarations()?;
        a.resolve_names()?;
        let types = a.resolve_types()?;
        Ok((a, types))
    }
    fn good(source: &str) {
        run(source).unwrap_or_else(|e| panic!("expected successful typing: {source}\n{e:#}"));
    }
    fn bad(source: &str, fragment: &str) {
        let e = run(source).err().expect("invalid program accepted");
        let d = e
            .downcast_ref::<CompilerError>()
            .unwrap_or_else(|| panic!("expected structured type diagnostic, got {e:#}"));
        assert_eq!(d.kind, ErrorKind::Type, "{e:#}");
        assert!(d.span.is_some(), "missing location: {e:#}");
        assert!(d.message.contains(fragment), "expected {fragment:?}: {e:#}");
        assert!(
            !d.message.contains("not implemented"),
            "stub is not a semantic diagnostic"
        );
    }
    fn analysis_bad(source: &str, fragment: &str) {
        let e = analyzer(source)
            .analyze()
            .expect_err("invalid analysis accepted");
        let d = e
            .downcast_ref::<CompilerError>()
            .expect("structured diagnostic");
        assert_eq!(d.kind, ErrorKind::Type);
        assert!(d.span.is_some(), "{e:#}");
        assert!(d.message.contains(fragment), "expected {fragment}: {e:#}");
    }
    macro_rules! ok {
        ($name:ident, $source:expr) => {
            #[test]
            fn $name() {
                good($source);
            }
        };
    }
    macro_rules! err {
        ($name:ident, $source:expr, $fragment:expr) => {
            #[test]
            fn $name() {
                bad($source, $fragment);
            }
        };
    }

    ok!(
        inference_and_context,
        "small: int8 = 127; negative: int8 = -128; count := 1; fraction := 1.5; flag := true;"
    );
    ok!(
        recursive_pointer,
        "comp Node: type = struct { next: *Node }; node: Node;"
    );
    ok!(
        recursive_slice,
        "comp Node: type = struct { children: Node[] }; node: Node;"
    );
    ok!(
        recursive_function_type,
        "comp Node: type = struct { visit: fn(Node) -> Node }; node: Node;"
    );
    ok!(
        unit_storage,
        "unit: void; comp Box: type = struct { unit: void }; box: Box; fn f(x: void) void { return; }"
    );
    ok!(
        forward_calls,
        "answer := add(1, 2); fn add(a: int32, b: int32) int32 { return a + b; }"
    );
    ok!(
        function_values,
        "fn f(x: int32) int32 { return x; } callable := f; result := callable(2);"
    );
    ok!(recursive_calls, "fn f(x: int32) int32 { return f(x); }");
    ok!(
        tuple_results,
        "fn pair() (int32, string) { return 1, \"x\"; } a, b := pair();"
    );
    ok!(
        independent_multi_binding,
        "const a: int8, b: string = 1, \"x\";"
    );
    ok!(
        numeric_casts,
        "small: int8 = 1; wide := small as int64; fraction := wide as float64;"
    );
    ok!(
        pointer_operations,
        "fn f() { x := 1; p := x&; n := p*; opaque := p as *void; back := opaque as *int32; same := back == null; }"
    );
    ok!(
        const_pointer_pointee,
        "comp S: type = struct { value: int32 }; fn f() { value: S; const p := value&; item := (p*).value; }"
    );
    ok!(
        arrays_and_slices,
        "items: int8[2] = [1, 2]; first := items[0]; view := items[..]; size := view.len;"
    );
    ok!(
        empty_array_context,
        "empty: int32[0] = []; view := empty[..];"
    );
    ok!(
        struct_construction,
        "comp P: type = struct { x: int8, y: int32 }; p := P { y: 2, x: 1 }; value := p.x;"
    );
    ok!(
        enum_constructors,
        "comp E: type = enum { Empty, Number { value: int32 }, Unit {} }; a := E.Empty; b := E.Number(7); c := E.Unit();"
    );
    ok!(
        payload_pattern,
        "comp E: type = enum { Number { value: int32 }, Empty }; fn f(e: E) { switch (e) { when .Number(p) { x := p.value; } when else {} }; }"
    );
    ok!(
        real_len_field,
        "comp S: type = struct { len: bool }; s := S { len: true }; result: bool = s.len;"
    );
    ok!(
        type_parameters_and_returns,
        "fn identity(T: type, x: T) T { return x; } fn choose(T: type) type { return T; }"
    );
    ok!(
        ordinary_former_builtins,
        "fn concat(a: int32, b: int32) int32 { return a + b; } fn str_len(x: bool) bool { return x; } fn str_eq() int32 { return 3; } n := concat(1, str_eq()); flag := str_len(true);"
    );

    err!(integer_overflow, "x: int8 = 128;", "range");
    err!(negative_overflow, "x: int8 = -129;", "range");
    err!(
        typed_integer_mismatch,
        "a: int32 = 1; b: int8 = a;",
        "incompatible"
    );
    err!(value_in_type_position, "value := 1; x: value;", "type");
    err!(
        direct_size_cycle,
        "comp Node: type = struct { next: Node };",
        "cycle"
    );
    err!(
        zero_array_cycle,
        "comp Node: type = struct { next: Node[0] };",
        "cycle"
    );
    err!(
        nominal_mismatch,
        "comp A: type = struct { x: int32 }; comp B: type = struct { x: int32 }; a: A; b: B = a;",
        "incompatible"
    );
    err!(
        duplicate_field,
        "comp A: type = struct { x: int32, x: int32 };",
        "duplicate"
    );
    err!(
        duplicate_variant,
        "comp E: type = enum { X, X };",
        "duplicate"
    );
    err!(never_storage, "x: never;", "never");
    err!(null_without_context, "x := null;", "context");
    err!(empty_without_context, "x := [];", "context");
    err!(bad_arithmetic, "x := true + 1;", "operand");
    err!(
        mixed_numbers,
        "a: int8 = 1; b: int32 = 2; c := a + b;",
        "incompatible"
    );
    err!(division_zero, "x := 1 / 0;", "zero");
    err!(shift_range, "x := 1 << 32;", "shift");
    err!(constant_overflow, "x: int8 = 127 + 1;", "range");
    err!(
        immutable_write,
        "fn f() { const x := 1; x = 2; }",
        "immutable"
    );
    ok!(
        immutable_address_is_readonly,
        "fn f() { const x := 1; p: const *int32 = x&; }"
    );
    err!(opaque_dereference, "p: *void; x := p*;", "opaque");
    err!(pointer_index, "p: *int32; x := p[0];", "index");
    err!(pointer_cast, "p: *int32; x := p as *bool;", "cast");
    err!(pointer_integer_cast, "p: *int32; x := p as usize;", "cast");
    err!(call_non_function, "x := 1; y := x();", "callable");
    err!(call_arity, "fn f(x: int32) {} x := f();", "argument");
    err!(
        call_argument,
        "fn f(x: int32) {} x := f(true);",
        "incompatible"
    );
    err!(
        return_type,
        "fn f() int32 { return false; }",
        "incompatible"
    );
    err!(return_arity, "fn f() int32 { return 1, 2; }", "return");
    err!(binding_arity, "a, b := 1;", "arity");
    err!(
        missing_member,
        "comp S: type = struct { x: int32 }; s: S; x := s.missing;",
        "member"
    );
    err!(invalid_len, "x := 1; n := x.len;", "member");
    err!(
        missing_field,
        "comp S: type = struct { x: int32 }; s := S {};",
        "missing"
    );
    err!(
        duplicate_initializer,
        "comp S: type = struct { x: int32 }; s := S { x: 1, x: 2 };",
        "duplicate"
    );
    err!(
        unknown_initializer,
        "comp S: type = struct { x: int32 }; s := S { other: 1 };",
        "member"
    );
    err!(array_size, "items: int32[2] = [1];", "length");
    err!(heterogeneous_array, "items := [1, true];", "incompatible");
    err!(index_bounds, "items := [1, 2]; x := items[2];", "bounds");
    err!(negative_index, "items := [1, 2]; x := items[-1];", "bounds");
    err!(
        reversed_slice,
        "items := [1, 2]; x := items[1..0];",
        "bounds"
    );
    err!(
        inclusive_bounds,
        "items := [1, 2]; x := items[0.=2];",
        "bounds"
    );
    err!(condition_boolean, "fn f() { if 1 {} ; }", "incompatible");

    #[test]
    fn inferred_types_and_member_identity_are_observable() {
        let (a, t) =
            run("comp S: type = struct { x: int32 }; s: S; x := s.x; n := \"hi\".len;").unwrap();
        let StatementKind::VariableDeclaration(x) = &a.units[0].ast.statements[2].kind else {
            panic!()
        };
        let expr = x.expression.as_ref().unwrap();
        let ExpressionKind::ExprAcess { member, .. } = &expr.kind else {
            panic!()
        };
        let info = expr.annotation.resolved.unwrap();
        assert_eq!(
            t.types[info.ty.0 as usize],
            SemanticType::Builtin(BuiltinType::Int32)
        );
        assert_eq!(member.resolved.unwrap().ty, info.ty);
        let StatementKind::VariableDeclaration(n) = &a.units[0].ast.statements[3].kind else {
            panic!()
        };
        assert_eq!(
            t.types[n
                .expression
                .as_ref()
                .unwrap()
                .annotation
                .resolved
                .unwrap()
                .ty
                .0 as usize],
            SemanticType::Builtin(BuiltinType::Usize)
        );
    }

    ok!(
        mutual_recursion,
        "comp A: type = struct { other: *B }; comp B: type = struct { other: *A }; a: A;"
    );
    ok!(
        struct_alias_identity,
        "comp A: type = struct { x: int32 }; comp Alias := A; a: A; b: Alias = a;"
    );
    ok!(
        local_type_and_alias,
        "fn f() { comp A: type = struct { x: int32 }; comp Alias := A; a: Alias; x := a.x; }"
    );
    ok!(
        nested_mutable_field,
        "comp A: type = struct { x: int32 }; fn f() { a: A; a.x = 2; }"
    );
    ok!(
        slice_pointee_mutability,
        "fn f() { items := [1, 2]; const view := items[..]; view[0] = 3; }"
    );
    ok!(context_from_right_operand, "a: int8 = 1; b := 2 + a;");
    ok!(contextual_null_comparison, "p: *int32; yes := null == p;");
    ok!(unit_equality, "a: void; b: void; same := a == b;");
    ok!(
        unsigned_128_constant,
        "x: uint128 = 18446744073709551615 * 18446744073709551615;"
    );
    ok!(
        typed_forward_multi_binding,
        "value := later; later: int8, other: int32 = 1, 2;"
    );
    ok!(
        forward_tuple_binding,
        "value := first; first, second := pair(); fn pair() (int32, int32) { return 1, 2; }"
    );
    err!(
        immutable_field,
        "comp A: type = struct { x: int32 }; fn f() { const a: A = A { x: 1 }; a.x = 2; }",
        "immutable"
    );
    ok!(
        const_binding_mutable_array_elements,
        "fn f() { const a := [1]; a[0] = 2; }"
    );
    err!(
        constant_float_overflow,
        "x: float16 = 65504.0 + 65504.0;",
        "range"
    );
    err!(negative_float_cast_range, "x := -300.0 as int8;", "range");
    #[test]
    fn nonfinite_float_arithmetic() {
        let large = format!("1{}.0", "0".repeat(308));
        bad(&format!("x := {large} * {large};"), "range");
    }
    err!(
        aggregate_equality,
        "comp A: type = struct { x: int32 }; a: A; b: A; equal := a == b;",
        "operand"
    );
    err!(
        wrong_variant_receiver,
        "comp E: type = enum { A }; e: E; x := e.A;",
        "type value"
    );
    err!(
        field_on_metatype,
        "comp A: type = struct { x: int32 }; x := A.x;",
        "receiver"
    );
    err!(
        array_to_slice_cast,
        "a := [1]; view := a as int32[];",
        "cast"
    );
    err!(bool_numeric_cast, "x := true as int32;", "cast");
    err!(
        bad_payload_argument,
        "comp E: type = enum { N { value: int32 } }; x := E.N(true);",
        "incompatible"
    );
    err!(
        payloadless_binding,
        "comp E: type = enum { Empty }; fn f(e: E) { switch (e) { when .Empty(p) {} }; }",
        "payload"
    );
    err!(
        missing_pattern_variant,
        "comp E: type = enum { Empty }; fn f(e: E) { switch (e) { when .Missing {} }; }",
        "member"
    );
    err!(bad_index_type, "a := [1]; x := a[true];", "integer");

    mod independent_mutability {
        use super::*;

        fn pass(source: &str) {
            analyzer(source)
                .analyze()
                .unwrap_or_else(|e| panic!("{source}\n{e:#}"));
        }

        #[test]
        fn pointer_binding_and_pointee_matrix() {
            for binding in ["", "const "] {
                pass(&format!(
                    "fn f() {{ x := 1; {binding}a: *int32 = x&; (a*) = 2; }}"
                ));
                pass(&format!(
                    "fn f() {{ const x := 1; {binding}a: const *int32 = x&; y := a*; }}"
                ));
                bad(
                    &format!("fn f() {{ const x := 1; {binding}a: const *int32 = x&; (a*) = 2; }}"),
                    "immutable",
                );
            }
            pass("fn f() { const x := 1; const y := 2; a: const *int32 = x&; a = y&; }");
            bad(
                "fn f() { x := 1; const a: *int32 = x&; a = x&; }",
                "immutable",
            );
            bad(
                "fn f() { const x := 1; const a: const *int32 = x&; a = x&; }",
                "immutable",
            );
        }

        #[test]
        fn array_binding_and_element_matrix() {
            for binding in ["", "const "] {
                pass(&format!(
                    "fn f() {{ {binding}a: int32[2] = [1, 2]; a[0] = 3; v := a[..]; v[0] = 4; }}"
                ));
                pass(&format!(
                    "fn f() {{ {binding}a: const int32[2] = [1, 2]; v: const int32[] = a[..]; x := v[0]; n := a.len; }}"
                ));
                bad(
                    &format!("fn f() {{ {binding}a: const int32[2] = [1, 2]; a[0] = 3; }}"),
                    "immutable",
                );
            }
            pass("fn f() { a: const int32[1] = [1]; a = [2]; }");
            bad(
                "fn f() { const a: const int32[1] = [1]; a = [2]; }",
                "immutable",
            );
            bad("fn f() { const a: int32[1] = [1]; a = [2]; }", "immutable");
        }

        #[test]
        fn slice_binding_and_element_matrix() {
            for binding in ["", "const "] {
                pass(&format!(
                    "fn f() {{ a := [1]; {binding}v: int32[] = a[..]; v[0] = 2; }}"
                ));
                pass(&format!(
                    "fn f() {{ a: const int32[1] = [1]; {binding}v: const int32[] = a[..]; x := v[0]; n := v.len; }}"
                ));
                bad(
                    &format!(
                        "fn f() {{ a: const int32[1] = [1]; {binding}v: const int32[] = a[..]; v[0] = 2; }}"
                    ),
                    "immutable",
                );
            }
            pass("fn f() { a: const int32[1] = [1]; v := a[..]; v = a[..]; }");
            bad(
                "fn f() { a := [1]; const v := a[..]; v = a[..]; }",
                "immutable",
            );
        }

        #[test]
        fn readonly_paths_and_addressing() {
            pass("comp S: type = struct { x: int32 }; fn f(p: const *S) int32 { return (p*).x; }");
            pass("fn f() { a: const int32[1] = [1]; p: const *int32 = a[0]&; x := p*; }");
            pass(
                "fn f() { a: const int32[1] = [1]; v := a[..]; w: const int32[] = v[..]; x := w[0]; }",
            );
            for source in [
                "comp S: type = struct { x: int32 }; fn f(p: const *S) { (p*).x = 2; }",
                "fn f() { a: const int32[1] = [1]; v := a[..]; w := v[..]; w[0] = 2; }",
                "fn f(p: const *int32[1]) { (p*)[0] = 2; }",
                "fn f(p: const *int32[1]) { v := (p*)[..]; v[0] = 2; }",
                "comp S: type = struct { a: int32[1] }; fn f(p: const *S) { (p*).a[0] = 2; }",
                "comp S: type = struct { x: int32 }; fn f() { a: const S[1] = [S { x: 1 }]; a[0].x = 2; }",
                "fn f() { a: const int32[1][1] = [[1]]; a[0][0] = 2; }",
            ] {
                bad(source, "immutable");
            }
            bad(
                "fn f() { a: const int32[1] = [1]; p: *int32 = a[0]&; }",
                "incompatible",
            );
            bad(
                "fn f(p: const *int32[1]) { v: int32[] = (p*)[..]; }",
                "incompatible",
            );
        }

        #[test]
        fn qualifications_are_layer_local() {
            // Readonly outer storage does not requalify a pointer/slice value it contains.
            pass("fn f(p: const **int32) { q := p*; (q*) = 2; }");
            pass("fn f(p: const *int32[]) { v := p*; v[0] = 2; }");
            bad("fn f(p: *const *int32) { q := p*; (q*) = 2; }", "immutable");
            bad(
                "fn f(p: *const int32[]) { v := p*; v[0] = 2; }",
                "immutable",
            );
            pass("fn f(p: const *const *int32) { q: const *int32 = p*; x := q*; }");
        }

        #[test]
        fn exact_compatibility_at_all_boundaries() {
            for source in [
                "fn f(p: const *int32) { q: *int32 = p; }",
                "fn f(p: *int32) { q: const *int32 = p; }",
                "fn f(p: const int32[]) { q: int32[] = p; }",
                "fn f(p: int32[]) { q: const int32[] = p; }",
                "fn f() { p: const int32[1] = [1]; q: int32[1] = p; }",
                "fn f() { p: int32[1] = [1]; q: const int32[1] = p; }",
                "fn f(p: const *int32) *int32 { return p; }",
                "fn take(p: *int32) {} fn f(p: const *int32) { take(p); }",
                "fn f(p: *const *int32) { q: **int32 = p; }",
                "fn f(p: *int32) { q: const *int32 = null; q = p; }",
            ] {
                bad(source, "incompatible");
            }
        }

        #[test]
        fn casts_cannot_erase_qualifiers() {
            for source in [
                "fn f(p: const *int32) { q := p as *int32; }",
                "fn f(p: *int32) { q := p as const *int32; }",
                "fn f(p: const *int32) { q := p as *void; }",
                "fn f(p: *const *int32) { q := p as *void; }",
                "fn f(p: const int32[]) { q := p as int32[]; }",
                "fn f(p: const int32[1]) { q := p as int32[1]; }",
                "comp S: type = struct { p: const *int32 }; fn f(p: *S) { q := p as *void; }",
            ] {
                bad(source, "cast");
            }
            pass(
                "fn f(p: const *int32) { q := p as const *void; r := q as const *int32; x := r*; }",
            );
            pass("fn f(p: const *int32) { q := p as const *int32; same := q == null; }");
        }

        #[test]
        fn aliases_generics_and_type_evaluation_preserve_qualification() {
            pass("comp P: type = const *int32; fn id(p: P) const *int32 { return p; }");
            pass(
                "fn id(T: type, p: const *T) const *T { return p; } fn f() { const x := 1; p := id(int32, x&); y := p*; }",
            );
            pass(
                "fn id(T: type, p: T) T { return p; } fn f(p: const int32[]) { q := id(const int32[], p); x := q[0]; }",
            );
            pass(
                "comp fn View(T: type) type { return const T[]; } comp V: type = View(int32); fn f(v: V) const int32[] { return v; }",
            );
            bad(
                "fn id(T: type, p: const *T) const *T { return p; } fn f() { const x := 1; p := id(int32, x&); (p*) = 2; }",
                "immutable",
            );
            bad("comp A: type = struct { a: const A[1] };", "cycle");
        }

        #[test]
        fn qualification_does_not_bypass_initialization() {
            for source in [
                "fn f() { a: const int32[1]; x := a[0]; }",
                "fn f() { a: const int32[1]; v := a[..]; }",
                "fn f() { a: const int32[1]; p := a[0]&; x := a[0]; }",
                "fn f() { a: const int32[1]; const p := a&; x := a[0]; }",
            ] {
                analysis_bad(source, "uninitialized");
            }
            pass("fn f() { a: const int32[1]; a = [1]; x := a[0]; }");
            pass("fn f() { const a: int32[1] = [1]; a[0] = 2; x := a[0]; }");
        }

        #[test]
        fn readonly_annotation_corruption_is_rejected() {
            use crate::frontend::analysis_annotation::ValueCategory as C;
            for source in [
                "p: const *int32; p*;",
                "a: const int32[1] = [1]; a[0];",
                "comp S: type = struct { x: int32 }; p: const *S; (p*).x;",
            ] {
                let (mut a, types) = run(&format!("fn f() {{ {source} }}")).unwrap();
                let StatementKind::FunctionDeclaration(fun) =
                    &mut a.units[0].ast.statements[0].kind
                else {
                    panic!()
                };
                let StatementKind::ExpressionStatement(expr) = &mut fun
                    .body
                    .as_mut()
                    .unwrap()
                    .statements
                    .last_mut()
                    .unwrap()
                    .kind
                else {
                    panic!()
                };
                expr.annotation.resolved.as_mut().unwrap().category = C::MutablePlace;
                let e = crate::frontend::analyzer::validate_type_annotations(
                    &mut a.units,
                    &a.symbols,
                    &types,
                )
                .unwrap_err();
                assert!(e.to_string().contains("mutability"), "{e:#}");
            }
            for source in [
                "const x := 1; x&;",
                "a: const int32[1] = [1]; a[..];",
                "p: const *int32; p as const *int32;",
            ] {
                let (mut a, types) = run(&format!("fn f() {{ {source} }}")).unwrap();
                let StatementKind::FunctionDeclaration(fun) =
                    &mut a.units[0].ast.statements[0].kind
                else {
                    panic!()
                };
                let StatementKind::ExpressionStatement(expr) = &mut fun
                    .body
                    .as_mut()
                    .unwrap()
                    .statements
                    .last_mut()
                    .unwrap()
                    .kind
                else {
                    panic!()
                };
                let info = expr.annotation.resolved.as_mut().unwrap();
                let SemanticType::Readonly(mutable) = types.get(info.ty).unwrap() else {
                    panic!()
                };
                info.ty = *mutable;
                let invalid_cast = matches!(expr.kind, ExpressionKind::Cast { .. });
                if let ExpressionKind::Cast { target_type, .. } = &mut expr.kind {
                    let crate::frontend::ast::type_expression::TypeExpressionKind::Readonly(inner) =
                        &target_type.kind
                    else {
                        panic!()
                    };
                    *target_type = (**inner).clone();
                }
                let e = crate::frontend::analyzer::validate_type_annotations(
                    &mut a.units,
                    &a.symbols,
                    &types,
                )
                .unwrap_err();
                let expected = if invalid_cast {
                    "unsupported cast"
                } else {
                    "mutability"
                };
                assert!(e.to_string().contains(expected), "{e:#}");
            }
        }

        #[test]
        fn semantic_identity_and_raw_validation() {
            let mut types = SemanticTypes::default();
            let int = types.builtin(BuiltinType::Int32);
            let pointer = types.intern(SemanticType::Pointer(int));
            let readonly = types.intern(SemanticType::Readonly(pointer));
            assert_eq!(readonly, types.intern(SemanticType::Readonly(pointer)));
            assert_ne!(readonly, pointer);
            assert_eq!(types.canonical(readonly).unwrap(), readonly);
            types.validate_type(readonly).unwrap();
            for (a, b) in [(readonly, pointer), (pointer, readonly)] {
                assert!(check_assignable(a, b, &types).is_err());
            }
            let invalid = types.intern(SemanticType::Readonly(int));
            assert!(types.validate_type(invalid).is_err());
            let invalid = types.intern(SemanticType::Readonly(TypeId(999)));
            assert!(types.validate_type(invalid).is_err());
        }
    }

    #[test]
    fn compatibility_is_pure_and_nominal() {
        use crate::frontend::semantic_type::FieldType;
        let mut t = SemanticTypes::default();
        let int = t.intern(SemanticType::Builtin(BuiltinType::Int32));
        let a = t.intern(SemanticType::Struct(vec![FieldType {
            spelling: "x".into(),
            ty: int,
        }]));
        let b = t.intern(SemanticType::Struct(vec![FieldType {
            spelling: "x".into(),
            ty: int,
        }]));
        let before = t.types.clone();
        check_assignable(a, a, &t).unwrap();
        assert!(check_assignable(a, b, &t).is_err());
        assert!(check_assignable(TypeId(999), TypeId(999), &t).is_err());
        assert_eq!(t.types, before);
    }
    #[test]
    fn complete_type_annotations_validate() {
        let (mut a, t) = run("comp S: type = struct { x: int32 }; s: S; x := s.x;").unwrap();
        crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &t).unwrap();
    }
    #[test]
    fn corrupted_annotations_are_rejected() {
        let (a, t) = run("comp S: type = struct { x: int32 }; s: S; x := s.x;").unwrap();
        for corruption in 0..6 {
            let mut a = a.clone();
            let StatementKind::VariableDeclaration(v) = &mut a.units[0].ast.statements[2].kind
            else {
                panic!()
            };
            let expr = v.expression.as_mut().unwrap();
            match corruption {
                0 => expr.annotation.resolved = None,
                1 => expr.annotation.resolved.as_mut().unwrap().ty = TypeId(999),
                2 => v.annotation.binding = None,
                3 => {
                    if let ExpressionKind::ExprAcess { member, .. } = &mut expr.kind {
                        member.resolved = None;
                    }
                }
                4 => {
                    if let ExpressionKind::ExprAcess { member, .. } = &mut expr.kind {
                        member.resolved.as_mut().unwrap().owner = TypeId(999);
                    }
                }
                5 => {
                    if let ExpressionKind::ExprAcess { expr, .. } = &mut expr.kind
                        && let ExpressionKind::QualifiedIdentifier(name) = &mut expr.kind
                    {
                        name.annotation.resolved = Some(ResolvedName::Local(LocalId(999)));
                    }
                }
                _ => unreachable!(),
            }
            let error =
                crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &t)
                    .unwrap_err();
            assert!(
                !error.to_string().contains("not implemented"),
                "validator is still a stub"
            );
            assert!(error.downcast_ref::<CompilerError>().is_some());
        }
    }

    ok!(
        typed_initialization_cycle_is_not_type_inference_cycle,
        "a: int32 = b; b: int32 = a;"
    );
    err!(inference_cycle, "a := b; b := a;", "cycle");
    err!(
        float_constant_division_zero,
        "x := 1.0 / (2.0 - 2.0);",
        "zero"
    );
    err!(unsigned_constant_underflow, "x: uint8 = 0 - 1;", "range");
    err!(unsigned_constant_overflow, "x: uint8 = 255 + 1;", "range");
    ok!(unsigned_complement, "x: uint8 = ~1;");
    err!(shift_result_overflow, "x: int8 = 1 << 7;", "range");
    ok!(
        zero_length_full_slice,
        "items: int32[0] = []; x := items[..];"
    );
    err!(
        empty_inclusive_slice,
        "items: int32[0] = []; x := items[0.=0];",
        "bounds"
    );
    #[test]
    fn extended_float_exponent_does_not_use_host_range() {
        let large = format!("1{}.0", "0".repeat(308));
        good(&format!("x: float128 = {large} * {large};"));
    }
    #[test]
    fn pass_order_and_failed_retry_are_enforced() {
        let mut a = analyzer("x: int8 = 300;");
        assert!(
            a.resolve_types()
                .unwrap_err()
                .to_string()
                .contains("completed names")
        );
        a.collect_declarations().unwrap();
        a.resolve_names().unwrap();
        let error = a.resolve_types().unwrap_err();
        let error = error.downcast_ref::<CompilerError>().unwrap();
        assert_eq!(error.filename.as_deref(), Some(Path::new("types.fib")));
        assert!(error.span.is_some());
        assert!(
            a.resolve_types()
                .unwrap_err()
                .to_string()
                .contains("only run once")
        );
    }
    #[test]
    fn type_slots_and_semantic_tables_cannot_be_fabricated() {
        let (a, t) =
            run("comp S: type = struct { x: int32 }; s: S; fn f(p: int32) int32 { return p; }")
                .unwrap();
        for corruption in 0..5 {
            let mut a = a.clone();
            let mut t = t.clone();
            let StatementKind::VariableDeclaration(var) = &mut a.units[0].ast.statements[1].kind
            else {
                panic!()
            };
            match corruption {
                0 => var.ty.as_mut().unwrap().annotation.resolved = None,
                1 => var.ty.as_mut().unwrap().annotation.resolved = Some(TypeId(999)),
                2 => {
                    t.locals.insert(LocalId(999), TypeId(0));
                }
                3 => {
                    let id = var.annotation.binding.unwrap();
                    t.bindings.get_mut(&id).unwrap().type_value = Some(TypeId(0));
                }
                4 => {
                    let StatementKind::FunctionDeclaration(f) =
                        &mut a.units[0].ast.statements[2].kind
                    else {
                        panic!()
                    };
                    f.signature.parameters[0].annotation.binding = None;
                }
                _ => unreachable!(),
            }
            assert!(
                crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &t)
                    .is_err()
            );
        }
    }
    #[test]
    fn node_entrypoint_checks_function_body() {
        let mut a = analyzer("fn f(x: int32) int32 { return x; }");
        a.collect_declarations().unwrap();
        a.resolve_names().unwrap();
        let mut t = SemanticTypes::default();
        resolve_types(&mut a.units, &a.symbols, &mut t).unwrap();
        let StatementKind::FunctionDeclaration(fun) = &mut a.units[0].ast.statements[0].kind else {
            panic!()
        };
        let BindingAnnotation { binding: Some(id) } = fun.signature.annotation else {
            panic!()
        };
        let signature = t.bindings[&id].ty;
        fun.resolve_type(
            &mut TypeContext::new(FileId(0), &a.symbols, &mut t),
            signature,
        )
        .unwrap();
    }

    fn run_files(files: &[(&str, &str)]) -> anyhow::Result<(Analyzer, SemanticTypes)> {
        use crate::driver::CompilationUnit;
        use crate::plan::{BuildPlan, SourceFile};
        let plan = BuildPlan {
            files: files
                .iter()
                .enumerate()
                .map(|(i, (name, _))| SourceFile {
                    id: FileId(i as u32),
                    name: (*name).to_string().into(),
                    path: format!("{name}.fib").into(),
                })
                .collect(),
            entry: FileId(0),
            output: "unused".into(),
        };
        let units = files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                let path = format!("{name}.fib");
                CompilationUnit {
                    source_file_id: FileId(i as u32),
                    ast: Parser::with_source(Tokenizer::new(source), Path::new(&path), source)
                        .parse()
                        .expect("valid multi-file fixture"),
                }
            })
            .collect();
        let mut a = Analyzer::new(plan, units)?;
        a.collect_declarations()?;
        a.resolve_names()?;
        let t = a.resolve_types()?;
        Ok((a, t))
    }
    #[test]
    fn qualified_types_aliases_and_ordinary_calls() {
        run_files(&[("main","comp util := lib; item: util::Item; value := item.value; result := util::concat(value);"),("lib","pub comp Item: type = struct { value: int32 }; pub concat: fn(int32) -> int32;")]).unwrap();
    }
    #[test]
    fn namespace_aliases_have_no_fake_runtime_annotations() {
        run_files(&[("main","comp util := lib; fn f() { comp alias := lib; comp a, comp b := lib, lib; value: alias::Item; }"),("lib","pub comp Item: type = struct { value: int32 };")]).unwrap();
    }
    #[test]
    fn cross_file_diagnostic_retains_owning_path() {
        let error = run_files(&[
            ("main", "value: lib::Item;"),
            (
                "lib",
                "pub comp Item: type = struct { x: int32, x: int32 };",
            ),
        ])
        .err()
        .unwrap();
        let error = error.downcast_ref::<CompilerError>().unwrap();
        assert_eq!(error.filename.as_deref(), Some(Path::new("lib.fib")));
        assert!(error.span.is_some());
    }
    #[test]
    fn validation_is_read_only() {
        let (mut a, t) =
            run("comp E: type = enum { Value { x: int32 }, Empty }; value := E.Value(1);").unwrap();
        let before_units = a.units.clone();
        let before_types = t.types.clone();
        crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &t).unwrap();
        assert_eq!(a.units[0].ast, before_units[0].ast);
        assert_eq!(t.types, before_types);
    }
    #[test]
    fn variant_constructor_table_is_validated_even_without_calls() {
        let (mut a, mut t) = run("comp E: type = enum { Value { x: int32 } };").unwrap();
        let key = *t.variant_constructors.keys().next().unwrap();
        t.variant_constructors.insert(key, TypeId(999));
        assert!(
            crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &t)
                .is_err()
        );
    }
    ok!(
        exact_large_integer_divisor,
        "x: uint128 = 1 / (18446744073709551615 - 18446744073709551614);"
    );
    mod never_regressions {
        use super::*;
        use crate::frontend::semantic_type::{FunctionType, SemanticType as T};

        #[test]
        fn bottom_is_directional_and_not_recursive() {
            let mut types = SemanticTypes::default();
            let int = types.builtin(BuiltinType::Int32);
            let never = types.builtin(BuiltinType::Never);
            check_assignable(int, never, &types).unwrap();
            assert!(check_assignable(never, int, &types).is_err());
            assert!(check_assignable(TypeId(999), never, &types).is_err());
            let pairs = [
                (T::Pointer(int), T::Pointer(never)),
                (T::Slice(int), T::Slice(never)),
                (
                    T::Array {
                        element: int,
                        length: 1,
                    },
                    T::Array {
                        element: never,
                        length: 1,
                    },
                ),
                (T::Tuple(vec![int]), T::Tuple(vec![never])),
                (
                    T::Function(FunctionType {
                        is_comp: false,
                        parameters: vec![],
                        return_type: int,
                    }),
                    T::Function(FunctionType {
                        is_comp: false,
                        parameters: vec![],
                        return_type: never,
                    }),
                ),
                (
                    T::Function(FunctionType {
                        is_comp: false,
                        parameters: vec![int],
                        return_type: int,
                    }),
                    T::Function(FunctionType {
                        is_comp: false,
                        parameters: vec![never],
                        return_type: int,
                    }),
                ),
            ];
            for (a, b) in pairs {
                let a = types.intern(a);
                let b = types.intern(b);
                assert!(check_assignable(a, b, &types).is_err());
                assert!(check_assignable(b, a, &types).is_err());
            }
        }

        #[test]
        fn contextual_bottom_and_eager_operands_diverge() {
            let prefix = "fn stop() never { while (true) {}; } fn take(x: int32, y: int32) int32 { return x; } ";
            for body in [
                "x: int32 = stop();",
                "x: int32; x = stop();",
                "return stop();",
                "return -stop();",
                "return stop() + 1;",
                "return 1 + stop();",
                "return stop() && true;",
                "return true && stop();",
                "return take(stop(), 1);",
                "x: int32; take(stop(), x);",
                "x: int32; return stop() + x;",
                "if stop() {};",
                "while (stop()) {};",
                "a: int32[1] = [stop()];",
                "a: int32[1] = [1]; return a[stop()];",
                "a: int32[1]; return a[stop()];",
                "return stop() as int32;",
                "[stop(), 1];",
                "[1, stop()];",
                "a: int32[1] = [1]; a[stop()..];",
                "a: int32[1] = [1]; a[0..stop()];",
                "comp S: type = struct { x: int32 }; s: S = S { x: stop() };",
                "x: int32; defer take(x, x); return stop();",
                "x: int32; if true { x = stop(); } else { x = 1; }; return x;",
                "x: int32; take(1, stop()); return x;",
            ] {
                let source = format!("{prefix} fn f() int32 {{ {body} }}");
                analyzer(&source)
                    .analyze()
                    .unwrap_or_else(|e| panic!("{source}\n{e:#}"));
            }
        }

        #[test]
        fn bottom_operators_do_not_adopt_unrelated_result_context() {
            for result in ["bool", "int32", "float64", "never"] {
                for value in [
                    "1 + stop()",
                    "stop() + 1",
                    "1 << stop()",
                    "1 & stop()",
                    "true && stop()",
                    "stop() == 1",
                ] {
                    let source = format!("fn stop() never; fn f() {result} {{ return {value}; }}");
                    analyzer(&source)
                        .analyze()
                        .unwrap_or_else(|e| panic!("{source}\n{e:#}"));
                }
            }
        }

        #[test]
        fn never_returns_and_normal_returns_are_distinct() {
            for source in ["fn f() never { return; }", "fn f() never { return 1; }"] {
                bad(source, "incompatible types");
            }
            analysis_bad("fn f() never {}", "never function cannot return normally");
            analyzer("fn stop() never { while (true) {}; } fn f() never { return stop(); }")
                .analyze()
                .unwrap();
            bad("fn stop() never; x := stop();", "never cannot be stored");
        }

        #[test]
        fn short_circuit_divergence_is_conditional() {
            for body in ["true && stop();", "false || stop();"] {
                analyzer(&format!("fn stop() never; fn f() int32 {{ {body} }}"))
                    .analyze()
                    .unwrap();
            }
            for body in [
                "false && stop();",
                "true || stop();",
                "b && stop();",
                "b || stop();",
            ] {
                analysis_bad(
                    &format!("fn stop() never; fn f(b: bool) int32 {{ {body} }}"),
                    "all paths",
                );
            }
            analysis_bad(
                "fn stop() never; fn f(b: bool) int32 { x: int32; take(b && stop(), x); } fn take(b: bool, x: int32);",
                "uninitialized",
            );
            analysis_bad(
                "fn stop() never; fn f() int32 { x: int32; return x + stop(); }",
                "uninitialized",
            );
            bad(
                "fn stop() never; fn f() { take(stop(), true); } fn take(x: int32, y: int32);",
                "incompatible types",
            );
        }

        #[test]
        fn divergent_annotations_are_validated() {
            let (mut a, mut types) =
                run("fn stop() never; fn take(x: int32) int32; fn f() { take(stop()); }").unwrap();
            let StatementKind::FunctionDeclaration(f) = &mut a.units[0].ast.statements[2].kind
            else {
                panic!()
            };
            let StatementKind::ExpressionStatement(expr) =
                &mut f.body.as_mut().unwrap().statements[0].kind
            else {
                panic!()
            };
            expr.annotation.resolved.as_mut().unwrap().ty = types.builtin(BuiltinType::Int32);
            crate::frontend::analyzer::validate_type_annotations(&mut a.units, &a.symbols, &types)
                .expect_err("diverging call cannot be annotated with its nominal return type");
        }

        #[test]
        fn runtime_remainder_zero_and_inclusive_overflow() {
            bad(
                "fn f(x: int32) int32 { return x % 0; }",
                "remainder by zero",
            );
            bad(
                "fn f(x: int32) int32 { return x % (2 - 2); }",
                "remainder by zero",
            );
            good("fn f(x: int32) int32 { return x % 2; }");
            let max = "(((1 as int128) << 126) - 1 + ((1 as int128) << 126))";
            bad(
                &format!("fn f(a: int32[]) {{ a[0.={max}]; }}"),
                "inclusive slice bound overflows",
            );
            good(&format!("fn f(a: int32[]) {{ a[0..{max}]; }}"));
            good(&format!("fn f(a: int32[]) {{ a[0.=({max} - 1)]; }}"));
        }
    }

    mod completion_regressions {
        use super::*;
        use crate::frontend::analysis_annotation::BindingId;
        use crate::frontend::analyzer::validate_type_annotations;
        macro_rules! pass {
            ($name:ident, $source:expr) => {
                #[test]
                fn $name() {
                    analyzer($source).analyze().unwrap();
                }
            };
        }
        macro_rules! fail {
            ($name:ident, $source:expr, $message:expr) => {
                #[test]
                fn $name() {
                    analysis_bad($source, $message);
                }
            };
        }
        pass!(
            aggregate_join_preserves_full_initialization,
            "comp S: type = struct { x: int32 }; fn f(b: bool) S { s: S; if b { s = S { x: 1 }; } else { s.x = 2; }; return s; }"
        );
        pass!(
            aggregate_join_preserves_initialized_field,
            "comp S: type = struct { x: int32, y: int32 }; fn f(b: bool) int32 { s: S; if b { s = S { x: 1, y: 1 }; } else { s.x = 2; }; return s.x; }"
        );
        pass!(
            assigned_local,
            "fn f() int32 { x: int32; x = 1; return x; }"
        );
        pass!(
            both_branches_initialize,
            "fn f(b: bool) int32 { x: int32; if b { x = 1; } else { x = 2; }; return x; }"
        );
        pass!(
            returning_branch_is_not_joined,
            "fn f(b: bool) int32 { x: int32; if b { return 1; } else { x = 2; }; return x; }"
        );
        pass!(
            both_branches_return,
            "fn f(b: bool) int32 { if b { return 1; } else { return 2; }; }"
        );
        pass!(
            infinite_loop_diverges,
            "fn f() int32 { while (true) { continue; }; }"
        );
        pass!(
            break_carries_initialization,
            "fn f() int32 { x: int32; while (true) { x = 2; break; }; return x; }"
        );
        pass!(
            partial_field_initialization,
            "comp S: type = struct { x: int32, y: int32 }; fn f() S { s: S; s.x = 1; s.y = 2; return s; }"
        );
        pass!(
            array_element_initialization,
            "fn f() int32[2] { a: int32[2]; a[0] = 1; a[1] = 2; return a; }"
        );
        pass!(
            defer_reads_at_exit,
            "fn use(x: int32); fn f() { x: int32; defer use(x); x = 1; }"
        );
        pass!(
            defer_reverse_order,
            "fn use(x: int32); fn f() { x: int32; defer use(x); defer x = 1; }"
        );
        pass!(
            inner_defer_initializes_outer,
            "fn f() int32 { x: int32; if true { defer x = 1; }; return x; }"
        );
        pass!(
            loop_exit_defer,
            "fn use(x: int32); fn f() { x: int32; while (true) { defer use(x); x = 1; break; }; }"
        );
        pass!(
            exhaustive_enum_return,
            "comp E: type = enum { A, B }; fn f(e: E) int32 { switch(e) { when .A { return 1; } when .B { return 2; } }; }"
        );
        pass!(
            enum_payload_initialized,
            "comp E: type = enum { A { x: int32 } }; fn f(e: E) int32 { switch(e) { when .A(p) { return p.x; } }; }"
        );
        fail!(
            branch_may_not_initialize,
            "fn f(b: bool) int32 { x: int32; if b { x = 1; }; return x; }",
            "uninitialized"
        );
        fail!(
            loop_may_not_run,
            "fn f(b: bool) int32 { x: int32; while (b) { x = 1; }; return x; }",
            "uninitialized"
        );
        fail!(
            return_before_defer_write,
            "fn f() int32 { x: int32; defer x = 1; return x; }",
            "uninitialized"
        );
        fail!(
            defer_before_assignment_exit,
            "fn use(x: int32); fn f(b: bool) { x: int32; defer use(x); if b { return; }; x = 1; }",
            "uninitialized"
        );
        fail!(
            defer_wrong_order,
            "fn use(x: int32); fn f() { x: int32; defer x = 1; defer use(x); }",
            "uninitialized"
        );
        fail!(
            nested_defer_rejected,
            "fn f() { defer defer f(); }",
            "defer"
        );
        fail!(
            deferred_break_rejected,
            "fn f() { while (true) { defer break; }; }",
            "defer"
        );
        fail!(break_outside_loop, "fn f() { break; }", "loop");
        fail!(continue_outside_loop, "fn f() { continue; }", "loop");
        fail!(
            missing_enum_path,
            "comp E: type = enum { A, B }; fn f(e: E) int32 { switch(e) { when .A { return 1; } }; }",
            "return"
        );
        fail!(
            duplicate_enum_arm,
            "comp E: type = enum { A }; fn f(e: E) { switch(e) { when .A {} when .A {} }; }",
            "duplicate"
        );
        fail!(
            partially_initialized_struct,
            "comp S: type = struct { x: int32, y: int32 }; fn f() S { s: S; s.x = 1; return s; }",
            "uninitialized"
        );
        fail!(
            partially_initialized_array,
            "fn f() int32[2] { a: int32[2]; a[0] = 1; return a; }",
            "uninitialized"
        );
        fail!(
            file_forward_read,
            "first := second; second := 1;",
            "uninitialized"
        );
        fail!(
            file_initialization_cycle,
            "first: int32 = second; second: int32 = first;",
            "uninitialized"
        );
        pass!(
            specialized_pointer,
            "fn identity(T: type, x: *T) *T { return x; } x := 1; p := identity(int32, x&);"
        );
        pass!(
            specialized_array,
            "fn identity(T: type, x: T[2]) T[2] { return x; } a := identity(int8, [1, 2]);"
        );
        pass!(
            specialized_function,
            "fn identity(T: type, f: fn(T) -> T) fn(T) -> T { return f; } fn inc(x: int32) int32 { return x + 1; } result := identity(int32, inc)(2);"
        );
        pass!(
            forwarding_generic,
            "fn identity(T: type, x: T) T { return x; } fn forward(U: type, x: U) U { return identity(U, x); } result := forward(int32, 1);"
        );
        pass!(
            recursive_generic,
            "fn recur(T: type, x: T) T { return recur(T, x); } result := recur(int32, 1);"
        );
        pass!(
            computed_local_type,
            "comp fn choose() type { return int32; } fn f() int32 { comp T := choose(); x: T = 1; return x; }"
        );
        pass!(
            computed_type_parameter,
            "comp fn choose(T: type) type { return T; } comp T := choose(int8); value: T = 127;"
        );
        pass!(
            computed_bool_branch,
            "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp T := choose(true); value: T = 127;"
        );
        pass!(
            computed_aggregate_types_are_distinct,
            "comp fn box(T: type) type { return struct { x: T }; } comp A := box(int8); comp B := box(int32); a: A = A { x: 1 }; b: B = B { x: 2 };"
        );
        pass!(
            computed_bool_forwarding,
            "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp fn forward(b: bool) type { return choose(b); } comp T := forward(true); x: T = 1;"
        );
        err!(
            specialized_argument_mismatch,
            "fn identity(T: type, x: T) T { return x; } result := identity(int32, true);",
            "incompatible"
        );
        err!(
            specialized_literal_range,
            "fn identity(T: type, x: T) T { return x; } result := identity(int8, 128);",
            "range"
        );
        err!(
            computed_no_body,
            "comp fn choose() type; comp T := choose();",
            "body"
        );
        err!(
            computed_recursion_limit,
            "comp fn choose() type { return choose(); } comp T := choose();",
            "limit"
        );
        pass!(
            computed_local_alias,
            "comp fn choose() type { return int32; } fn f() int32 { comp T := choose(); comp U := T; x: U = 1; return x; }"
        );
        pass!(
            computed_multi_binding,
            "comp fn choose() type { return int32; } comp A, comp B := choose(), choose(); a: A = 1; b: B = 2;"
        );
        pass!(
            computed_named_local_aggregate,
            "comp fn box(T: type) type { comp Box: type = struct { x: T }; return Box; } comp A := box(int8); comp B := box(int32); a: A = A { x: 1 }; b: B = B { x: 2 };"
        );
        pass!(
            computed_cache_preserves_nominal_identity,
            "comp fn box(T: type) type { return struct { x: T }; } comp A := box(int8); comp B := box(int8); a: A = A { x: 1 }; b: B = a;"
        );
        fail!(
            unreachable_break_is_invalid,
            "fn f() { return; break; }",
            "loop"
        );
        fail!(
            false_branch_break_is_invalid,
            "fn f() { if false { break; }; }",
            "loop"
        );
        #[test]
        fn computed_cross_file_type_and_generic_alias() {
            let mut lib = analyzer("fn identity(T: type, x: T) T { return x; } comp fn box(T: type) type { return struct { x: T }; }").units.remove(0);
            lib.source_file_id = FileId(1);
            for node in &mut lib.ast.statements {
                if let StatementKind::FunctionDeclaration(f) = &mut node.kind {
                    f.is_pub = true;
                }
            }
            let main = analyzer("comp ident := lib::identity; a := ident(int8, 1); comp maker := lib::box; comp Box := maker(int32); b: Box = Box { x: 1 };").units.remove(0);
            let mut a = Analyzer::new(
                BuildPlan {
                    files: vec![
                        SourceFile {
                            id: FileId(0),
                            name: "main".to_string().into(),
                            path: "main.fib".into(),
                        },
                        SourceFile {
                            id: FileId(1),
                            name: "lib".to_string().into(),
                            path: "lib.fib".into(),
                        },
                    ],
                    entry: FileId(0),
                    output: "unused".into(),
                },
                vec![main, lib],
            )
            .unwrap();
            a.collect_declarations().unwrap();
            a.resolve_names().unwrap();
            a.resolve_types().unwrap();
        }
        #[test]
        fn corrupt_cached_signature_is_rejected() {
            let (mut a, mut types) =
                run("fn identity(T: type, x: T) T { return x; } a := identity(int8, 1);").unwrap();
            let wrong = types.builtin(BuiltinType::Boolean);
            types
                .generic_instances
                .values_mut()
                .next()
                .unwrap()
                .signature = wrong;
            assert!(validate_type_annotations(&mut a.units, &a.symbols, &types).is_err());
        }
        #[test]
        fn corrupt_call_result_is_rejected() {
            let (mut a, mut types) =
                run("fn identity(T: type, x: T) T { return x; } a := identity(int8, 1);").unwrap();
            let wrong = types.builtin(BuiltinType::Boolean);
            let StatementKind::VariableDeclaration(v) = &mut a.units[0].ast.statements[1].kind
            else {
                panic!()
            };
            v.expression
                .as_mut()
                .unwrap()
                .annotation
                .resolved
                .as_mut()
                .unwrap()
                .ty = wrong;
            assert!(validate_type_annotations(&mut a.units, &a.symbols, &types).is_err());
        }
        #[test]
        fn corrupt_parameter_metadata_is_rejected() {
            let (mut a, mut types) = run("fn identity(T: type, x: T) T { return x; }").unwrap();
            types
                .generic_parameters
                .values_mut()
                .next()
                .unwrap()
                .clear();
            assert!(validate_type_annotations(&mut a.units, &a.symbols, &types).is_err());
        }
        #[test]
        fn full_analysis_revalidates_public_ast() {
            let mut project = analyzer("fn f() int32 { x := 1; return x; }")
                .analyze()
                .unwrap();
            let StatementKind::FunctionDeclaration(fun) =
                &mut project.units[0].ast.statements[0].kind
            else {
                panic!()
            };
            let StatementKind::VariableDeclaration(v) =
                &mut fun.body.as_mut().unwrap().statements[0].kind
            else {
                panic!()
            };
            v.expression = None;
            let error = validate_analysis(&project).unwrap_err();
            assert!(error.to_string().contains("uninitialized"));
        }
        #[test]
        fn flow_diagnostics_keep_source_paths() {
            let error = analyzer("fn f() int32 { x: int32; return x; }")
                .analyze()
                .unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<CompilerError>()
                    .unwrap()
                    .filename
                    .as_deref(),
                Some(Path::new("types.fib"))
            );
        }
        fail!(
            initializer_function_reads_later_global,
            "fn f() int32 { return late; } early := f(); late := 1;",
            "uninitialized"
        );
        fail!(
            initializer_function_chain_reads_later_global,
            "fn f() int32 { return g(); } fn g() int32 { return late; } early := f(); late := 1;",
            "uninitialized"
        );
        pass!(
            initializer_function_reads_earlier_global,
            "late := 1; fn f() int32 { return late; } early := f();"
        );
        err!(
            computed_runtime_call_is_rejected,
            "fn f() int32 { return 1; } comp value := f();",
            "type value"
        );
        err!(
            computed_side_effect_is_rejected,
            "fn side() bool; comp fn choose() type { b := side(); return int32; } comp T := choose();",
            "known type or bool"
        );
        err!(
            computed_uninitialized_local_is_rejected,
            "comp fn choose() type { b: bool; return int32; } comp T := choose();",
            "known type or bool"
        );
        #[test]
        fn instances_cache_by_template_and_type_identity() {
            let (_, types) = run("fn identity(T: type, x: T) T { return x; } a := identity(int32, 1); b := identity(int32, 2); c := identity(int8, 3);").unwrap();
            assert_eq!(types.generic_instances.len(), 2);
            assert_ne!(
                types.generic_instances.values().next().unwrap().signature,
                types.bindings[&BindingId::Declaration(DeclId {
                    file_id: FileId(0),
                    index: 0
                })]
                    .ty
            );
        }
    }
    mod comp_function_eligibility {
        use super::*;
        use crate::frontend::{
            analysis_annotation::BindingId,
            analyzer::{
                types::check_assignable, validation::validate as validate_type_annotations,
            },
            semantic_type::SemanticType as T,
        };

        #[test]
        fn known_runtime_identity_does_not_authorize_evaluation() {
            for source in [
                "fn choose() type { return int32; } comp T := choose();",
                "fn choose(T: type) type { return T; } comp T := choose(int32);",
                "fn choose() type { return int32; } comp alias := choose; comp T := alias();",
                "fn choose(b: bool) type { return int32; } comp fn forward(b: bool) type { return choose(b); }",
            ] {
                bad(source, "runtime function is not eligible");
            }
        }

        #[test]
        fn explicit_eligibility_supports_known_aliases_and_bounded_computation() {
            analyzer("comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp alias := choose; comp T := alias(true); x: T = 127;").analyze().unwrap();
            analyzer("comp fn choose(T: type) type { return T; } fn f() { comp alias := choose; comp T := alias(int32); value: T = 1; }").analyze().unwrap();
        }

        #[test]
        fn eligible_type_does_not_fabricate_indirect_callee_identity() {
            bad(
                "comp fn choose(b: bool) type { return int32; } fn f(b: bool) { comp T := choose(b); }",
                "known type or bool",
            );
            bad(
                "fn invoke(f: comp fn() type) type { return f(); }",
                "requires a named function",
            );
            bad(
                "comp fn choose() type { return int32; } callback: comp fn() type = choose; comp T := callback();",
                "requires a function definition",
            );
        }

        #[test]
        fn eligibility_is_interned_and_compatible_only_with_itself() {
            let (_, types) = run("comp Runtime: type = fn() type; comp Eligible: type = comp fn() type; comp Again: type = comp fn() type;").unwrap();
            let values: Vec<_> = (0..3)
                .map(|index| {
                    types.type_values[&BindingId::Declaration(DeclId {
                        file_id: FileId(0),
                        index,
                    })]
                })
                .collect();
            assert_ne!(values[0], values[1]);
            assert_eq!(values[1], values[2]);
            assert!(check_assignable(values[0], values[1], &types).is_err());
            assert!(check_assignable(values[1], values[0], &types).is_err());
            good(
                "comp fn choose() type { return int32; } comp Eligible: type = comp fn() type; callback: Eligible = choose;",
            );
            bad(
                "comp fn choose() type { return int32; } comp Runtime: type = fn() type; callback: Runtime = choose;",
                "incompatible",
            );
            bad(
                "fn choose() type { return int32; } callback: comp fn() type = choose;",
                "incompatible",
            );
        }

        #[test]
        fn generic_runtime_signatures_preserve_comp_callback_eligibility() {
            let (a, mut types) = run("fn accept(T: type, f: comp fn(T) type) comp fn(T) type { return f; } comp fn choose(b: bool) type { return int32; } callback := accept(bool, choose);").unwrap();
            let instance = types.generic_instances.values().next().unwrap();
            let T::Function(signature) = types.get(instance.signature).unwrap().clone() else {
                panic!()
            };
            assert!(!signature.is_comp);
            let callback = signature.return_type;
            assert!(matches!(types.get(callback).unwrap(), T::Function(f) if f.is_comp));
            let T::Function(mut corrupt) = types.get(callback).unwrap().clone() else {
                panic!()
            };
            corrupt.is_comp = false;
            let corrupt = types.intern(T::Function(corrupt));
            let mut signature = signature.clone();
            signature.parameters[1] = corrupt;
            signature.return_type = corrupt;
            let corrupt = types.intern(T::Function(signature));
            types
                .generic_instances
                .values_mut()
                .next()
                .unwrap()
                .signature = corrupt;
            assert!(validate_type_annotations(&a.units, &a.symbols, &types).is_err());
            analyzer("fn identity(T: type, x: T) T { return x; } value := identity(int32, 7);")
                .analyze()
                .unwrap();
        }

        #[test]
        fn declaration_and_type_annotations_validate_eligibility() {
            let (mut a, types) = run(
                "comp fn choose() type { return int32; } comp Callback: type = comp fn() type;",
            )
            .unwrap();
            let StatementKind::FunctionDeclaration(fun) = &mut a.units[0].ast.statements[0].kind
            else {
                panic!()
            };
            fun.signature.is_comp = false;
            assert!(validate_type_annotations(&a.units, &a.symbols, &types).is_err());
            let (mut a, types) = run("comp Callback: type = comp fn() type;").unwrap();
            let StatementKind::VariableDeclaration(var) = &mut a.units[0].ast.statements[0].kind
            else {
                panic!()
            };
            let crate::frontend::ast::expression::ExpressionKind::TypeValue(ty) =
                &mut var.expression.as_mut().unwrap().kind
            else {
                panic!()
            };
            let crate::frontend::ast::type_expression::TypeExpressionKind::Function {
                is_comp, ..
            } = &mut ty.kind
            else {
                panic!()
            };
            *is_comp = false;
            assert!(validate_type_annotations(&a.units, &a.symbols, &types).is_err());
        }

        #[test]
        fn unsupported_comp_declarations_have_diagnostics_even_without_calls() {
            bad("comp fn choose() type;", "body");
            bad("comp fn choose() int32 { return 1; }", "must return type");
            bad(
                "comp fn choose() type { while (true) { return int32; }; }",
                "bounded comp function",
            );
            bad("comp fn choose() type { return; }", "bounded comp function");
            analysis_bad(
                "comp fn choose(b: bool) type { if b { return int32; }; }",
                "return",
            );
        }
    }

    mod generic_body_strategy {
        use super::*;
        use crate::frontend::analysis_annotation::BindingId;
        use crate::frontend::semantic_type::SemanticType as T;

        #[test]
        fn symbolic_arithmetic_is_rechecked_per_instance() {
            good("fn add(T: type, x: T) T { return x + x; }");
            good(
                "fn add(T: type, x: T) T { return x + 1; } a := add(int32, 2); b := add(int8, 3);",
            );
            bad(
                "fn add(T: type, x: T) T { return x + x; } a := add(bool, true);",
                "invalid binary",
            );
            bad(
                "fn add(T: type, x: T) T { return x + true; }",
                "invalid binary",
            );
            bad(
                "fn add(T: type, x: T) T { broken := true + false; return x; }",
                "invalid binary",
            );
            bad(
                "fn f(T: type, x: T) T { return true; } a := f(int32, 1);",
                "incompatible",
            );
        }

        #[test]
        fn dependent_members_and_indices_are_concrete_obligations() {
            good(
                "fn get(T: type, x: T) int32 { return x.value; } comp S := struct { value: int32 }; a := get(S, S { value: 3 });",
            );
            bad(
                "fn get(T: type, x: T) int32 { return x.value; } a := get(int32, 1);",
                "member",
            );
            good("fn first(T: type, x: T) int32 { return x[0]; } a := first(int32[2], [1, 2]);");
            bad(
                "fn first(T: type, x: T) int32 { return x[0]; } a := first(int32, 1);",
                "indexable",
            );
            bad(
                "fn first(T: type, x: T) int32 { return x[2]; } a := first(int32[2], [1, 2]);",
                "bounds",
            );
            bad(
                "fn first(T: type, x: T) int32 { return x[true]; }",
                "integer",
            );
        }

        #[test]
        fn instance_environment_is_owned_and_templates_remain_symbolic() {
            let (a, types) = run("fn add(T: type, x: T) T { value := x + x; return value; } a := add(int32, 2); b := add(int8, 3);").unwrap();
            assert_eq!(types.generic_instances.len(), 2);
            let StatementKind::FunctionDeclaration(template) = &a.units[0].ast.statements[0].kind
            else {
                panic!()
            };
            let parameter = template.signature.parameters[1].annotation.binding.unwrap();
            assert!(matches!(
                types.get(types.bindings[&parameter].ty).unwrap(),
                T::Parameter(_)
            ));
            for instance in types.generic_instances.values() {
                assert!(instance.body.is_some());
                assert!(
                    !types
                        .is_dependent(instance.bindings[&parameter].ty)
                        .unwrap()
                );
                assert_ne!(
                    instance.bindings[&parameter].ty,
                    types.bindings[&parameter].ty
                );
            }
            super::super::super::validation::validate(&a.units, &a.symbols, &types).unwrap();
        }

        #[test]
        fn recursive_instances_and_invalid_recursive_bodies() {
            good("fn f(T: type, x: T) T { return f(T, x); } a := f(int32, 1);");
            good(
                "fn f(T: type, x: T) T { return g(T, x); } fn g(U: type, x: U) U { return f(U, x); } a := f(int32, 1);",
            );
            bad(
                "fn f(T: type, x: T) T { y := f(T, x); return x + x; } a := f(bool, true);",
                "invalid binary",
            );
        }

        #[test]
        fn concrete_flow_and_corrupted_handoff_are_rejected() {
            bad(
                "fn f(T: type) T { x: T; return x; } a := f(int32);",
                "uninitialized",
            );
            let (a, types) = run("fn f(T: type, x: T) T { return x; } a := f(int32, 1);").unwrap();
            let mut corrupt = types.clone();
            corrupt.generic_instances.values_mut().next().unwrap().body = None;
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &corrupt).is_err()
            );
            let mut corrupt = types.clone();
            corrupt
                .generic_instances
                .values_mut()
                .next()
                .unwrap()
                .bindings
                .clear();
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &corrupt).is_err()
            );
            let mut corrupt = types;
            let instance = corrupt.generic_instances.values_mut().next().unwrap();
            let binding = instance
                .bindings
                .keys()
                .copied()
                .find(|id| matches!(id, BindingId::Local(_)))
                .unwrap();
            instance.bindings.get_mut(&binding).unwrap().ty = TypeId(u32::MAX);
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &corrupt).is_err()
            );
        }

        #[test]
        fn failed_instantiation_restores_template_facts_and_does_not_publish_a_body() {
            let (a, mut types) = run("fn add(T: type, x: T) T { return x + x; }").unwrap();
            let StatementKind::FunctionDeclaration(fun) = &a.units[0].ast.statements[0].kind else {
                panic!()
            };
            let Some(BindingId::Declaration(template)) = fun.signature.annotation.binding else {
                panic!()
            };
            let argument = types.builtin(BuiltinType::Boolean);
            let saved_bindings = types.bindings.clone();
            let saved_values = types.type_values.clone();
            let saved_locals = types.locals.clone();
            let mut context = TypeContext::new(FileId(0), &a.symbols, &mut types);
            context.state.functions.insert(template, fun.clone());
            let key = GenericInstanceKey {
                template,
                arguments: vec![argument],
            };
            assert!(resolve_generic_instance(&key, &mut context).is_err());
            assert!(context.state.active_instances.is_empty());
            assert!(!context.types.generic_instances.contains_key(&key));
            assert_eq!(context.types.bindings, saved_bindings);
            assert_eq!(context.types.type_values, saved_values);
            assert_eq!(context.types.locals, saved_locals);
        }

        #[test]
        fn expanding_instantiation_is_bounded() {
            bad(
                "fn f(T: type, x: T) T { return f(*T, x&)*; } a := f(int32, 1);",
                "recursion",
            );
        }

        #[test]
        fn computed_pointer_and_callback_shapes_check_symbolically() {
            good(
                "comp fn ptr(T: type) type { return *T; } fn f(T: type, x: T) T { comp P := ptr(T); p: P = x&; return p*; } a := f(int32, 1);",
            );
            good(
                "comp fn callback(T: type) type { return fn(T) -> T; } fn f(T: type, x: T) T { comp F := callback(T); cb: F; return x; } a := f(int32, 1);",
            );

            good("fn f(T: type, x: T) T { return -x; } a := f(int32, 1);");
            bad(
                "fn f(T: type, x: T) T { return -x; } a := f(bool, true);",
                "unary",
            );
            bad(
                "fn f(T: type, x: T[2]) T[2] { return x + x; }",
                "invalid binary",
            );
            bad(
                "fn f(T: type, x: T) bool { return x < true; }",
                "invalid binary",
            );
            bad("fn f(T: type, x: T) T { return x / 0; }", "zero");
            bad("fn f(T: type, x: T) T { return x << -1; }", "range");
        }

        #[test]
        fn nested_computation_identity_and_boolean_recipes() {
            let (a, types) = run("comp fn box(T: type) type { return struct { value: T }; } comp fn forward(T: type) type { return box(T); } comp A := forward(int32); comp B := forward(int32); comp C := forward(int8);").unwrap();
            let values: Vec<_> = a.units[0].ast.statements[2..]
                .iter()
                .map(|node| {
                    let StatementKind::VariableDeclaration(v) = &node.kind else {
                        panic!()
                    };
                    types.type_values[&v.annotation.binding.unwrap()]
                })
                .collect();
            assert_eq!(values[0], values[1]);
            assert_ne!(values[0], values[2]);
            assert!(
                matches!(types.get(values[0]).unwrap(), T::Struct(fields) if matches!(types.get(fields[0].ty).unwrap(), T::Builtin(BuiltinType::Int32)))
            );
            good(
                "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp fn forward(b: bool) type { return choose(!b); } comp A := forward(false); x: A = 127;",
            );
            bad(
                "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp fn forward(b: bool) type { return choose(!b); } comp A := forward(false); x: A = 128;",
                "range",
            );
            bad(
                "comp fn loop(T: type) type { return loop(T); } comp A := loop(int32);",
                "recursion",
            );
        }

        #[test]
        fn corrupted_body_and_computation_dependencies_are_rejected() {
            let (a, mut types) =
                run("fn f(T: type, x: T) T { return x; } a := f(int32, 1);").unwrap();
            let body = types
                .generic_instances
                .values_mut()
                .next()
                .unwrap()
                .body
                .as_mut()
                .unwrap()
                .body
                .as_mut()
                .unwrap();
            body.statements.clear();
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &types).is_err()
            );
            let (a, mut types) =
                run("fn add(T: type, x: T) T { return x + x; } a := add(int32, 1);").unwrap();
            let boolean = types.builtin(BuiltinType::Boolean);
            let instance = types.generic_instances.values_mut().next().unwrap();
            let StatementKind::Return(Some(values)) = &mut instance
                .body
                .as_mut()
                .unwrap()
                .body
                .as_mut()
                .unwrap()
                .statements[0]
                .kind
            else {
                panic!()
            };
            values[0].annotation.resolved.as_mut().unwrap().ty = boolean;
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &types).is_err()
            );
            let (a, mut types) = run("comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp fn forward(b: bool) type { return choose(b); }").unwrap();
            let recipe = types
                .types
                .iter_mut()
                .find(|t| matches!(t, T::TypeResult { .. }))
                .unwrap();
            let T::TypeResult { arguments, .. } = recipe else {
                unreachable!()
            };
            arguments.clear();
            assert!(
                super::super::super::validation::validate(&a.units, &a.symbols, &types).is_err()
            );
        }

        #[test]
        fn retained_computations_resolve_nested_types() {
            good(
                "comp fn choose(T: type, b: bool) type { if b { return *T; } else { return T; }; } comp fn forward(T: type, b: bool) type { comp R := choose(T, b); return R; } comp P := forward(int32, true); x := 1; p: P = x&;",
            );
            good(
                "comp fn ptr(T: type) type { return *T; } comp fn wrap(T: type) type { comp P := ptr(T); return struct { values: P[2] }; } fn f(T: type, x: T) T { comp P := wrap(T); return x; } a := f(int32, 1);",
            );
            good(
                "comp fn ptr(T: type) type { return *T; } comp fn forward(T: type) type { return ptr(T); } comp P := forward(int32); x := 1; p: P = x&;",
            );
            bad(
                "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } fn f(b: bool) { comp T := choose(b); x: T = 1; }",
                "known type or bool",
            );
        }
    }

    mod completion_acceptance {
        use super::*;
        #[test]
        fn generic_specialization() {
            good("fn identity(T: type, x: T) T { return x; } result := identity(int32, 7);");
        }
        #[test]
        fn compile_time_function_evaluation() {
            good("comp fn choose() type { return int32; } comp T := choose(); value: T = 1;");
        }
        #[test]
        fn definite_initialization() {
            analysis_bad(
                "fn f() int32 { value: int32; return value; }",
                "uninitialized",
            );
        }
        #[test]
        fn all_return_paths() {
            analysis_bad("fn f(x: bool) int32 { if x { return 1; }; }", "return");
        }
        #[test]
        fn defer_semantics() {
            bad("fn f() { defer return; }", "defer");
        }
        #[test]
        fn complete_project_analysis() {
            analyzer("fn f() int32 { return 1; }")
                .analyze()
                .expect("complete semantic analysis remains required");
        }
    }
}

mod generic_flow_regressions {
    //! Full-pipeline generic flow and transactional instance-cache regressions.

    use super::super::generics::{GenericInstanceKey, resolve_generic_instance};
    use crate::{
        diagnostics::{CompilerError, ErrorKind},
        driver::CompilationUnit,
        frontend::{
            analysis_annotation::BindingId,
            analyzer::{AnalyzedProject, Analyzer, TypeContext},
            ast::statement::StatementKind,
            parser::Parser,
            tokenizer::Tokenizer,
            tokens::builtin::BuiltinType,
        },
        plan::{BuildPlan, FileId, SourceFile},
    };
    use std::path::Path;

    fn analyzer(source: &str) -> Analyzer {
        let ast = Parser::with_source(
            Tokenizer::new(source),
            Path::new("generic_flow.fib"),
            source,
        )
        .parse()
        .unwrap_or_else(|error| panic!("fixture did not parse: {source}\n{error:#}"));
        Analyzer::new(
            BuildPlan {
                files: vec![SourceFile {
                    id: FileId(0),
                    name: "main".to_string().into(),
                    path: "generic_flow.fib".into(),
                }],
                entry: FileId(0),
                output: "unused".into(),
            },
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )
        .unwrap()
    }

    fn analyze(source: &str) -> AnalyzedProject {
        analyzer(source).analyze().unwrap_or_else(|error| {
            panic!("expected full analysis to succeed: {source}\n{error:#}")
        })
    }

    fn reject(source: &str, fragment: &str) {
        let error = analyzer(source)
            .analyze()
            .expect_err("invalid program passed full analysis");
        let diagnostic = error
            .downcast_ref::<CompilerError>()
            .unwrap_or_else(|| panic!("expected a structured diagnostic: {error:#}"));
        assert_eq!(diagnostic.kind, ErrorKind::Type, "{error:#}");
        assert!(diagnostic.span.is_some(), "{error:#}");
        assert!(
            diagnostic.message.contains(fragment),
            "expected {fragment:?}: {error:#}"
        );
    }

    #[test]
    fn dependent_return_completeness_is_checked_on_the_concrete_instance() {
        analyze("fn f(T: type) T {} fn main() { f(void); }");
        analyze("fn f(T: type) T {}");
        reject(
            "fn f(T: type) T {} fn main() { f(int32); }",
            "return a value on all paths",
        );
        reject("fn f(T: type) int32 {}", "return a value on all paths");
        reject("fn f(T: type) *T {}", "return a value on all paths");
    }

    #[test]
    fn dependent_struct_projections_can_initialize_the_concrete_value() {
        analyze(
            "comp S := struct { x: int32 }; fn make(T: type) T { value: T; value.x = 1; return value; } fn main() { value := make(S); }",
        );
        reject(
            "comp S := struct { x: int32, y: int32 }; fn make(T: type) T { value: T; value.x = 1; return value; } fn main() { value := make(S); }",
            "uninitialized",
        );
    }

    #[test]
    fn dependent_index_projections_can_initialize_the_concrete_value() {
        analyze(
            "fn make(T: type) T { value: T; value[0] = 1; return value; } fn main() { value := make(int32[1]); }",
        );
        reject(
            "fn make(T: type) T { value: T; value[0] = 1; return value; } fn main() { value := make(int32[2]); }",
            "uninitialized",
        );
    }

    #[test]
    fn template_initializer_calls_do_not_reapply_concrete_projection_rules() {
        analyze(
            "comp S := struct { x: int32 }; fn make(T: type) T { value: T; value.x = 1; return value; } value := make(S);",
        );
    }

    #[test]
    fn template_flow_still_checks_independent_reads_and_index_addresses() {
        reject(
            "fn f(T: type) { value: int32; copy := value; }",
            "uninitialized",
        );
        reject(
            "fn make(T: type) T { value: T; index: int32; value[index] = 1; return value; }",
            "uninitialized",
        );
        reject(
            "fn f(T: type, b: bool) { value: int32; if b { value = 1; }; copy := value; }",
            "uninitialized",
        );
    }

    #[test]
    fn template_flow_still_checks_independent_loop_and_defer_obligations() {
        reject("fn f(T: type) { break; }", "loop");
        reject("fn f(T: type) { continue; }", "loop");
        reject("fn f(T: type) { defer return; }", "defer");
        reject(
            "fn take(value: int32); fn f(T: type) { value: int32; defer take(value); }",
            "uninitialized",
        );
        analyze(
            "fn take(value: int32); fn f(T: type) { value: int32; defer take(value); value = 1; }",
        );
    }

    #[test]
    fn concrete_dependent_reads_remain_initialization_obligations() {
        reject(
            "fn f(T: type) T { value: T; return value; } fn main() { value := f(int32); }",
            "uninitialized",
        );
        analyze(
            "comp Empty := struct {}; fn f(T: type) T { value: T; return value; } fn main() { value := f(Empty); }",
        );
    }

    #[test]
    fn full_analysis_rejects_invalid_recursive_specializations() {
        analyze(
            "fn f(T: type, x: T) T { y := g(T, x); return x + x; } fn g(U: type, x: U) U { return f(U, x); } fn main() { value := f(int32, 1); }",
        );
        reject(
            "fn f(T: type, x: T) T { y := g(T, x); return x + x; } fn g(U: type, x: U) U { return f(U, x); } fn main() { value := f(bool, true); }",
            "invalid binary",
        );
    }

    #[test]
    fn failed_recursive_group_rolls_back_new_siblings_but_preserves_old_instances() {
        let mut analyzer = analyzer(
            "fn f(T: type, x: T) T { y := g(T, x); return x + x; } fn g(U: type, x: U) U { return f(U, x); }",
        );
        analyzer.collect_declarations().unwrap();
        analyzer.resolve_names().unwrap();
        let mut types = analyzer.resolve_types().unwrap();
        let mut functions = Vec::new();
        for node in &analyzer.units[0].ast.statements {
            let StatementKind::FunctionDeclaration(fun) = &node.kind else {
                panic!("expected a function fixture");
            };
            let Some(BindingId::Declaration(id)) = fun.signature.annotation.binding else {
                panic!("expected a resolved template identity");
            };
            functions.push((id, fun.clone()));
        }
        let f = functions[0].0;
        let g = functions[1].0;
        let int32 = types.builtin(BuiltinType::Int32);
        let boolean = types.builtin(BuiltinType::Boolean);
        let mut context = TypeContext::new(FileId(0), &analyzer.symbols, &mut types);
        context.state.functions.extend(functions);
        resolve_generic_instance(
            &GenericInstanceKey {
                template: f,
                arguments: vec![int32],
            },
            &mut context,
        )
        .unwrap();
        assert!(
            context
                .types
                .generic_instances
                .contains_key(&GenericInstanceKey {
                    template: g,
                    arguments: vec![int32],
                })
        );
        let saved_instances = context.types.generic_instances.clone();
        let saved_bindings = context.types.bindings.clone();
        let saved_values = context.types.type_values.clone();
        let saved_locals = context.types.locals.clone();
        let saved_declarations = context.types.declarations.clone();

        // g(bool) finishes using f(bool)'s provisional signature before f's
        // arithmetic fails. Neither member of that group may survive the failure.
        for template in [f, g] {
            let error = resolve_generic_instance(
                &GenericInstanceKey {
                    template,
                    arguments: vec![boolean],
                },
                &mut context,
            )
            .expect_err("invalid recursive group was cached as checked");
            let diagnostic = error.downcast_ref::<CompilerError>().unwrap();
            assert_eq!(diagnostic.kind, ErrorKind::Type);
            assert!(diagnostic.message.contains("invalid binary"), "{error:#}");
            assert!(context.state.active_instances.is_empty());
            assert_eq!(context.types.generic_instances, saved_instances);
            assert_eq!(context.types.bindings, saved_bindings);
            assert_eq!(context.types.type_values, saved_values);
            assert_eq!(context.types.locals, saved_locals);
            assert_eq!(context.types.declarations, saved_declarations);
        }
    }
}

mod validation_regressions {
    //! Corruption checks independent of inference and the existing analyzer suite.
    use super::super::validation::validate;
    use crate::{
        diagnostics::{CompilerError, ErrorKind},
        driver::CompilationUnit,
        frontend::{
            analysis_annotation::{ExpressionInfo, ValueCategory as C},
            analyzer::{AnalyzedProject, Analyzer},
            ast::{
                expression::{Expression, ExpressionKind as E},
                function_declaration::FunctionDeclaration,
                statement::StatementKind as S,
            },
            parser::Parser,
            tokenizer::Tokenizer,
            tokens::{Literal, builtin::BuiltinType as B},
        },
        plan::{BuildPlan, FileId, SourceFile},
    };
    use std::path::Path;

    fn analyze(source: &str) -> AnalyzedProject {
        let ast = Parser::with_source(Tokenizer::new(source), Path::new("validation.fib"), source)
            .parse()
            .unwrap_or_else(|error| panic!("fixture did not parse: {source}\n{error:#}"));
        Analyzer::new(
            BuildPlan {
                files: vec![SourceFile {
                    id: FileId(0),
                    name: "main".to_string().into(),
                    path: "validation.fib".into(),
                }],
                entry: FileId(0),
                output: "unused".into(),
            },
            vec![CompilationUnit {
                source_file_id: FileId(0),
                ast,
            }],
        )
        .unwrap()
        .analyze()
        .unwrap_or_else(|error| panic!("fixture failed analysis: {source}\n{error:#}"))
    }

    fn reject(project: &AnalyzedProject, fragment: &str) {
        let error = validate(&project.units, &project.symbols, &project.types)
            .expect_err("corrupt annotations passed independent validation");
        let diagnostic = error
            .downcast_ref::<CompilerError>()
            .unwrap_or_else(|| panic!("expected a structured type diagnostic: {error:#}"));
        assert_eq!(diagnostic.kind, ErrorKind::Type, "{error:#}");
        assert!(diagnostic.span.is_some(), "{error:#}");
        assert!(diagnostic.message.contains(fragment), "{error:#}");
    }

    fn function(project: &mut AnalyzedProject, index: usize) -> &mut FunctionDeclaration {
        let S::FunctionDeclaration(fun) = &mut project.units[0].ast.statements[index].kind else {
            panic!("expected function");
        };
        fun
    }

    fn returns(fun: &mut FunctionDeclaration) -> &mut Option<Vec<Expression>> {
        let S::Return(values) = &mut fun.body.as_mut().unwrap().statements[0].kind else {
            panic!("expected return");
        };
        values
    }

    #[test]
    fn concrete_instance_return_literals_are_not_trusted() {
        for builtin in [B::Boolean, B::Int64] {
            let mut project = analyze("fn f(T: type) T { return 1; } value := f(int32);");
            let ty = project.types.builtin(builtin.clone());
            let instance = project.types.generic_instances.values_mut().next().unwrap();
            returns(instance.body.as_mut().unwrap()).as_mut().unwrap()[0]
                .annotation
                .resolved
                .as_mut()
                .unwrap()
                .ty = ty;
            reject(
                &project,
                if builtin == B::Boolean {
                    "literal kind"
                } else {
                    "literal context"
                },
            );
        }
    }

    #[test]
    fn return_types_are_directional_and_scoped_to_each_function() {
        let mut project = analyze("fn f() int32 { return 1; } fn g() bool { return true; }");
        let boolean = project.types.builtin(B::Boolean);
        let expr = &mut returns(function(&mut project, 0)).as_mut().unwrap()[0];
        expr.kind = E::Literal(Literal::Boolean(true));
        expr.annotation.resolved = Some(ExpressionInfo {
            ty: boolean,
            category: C::Value,
            type_value: None,
        });
        reject(&project, "incompatible");

        let mut project = analyze("fn f() never { return f(); }");
        let int32 = project.types.builtin(B::Int32);
        let expr = &mut returns(function(&mut project, 0)).as_mut().unwrap()[0];
        expr.kind = E::Literal(Literal::Integer(1));
        expr.annotation.resolved = Some(ExpressionInfo {
            ty: int32,
            category: C::Value,
            type_value: None,
        });
        reject(&project, "incompatible");
    }

    #[test]
    fn returns_cannot_change_container_qualification() {
        for (from, to) in [
            ("const *int32", "*int32"),
            ("const int32[]", "int32[]"),
            ("const int32[1]", "int32[1]"),
        ] {
            let source = format!("fn f(p: {from}, q: {to}) {from} {{ q; return p; }}");
            let mut project = analyze(&source);
            let nodes = &mut function(&mut project, 0).body.as_mut().unwrap().statements;
            let S::ExpressionStatement(replacement) = &nodes[0].kind else {
                panic!("expected expression");
            };
            let replacement = replacement.clone();
            let S::Return(Some(values)) = &mut nodes[1].kind else {
                panic!("expected return");
            };
            values[0] = replacement;
            reject(&project, "incompatible");
        }
    }

    #[test]
    fn return_outside_a_function_is_not_annotation_complete() {
        let mut project = analyze("fn f() { return; }");
        let statement = function(&mut project, 0).body.as_ref().unwrap().statements[0].clone();
        project.units[0].ast.statements.push(statement);
        reject(&project, "return outside function");
    }

    #[test]
    fn symbolic_literal_defaults_are_still_validated() {
        let mut project = analyze("fn f(T: type) T { return 1; }");
        let int64 = project.types.builtin(B::Int64);
        returns(function(&mut project, 0)).as_mut().unwrap()[0]
            .annotation
            .resolved
            .as_mut()
            .unwrap()
            .ty = int64;
        reject(&project, "literal context");
    }

    #[test]
    fn comp_returns_must_still_produce_metatype_values() {
        let mut project = analyze("comp fn f() type { return int32; }");
        let boolean = project.types.builtin(B::Boolean);
        let expr = &mut returns(function(&mut project, 0)).as_mut().unwrap()[0];
        expr.kind = E::Literal(Literal::Boolean(true));
        expr.annotation.resolved = Some(ExpressionInfo {
            ty: boolean,
            category: C::Value,
            type_value: None,
        });
        reject(&project, "incompatible");
    }

    #[test]
    fn return_arity_and_tuple_elements_are_checked() {
        let mut project = analyze("fn f() int32 { return 1; }");
        *returns(function(&mut project, 0)) = None;
        reject(&project, "empty return");

        let mut project = analyze("fn f() int32 { return 1; }");
        let values = returns(function(&mut project, 0)).as_mut().unwrap();
        values.push(values[0].clone());
        reject(&project, "arity");

        let mut project = analyze("fn f() (int32, bool) { return 1, true; }");
        returns(function(&mut project, 0)).as_mut().unwrap().pop();
        reject(&project, "incompatible");

        let mut project = analyze("fn f() (int32, bool) { return 1, true; }");
        returns(function(&mut project, 0))
            .as_mut()
            .unwrap()
            .swap(0, 1);
        reject(&project, "incompatible");

        let mut project = analyze("fn f() (int32, bool) { return 1, true; }");
        let values = returns(function(&mut project, 0)).as_mut().unwrap();
        values.push(values[0].clone());
        reject(&project, "arity");
    }

    #[test]
    fn literals_must_match_their_kind_and_fit_their_context() {
        for (source, replacement) in [
            ("fn f() int8 { return 1; }", Literal::Integer(128)),
            ("fn f() uint8 { return 1; }", Literal::Integer(256)),
            ("fn f() float16 { return 1.0; }", Literal::Float(70000.0)),
        ] {
            let mut project = analyze(source);
            returns(function(&mut project, 0)).as_mut().unwrap()[0].kind = E::Literal(replacement);
            reject(&project, "out of range");
        }
        for source in [
            "fn f() bool { return true; }",
            "fn f() char { return 'a'; }",
            "fn f() string { return \"x\"; }",
            "fn f() *int32 { return null; }",
        ] {
            let mut project = analyze(source);
            let int32 = project.types.builtin(B::Int32);
            returns(function(&mut project, 0)).as_mut().unwrap()[0]
                .annotation
                .resolved
                .as_mut()
                .unwrap()
                .ty = int32;
            reject(&project, "literal kind");
        }
    }

    #[test]
    fn array_return_elements_use_the_array_context() {
        let mut project = analyze("fn f() int8[1] { return [1]; }");
        let int32 = project.types.builtin(B::Int32);
        let E::ArrayLiteral { elements } =
            &mut returns(function(&mut project, 0)).as_mut().unwrap()[0].kind
        else {
            panic!("expected array literal");
        };
        elements[0].annotation.resolved.as_mut().unwrap().ty = int32;
        reject(&project, "literal context");
    }

    #[test]
    fn valid_return_contexts_remain_supported() {
        for source in [
            "fn f() { return; }",
            "fn f() int8 { return -128; }",
            "fn f() int16 { return -32768; }",
            "fn f(b: bool) int32 { if b { return 1; } else { return 2; }; }",
            "fn f(b: bool) int32 { while (b) { return 1; }; return 2; }",
            "fn f() float32 { return 1; }",
            "fn f() int8[1] { return [-128]; }",
            "fn f() *int32 { return null; }",
            "fn pair() (int32, bool) { return 1, true; } fn f() (int32, bool) { return pair(); }",
            "fn stop() never; fn f() int32 { return stop(); }",
            "fn stop() never; fn f() (int32, bool) { return stop(), true; }",
            "fn f(T: type) T { return 1; }",
            "fn f(T: type) T { return; } fn g() { f(void); }",
            "comp fn choose(T: type) type { return T; } comp T := choose(int32);",
            "comp fn choose(b: bool) type { if b { return int8; } else { return int32; }; } comp fn forward(b: bool) type { return choose(b); }",
        ] {
            let project = analyze(source);
            validate(&project.units, &project.symbols, &project.types).unwrap();
        }
    }

    #[test]
    fn coherently_rewritten_cast_results_cannot_bypass_cast_legality() {
        for (from, to) in [
            ("const int32[]", "int32[]"),
            ("const int32[1]", "int32[1]"),
            ("const *int32", "*int32"),
            ("*const *int32", "*void"),
            ("*const int32[]", "*void"),
            ("*const int32[1]", "*void"),
            ("(const int32[], int32)[]", "(int32[], int32)[]"),
            ("(const int32[], int32)", "(int32[], int32)"),
            ("const int32[][1]", "int32[][1]"),
            ("int32[]", "bool"),
            ("int32[1]", "int32[2]"),
            ("int32", "bool"),
            ("bool", "int32"),
            ("*int32", "int64"),
            ("int64", "*int32"),
        ] {
            let source = format!("fn f(p: {from}, target: {to}) {{ result := p as {from}; }}");
            let mut project = analyze(&source);
            let fun = function(&mut project, 0);
            let target = fun.signature.parameters[1].parameter_type.clone();
            let ty = target.annotation.resolved.unwrap();
            let S::VariableDeclaration(var) = &mut fun.body.as_mut().unwrap().statements[0].kind
            else {
                panic!("expected result binding");
            };
            let binding = var.annotation.binding.unwrap();
            let expr = var.expression.as_mut().unwrap();
            let E::Cast { target_type, .. } = &mut expr.kind else {
                panic!("expected cast");
            };
            *target_type = target;
            expr.annotation.resolved.as_mut().unwrap().ty = ty;
            let mut info = project.types.bindings[&binding];
            info.ty = ty;
            project.types.bind(binding, info);
            reject(&project, "unsupported cast");
        }
    }

    #[test]
    fn numeric_cast_ranges_are_revalidated() {
        let mut project = analyze("fn f() { result := 127 as int8; }");
        let fun = function(&mut project, 0);
        let S::VariableDeclaration(var) = &mut fun.body.as_mut().unwrap().statements[0].kind else {
            panic!("expected binding");
        };
        let E::Cast { expr, .. } = &mut var.expression.as_mut().unwrap().kind else {
            panic!("expected cast");
        };
        expr.kind = E::Literal(Literal::Integer(128));
        reject(&project, "cast value is out of range");
    }

    #[test]
    fn legal_cast_matrix_remains_supported() {
        for source in [
            "fn f(p: const int32[]) { q := p as const int32[]; }",
            "fn f(p: const int32[1]) { q := p as const int32[1]; }",
            "fn f(p: const *int32) { q := p as const *void; r := q as const *int32; }",
            "fn f(p: *int32) { q := p as *void; r := q as *int32; }",
            "fn f(x: int32) { y := x as float64; z := y as int64; }",
            "fn f() { p := null as *int32; }",
            "fn stop() never; fn f() int32 { return stop() as int32; }",
            "fn f(T: type, x: T) int32 { return x as int32; }",
        ] {
            let project = analyze(source);
            validate(&project.units, &project.symbols, &project.types).unwrap();
        }
    }
}
