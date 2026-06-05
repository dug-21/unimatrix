# Agent Report — vnc-024-agent-7-transcript-retention (Stage 3b, Wave 1, Component 5)

> Reconstructed by the Delivery Leader: the primary agent's connection dropped before returning
> (temp-filesystem exhaustion). Its enum + field + default + validate() edits landed; the
> project-wins merge arm and the test-cfg literal fixups were completed by recovery agents
> `vnc-024-agent-7b` (merge + re-validation tests) and `vnc-024-agent-7c` (status.rs literals).
> End-state verified against committed HEAD (`0096c58e`) + test runs.

## Scope
Component 5 — `transcript_retention` enum on `RetentionConfig` (Deliverable 4, ADR-005).

## Files modified
- `crates/unimatrix-server/src/infra/config.rs`:
  - `pub enum TranscriptRetention { PurgeOnCycleClose, RetainDays(u32) }` deriving `Deserialize, Serialize, Debug, Clone, PartialEq` (~:1506).
  - `RetentionConfig.transcript_retention` with `#[serde(default = "default_transcript_retention")]` (~:1561); `default_transcript_retention()` → `PurgeOnCycleClose` (~:1574); `Default` impl entry (~:1584).
  - `validate()` arm (~:1634): OSS REJECTS `RetainDays(_)` for any N (incl. 0) as enterprise-only (field `transcript_retention`, message names `RetainDays` as enterprise-only) — NOT a range check; accepts `PurgeOnCycleClose` unconditionally; bare `u32` rejected at deserialize.
  - Project-wins merge arm (~:3378): `!=`-based, `.clone()` (enum is Clone, not Copy).
- `crates/unimatrix-server/src/services/status.rs` — 4 existing `RetentionConfig` literals (~:3386/3482/3592/3665) gained `transcript_retention: TranscriptRetention::PurgeOnCycleClose`.

## Tests
Retention unit + merge tests pass: default == `PurgeOnCycleClose`; absent-config → default; `validate()` rejects `RetainDays(N)` for any N with enterprise-only error; accepts `PurgeOnCycleClose`; `"PurgeOnCycleClose"` TOML deserializes; bare-`u32` rejected; project-wins merge (`test_retention_merge_project_wins`); merged `RetainDays` still rejected (`test_retention_merge_revalidated_rejects_retaindays`, AC-14).

## Confirmed
- `validate()` error variant: REUSED `ConfigError::RetentionFieldOutOfRange` with an enterprise-only message naming `RetainDays` (not a generic range message).
- Post-merge re-validation site: EXISTING — `validate_config(&merged, ...)` at `config.rs:2461` (load_config step 4) already re-validates the merged result, so a merged `RetainDays` is rejected (#3905 / Constraint 10). No new call site added.

## Knowledge Stewardship
- Queried: `context_search` for RetentionConfig validate/merge patterns + vnc-024 ADRs (ADR-005).
- Stored: nothing novel — the enum-as-enterprise-seam + OSS-rejects-`RetainDays` rationale is already an ADR (ADR-005); the per-field project-wins merge + post-merge re-validation is the established crt-036/#3905 pattern. No runtime-invisible gotcha surfaced.
