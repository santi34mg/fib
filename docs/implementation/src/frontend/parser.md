# `src/frontend/parser.rs` and `src/frontend/parser/`

## State and Lookahead

`Parser<I>` wraps a token iterator with a comment-filtering lookahead queue.
Two parser-wide flags resolve grammar ambiguities:

- `no_struct_literal` while parsing conditions;
- `slice_bound` while parsing range bounds.

Top-level declarations are imports, functions/extern functions, and type
declarations. There are no parsed top-level value constants.

## Expression Precedence

Lowest to highest:

1. logical `||`
2. logical `&&`
3. bitwise `|`
4. bitwise `^`
5. bitwise `&`
6. equality
7. relational comparison
8. cast `as`
9. shifts
10. addition/subtraction
11. multiplication/division/remainder
12. unary `!`, `-`, `~`
13. primary and postfix access/call/index/slice

`parser/binary.rs` supplies the shared left-associative binary helper.
Assignment is statement syntax rather than an expression.

## Statement Grammar

The parser currently requires a trailing semicolon after every statement,
including compound control-flow statements. This conflicts with several
language-guide examples that describe or show optional semicolons.

Supported statements include bindings, expression statements, assignment and
compound assignment, multiple assignment/binding, return, if/else, C-style for,
break, continue, defer, and enum switch.

## Known Risks

- Several delimited parsers accept EOF without requiring the closing token,
  including parameter, field, enum, switch, tuple, and construction lists.
- Commas are inconsistently optional in lists despite documentation describing
  comma-separated syntax.
- `defer` accepts arbitrary statements, including declarations and control
  transfer whose runtime meaning is unsafe or undefined.
- loop initializer/post positions use broad statement parsing and can produce
  analysis visibility inconsistent with execution order.
- several guarded `expect` calls remain in slice parsing.
- parser recovery is absent, so one error ends the parse.

## Improvement Direction

Introduce one reusable delimited-list helper with explicit separator and EOF
contracts. Publish an EBNF and align examples/tests with the chosen semicolon
and comma policy. Restrict syntactic forms in defer and loop headers unless the
analyzer has a precise model for them.
