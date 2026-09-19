# Reliability and Error Handling

> Note (2026-09-18): addressed items removed. This file now lists only
> the remaining diagnostics item. Removed: assignment soundness holes
> (fixed in `analyze/statements.rs` + `analyze/expressions.rs`), quick-win
> panics (import path, `TypedReturn`, scope stack, param `unwrap`,
> `UnknownLayout`, parser `expect_*`), lexer `Error` propagation
> (`primary.rs`, `type_expression.rs`, `statement.rs`), typed lowering
> errors (see below).

## Current state (verified 2026-09-18)

- Typed lowering errors are DONE: `LowerError` (`src/backend/lowering/error.rs`)
  has `MissingBlock{what,fn_name,line}`, `UnknownLayout`, `Unsupported`,
  `Llvm`. `map_type_to_llvm` folds `UnknownLayout`/`Unsupported` from
  `Ty::Identifier`/`Void`/`Ty::Type` (`src/backend/lowering/types.rs`); the
  `parent_function`/`insert_block` ladder returns `LowerError::missing_block`
  (`context.rs:76,89`). `expressions.rs`/`statements.rs`/`ir_lower.rs` all
  return `LowerError` (string `format!().into()` wraps as `Unsupported`).
  No `Box<dyn Error>` in the backend.
- Zero non-test `unwrap/expect/unreachable!` in `src/backend` + `src/frontend`;
  `cargo clippy --all-features --all-targets -D warnings` is clean.

- **2. Structured diagnostics with spans — DONE.** Every stage promotes into
  one `CompilerError` (`src/diagnostics.rs`) and the CLI has a single
  rendering path, so the user always sees the same aligned panel:
  `error[kind]: message` / `--> file:line:col` / source line + caret /
  `= help: hint`.

  Design (decided here; deliberately **not** `thiserror`/`miette` — the
  compiler only produces a handful of error types, so a hand-rolled renderer
  keeps the dependency surface at zero):
  - `Span { line, column }` — 1-based start position only.
  - `ErrorKind { Io, Lex, Parse, Name, Type, Backend, Toolchain }` with
    stable `tag()` (`io|lex|parse|name|type|backend|toolchain`) for tooling.
  - `CompilerError { kind, message, span, hint, filename, source_line }`.
    `filename`/`source_line` are render context attached at promote time, when
    the driver has the source text. `render()` draws the rustc-style panel
    (final `^` column, `= help:` on its own line); `render_plain()` is used
    when no span is known.
  - The public `compile()` / `compile_project()` path returns
    `Result<_, Box<CompilerError>>` (the error is large; `result_large_err`
    is clean). `DriverError` stays the internal pipeline type and converts
    via `From<DriverError> for CompilerError` (`driver.rs`), mapping each
    variant to a `ErrorKind` (`Name`/`Io` for imports, `Toolchain` for
    `cc`/LLVM/link failures, `Backend` for lowering).
  - Entry-file **parse** errors pre-fill `filename`+`source_line`
    (`ParseError` already carried them). Entry-file **analysis** errors get
    the panel by re-extracting the offending line from the entry source in
    `compile()`; module analysis errors keep the module name as the filename
    (their source is not retained) and render without a caret.

  Frontend plumbing that made precise spans possible:
  - `AnalysisError` is now `{ kind, span: Option<Span>, message, hint }`;
    `Display` is unchanged (`AnalysisError at line N: …`). `with_line`
    fills a missing span (get-or-insert), `with_span` overwrites (the caller
    is closer to the node), `with_span_fallback` fills gaps when bubbling up
    (the innermost node keeps its precise position).
  - AST spans: `Statement { kind, span }`, and `Expression`/`TypeExpression`
    became wrappers `{ kind, span }` over the (renamed) `ExpressionKind` /
    `TypeExpressionKind` enums, constructed via `::at(kind, span)`. The
    parser records the node's first token position; rebuilt nodes reuse the
    source span.
  - The analyzer now inherits those spans without touching each error site:
    `expr_to_typed`, `map_type`, and `stmt_to_typed` wrap their core with
    `with_span_fallback`, so every expression/type/statement error points at
    its own node. A representative handful of sites also carry `= help:`
    hints (immutable assignment → `var`, inferred decl without initializer,
    unknown function → `extern`).
  - The lexer stays non-failing: it emits `TokenKind::Error(String)` that the
    parser surfaces as `ParseError` at the offending token.

  Trade-offs:
  - **Pro:** actionable, rustc-style errors with `file:line:col`, a caret
    and a hint line; one rendering shape across every stage.
  - **Con:** the wrapper change was invasive — every `Expression::V` /
    `TypeExpression::V` site became `Expression { kind: ExpressionKind::V, .. }`
    and analyzer matches now go through `.kind` (parser and analyzer plus
    parser test patterns were updated mechanically). Hand-rolled renderer
    means features like multi-line context/warnings/fix-assembly are not
    modeled; a future `miette` migration would be localized to
    `diagnostics.rs` + the driver promotion.

## Remaining points

- None. (Future observations marked here until triaged.)