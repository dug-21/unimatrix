# Gate 3b Report: vnc-024

> Gate: 3b (Code Review)
> Date: 2026-06-05 (rev2 — Check 7 re-verified after rework)
> Result: PASS
> Branch/HEAD: feature/vnc-024 @ 95d6c389

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | All 5 components match validated pseudocode; one beneficial, stronger-than-spec departure (cfg(test)-gated ts-rs derive) documented below. |
| 2. Architecture compliance | PASS | ADR-001..005 followed; component boundaries (HTTP-only D2, config-only D4, wire.rs+bindings D1) honored; UDS path untouched. |
| 3. Interface implementation | PASS | Function Signatures table matched exactly (format_injection, observe_response_to_http, Accept-read, defaulter, validate arm). |
| 4. Test case alignment | PASS | Every component test-plan scenario has a corresponding test; gate-critical scenarios all green. |
| 5. Code quality | PASS (1 WARN) | Builds clean; no stubs/TODO/unimplemented; no non-test .unwrap(); no NEW file >500 lines (only pre-existing large files received additive inserts, per Constraint 8). |
| 6. Security | PASS (2 pre-existing WARNs) | No secrets, input validated at boundaries (sanitize_session_id, body limit, Accept), no path traversal, serde-malformed-safe. cargo audit: 1 pre-existing transitive advisory (not vnc-024). |
| 7. Knowledge stewardship | **PASS** | All 5 implementation-component reports now recorded under `agents/`; each carries a `## Knowledge Stewardship` block with `Queried:` + `Stored:` (patterns #4723, #4724 stored; 3 explicit "nothing novel -- {reason}"). |
| GATE-CRITICAL: transcript-delta guard | PASS | Early Ack after SessionWrite+sanitize, before :793/:849; typed parse; batch filter; no #1266 fall-through; no in-memory accumulation. 5/5 tests green. |
| GATE-CRITICAL: ts-rs dev-only | PASS | [dev-dependencies] only; absent from `cargo tree --edges normal` (0); cfg(test)-gated derive. |
| GATE-CRITICAL: contract fixtures | PASS | AC-11 dual-sided (Rust parse + node {offset,bytes} shape); AC-06 dual-direction all 4 skip_serializing_if fields, both runtimes. Node 4/4. |
| GATE-CRITICAL: transcript_retention | PASS | validate() REJECTS RetainDays (enterprise-only, not range); bare u32 rejected; merged config re-validated in production (#3905); enum has PartialEq. |
| GATE-CRITICAL: content negotiation | PASS | Accept read before into_parts (:206/:213); format_injection reused with MAX_INJECTION_BYTES=1400; text path Entries/BriefingContent only; UDS untouched; no new wire variant. |

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**: Each component's code follows the validated pseudocode:
- **Delta guard** (`listener.rs:765-786`): matches `transcript-delta-guard.md` — `if event.event_type == TRANSCRIPT_DELTA_EVENT { match from_value::<TranscriptDeltaPayload> { debug log } return Ack }`, placed after the SessionWrite check (:738) and `sanitize_session_id` (:748), before col-022 routing/feature-extract/insert. Batch arm (`:1007-1020`) uses `.filter(|event| event.event_type != TRANSCRIPT_DELTA_EVENT)` exactly as specified.
- **Retention** (`config.rs:1505-1646, 3376-3384`): enum, defaulter, Default impl, validate() arm, project-wins merge arm all match `transcript-retention.md` line-for-line, including the `ConfigError::RetentionFieldOutOfRange` reuse with a reason naming `RetainDays` as enterprise-only.
- **Content negotiation** (`observe.rs`, `router.rs:202-210`): matches `observe-content-negotiation.md`.
- **ts-rs codegen** (`wire.rs:55-289, 442-470`): 6 `#[ts(export)]` types + sentinel test.

**Documented departure (beneficial, stronger guarantee)**: The brief's Function Signatures table specified always-on `#[derive(...TS)]` + `#[ts(export)]`. The implementation uses `#[cfg_attr(test, derive(ts_rs::TS))]` + `#[cfg_attr(test, ts(export, ...))]` (wire.rs:55-56, 106-107, 190-191, 223-224, 257-258, 282-283). This is a STRONGER form of the ADR-001 dev-only constraint — the derive does not exist at all in non-test builds, making the "ts-rs never enters the runtime graph" guarantee structural rather than reliant on the dependency-kind alone. Consistent with `[dev-dependencies]` placement. Not a regression; an improvement. (No documented-rationale gap — the commit body and code comment both explain it.)

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**:
- **ADR-001** (ts-rs dev-dependency): `Cargo.toml:25-30` ts-rs under `[dev-dependencies]` only; bindings committed at `crates/unimatrix-engine/bindings/` (6 .ts + fixtures + harness).
- **ADR-002** (fixture-as-authority): `contract.test.mjs` reads committed fixtures, not the erased .ts; Rust emitter + node harness both assert on the same fixtures.
- **ADR-003** (HTTP-only negotiation): `observe.rs` text path is HTTP-mapper-only; UDS path in `hook.rs` untouched (diff is visibility-bump only).
- **ADR-004** (typed accept-and-drop): early-return guard, typed parse, no fall-through.
- **ADR-005** (OSS rejects RetainDays): hard validation failure, not accept-and-ignore.
Component boundaries respected: D2 HTTP-only, D4 config-only (not in wire bindings set), D1 wire.rs #[cfg(test)] + bindings/.

### Check 3 — Interface implementation
**Status**: PASS
**Evidence** (against the brief Function Signatures table):
- `pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta"` — `wire.rs:46`. ✓
- `pub struct TranscriptDeltaPayload { pub offset: u64, pub bytes: String }` — `wire.rs:284-289`. ✓
- `pub(crate) fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>` — `hook.rs:1047` (visibility bump only, no reimplementation). ✓
- `observe_response_to_http(resp: HookResponse, wants_text: bool) -> Response<...>` — `observe.rs:25`. ✓
- Accept read via `request.headers().get(http::header::ACCEPT)` before `into_parts()` — `router.rs:206-210` / `:213`. ✓
- `default_transcript_retention() -> TranscriptRetention { PurgeOnCycleClose }` — `config.rs:1574`. ✓
- validate() retention arm rejecting RetainDays as enterprise-only — `config.rs:1634-1646`. ✓
Error handling follows project patterns: guard returns `Ack` (fire-and-forget, no Error leak on malformed payload); validate returns `Err(ConfigError::...)`; serde-deserialize failures handled, no panics.

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: Component test-plan scenarios each have a corresponding passing test:
- transcript-delta-guard: `test_transcript_delta_uds_acks_zero_rows`, `test_transcript_delta_in_batch_dropped_rest_persist`, `test_transcript_delta_malformed_payload_still_acks_zero_rows`, `test_transcript_delta_requires_session_write`, `test_transcript_delta_parses_into_typed_payload` — **5/5 green**.
- transcript-retention: default, defaulter, absent-block, present-but-field-absent, accept-PurgeOnCycleClose, reject-RetainDays(N for N=0 and N>0, naming RetainDays not range), bare-u32 rejected, AC-14 merge project-wins, post-merge re-validation (`test_retention_merge_revalidated_rejects_retaindays`) — all green (server config tests 420 pass).
- content-negotiation: `test_observe_text_entries_byte_identical`, `test_observe_text_entries_over_budget_matches_truncation`, `test_observe_text_entries_empty_returns_204`, `test_observe_text_briefingcontent_returns_text`, `test_observe_text_pong_stays_json`, `test_observe_text_ack_stays_204_json_path`, `test_observe_text_error_stays_json` — all green.
- ts-rs/fixtures: `test_export_bindings_all_six_written_and_nonempty`, `test_emit_fixtures`, `test_round_trip_request_fixtures`, Rust AC-11 half, node harness 4/4 — all green. Engine: 435 pass.
Full server lib: **3492 pass, 1 fail** — the single failure (`server::tests::test_schema_integer_type_preserved_for_all_nine_fields`) is **pre-existing** (in `server.rs`, untouched by vnc-024; MCP schemars context_lookup id type) and confirmed failing on base 17b05639. Out of vnc-024 scope.

### Check 5 — Code quality
**Status**: PASS (1 WARN)
**Evidence**:
- `cargo build --workspace` finishes clean (warnings only, pre-existing dead-code).
- **Anti-stub**: scan of the added diff for `todo!()`/`unimplemented!()`/`TODO`/`FIXME` — none in production code. The `panic!` hits are all inside `#[cfg(test)]` fixture/round-trip assertions.
- **No non-test `.unwrap()`**: all `.unwrap()`/`.expect()` in the diff are inside `#[cfg(test)]` modules (fixture emitter, round-trip tests) or the pre-existing `.expect("...builder cannot fail")` static-builder convention in `observe.rs`. The guard uses `match` (no unwrap); validate returns `Err`.
- **500-line rule (WARN)**: No NEW file exceeds 500 lines — the only new code file is `contract.test.mjs` (78 lines). All over-limit files (config.rs 10492, listener.rs 8567, wire.rs 2153, tests.rs 1297, router.rs 520) were ALREADY over 500 at base 17b05639 (10246/8280/1602/1002/510 respectively); vnc-024 only made additive inserts into existing sections, exactly as brief Constraint 8 authorizes ("additions slot into existing sections ...; no NEW file exceeds 500 lines"). Flagged as a WARN (pre-existing codebase condition the brief accommodates), not a vnc-024-introduced FAIL.

### Check 6 — Security
**Status**: PASS (2 pre-existing WARNs)
**Evidence**:
- **Principle 8 / secrets-to-disk**: the central security control of this feature. The accept-and-drop guard provably prevents raw transcript bytes reaching SQLite on both transports + the batch arm; OSS validate() rejects `RetainDays` (no durable secret-bearing persistence). Verified by the 5 gate-critical tests (zero durable rows).
- **No hardcoded secrets**: none introduced.
- **Input validation at boundaries**: `sanitize_session_id` runs before the guard (load-bearing, SEC-01); body size limit + Accept header parsed defensively (`to_str().ok()`); the guard's typed parse failure cannot panic (matched, logged at debug, still Ack).
- **No path traversal / command injection**: file ops are the test-only fixture emitter writing to a fixed `bindings/fixtures/` path (atomic temp+rename); no user-controlled paths; no shell/process invocation added.
- **Serde safety**: malformed delta payload → debug log + Ack (no panic, no state corruption); bare-u32 retention → deserialize rejection (no panic).
- **WARN (pre-existing) — `cargo audit`**: 1 vulnerability `RUSTSEC-2023-0071` (rsa 0.9.10, Marvin timing sidechannel, medium, NO fix available) reaching the graph transitively via `sqlx-mysql` (the workspace uses the SQLite backend). Present in base 17b05639's Cargo.lock; **not introduced by vnc-024** (ts-rs adds zero runtime transitive deps — confirmed 0 in `cargo tree --edges normal`). Plus unmaintained-crate warnings (bincode). Out of vnc-024 scope; same class as the documented pre-existing test failure. Recommend tracking separately.
- **WARN (pre-existing) — `cargo clippy --workspace -- -D warnings`**: fails, but EVERY error is in files untouched by vnc-024 (`unimatrix-observe/*`, engine `auth.rs:113`, `event_queue.rs:164`, `patches/anndists`) — `collapsible_if` / `manual_pattern_char_comparison` lints newly promoted by the rust-1.95.0 toolchain. vnc-024's own touched files (wire.rs, config.rs, listener.rs, router.rs, observe.rs, hook.rs) produce **zero** clippy hits. Out of vnc-024 scope.

### Check 7 — Knowledge stewardship compliance
**Status**: PASS (rev2)
**Evidence**: All five implementation-component reports are now recorded under `product/features/vnc-024/agents/`, each with a `## Knowledge Stewardship` block carrying both a `Queried:` line and a `Stored:` line:
- **Component 1** (`vnc-024-agent-3-ts-rs-codegen-report.md`): Queried `context_search` (ts-rs/serde codegen + ADR-001); Stored: nothing novel — defers to #4722 (captured by Component 2). Explicit reason given.
- **Component 2** (`vnc-024-agent-4-contract-fixtures-report.md`): Queried `context_search` (surfaced #4722, #4719-4721); **Stored: entry #4724** (atomic fixture-write race + bigint offset trap).
- **Component 3** (`vnc-024-agent-5-observe-content-negotiation-report.md`): Queried `context_search` (Accept-header/tower + ADR-003); Stored: nothing novel — additive mapper branch reusing existing formatter; trap already in ADR-003. Explicit reason given.
- **Component 4** (`vnc-024-agent-6-transcript-delta-guard-report.md`): Queried `context_briefing` + 3 `context_search` (#1266, #4711, #4720, #763); **Stored: entry #4723** (two-arm accept-and-drop guard asymmetry).
- **Component 5** (`vnc-024-agent-7-transcript-retention-report.md`): Queried `context_search` (RetentionConfig validate/merge + ADR-005); Stored: nothing novel — enum-as-enterprise-seam already ADR-005; merge pattern is crt-036/#3905. Explicit reason given.

Both stored pattern IDs (**#4723**, **#4724**) are referenced. The three "nothing novel" entries each provide an explicit reason (no bare assertion → no WARN). The reports note the primary agents for Components 1/3/5 lost their API connection (temp-filesystem exhaustion) before self-reporting; their work had already landed and was independently verified at HEAD, and the reports were reconstructed by the Delivery Leader. The stewardship-artifact gap from rev1 is closed.

## GATE-CRITICAL — detailed verification

**transcript-delta guard** (PASS): `listener.rs:774-786` early `return HookResponse::Ack` placed AFTER the SessionWrite check (:738) + `sanitize_session_id` (:748), BEFORE feature-extraction (:817+) and `insert_observation`. Parses into the typed `TranscriptDeltaPayload` (imported from `unimatrix_engine::wire`, listener.rs:24), NOT raw `serde_json::Value`. Batch arm (`:1007-1020`) drops deltas via `.filter(... != TRANSCRIPT_DELTA_EVENT)` while the rest persist. Does NOT reuse the col-022 specialize-then-fall-through (#1266) — explicit anti-pattern comment + structurally unreachable persistence. No in-memory accumulation (no #670 pull-forward) — the only batch change is the filter. 5/5 tests green.

**ts-rs dev-only** (PASS): `[dev-dependencies]` only (Cargo.toml:25-30); absent from `cargo tree --edges normal` (count 0); cfg(test)-gated derive makes runtime absence structural. cargo audit's 1 advisory is pre-existing and unrelated (ts-rs adds no runtime deps).

**contract fixtures** (PASS): AC-11 dual-sided — Rust parses the committed `transcript_delta_payload.json` into `TranscriptDeltaPayload` (wire.rs:2101+), node harness asserts the `{offset, bytes}` shape with exactly `["bytes","offset"]` keys + lossless `Number.isSafeInteger` offset (contract.test.mjs:70-78). AC-06 dual-direction None-vs-omission for all four `skip_serializing_if` fields (topic_signal, provider, source, transcript_excerpt) in BOTH Rust and node (contract.test.mjs:46-60). The fixture is the contract authority (.ts erased at runtime). Node 4/4.

**transcript_retention** (PASS): `validate()` (config.rs:1634-1646) REJECTS `RetainDays` with an enterprise-only error (NOT a range check — test asserts the message names RetainDays and is not a range error); bare u32 rejected at deserialize; merged config re-validated in production via `validate_config(&merged)` at config.rs:2461 → `config.retention.validate()` at :2718 (#3905, AC-14); enum derives `PartialEq` (config.rs:1505).

**content negotiation** (PASS): Accept read at router.rs:206 BEFORE `request.into_parts()` at :213; `format_injection` reused (not re-implemented) with production `MAX_INJECTION_BYTES = 1400` (observe.rs:12,35); text path for `Entries`/`BriefingContent` ONLY (Pong/Ack/Error fall through to JSON); UDS path untouched (hook.rs diff is visibility-bump only); no new wire variant.

## Rework Resolution (rev2)

The rev1 REWORKABLE FAIL on Check 7 is resolved. The five implementation-component reports were recorded under `product/features/vnc-024/agents/`, each with a compliant `## Knowledge Stewardship` block (`Queried:` + `Stored:`). Patterns #4723 and #4724 are stored and referenced; the three "nothing novel" entries carry explicit reasons. No code change was required — all 11 other technical and gate-critical checks remained PASS. **Overall gate result: PASS.**

## Scope Concerns

None. Scope is correctly frozen to F1. No #670/F2 pull-forward. The architecture and code support every requirement. The only blocker is a missing process artifact, fixable without code change.
