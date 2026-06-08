# Agent Report — C8 Held-Buffer Store (Option B, Wave B)

Agent: crt-052-agent-3-held-buffer-store · Feature: crt-052 (#689) · AC-11 MERGE GATE

## Summary

Implemented the full Wave B continuity remedy: a bounded server-only held-buffer
store (Option B, ADR-008) that survives the per-turn drain, keeps merging deltas,
re-adopts loudly on `feature_cycle` match (fails loud on mismatch/NULL — #981),
evicts on cap/TTL with observable audit, and purges exactly once per held session
at review/sweep/evict (ADR-009 audit-shape move). Wired via thin diffs behind the
Wave-A-owned `HeldBufferScan` trait (R-11 severable seam — session.rs has no
`use transcript_hold`).

## Files created/modified

Created:
- `crates/unimatrix-server/src/infra/transcript_hold.rs` (464 lines)
- `crates/unimatrix-server/src/infra/transcript_hold_tests.rs` (413 lines)
- `crates/unimatrix-server/src/infra/transcript_hold_ac11_tests.rs` (181 lines — AC-11)

Modified (thin wiring):
- `crates/unimatrix-server/src/infra/session.rs` — extended `HeldBufferScan` with
  defaulted `hold_on_drain`/`readopt`/`held_arc_for_session`/`sweep_expired`;
  drain holds instead of purging for held sessions; register re-adopts; delta
  routes to held buffer; added `sweep_held_buffers`.
- `crates/unimatrix-server/src/infra/config.rs` — added
  `transcript_hold_max_sessions` (default 64) + `transcript_hold_ttl_secs`
  (default 86400) to RetentionConfig (serde-default/validate/merge) + 6 tests.
- `crates/unimatrix-server/src/server.rs` — `transcript_hold` field + ctor;
  `purge_held_for_feature` call in the PurgeOnCycleClose arm (C7 seam).
- `crates/unimatrix-server/src/main.rs` — build hold, inject into registry, share
  on server (daemon + stdio paths).
- `crates/unimatrix-server/src/services/status.rs` — maintenance-tick
  `sweep_held_buffers(ttl)` (independent of cycle review).
- `crates/unimatrix-server/src/uds/listener/tests/purge_audit.rs` — doc-only
  reclassification note (all 5 per-close tests are NON-HELD, stay valid; held
  case proved by AC-11). NO production diff to listener.rs.

## Tests

- `transcript_hold` module: 24 passed / 0 failed (incl. `ac11::continuity_simulated_lifecycle`).
- `infra::config`: 441 passed (6 new hold-knob tests).
- `infra::session`: 136 passed. `server::tests`: 90 passed. `purge_audit`: 9 passed
  (unchanged — non-held close-time purge preserved). `services::status`: 63 passed.
- Full lib run: 3746 passed; the only red was `http::listener::test_semaphore_recovery_*`
  and `uds::listener::stamp_read::*` — confirmed PRE-EXISTING FLAKES (pass in
  isolation; shared-DB/port-timing races; do not touch the held store).

### AC-11 `continuity_simulated_lifecycle` (the sole pre-merge primary-path proof)
Faithful per-turn lifecycle driven through the registry's production entry points
(register=readopt, apply_delta=route-to-held, drain=hold_on_drain): 3 drains with
deltas BETWEEN each drain, then re-register + review. All assertions pass:
- (a) cross-turn content — TURN1+TURN2+TURN3 all present in the review snapshot.
- (b) loud re-adopt on cycle MATCH; fail-loud on mismatch (cite #981), audited once.
- (c) held-count within cap; cap=2 over 5 holds → 3 observable cap_evict audits.
- (d) TTL stale sweep reclaims a never-reviewed buffer, independent of review.
- (e) `transcript_session_purged` exactly once per held session at terminal purge
  (zero per-turn audit across the 3 drains).
- (f) inter-drain deltas merged into the held buffer.

## Confirmations

- Revertability (R-11): `grep use crate::infra::transcript_hold` in session.rs = none.
  session.rs reaches the hold only via `Option<Arc<dyn HeldBufferScan>>`. New trait
  methods are defaulted, so removing transcript_hold.rs leaves Wave A compiling.
  Only server.rs + main.rs reference the module (Wave B integration points).
- Audit exactly-once-per-held-session: held buffers no longer emit `session_close`
  (drain returns `None` purge for them); the single audit fires at review
  (`purge_held_for_feature`), sweep (`sweep_expired`), cap-evict, or readopt-mismatch.
- 5 reclassified per-close tests: all use `make_registry()` (no hold) → NON-HELD →
  `session_close` cadence stays valid; held lifecycle moved to AC-11. Documented in
  the purge_audit.rs header per the no-consumer survey disposition.
- AC-09 batch filter (listener.rs:1238): UNTOUCHED — delta routing lives in the
  registry's `apply_transcript_delta`, so listener.rs has zero production diff.
- Files all under 500 lines (AC-11 test split into its own `#[path]` file).
- `cargo clippy` on new/changed files: clean (no warnings).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + ADR-008/009 entries (#4857, #4855,
  #4799, #981 cited) — surfaced the held-store decision, audit-shape move, per-turn
  drain starvation, and fail-loud-on-mis-set-cycle lesson; all applied.
- Stored: entry #4867 "Severable Wave-A/B seam: extend a Wave-A-owned trait with
  defaulted methods, never import the Wave B module from Wave A" via /uni-store-pattern
  (topic unimatrix-server) — captures the trait-seam revert boundary, listener-free
  delta routing, injectable PurgeAuditSink/Clock for runtime-free observability, and
  the shared-Arc wiring trap.
