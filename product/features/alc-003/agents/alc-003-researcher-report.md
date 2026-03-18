# alc-003 Researcher Report

## Summary

SCOPE.md written to `product/features/alc-003/SCOPE.md`.

## Key Findings

1. **`PERMISSIVE_AUTO_ENROLL = true` is fully plumbed but compile-time** — The store layer
   (`crates/unimatrix-store/src/registry.rs:113-121`) already branches on the `permissive`
   bool to produce either `[Read, Write, Search]` or `[Read, Search]`. Only the constant in
   `infra/registry.rs:25` needs to change; no SQL migration is required.

2. **`extract_agent_id()` currently ignores `UNIMATRIX_SESSION_AGENT`** — The fallback chain
   is `params.agent_id` → `"anonymous"`. Adding the session agent as an intermediate fallback
   requires threading it through `UnimatrixServer` (a `Clone` struct) and modifying
   `build_context()`. The change is localized to `mcp/identity.rs` and `server.rs`.

3. **ADR #1839 is a designed-but-unimplemented Gen 3 identity model** (token hashing,
   bcrypt, pre-enrollment) — it addresses the same problem with higher trust assumptions.
   The two mechanisms are compatible if positioned as Gen 2 (alc-003: named identifier)
   vs Gen 3 (ADR #1839: token credential). They must not both ship as defaults
   simultaneously. Reconciliation question is the critical open question for the human.

4. **Test coverage gap** — 27 tests in `infra/registry.rs` assert `[Read, Write, Search]`
   for unknown agents under the current `PERMISSIVE_AUTO_ENROLL=true` default. Changing
   the default will break these tests. The env var approach creates a process-global
   isolation problem in parallel tests; an explicit `permissive: Option<bool>` override
   at construction time is the cleaner solution.

5. **`settings.json` currently has no `"env"` key** — Adding `UNIMATRIX_SESSION_AGENT`
   requires the user to add an `"env"` section. This is pure configuration, not a code
   change, but must be documented clearly for users.

## Files Read

- `/workspaces/unimatrix/product/PRODUCT-VISION.md` (W0-2, W0-3, security requirements sections)
- `/workspaces/unimatrix/product/research/ass-020/SECURITY-AUDIT.md`
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/registry.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/identity.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/mcp/tools.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/server.rs`
- `/workspaces/unimatrix/crates/unimatrix-server/src/main.rs`
- `/workspaces/unimatrix/crates/unimatrix-store/src/registry.rs`
- `/workspaces/unimatrix/.claude/settings.json`
- Unimatrix entries #1839 (ADR: UNIMATRIX_CLIENT_TOKEN) and #79 (ADR-003: Agent Identity via Tool Parameters)

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for session identity, enrollment, env var -- found ADR #1839
  (UNIMATRIX_CLIENT_TOKEN) and ADR-003 (Agent Identity via Tool Parameters) — directly relevant
- Stored: attempted to store "Compile-time bool to runtime env var: conversion pattern and test
  isolation" via `/uni-store-pattern` — storage returned internal error on both attempts; pattern
  not persisted. The pattern is documented in the SCOPE.md Constraints section instead.
