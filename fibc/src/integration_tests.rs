#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::analysis::analyze;
    use crate::lexer::Lexer;
    use crate::lowering::lower;
    use crate::parser::Parser;
    use crate::token::Token;

    fn compile_to_ir(src: &str) -> String {
        let lexer = Lexer::new(src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.to_string());
        let ast = parser.parse().expect("parse failed");
        let cu = analyze(ast).expect("analysis failed");
        lower(cu, "test_module").expect("lowering failed")
    }

    #[test]
    fn test_constant_return() {
        let src = "fn answer() sint32 { return 42; }";
        let ir = compile_to_ir(src);
        assert!(ir.contains("answer"), "IR should contain function name 'answer'");
        assert!(ir.contains("ret"), "IR should contain a ret instruction");
    }

    #[test]
    fn test_arithmetic_function() {
        let src = "fn add(a sint32, b sint32) sint32 { return a + b; }";
        let ir = compile_to_ir(src);
        assert!(ir.contains("add"), "IR should contain function name 'add'");
        assert!(ir.contains("add nsw") || ir.contains("add i32"), "IR should contain add instruction");
    }

    #[test]
    fn test_if_statement() {
        let src = r#"fn abs_val(x sint32) sint32 {
    if x < 0 {
        return 0 - x;
    }
    return x;
}"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("abs_val"), "IR should contain function name 'abs_val'");
        assert!(ir.contains("icmp"), "IR should contain comparison instruction");
        assert!(ir.contains("br"), "IR should contain branch instruction");
    }

    #[test]
    fn test_void_function() {
        let src = "fn noop() void { return; }";
        let ir = compile_to_ir(src);
        assert!(ir.contains("noop"), "IR should contain function name 'noop'");
        assert!(ir.contains("ret void"), "IR should contain 'ret void'");
    }

    #[test]
    fn test_equality_comparison() {
        let src = "fn is_zero(x sint32) bool { return x == 0; }";
        let ir = compile_to_ir(src);
        assert!(ir.contains("is_zero"), "IR should contain function name 'is_zero'");
        assert!(ir.contains("icmp eq"), "IR should contain equality comparison");
    }

    #[test]
    fn test_break_in_loop() {
        let src = r#"fn find_five() sint32 {
    var sint32 i = 0;
    for (i = 0; i < 100; i = i + 1) {
        if i == 5 {
            break;
        }
    }
    return i;
}"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("find_five"), "IR should contain function name");
        assert!(ir.contains("afterloop"), "IR should contain afterloop block for break");
        assert!(ir.contains("forcond"), "IR should contain forcond block");
    }

    #[test]
    fn test_continue_in_loop() {
        let src = r#"fn sum_even(n sint32) sint32 {
    var sint32 i = 0;
    var sint32 s = 0;
    for (i = 0; i < n; i = i + 1) {
        if i == 1 {
            continue;
        }
        s = s + i;
    }
    return s;
}"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("sum_even"), "IR should contain function name");
        assert!(ir.contains("forpost"), "IR should contain forpost block for continue");
        assert!(ir.contains("afterloop"), "IR should contain afterloop block");
    }

    #[test]
    fn test_break_in_if_within_for() {
        let src = r#"fn first_neg(a sint32, b sint32) sint32 {
    var sint32 i = 0;
    for (i = 0; i < 2; i = i + 1) {
        if i == 0 {
            if a < 0 {
                break;
            }
        } else {
            if b < 0 {
                break;
            }
        }
    }
    return i;
}"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("first_neg"), "IR should contain function name");
        assert!(ir.contains("afterloop"), "IR should contain afterloop block");
    }

    #[test]
    fn test_nested_loops_break() {
        let src = r#"fn nested() sint32 {
    var sint32 i = 0;
    var sint32 j = 0;
    for (i = 0; i < 3; i = i + 1) {
        for (j = 0; j < 3; j = j + 1) {
            if j == 1 {
                break;
            }
        }
    }
    return i;
}"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("nested"), "IR should contain function name");
        // Two separate afterloop blocks exist (one per loop)
        assert!(ir.matches("afterloop").count() >= 2, "IR should contain at least 2 afterloop blocks for nested loops");
    }
}
