# vnc-030 Agent 3 — FeatureSource (infra/session.rs) Report

## Files Modified
- `crates/unimatrix-server/src/infra/session.rs` (extend)
- `crates/unimatrix-server/src/services/index_briefing.rs` (1-line test-helper field add — required for compile; out-of-file `SessionState {}` literal)

## What Was Implemented (ADR-004, four fenced touchpoints — C-09)
1. **Enums + field**: `pub enum FeatureSource { Declared, Inferred(InferredOrigin) }`, `pub enum InferredOrigin { Registered, Voted }`; `SessionState.feature_source` (after `feature`).
2. **Source assignment at existing set sites**:
   - `register_session` → `Inferred(Registered)` (default, Some/None alike).
   - `set_feature_force` all three Some-arms → `Declared` (incl. AlreadyMatches re-affirm).
   - `set_feature_if_absent` → `Inferred(Voted)`.
3. **New `apply_stamp(&self, session_id, topic)`** — idempotent Declared set; no-op when feature+source match (no Overridden log); last-writer-wins + single info log on contradiction; absent session = no-op (no pre-register).
4. **Sweep inversion flip (session.rs sweep_stale_sessions)** — one guard around the existing `or_else` vote chain. `SweepResult.resolved_feature` shape unchanged. Zero changes to `drain_and_signal_session` / `clear_transcripts_for_feature` / transcript buffer (C-10).

## Tests (per test-plan/feature-source.md)
Added 13 tests covering: source assignment at each set site, `apply_stamp` idempotency/contradiction/absent-session/restore-after-reregister, sweep declared-beats-vote, inferred-uses-vote-then-registry, crt-052 citable interface, and the R-13 re-register accepted-consequence boundary.

- `cargo test -p unimatrix-server --lib infra::session` → **157 passed / 0 failed** (13 new).
- `cargo test -p unimatrix-server` → **3632 passed / 1 failed**. The 1 failure is `http::token::tests::test_concurrent_creation_no_corruption` — a PRE-EXISTING parallel-execution flake (concurrent token-file write race); it **passes in isolation** and is unrelated to vnc-030.
- `test_signal_output_shape_unchanged` byte-baseline gate still **passes** (feature_source is in-memory-only, never serialized).
- `cargo build -p unimatrix-server`: zero new warnings (25 pre-existing baseline unchanged). `cargo fmt` clean.

## OQ-A Finding (REQUIRED)
**No sweep path writes vote-derived attribution at row level.** The session-level majority vote in `sweep_stale_sessions` resolves `SweepResult.resolved_feature` → `sessions.feature_cycle`, NOT observation rows. Confirmed on this (session.rs) side:
- `Inferred(Voted)` is SET here (`set_feature_if_absent`), carrying the source forward.
- The `'vote'` `topic_source` row value is written on the LISTENER side at the enrich step when an unstamped, no-extraction event fills from a `Voted` registry feature (ADR-005 taxonomy: `vote` = extracted-None + `Inferred(Voted)` fill).
- `feature_source` carries vote-vs-registry distinction purely so the listener's enrich decision tree can emit `registry-fill` vs `vote` — it never affects precedence (only `matches!(src, Declared)` does).

FR-21 one-source-per-write-site holds on this side: session.rs writes no `topic_source` at all; it only sets the in-memory `feature_source` class. Row-level write sites are entirely in listener.rs.

## Issues / Blockers
None. One out-of-scope-file edit was unavoidable (`index_briefing.rs` test helper holds a second `SessionState {}` literal — crate won't compile without the field). This is mechanical field-init only, no logic.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced #4816 (ADR-004 itself), #4134 (pre-register evicted sessions before set_feature_force on cycle_start), #3382 (write-time enrich registry fallback), #4799 (per-turn drain empties SessionState). Applied: apply_stamp deliberately does NOT pre-register (unlike the #4134 cycle_start path) — best-effort affirmation only.
- Stored: entry #4838 "FeatureSource precedence guard — minimal-diff sweep inversion flip + additive in-memory registry field" via /uni-store-pattern (4 traps: out-of-file SessionState literal, apply_stamp idempotency/no-pre-register, byte-baseline gate safety of in-memory fields, AlreadyMatches re-affirm).
