# Unfinished IR rewrite

This directory preserves `src/ir/{mod,builder,display,expressions,instruction,statements,test}.rs` from `refactor/modules-and-generic-functions` (`8318882`) without making it the active IR module. The working compiler still uses the parent `src/ir/` files. The related `src/ir_interpreter.rs` and `src/backend/lowering/next/ir_lower.rs` (the unfinished IR backend consumer) are also preserved but not registered.

This is **source material, not a compiled alternative**: its imports refer to a removed `frontend::typed_ast` layout; `lower_const` contains `todo!()` and `src/ir/next/statements.rs` contains incomplete code. The preserved backend consumer also has a `todo!()` entry point. Porting this rewrite requires a defined typed-AST/IR contract and backend consumer before it can replace the current IR safely. The parent `instruction.rs` is a behavior-preserving extraction of the working instruction set, not this rewrite.
