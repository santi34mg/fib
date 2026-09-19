# Testing Strategy and Gaps

> Note (2026-09-18): addressed items removed. This file has NO remaining
> points. Removed: e2e sample harness (`tests/e2e.rs`), backend error-path
> tests, negative parser/analyze tests, lexer edge cases, and the coverage
> CI job (details below).

## Current state (verified 2026-09-18)

- `tests/e2e.rs` (`#[cfg(feature = "llvm")]`) compiles + runs 15 samples
  via `compile_project`, asserting exact stdout (incl. new
  `e2e_unsigned_ops` `:101`); `fib_bench` is compile-only;
  `e2e_broken_source_is_an_error_not_a_panic` pins no-panic. Full suite:
  269 lib tests + 15 e2e + 3, zero `#[ignore]`.
- §1 Backend error paths (gated `#[cfg(all(test, feature = "llvm"))]`):
  `coerce_width_change_without_insert_block_errors`, one `compute_lvalue_ptr`
  error per shape (`lvalue_of_undeclared_identifier_errors`,
  `lvalue_of_non_lvalue_expression_errors`,
  `lvalue_field_access_on_non_struct_errors`), `build_tuple_value_arity_mismatch_errors`,
  `unpack_tuple_value_of_non_tuple_errors`, `call_result_of_void_call_errors`,
  `unknown_layout`/`Void`-is-err mapping, `lower_ir` incl. `udiv/ugt`).
- §2 Parser negatives: `get_ast` returns `Result<Ast, ParseError>`
  (`parser/test.rs:14`); 16 `get_ast_err` negatives (EOF shapes, duplicate
  struct fields, `raw_unary`, etc.). Analyze negatives: 32 `get_typed_err`
  call sites with substring/line asserts — incl. `test_return_type_mismatch_errors`
  and `test_break_outside_loop_errors` (both formerly `#[ignore]`, now
  enforced) plus new `test_continue_outside_loop_errors`,
  `test_break_inside_nested_loop_is_fine`,
  `test_assign_to_immutable_switch_binding_errors`,
  `test_switch_non_exhaustive_errors`, `test_switch_wildcard_is_exhaustive`.
- §3 Lexer edges: direct `TokenKind::Unknown` (`test_unknown_character_is_direct_unknown_token`
  `:552`), unterminated char/string, char-escape/hex-escape/string-escape
  error branches (`:560-649`), `@nope`/`$` negatives.
- §4 Coverage: `cargo tarpaulin` runs as an advisory, non-blocking CI job
  (never gating; comment warns against chasing 100% on lowering files).
- Policy prose exists (`CONTRIBUTING` + PR-template checklist);
  `cargo clippy --all-features --all-targets -D warnings` in CI.

Cost ordering from the original audit is now moot: the lexer edges,
negatives, and parser-`Result` items all landed before the IR cutover.