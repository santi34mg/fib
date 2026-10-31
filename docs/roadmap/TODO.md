# Implementation follow-up

- Define package/source-discovery build-API signatures and implement deterministic
  recursive selection, exclusions, symlink skipping and deduplication.
- Define and enforce borrowed-view lifetimes; checked native indexing/slicing
  already execute, but do not extend backing-storage lifetimes.
- Choose the public compiler namespace spelling.

See the [implementation guide](../agents/README.md) for implemented specification features.
