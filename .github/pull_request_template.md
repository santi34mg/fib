# Pull request

<!-- Use a Conventional Commits title so squash-merge produces a clean history:
     feat(frontend): ..., fix(backend): ..., refactor(driver,cli): ...,
     docs: ..., test: ..., chore: ... -->

## What

<!-- One paragraph: what changed and why. Link issues with `Closes #N`. -->

## Checklist

- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes (plus `--no-default-features` if frontend-only)
- [ ] New lowering branch ships with a `samples/*.fib` reproducer
- [ ] Soundness fix ships with a `get_typed_err` negative test
- [ ] `CHANGELOG.md` updated under `[Unreleased]`
