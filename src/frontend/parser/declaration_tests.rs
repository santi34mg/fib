use std::path::Path;

use super::Parser;
use crate::frontend::{
    ast::{expression::ExpressionKind, statement::StatementKind},
    tokenizer::Tokenizer,
};

fn parse(source: &str) -> anyhow::Result<crate::frontend::ast::Ast> {
    Parser::with_source(
        Tokenizer::new(source),
        Path::new("declarations.fib"),
        source,
    )
    .parse()
}

#[test]
fn unified_declaration_modifiers() {
    for prefix in ["", "pub ", "const ", "comp ", "pub const ", "pub comp "] {
        for binding in [
            "value := 1;",
            "value: int32 = 1;",
            "Value: type = int32;",
            "Value := struct { field: int32 };",
            "Value: type = *int32;",
        ] {
            let source = format!("{prefix}{binding}");
            let ast = parse(&source).unwrap_or_else(|error| panic!("{source}: {error}"));
            let StatementKind::VariableDeclaration(declaration) = &ast.statements[0].kind else {
                panic!("expected binding: {source}");
            };
            assert_eq!(declaration.is_pub, prefix.contains("pub"));
            assert_eq!(declaration.is_const, prefix.contains("const"));
            assert_eq!(declaration.is_comp, prefix.contains("comp"));
            assert_eq!(ast.statements[0].span.line, 1);
            assert_eq!(ast.statements[0].span.column, 1);
        }
    }
}

#[test]
fn unified_multi_declarations() {
    for source in [
        "pub comp a: int32, const b, pub c := 1, 2, 3;",
        "pub A: type, comp B: type = struct { x: int32 }, enum { Done };",
        "pub comp A, pub const B := int32, bool;",
    ] {
        let ast = parse(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let StatementKind::MultiVariableDeclaration { bindings, values } = &ast.statements[0].kind
        else {
            panic!("expected multi-declaration: {source}");
        };
        assert!(bindings.len() >= 2);
        assert_eq!(bindings.len(), values.len());
        let flags: Vec<_> = bindings
            .iter()
            .map(|binding| (binding.is_pub, binding.is_const, binding.is_comp))
            .collect();
        let expected = match bindings.len() {
            3 => vec![
                (true, false, true),
                (false, true, false),
                (true, false, false),
            ],
            _ if source.starts_with("pub comp") => vec![(true, false, true), (true, true, false)],
            _ => vec![(true, false, false), (false, false, true)],
        };
        assert_eq!(flags, expected);
    }
}

#[test]
fn unified_types_are_binding_initializers() {
    for source in [
        "Value: type = struct { x: int32 };",
        "Value := enum { Ready };",
        "Value: type = int32[4];",
        "Value: type = (int32, string);",
        "Value := (pkg::First, *Second);",
        "Value: type = (int32, string)[4];",
        "Value: type = fn(int32) -> int32;",
    ] {
        let ast = parse(source).unwrap_or_else(|error| panic!("{source}: {error}"));
        let StatementKind::VariableDeclaration(binding) = &ast.statements[0].kind else {
            panic!("expected binding")
        };
        assert!(matches!(
            binding.expression.as_ref().unwrap().kind,
            ExpressionKind::TypeValue(_)
        ));
        assert!(binding.annotation.binding.is_none());
    }
    parse("fn f() { comp Local := struct { x: int32 }; value: Local = Local { x: 1 }; }").unwrap();
    for source in [
        "pub comp Alias: type = pkg::Value;",
        "pub comp Alias := make_type(int32);",
    ] {
        let ast = parse(source).unwrap();
        let StatementKind::VariableDeclaration(binding) = &ast.statements[0].kind else {
            panic!("expected binding")
        };
        assert!(matches!(
            binding.expression.as_ref().unwrap().kind,
            ExpressionKind::QualifiedIdentifier(_) | ExpressionKind::Call { .. }
        ));
    }
}

#[test]
fn malformed_unified_declarations_are_rejected() {
    for source in [
        "pub pub x := 1;",
        "const comp x := 1;",
        "comp const x := 1;",
        "comp comp x := 1;",
        "pub const const x := 1;",
        "const pub x := 1;",
        "comp pub x := 1;",
        "pub := 1;",
        "pub comp x = 1;",
        "pub comp x :=;",
        "pub comp a, const comp b := 1, 2;",
        "pub a, comp := 1;",
        "Value: type = struct { x: };",
        "Value: type = ;",
        "type Value int32",
        "Value: type = int32",
    ] {
        assert!(parse(source).is_err(), "unexpectedly accepted {source}");
    }
    let error = parse("pub const comp x := 1;").unwrap_err();
    let diagnostic = error
        .downcast_ref::<super::ParseError>()
        .expect("source-aware declaration error");
    assert!(diagnostic.message.contains("expected variable name"));
    assert_eq!((diagnostic.line, diagnostic.column), (1, 11));
}
