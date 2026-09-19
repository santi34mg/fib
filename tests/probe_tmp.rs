use std::collections::HashMap;
use std::path::Path;

use fibc::frontend::analyze::analyze;
use fibc::frontend::lexer::Lexer;
use fibc::frontend::parser::Parser;

fn parse_err(src: &str) -> String {
    let lexer = Lexer::new(src);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens.into_iter(), Path::new("probe"), src.to_string());
    match parser.parse() {
        Ok(_) => "PARSE-OK".to_string(),
        Err(e) => format!(
            "PARSE-ERR msg={:?} line={} col={}",
            e.message, e.line, e.column
        ),
    }
}

fn analyze_msg(src: &str) -> String {
    let lexer = Lexer::new(src);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens.into_iter(), Path::new("probe"), src.to_string());
    match parser.parse() {
        Ok(ast) => match analyze(ast, &HashMap::new()) {
            Ok(_) => "ANALYZE-OK".to_string(),
            Err(e) => format!("ANALYZE-ERR msg={:?} span={:?}", e.message, e.span),
        },
        Err(e) => format!("PARSE-ERR msg={:?}", e.message),
    }
}

#[test]
fn probe_parser_shapes() {
    let cases = [
        "unclosed-fn-body",
        "truncated-return",
        "truncated-expr",
        "unclosed-call",
        "unclosed-struct",
        "stray-else",
        "unclosed-str-call",
        "truncated-for",
        "unclosed-switch",
        "bad-param",
        "unclosed-enum",
        "truncated-import-brace",
        "break-outside",
        "dup-struct-parse",
        "missing-semi-fn",
        "top-level-stmt",
    ];
    let srcs = [
        "fn f() {",
        "fn f() @int4 { return",
        "fn f() { x := }",
        "fn f() { foo(1, 2 }",
        "fn f() { x := Point { x: 1 }",
        "fn f() { else { } }",
        "fn f() { x := @str_len(\"hi\" }",
        "fn f() { for (i := 0; i < 10 }",
        "fn f(c: @int4) @void { switch (c) { when .R { } }",
        "fn f(@int4 x) @void { }",
        "type Color enum { Red",
        "import std::io::{",
        "fn f() @void { break }",
        "fn f() { x := Point { x: 1, x: 2 } }",
        "fn f() @int4 { return 1 ",
        "x := 1",
    ];
    for (n, s) in cases.iter().zip(srcs.iter()) {
        println!("PARSER {} {:?} => {}", n, s, parse_err(s));
    }
}

#[test]
fn probe_analyze_shapes() {
    let cases = [
        "ret-mismatch",
        "ret-void-with-val",
        "ret-missing-val",
        "break-outside",
        "continue-outside",
        "payload-assign",
        "payload-field-assign",
        "assign-to-func",
        "switch-unknown-variant",
        "switch-nonexhaustive",
        "switch-exhaustive-else",
        "multi-assign-const",
        "ret-undefined",
    ];
    let srcs = [
        "fn f() @int4 { return \"s\" }",
        "fn f() @void { return 1 }",
        "fn f() @int4 { return }",
        "fn f() @void { break }",
        "fn f() @void { continue }",
        "type T enum { A { v: @int4 }, B }\nfn f(t: T) @int4 { switch (t) { when .A(x) { x = 1\n return 0 } when else { return 1 } } }",
        "type T enum { A { v: @int4 }, B }\nfn f(t: T) @int4 { switch (t) { when .A(x) { x.v = 1\n return 0 } when else { return 1 } } }",
        "fn foo() @int4 { return 1 }\nfn f() { foo = 1 }",
        "type T enum { A, B }\nfn f(t: T) @void { switch (t) { when .C { } when else { } } }",
        "type T enum { A, B }\nfn f(t: T) @int4 { switch (t) { when .A { return 0 } } }",
        "type T enum { A, B }\nfn f(t: T) @int4 { switch (t) { when .A { return 0 } when else { return 1 } } }",
        "type T enum { A { v: @int4 }, B }\nfn f(t: T) @void { switch (t) { when .A(x) { x, y := 1, 2 } when else { } } }",
        "fn f() @int4 { return nope }",
    ];
    for (n, s) in cases.iter().zip(srcs.iter()) {
        println!("ANALYZE {} => {}", n, analyze_msg(s));
    }
}

#[test]
fn probe_lexer_shapes() {
    let cases = [
        ("unterminated-string", "\"abc"),
        ("unterminated-str-escape", "\"abc\\"),
        ("unterminated-char", "'"),
        ("unterminated-char-escape", "'\\"),
        ("unterminated-hex", "'\\x1"),
        ("unterminated-hex2", "'\\x"),
        ("bad-char-escape", "'\\q'"),
        ("bad-hex", "'\\xZZ'"),
        ("bad-unicode", "'\\u004'"),
        ("dollar", "$"),
        ("char-ok", "'a'"),
    ];
    for (n, s) in cases {
        let toks: Vec<_> = Lexer::new(s).collect();
        println!(
            "LEXER {} {:?} => {:?}",
            n,
            s,
            toks.iter().map(|t| &t.kind).collect::<Vec<_>>()
        );
    }
}
