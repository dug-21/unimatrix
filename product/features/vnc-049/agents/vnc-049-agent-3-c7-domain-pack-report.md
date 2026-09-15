# Agent Report — C7 OpenCode Domain Pack (vnc-049)

**Agent:** vnc-049-agent-3-c7-domain-pack (uni-rust-dev) · **Wave:** 1 · **Status:** COMPLETE

## Files modified
- `crates/unimatrix-observe/src/domain/mod.rs` — added `builtin_opencode_pack()` + `OPENCODE_PACK_SENTINEL_EVENT`; registered opencode zero-config in both `DomainPackRegistry::new` and `with_builtin_claude_code`. (261 lines, under cap)
- `crates/unimatrix-observe/tests/domain_pack_tests.rs` — added 3 C7 tests; fixed `test_iter_packs_returns_all_packs` count 2→3.

Committed: `4582388d impl(observe): add opencode builtin DomainPack (C7) (#986)`.

## EC-07 / OQ-3 resolution (the single correctness point)
The opencode pack's `event_types = ["__opencode_pack_sentinel__"]`:
- NOT empty → does not trigger the "claims-ALL" branch in `resolve_source_domain` (@183).
- NOT any shared canonical name (`PreToolUse` etc.) → no non-deterministic HashMap collision with claude-code.
- Pack exists only for `categories` registration + `lookup("opencode")`; the new-row opencode discriminator is the persisted `source_domain` column (C5, ADR-001), NOT this pack. `resolve_source_domain` @180 is unchanged and used for legacy NULL rows only.

## Tests
- `cargo test -p unimatrix-observe --test domain_pack_tests`: 47 passed, 0 failed (incl. 3 new: `test_opencode_domain_pack_registered`, `test_opencode_source_domain_format_valid`, `test_opencode_pack_does_not_shadow_claude_code_resolution`).
- `cargo test -p unimatrix-observe --lib`: 581 passed, 0 failed.
- `cargo clippy -p unimatrix-observe --all-targets`: clean.
- `cargo build -p unimatrix-server`: builds against the change.

## Cross-crate impact (verified via Explore agent)
No additional breakage. Config-parse pack counts (`infra/config.rs:7371/7427`) are pre-registry (safe). `main.rs:2169` `iter_packs` category registration is safe — opencode carries the identical 7 categories. All server `resolve_source_domain` callers safe due to the sentinel. Only the observe `iter_packs` count test needed the 2→3 update (done).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-001 (#5748), vnc-013 registry-with-fallback (#4308/#4304), observe-crate no-tracing gotcha (#2929). Applied ADR-001 legacy-fallback role framing.
- Stored: entry #5749 "Category-only DomainPack must use a private sentinel event_type, not empty and not shared canonical names" via /uni-store-pattern.

## Issues / blockers
None. Working tree carries in-flight edits from parallel wave agents (C2/C4/C5/C8); I staged only my two observe-crate files.
