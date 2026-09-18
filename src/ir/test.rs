// ---------------------------------------------------------------------------
// Tests: the IR contract (pretty-print + error cases)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::{IrProgram, lower_typed_program};
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use std::path::PathBuf;

    fn lower_src(src: &str) -> Result<IrProgram, String> {
        let tokens: Vec<_> = Lexer::new(src).collect();
        let path = PathBuf::from("test.fib");
        let mut parser = Parser::new(tokens.into_iter(), &path, src.to_string());
        let ast = parser.parse().map_err(|e| e.to_string())?;
        let typed = crate::frontend::analyze::analyze(ast, &Default::default())
            .map_err(|e| e.to_string())?;
        lower_typed_program(typed).map_err(|e| e.to_string())
    }

    #[test]
    fn ir_straight_line_binding_and_return() {
        let prog = lower_src("fn main() @int { x := 1\ny := 2\nreturn x + y }")
            .expect("lower straight-line");
        assert_eq!(prog.functions.len(), 1);
        let text = prog.to_string();
        assert!(
            text.contains("alloca x"),
            "expected alloca x, got:\n{}",
            text
        );
        assert!(
            text.contains("alloca y"),
            "expected alloca y, got:\n{}",
            text
        );
        assert!(text.contains("add"), "expected add, got:\n{}", text);
        assert!(text.contains("return"), "expected return, got:\n{}", text);
    }

    #[test]
    fn ir_if_and_for_emit_labels_and_gotos() {
        let prog = lower_src(
            "fn main() @int { x := 0\nif x == 0 { x = 1 } else { x = 2 }\nfor (i: @int = 0; i < 10; i = i + 1) { x = x + i }\nreturn x }",
        )
        .expect("lower if+for");
        let text = prog.to_string();
        assert!(text.contains("if "), "expected IfGoto, got:\n{}", text);
        assert!(text.contains("goto "), "expected Goto, got:\n{}", text);
        assert!(text.contains("return"), "expected return, got:\n{}", text);
    }

    #[test]
    fn ir_short_circuit_uses_join_temp() {
        let prog = lower_src("fn main() @bool { a := true\nb := false\nreturn a && b }")
            .expect("lower &&");
        let text = prog.to_string();
        // Join pattern: seed copy + rhs copy + merge label.
        assert!(text.contains("copy"), "expected join copy, got:\n{}", text);
        assert!(text.contains("if "), "expected IfGoto, got:\n{}", text);
    }

    #[test]
    fn ir_defer_runs_before_return() {
        let prog =
            lower_src("fn main() @int { x := 0\ndefer x = 1\nreturn x }").expect("lower defer");
        let text = prog.to_string();
        // Deferred store must appear before the return.
        let store_pos = text.find("store ").expect("expected store");
        let ret_pos = text.find("return").expect("expected return");
        assert!(
            store_pos < ret_pos,
            "defer store should precede return:\n{}",
            text
        );
    }

    #[test]
    fn ir_break_continue_become_gotos() {
        let prog = lower_src(
            "fn main() @int { x := 0\nfor (i: @int = 0; i < 10; i = i + 1) { if i == 2 { continue }\nif i == 5 { break }\nx = x + i }\nreturn x }",
        )
        .expect("lower break/continue");
        let text = prog.to_string();
        assert!(text.contains("goto "), "expected goto lowering:\n{}", text);
    }

    #[test]
    fn ir_symbols_survive_shadowing() {
        let prog = lower_src("fn main() @int { x := 1\nif true { x := 2 }\nreturn x }")
            .expect("lower shadowing");
        let func = &prog.functions[0];
        // Two distinct slots for the two `x` bindings.
        assert!(
            func.symbols.iter().filter(|(_, n, _)| n == "x").count() == 2,
            "expected 2 symbols for shadowed x, got {:?}",
            func.symbols
        );
    }

    #[test]
    fn ir_extern_has_no_body() {
        let prog = lower_src("extern fn puts(s: @string) @int\nfn main() @int { return 0 }")
            .expect("lower extern");
        let ext = prog
            .functions
            .iter()
            .find(|f| f.name == "puts")
            .expect("extern fn present");
        assert!(ext.is_extern);
        assert!(ext.blocks.iter().all(|b| b.instrs.is_empty()));
    }

    #[test]
    fn ir_switch_is_explicitly_unsupported() {
        let err = lower_src(
            "type Color enum { Red, Green }\nfn main() @int { c: Color = Color.Red\nswitch (c) { when .Red { return 1 }\nwhen else { return 0 } } }",
        )
        .expect_err("switch should be phase 2b");
        assert!(err.contains("switch"), "unexpected error: {}", err);
    }

    #[test]
    fn ir_struct_is_explicitly_unsupported() {
        let err = lower_src(
            "type Point struct { x: @int, y: @int }\nfn main() @int { p := Point { x: 1, y: 2 }\nreturn p.x }",
        )
        .expect_err("struct should be phase 2b");
        assert!(
            err.contains("struct") || err.contains("field"),
            "unexpected error: {}",
            err
        );
    }
}
