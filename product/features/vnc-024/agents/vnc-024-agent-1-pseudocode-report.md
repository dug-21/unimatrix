# Agent Report — vnc-024-agent-1-pseudocode (Stage 3a)

## Scope
Per-component pseudocode for vnc-024 F1 (wire-contract codegen + content negotiation +
transcript_delta + transcript_retention). Six files produced under
`product/features/vnc-024/pseudocode/`.

## Deliverables
- `OVERVIEW.md` — component interaction, data flow, shared `TranscriptDeltaPayload` shape +
  `TRANSCRIPT_DELTA_EVENT` const, sequencing constraints, AC→component map.
- `ts-rs-codegen.md` (Component 1)
- `contract-fixtures.md` (Component 2)
- `observe-content-negotiation.md` (Component 3)
- `transcript-delta-guard.md` (Component 4 — GATE-CRITICAL, AC-12/R-03)
- `transcript-retention.md` (Component 5)

## Key anchoring decisions (verified against codebase, not invented)
- `format_injection` is currently a **private** `fn` at `hook.rs:1047`; production callers
  (`:979`, `:1031`) pass `MAX_INJECTION_BYTES = 1400` (private `const` at `hook.rs:29`). Component 3
  bumps BOTH to `pub(crate)`. This RESOLVES the ARCHITECTURE open question and R-05 budget-parity:
  the text path's budget is `MAX_INJECTION_BYTES`, the same constant the UDS caller uses → AC-07
  byte-identity holds against one source.
- The col-022 contrast (#1266) is concrete: `CYCLE_START_EVENT` at `listener.rs:767` specializes
  THEN falls through to `:849 insert_observation`. The delta guard inverts this with an early
  `return Ack` inserted after `sanitize_session_id` (:757), before `:767`/`:793`/`:849`.
- `RecordEvents` batch arm (`:868`): the durable write is `obs_batch` built at `:975`, inserted at
  `:988`. Guard = `.filter(event_type != TRANSCRIPT_DELTA_EVENT)` on the `obs_batch` build so deltas
  never enter the persisted batch.
- `RetentionConfig` validate() reuses the existing `ConfigError::RetentionFieldOutOfRange` variant;
  message names `RetainDays` as enterprise-only (not a generic range error).
- All constraints honored: no new wire variant (3), `format_injection` single truth (4),
  no-secret-scanner-reliance (9), merge re-validation (10).

## Open questions surfaced (all non-blocking, delivery-time)
1. Exact ts-rs export API for the pinned major version (`export`/`export_all`/`export!`) — file
   contract is "after `cargo test`, all six `.ts` exist non-empty."
2. `u64`→TS representation for `TranscriptDeltaPayload.offset` (bigint vs number) — delta fixture
   value stays within 2^53 or uses ts-rs's mapping; coordinated between Components 1 and 2.
3. `.ts` consumption in the node harness (type-stripping vs tsc vs parsed-JSON-shape assertions).
4. validate() error-variant choice (reuse `RetentionFieldOutOfRange` vs new `EnterpriseOnly`).
5. Post-merge re-validation call site for `retention` — delivery confirms the existing finalize
   point; if none re-validates, add it (Constraint 10 / #3905).
6. Batch-arm pre-persistence loops (`:897-970`) confirmed safe (registry side effects, not durable
   `bytes` writes); flagged for reviewer verification per ADR-004 assumption A3.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` — found #4720 (ADR-004 typed-payload guard), #4721
  (ADR-005 OSS-rejects-RetainDays), #4711 (new event_type must not inherit generic-observation
  fall-through — the central F1 risk), #1266 (col-022 specialize-then-fall-through — the
  anti-pattern the guard must NOT reuse), #3255 (serde skip_serializing_if None-omission), #4714
  (ADR-003 content negotiation), #4718 (ADR-001 ts-rs codegen).
- Deviations from established patterns: none. The guard deliberately INVERTS #1266 (early-return vs
  fall-through) per ADR-004 — this is the prescribed design, not a deviation. Retention enum and
  merge arms reuse the existing `RetentionConfig` four-touchpoint pattern exactly.
