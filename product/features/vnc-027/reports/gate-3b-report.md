# Gate 3b Report: vnc-027

> Gate: 3b (Code Review)
> Date: 2026-06-08
> Result: PASS (3 WARNs)
> Branch: feature/vnc-027 @ ffc7717d (10 impl commits + stage-3b reports)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | transport-uds / config / index / state / build-request-tools / merge-settings / wire / listener / observe all match their pseudocode + ADRs function-for-function |
| 2. Architecture compliance | PASS | Component boundaries intact; all 7 ADRs honored; transport seam preserved (transform/queue/delta byte-untouched) |
| 3. Interface implementation | PASS | post() contract, config mode+socketPath, HookResponse::Text allowlist, per-spawn transport selection, FR-16 canonical keying — all verified |
| 4. Test case alignment | PASS | Component test plans realized; risk-pinning tests for R-04, R-07/08, R-09, R-11, R-18 present |
| 5. Code quality | PASS (WARN) | Build exit 0; no stubs/TODO/FIXME/unimplemented in production; no .unwrap() outside #[cfg(test)]; no NEW file > 500 lines. WARN: pre-existing listener.rs/wire.rs exceed 500 (not introduced by feature) |
| 6. Security | PASS (WARN) | Frame caps enforced pre-allocation; no path traversal (sha256 hex[..16]); fail-open deser; no secrets. WARN: pre-existing transitive rsa advisory (no fix, zero deps added) |
| 7. Knowledge stewardship | PASS | All 11 stage-3b agent reports carry `## Knowledge Stewardship` with Queried + Stored/"nothing novel (reason)" |
| Merge sequence (size-gate first) | PASS | git log: ba338f08 size-gate is literal first impl commit, before any lib/hook-client/ growth |
| AC-11 frozen-wire zero-diff | PASS | scripts/regen-parity.sh reproduces committed goldens byte-for-byte |

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**:
- `transport-uds.js` (233 L): `post(config, frame, opts)` resolves only (never rejects); `encodeFrame` injects `accept:"text/plain"` ONLY for sync `ContextSearch`/`CompactPayload` via Object.assign on a copy (queue stays transport-agnostic); 1 MiB cap checked before write; settle-once `done()` guard clears the unref'd deadline; FNF resolves on `'finish'`, destroy only after settle; sync accumulates chunks, rejects declared length 0/>1 MiB before allocating the body; no `process.exit`. Matches pseudocode/transport-uds.md + ADR-002/003.
- `config.js` (329 L): `resolve()` returns `mode:"http"|"uds"`; env pair → http, partial → terminal `partial_env`; settings.local.json `unimatrix.remote` → http; ENOENT/incomplete → UDS fall-through; terminal `missing` retired; socketPath + stateDir single-derivation from one projectHash. Matches FR-12..16.
- `index.js` (443 L): `selectTransport` once per spawn from `config.mode`; null build-request sentinel short-circuits BEFORE transport selection (line 366); `runFireAndForget` keys delete on `canonicalEvent === "TaskCompleted"` (not frame type, not request.type); `pruneOffsets` wired FNF-path-only. Matches ADR-002/004/006.
- `state.js`/`build-request-tools.js`/`merge-settings.js`: deleteOffset doc-corrected; non-cycle PreToolUse → `null` (F-02 exact-equality retained); matcher narrowed to `context_cycle|mcp__unimatrix__context_cycle`, SubagentStop opt-in + opt-out prune scoped to Unimatrix-owned entries.
- `wire.rs`: `accept: Option<String>` with `#[serde(default, skip_serializing_if="Option::is_none")]` on ContextSearch/CompactPayload; `HookResponse::Text { body }` added; all hook.rs construction sites pass `accept: None`.

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**: All 7 ADRs verified in code. ADR-001: listener `negotiate_text_response` converts only when `wants_text`, allowlist exactly {Entries, BriefingContent}, empty Entries → Ack, Ack/Error/Pong stay JSON; single `response_injection_text` core in observe.rs consumed by both HTTP and UDS. ADR-002: SendResult mapping table reproduced in `mapHookResponse`/FNF path (FNF success status 0, client reject http_4xx). ADR-003: flush-before-FIN, never destroy unflushed, unref'd timers, no process.exit. ADR-004/005/006/007 as above. transform.js/queue.js/delta.js byte-untouched (transport seam preserved).

### Check 3 — Interface implementation
**Status**: PASS
**Evidence**: post() signature + never-reject honored (test_never_rejects). HookResponse::Text returned only to accept-callers — pinned by Rust `test_no_accept_yields_typed_json_never_text`, `test_allowlist_ack_error_pong_always_json`, `test_accept_text_plain_entries_yields_text`. index `selectTransport` + `test_selects_http_when_mode_absent_or_config_null`. FR-16 keying pinned by `test_canonical_event_flag_passed_not_frame_type`. HTTP-vs-UDS body byte-equivalence: `test_http_text_plain_and_uds_text_body_byte_identical`, `test_shared_core_single_implementation`.

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: node hook-client suite 535/536 pass (1 skip = FR-22 lone-surrogate, formally excepted). preformatted.rs covers R-07/08/09 (wants_text pre-dispatch, allowlist, header presence). state/index tests cover R-04 keying incl. assertable negative `test_stop_spawn_preserves_offset_taskcompleted_deletes`. build-request tests cover R-11 sentinel/F-02 (`test_non_cycle_tool_name_returns_null`, `test_cycle_near_miss_not_intercepted`, `test_null_request_returns_before_transport_selection`). transport-uds tests cover R-01/R-06/R-18 (lifecycle, frame caps, settle-once, timers unref). merge-settings tests cover AC-08 opt-in/off/non-boolean/opt-out matrix. cargo parity tests 9/9 pass.

### Check 5 — Code quality
**Status**: PASS (WARN)
**Evidence**: `cargo build --workspace` exit 0. Anti-stub scan of production diff: no TODO/FIXME/unimplemented!/todo!/placeholder. No `.unwrap()`/`expect`/`panic!` in added production Rust — all such occurrences are inside `#[cfg(test)]` modules of wire.rs. No NEW file exceeds 500 lines (largest new: parity_corpus_uds.rs 319, transport-uds.js 233, index.js modified to 443).
**WARN (W1)**: `listener.rs` (8934 L) and `wire.rs` (2427 L) exceed the 500-line standard, BUT both predate this feature (8851 / 2153 L at parent commit) and were modified additively only (+87 / +274). The feature introduces no oversized file. The codebase-wide tech-debt of these monolithic modules is out of scope for vnc-027 and not a feature-attributable defect. Recorded as WARN, not FAIL.

### Check 6 — Security
**Status**: PASS (WARN)
**Evidence**: Frame read rejects hostile declared length (0 / >1 MiB) BEFORE allocating the body (transport-uds.js:200-211; R-18). Socket path = `~/.unimatrix/{sha256(root).hex[..16]}/unimatrix.sock` — hash output cannot contain `/` or `..`, no path traversal. Malformed response JSON → `connect`-class failure, never throws (parseResponse wrapped). transport emits no stdout/stderr (`test_no_stdout_no_stderr_from_transport`); no hardcoded secrets; breadcrumbs carry urlHost only. SubagentStop opt-in non-boolean treated as unset (type-confusion guard). `cargo audit` exit reports 1 advisory.
**WARN (W2)**: RUSTSEC-2023-0071 (rsa 0.9.10, Marvin timing sidechannel, medium 5.9) is transitive via `sqlx-mysql → sqlx`; "no fixed upgrade available". This feature changed zero Cargo/npm dependencies (NFR-6 honored — confirmed Cargo.lock/toml untouched), so the advisory is purely pre-existing and unfixable within feature scope. Plus one `bincode` unmaintained warning (also pre-existing, transitive via hnsw_rs). Not attributable to vnc-027.

### Check 7 — Knowledge stewardship
**Status**: PASS
**Evidence**: All 10 component implementer reports + the risk report contain `## Knowledge Stewardship` with `Queried:` entries. Stored entries: #4820 (size-gate), #4821 (wire), #4822 (sentinel), #4823 (config), #4824 (transport), #4825 (index), #4826 (merge-settings), #4809 (risk), plus parity-corpus pattern. Two "nothing novel to store" reports (listener-preformatted, state-offset-rekey) each give an explicit reason (textbook application of #4743/#4795; doc-only correction governed by #4809) — no WARN.

## Process / Contract Audits

- **Merge sequence (R-02, AC-09)**: PASS. Per-commit audit confirms `ba338f08 impl(size-gate)` touches only `test/check-hook-client-size.js` and is the first impl commit; the first `lib/hook-client/` growth (`b7c779e3 build-request-sentinel`) follows it. Size-gate-first contract honored.
- **AC-11 frozen F1 wire contract**: PASS. `scripts/regen-parity.sh` regenerates the corpus from the Rust oracle with zero git diff against committed goldens — additive `accept`/`Text` produce byte-identical existing frames. wire.rs `test_context_search_without_accept_serializes_byte_unchanged` asserts the key is omitted when None. Bindings change additively only (per Gate-3a OQ2 clarification).

## Pre-existing Failures (NOT attributable to vnc-027)

| Failure | Evidence it is pre-existing |
|---------|-----------------------------|
| node `writeMcpJson` (init installer LD_LIBRARY_PATH env baseline) | Named in spawn prompt as #679 (c1b10dd0) regression outside vnc-027 scope; assertion is on init env, not hook-client |
| Rust `http::token::tests::test_concurrent_creation_no_corruption` | token.rs not in vnc-027 diff; flaky under parallel load — passes 3/3 in isolation |
| RUSTSEC-2023-0071 (rsa) / bincode unmaintained | Transitive deps; zero dependency changes in this feature |

## Test Totals
- node full suite: 685 tests, 683 pass, 1 fail (known init #679), 1 skip (FR-22 lone-surrogate exception)
- node hook-client suite: 536, 535 pass, 1 skip
- cargo unimatrix-server lib: 3621, 3619 pass, 1 flaky-unrelated, 1 ignored (parity generator)
- cargo build/clippy: exit 0 (warnings pre-existing in unrelated crates)

## Rework Required
None.

## Scope Concerns
None.
