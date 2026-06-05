# Gate 3c Report: vnc-024

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-05
> Branch/HEAD: feature/vnc-024 @ 40970ec1
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | R-01..R-14 each map to passing, specific tests; RISK-COVERAGE-REPORT.md complete |
| 2. Test coverage completeness | PASS | All Phase-2 risk→scenario mappings exercised; integration counts present |
| 3. Specification compliance | PASS | AC-01..AC-15 all verified against running code |
| 4. Architecture compliance | PASS | ADR-001..005 followed; dispatch convergence + guard placement confirmed in source |
| 5. Knowledge stewardship | PASS | Tester report has `## Knowledge Stewardship` with Queried + Stored entries |
| **AC-12 GATE (3 arms)** | **PASS** | UDS row-count + batch + HTTP structural; all green; early-return, not col-022 fall-through |

## Gate Prerequisite — AC-12 zero-durable-rows (R-03/R-04, principle 8)

Verified GREEN on all three required arms by running the tests directly:

| Arm | Test | Result |
|-----|------|--------|
| UDS dispatch | `uds::listener::tests::test_transcript_delta_uds_acks_zero_rows` | PASS — `Ack` + `SELECT COUNT(*) observations == 0` |
| RecordEvents batch | `..._in_batch_dropped_rest_persist` | PASS — delta filtered from `obs_batch`, N=2 normal events persist |
| HTTP `/observe` | `http::router::tests::test_observe_http_prefix_session_id_preserves_delta_routing` + `..._batch_prefix_preserves_delta_drop_routing` + `..._delta_body_deserializes_to_record_event` | PASS |

5/5 UDS guard tests + 10/10 HTTP boundary tests pass (verified via `cargo test`).

**Guard structure (listener.rs:765-786) — confirmed correct:**
- Early `return HookResponse::Ack` sits AFTER the `SessionWrite` capability check (`:738`) and `sanitize_session_id` (`:748`), BEFORE all persistence (`:849` `insert_observation` provably unreachable for a delta).
- Control flow keys on `event_type == TRANSCRIPT_DELTA_EVENT` ONLY; the `serde_json::from_value::<TranscriptDeltaPayload>` is observability-only and never changes routing (malformed payload still drops + Acks — `test_transcript_delta_malformed_payload_still_acks_zero_rows`).
- This is an EARLY-RETURN drop, NOT the col-022 specialize-then-fall-through anti-pattern (#1266). The code comment documents this explicitly and the source confirms it.
- Batch arm (listener.rs:1007-1009) filters `event_type != TRANSCRIPT_DELTA_EVENT` before building `obs_batch` — deltas never reach `insert_observations_batch`.

**HTTP arm scoping assessed — SOUND, not an evasion.** The HTTP `/observe` handler (router.rs:206-260) reads `Accept` before `into_parts`, runs `prefix_session_id`, then calls the SAME `dispatch_request` imported from `uds::listener` (router.rs:35). The zero-rows behavior of that single dispatch arm is proven by the UDS row-count test. The only HTTP-unique risk (R-04 integration trap: `prefix_session_id` mutating the event before dispatch could bypass the guard) is exactly what the HTTP structural tests target — they prove the prefix changes `session_id` but NOT the `event_type` drop-routing key. A full HTTP-boot row-count test would re-exercise the identical dispatch arm and require the embedding model. The structural assertion targets the HTTP-unique surface precisely. AC-12 is genuinely green on all three arms.

## Detailed Findings

### Check 1 — Risk Mitigation Proof
**Status**: PASS
**Evidence**: Each of R-01..R-14 maps to a passing, specific test (RISK-COVERAGE-REPORT.md Coverage Summary). Independently verified the Critical/High risks:
- R-01 (serde mis-modeling): `wire::tests` (435 passed, 0 failed) + `node --test contract.test.mjs` 4/4 incl. dual-sided delta.
- R-02 (None-vs-omission dual-direction): node "None-vs-omission" test asserts omitted-key fixtures LACK the key (not null) — passing.
- R-03/R-04 (secrets-to-disk, both transports + batch): AC-12 three-arm gate green (above).
- R-05 (format_injection byte-identity): `test_observe_text_entries_byte_identical` + `..._over_budget_matches_truncation` assert body == `format_injection(&items, MAX_INJECTION_BYTES)` — production budget, truncation boundary. observe.rs:35 calls the real fn, no re-impl.
- R-06/R-07 (allowlist + Accept ordering): 7 observe_text + 10 observe_http boundary tests pass; Accept read before `into_parts` confirmed (router.rs:206-213).
- R-09 (retention touchpoints incl. RetainDays-reject): 11 retention tests pass incl. `test_validate_rejects_retaindays_enterprise_only`, `test_retention_merge_revalidated_rejects_retaindays`.
- R-14 (CI diff-gate): `.github/workflows/ci.yml` ordering load-bearing (test→diff→node); bindings regenerate clean (no drift).

### Check 2 — Test Coverage Completeness
**Status**: PASS
**Evidence**: All Phase-2 risk-to-scenario mappings exercised. Integration test counts present in RISK-COVERAGE-REPORT.md (10 new `test_observe_http_*`, 5 UDS guard, 7 observe_text, 11 retention, 196 hook parity). Cross-component HTTP↔UDS convergence risk (R-04 trap) covered by the HTTP structural tests. Edge cases covered (offset:0/empty bytes, malformed payload, multivalue/wildcard Accept).

### Check 3 — Specification Compliance
**Status**: PASS
**Evidence**: AC-01..AC-15 verified against running code:
- AC-01/02/15: six types carry `#[ts(export)]` under `cfg(test)` (wire.rs:55-284); ts-rs absent from `cargo tree --edges normal`, present only under dev edges (v12.0.1).
- AC-02/03: `cargo test -p unimatrix-engine` regenerates 6 `.ts` + fixtures; `git diff --exit-code bindings/` clean post-regen.
- AC-04/05/06/11: node harness 4/4 incl. dual-sided `TranscriptDeltaPayload` TS→Rust round-trip.
- AC-07/08/09/10: observe_text (7) + observe_http (10) + 196 hook parity tests pass; `format_injection`/`MAX_INJECTION_BYTES` reused.
- AC-12: three-arm gate green (above).
- AC-13/14: 11 retention tests; RetainDays rejected enterprise-only, bare-u32 rejected, PurgeOnCycleClose default + project-wins merge + merged re-validation.

### Check 4 — Architecture Compliance
**Status**: PASS
**Evidence**: ADR-001 (ts-rs dev-only, 6 exports) — confirmed. ADR-002 (fixtures as authority) — node harness is the asserting authority. ADR-003 (content negotiation allowlist, Accept before into_parts) — router.rs:206-260. ADR-004 (accept-and-drop early-return, typed payload) — listener.rs:765-786, not col-022 fall-through. ADR-005 (retention enum, OSS rejects RetainDays) — confirmed by tests. HTTP path routes through the same `uds::listener::dispatch_request` (router.rs:35) — convergence as designed. No architectural drift. infra-001 harness + Python bindings untouched by vnc-024 (verified `git diff bcd971e9..40970ec1`).

### Check 5 — Knowledge Stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md `## Knowledge Stewardship` block (lines 156-158): `Queried:` context_briefing surfacing #4452/#4515/ADR-003 #4714; `Stored:` a reusable transport-convergence guard-testing pattern.

## Integration / Smoke Gate Assessment

The 22 server-crate Rust integration tests covering vnc-024's integration ACs (AC-07/08/09/10/12-HTTP) all pass. Per the test-plan OVERVIEW, infra-001's stdio MCP harness does NOT reach the HTTP `/observe` tower handler or UDS dispatch — vnc-024's actual surfaces are out-of-band of the stdio harness. This scoping is sound: the feature's surfaces are HTTP `/observe` + UDS, both covered by passing in-crate tests, not by any new infra-001 suite.

**Smoke gate (GH#685) — NOT a genuine blocker to vnc-024.** The infra-001 smoke suite is BLOCKED as-committed by GH#685 (harness `client.py:97-98` launches the binary with a stale `serve --stdio` subcommand removed by the rmcp-1.7 migration, vnc-023 #674). This is harness-vs-binary CLI drift external to vnc-024 — vnc-024 touched no CLI entrypoint (`git diff bcd971e9..40970ec1` shows no changes to `main.rs`/`server.rs`). With the launch corrected locally for verification (committed harness reverted, `git diff` clean), 19/23 smoke pass; the 4 failures are all pre-existing/unrelated:
- 2× GH#684 (rmcp-1.7 string-int id coercion)
- 1× category-allowlist drift (cf. closed GH#632)
- 1× harness references a tool absent from this build (`context_cycle`)

GH#685 is external, does not mask any feature bug, and vnc-024's risk surface is fully covered by passing in-crate tests. The smoke prerequisite is satisfied.

## xfail / GH Issue Hygiene

| Issue | Verified | Genuinely unrelated to vnc-024? |
|-------|----------|---------------------------------|
| GH#684 | OPEN, title matches | YES — `server.rs` last touched by vnc-023 (#674); vnc-024 made zero changes. The single server-lib unit failure (`test_schema_integer_type_preserved_for_all_nine_fields`) and the two string-id integration failures share this rmcp-1.7 root cause. |
| GH#685 | OPEN, title matches | YES — harness CLI drift from the rmcp migration; vnc-024 touched no CLI entrypoint. |

No integration tests were deleted, commented out, or xfail-masked (`git log --diff-filter=D` over the range: none). No `@pytest.mark.xfail` markers added (failures tracked via issues; harness file unmodified).

## Code Quality / Independent Verification

- `cargo test -p unimatrix-server --lib`: **3502 passed, 1 failed** — the single failure is GH#684 (pre-existing, out-of-scope).
- `cargo test -p unimatrix-engine --lib`: **435 passed, 0 failed**.
- `node --test contract.test.mjs`: **4/4 pass** (exit 0), incl. dual-sided delta.
- Bindings diff-gate: clean after regeneration (no drift).
- ts-rs: absent from runtime edges; dev-only.

## Result

**PASS.** All 15 ACs verified; all 14 risks mitigated by passing, specific tests; AC-12 gate prerequisite green on all three arms (UDS + batch + HTTP) before downstream ACs trusted. Architecture and ADRs followed; no drift. The two open failures (GH#684, GH#685) are pre-existing, filed, and genuinely external to vnc-024 — they do not mask any feature defect. infra-001 harness and bindings left unmodified.

## Rework Required

None.
