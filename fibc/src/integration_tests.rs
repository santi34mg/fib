#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::analysis::analyze;
    use crate::lexer::Lexer;
    use crate::lowering::lower;
    use crate::parsing::Parser;
    use crate::tokens::Token;

    fn compile_to_ir(src: &str) -> String {
        let lexer = Lexer::new(src);
        let tokens: Vec<Token> = lexer.collect();
        let mut parser = Parser::new(tokens.into_iter(), Path::new("test"), src.to_string());
        let ast = parser.parse().expect("parse failed");
        let cu = analyze(ast, &HashMap::new()).expect("analysis failed");
        lower(cu, "test_module").expect("lowering failed")
    }

    #[test]
    fn test_constant_return() {
        let src = "fn answer() sint32 { return 42; }";
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("answer"),
            "IR should contain function name 'answer'"
        );
        assert!(ir.contains("ret"), "IR should contain a ret instruction");
    }

    #[test]
    fn test_arithmetic_function() {
        let src = "fn add(sint32 a, sint32 b) sint32 { return a + b; }";
        let ir = compile_to_ir(src);
        assert!(ir.contains("add"), "IR should contain function name 'add'");
        assert!(
            ir.contains("add nsw") || ir.contains("add i32"),
            "IR should contain add instruction"
        );
    }

    #[test]
    fn test_if_statement() {
        let src = r#"fn abs_val(sint32 x) sint32 {
    if x < 0 {
        return 0 - x;
    }
    return x;
}"#;
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("abs_val"),
            "IR should contain function name 'abs_val'"
        );
        assert!(
            ir.contains("icmp"),
            "IR should contain comparison instruction"
        );
        assert!(ir.contains("br"), "IR should contain branch instruction");
    }

    #[test]
    fn test_void_function() {
        let src = "fn noop() void { return; }";
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("noop"),
            "IR should contain function name 'noop'"
        );
        assert!(ir.contains("ret void"), "IR should contain 'ret void'");
    }

    #[test]
    fn test_equality_comparison() {
        let src = "fn is_zero(sint32 x) bool { return x == 0; }";
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("is_zero"),
            "IR should contain function name 'is_zero'"
        );
        assert!(
            ir.contains("icmp eq"),
            "IR should contain equality comparison"
        );
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
        assert!(
            ir.contains("afterloop"),
            "IR should contain afterloop block for break"
        );
        assert!(ir.contains("forcond"), "IR should contain forcond block");
    }

    #[test]
    fn test_continue_in_loop() {
        let src = r#"fn sum_even(sint32 n) sint32 {
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
        assert!(
            ir.contains("forpost"),
            "IR should contain forpost block for continue"
        );
        assert!(
            ir.contains("afterloop"),
            "IR should contain afterloop block"
        );
    }

    #[test]
    fn test_break_in_if_within_for() {
        let src = r#"fn first_neg(sint32 a, sint32 b) sint32 {
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
        assert!(
            ir.contains("afterloop"),
            "IR should contain afterloop block"
        );
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
        assert!(
            ir.matches("afterloop").count() >= 2,
            "IR should contain at least 2 afterloop blocks for nested loops"
        );
    }

    #[test]
    fn test_generic_identity() {
        // Generic function: T is a comptime type parameter.
        // The call site instantiates T = int32, producing a mangled function.
        let src = r#"
fn identity(type T, T val) T {
    return val
}
fn main() int32 {
    return identity(int32, 42)
}
"#;
        let ir = compile_to_ir(src);
        // The monomorphized function should appear with the mangled name
        assert!(
            ir.contains("identity__int32"),
            "IR should contain mangled generic instantiation 'identity__int32'"
        );
        assert!(ir.contains("main"), "IR should contain 'main'");
        assert!(ir.contains("ret"), "IR should contain a ret instruction");
    }

    #[test]
    fn test_generic_called_multiple_types() {
        // The same generic function instantiated for two different types
        // should produce two distinct mangled functions.
        let src = r#"
fn wrap(type T, T val) T {
    return val
}
fn main() int32 {
    const int32 a = wrap(int32, 1)
    const bool b = wrap(bool, true)
    return a
}
"#;
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("wrap__int32"),
            "IR should contain 'wrap__int32' instantiation"
        );
        assert!(
            ir.contains("wrap__bool"),
            "IR should contain 'wrap__bool' instantiation"
        );
    }

    #[test]
    fn test_non_generic_sort() {
        // Verify the sort pattern works without generics first.
        let src = r#"
fn sort(*int32 arr, int32 len) void {
    for (var int32 i = 1; i < len; i += 1) {
        for (var int32 j = i; j > 0; j -= 1) {
            if arr.[j] < arr.[j - 1] {
                const int32 t = arr.[j]
                arr.[j] = arr.[j - 1]
                arr.[j - 1] = t
            }
        }
    }
}
fn main() int32 {
    var int32[3] arr = [3, 1, 2]
    sort(arr.& as *int32, 3)
    return 0
}
"#;
        let ir = compile_to_ir(src);
        assert!(ir.contains("sort"), "IR should contain sort");
        assert!(ir.contains("main"), "IR should contain main");
    }

    #[test]
    fn test_generic_sort() {
        // Models the sorting sample: a generic insertion sort over a pointer+length.
        let src = r#"
fn insertion_sort(type T, *T arr, int32 len) void {
    for (var int32 i = 1; i < len; i += 1) {
        for (var int32 j = i; j > 0; j -= 1) {
            if arr.[j] < arr.[j - 1] {
                const T t = arr.[j]
                arr.[j] = arr.[j - 1]
                arr.[j - 1] = t
            }
        }
    }
}
fn main() int32 {
    var int32[3] arr = [3, 1, 2]
    insertion_sort(int32, arr.& as *int32, 3)
    return 0
}
"#;
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("insertion_sort__int32"),
            "IR should contain mangled sort instantiation"
        );
        assert!(ir.contains("main"), "IR should contain main");
    }

    #[test]
    fn test_const_type_declaration() {
        // const type declarations should register a type alias in scope
        // without producing a separate HIR declaration.
        let src = r#"
const type MyInt = int32
fn answer() MyInt {
    return 42
}
"#;
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("answer"),
            "IR should contain function name 'answer'"
        );
        assert!(ir.contains("ret"), "IR should contain a ret instruction");
    }

    #[test]
    fn test_generic_deduplication() {
        // Calling the same generic function twice with the same type
        // should produce only one instantiated function in the IR.
        let src = r#"
fn twice(type T, T val) T {
    return val
}
fn main() int32 {
    const int32 a = twice(int32, 1)
    const int32 b = twice(int32, 2)
    return a
}
"#;
        let ir = compile_to_ir(src);
        assert!(
            ir.contains("twice__int32"),
            "IR should contain 'twice__int32'"
        );
        // LLVM IR defines functions with `define ... @name(` at the start of a line.
        // Count only definition lines (not call sites) to verify deduplication.
        let define_count = ir
            .lines()
            .filter(|l| l.contains("define") && l.contains("twice__int32"))
            .count();
        assert_eq!(
            define_count, 1,
            "twice__int32 should be defined exactly once, found {}",
            define_count
        );
    }
}
