# Compiler implementation guide

Reviewed against this checkout on 2026-10-17. Source and tests take precedence over
historical comments, samples, and language proposals.

## Actual status

Working components are the tokenizer, parser, AST construction, lexical scope
utilities, diagnostic rendering, and raw IR data/access/formatting utilities.
Analysis, build-plan loading, pipeline orchestration, IR construction/verification,
interpretation, and LLVM emission are sketches, not a usable compiler pipeline.

```text
main -> Clap Command::parse -> cli::exec_command (todo!)

Directly usable: source -> Tokenizer -> Parser -> Ast (pending annotations)

Planned, not executable:
BuildPlan -> parse_files -> Analyzer -> AnalyzedProject -> lower_to_ir -> Ir
         -> verified/target-aware LLVM -> objects/link
build.fib -> IR interpreter -> project BuildPlan (also unimplemented)
```

There is no separate typed AST, direct AST-to-LLVM fallback, recursive import
loader, `compile_project`, `check_project`, or checked-in integration/e2e suite.
Public architecture types do not certify successful analysis or verification.

## Map

- [Repository and commands](repository.md)
- [Source ownership](src/README.md)
- [Tokenizer, parser, AST, and analysis](src/frontend/README.md)
- [SSA IR sketch](src/ir/README.md)
- [Backend sketch](src/backend/README.md)
- [Tests and validation](tests/README.md)
- [Samples](samples/README.md) and [standard library](std/README.md)
- [CI](.github/workflows/ci.md)
- [Open design boundaries](language-design-questions.md)
- [Documentation overview and language-design proposals](../README.md)
- [Verified parsing snapshot](../parsing/README.md), not a language specification
- [Analysis issues on GitHub](issues/README.md)

Legacy page names are retained as explicit redirects/status pages so existing
links remain useful. No missing roadmap is implied by this index.

## Maintenance rules

Distinguish working behavior, raw representation contracts, and intended future
passes. Do not turn a stub into successful empty output to satisfy tests. Preserve
source spellings/spans and stable identity through future passes. Add positive
and negative tests for implemented invariants, not assertions that pretend a
sketched pass runs. Update these pages with subsystem changes; avoid line-number
links and unverified runtime claims. Keep observed syntax in `docs/parsing/`;
parser limitations are not design decisions. Report analysis gaps as actionable
GitHub issues rather than embedding implementation backlogs in user-facing
language documentation. Keep only GitHub links in the issue index, not local
copies of report bodies.
