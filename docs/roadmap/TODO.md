# Implementation follow-up

- Settle Unicode namespace normalization, then integrate package-rooted lookup.
- Define package/source-discovery build-API signatures and implement deterministic
  recursive selection, exclusions, symlink skipping and deduplication.
- Review runtime indexing/slicing bounds and lifetime rules before native lowering.
- Choose the public compiler namespace spelling.

See the [implementation guide](../agents/README.md) for implemented specification features.
