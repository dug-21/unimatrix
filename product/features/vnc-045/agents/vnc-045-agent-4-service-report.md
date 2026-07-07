# Agent Report — vnc-045-agent-4-service (StoreTagService, Wave 2)

## Scope
Implement `StoreTagService` (the `context_tag` orchestration seam) per
`pseudocode/store-tag-service.md` and `test-plan/store-tag-service.md`.

## Files created / modified
- `crates/unimatrix-server/src/services/store_tag.rs` (NEW) — `StoreTagService`,
  `TagAction`, `TagResult`, `build_tag_metadata`, `map_store_err`.
- `crates/unimatrix-server/src/services/store_tag_tests.rs` (NEW) — 17 seam tests.
- `crates/unimatrix-server/src/services/mod.rs` (MOD) — module registration, `StoreTagService`
  re-export, `ServiceLayer.store_tag` field + construction (reuses the store/gateway/audit
  `Arc`s already built for `store_ops`).

Commit `74bc1fd4` on `feature/vnc-045`.

## Behavior delivered
- Order (binding): `check_write_rate(caller_id)` → store primitive dispatch → ONE
  fire-and-forget `context_tag` audit event AFTER commit. RateLimited returns before any write.
- Dispatch: `Add`→`add_tag`; `Remove`→`remove_tag`; `Replace` with `Some(ns)`→`replace_tag`
  (returns evicted prior); `Replace` with `None`→degrades to `add_tag` (ADR-004 colon-less edge).
- Audit (R-03/ADR-009): `operation="context_tag"`, `target_ids=[id]`, `agent_id`,
  `capability_used="write"`, metadata `{action, namespace, tag, prior_value, new_value}`.
  `prior_value` non-null on remove (= client tag, intent-of-record) and namespaced replace
  (= evicted prior); null on add / colon-less replace. `new_value` null on remove. `namespace`
  passed through and recorded, NEVER validated. `action` serialized as a variant STRING (#4366).
  `session_id` captured before `tokio::spawn` (#4388/#4389). On a metadata serialize error the
  code warns and SKIPS the event — the `"{}"` sentinel is NEVER emitted (#5468).
- Value-opacity: no allow-list, no `validate_outcome_tags`, no `min_trust_level`, no config —
  the tag is written verbatim.
- Lifecycle guards intentionally NOT here — they belong to the Wave 3 handler (which holds the
  loaded `EntryRecord`); the R-05 lifecycle tests live in the handler test plan, not this seam.

## Tests: 17 passed / 0 failed
`cargo test -p unimatrix-server --lib store_tag` → 17 passed. Coverage: prior_value on
add/remove/remove-absent/replace, colon-less-replace degrade, never-`{}`, one-event-per-mutation,
namespace-recorded-never-validated, action-is-string, session_id-before-spawn, field
completeness, value-opacity 3-value accept, throttle-before-write, UdsSession exemption, plus
`build_tag_metadata` / `TagAction::parse` unit tests.

## Self-check
- `cargo build --workspace` — clean.
- `cargo clippy -p unimatrix-server` — clean (module-level `#![allow(dead_code)]` on
  store_tag.rs with rationale: the whole surface is a forward-wired seam consumed by the Wave 3
  handler; `ServiceLayer.store_tag` field carries the same allow. Remove when Wave 3 lands).
- No `todo!()`/`unimplemented!()`/TODO/FIXME in non-test code. No `.unwrap()` in non-test code.
- Reverted out-of-scope `cargo fmt` churn on `mcp/edge_write_delete_agent_tests.rs`.
- Did NOT touch integration tests (Stage 3c) or Wave 1 store primitives.

## Issues / blockers
None. One first-run test failure was self-corrected (see Stewardship): fire-and-forget audit
events race on `event_id`, so multi-event read-back must match rows by content, not index.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced audit/store conventions (SqlxStore
  dual-pool #2147, combined audit txn #84, GH #302 async audit posture) but no direct
  context_tag pattern; applied the fire-and-forget precedent from store_correct.rs.
- Stored: entry #5613 "Fire-and-forget audit read-back: match rows by content, not event_id
  order" via /uni-store-pattern (topic unimatrix-server) — non-obvious test flake: `tokio::spawn`
  audit events assign `event_id` in lock-acquisition order, not call order.
