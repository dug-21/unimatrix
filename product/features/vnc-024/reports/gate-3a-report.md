# Gate 3a Report: vnc-024

> Gate: 3a (Component Design Review)
> Date: 2026-06-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | All 5 components map 1:1 to the ARCHITECTURE Component Breakdown + ADR-001..005; anchors/locations match. |
| 2. Specification coverage | PASS | FR-01..FR-15 + NFR-01..07 all have pseudocode; no scope additions; #670/F2 exclusions respected. |
| 3. Risk coverage | PASS | R-01..R-14 each map to ≥1 test scenario; Critical/High risks have concrete per-component assertions. |
| 4. Interface consistency | PASS | OVERVIEW shared types (`TranscriptDeltaPayload`, `TRANSCRIPT_DELTA_EVENT`) used consistently; signatures match the brief/architecture surface; verified against live source. |
| 5. Knowledge stewardship | PASS | All design-phase agent reports carry `## Knowledge Stewardship`; active agents (architect, risk) have Stored/Declined w/ reason; read-only agents (pseudocode/spec/testplan) have Queried. |
| GATE-CRITICAL: transcript-delta-guard | PASS | Early `return Ack` after SessionWrite+sanitize, before :793/:849; both transports + batch arm; typed parse; explicit anti-#1266 contrast. |
| GATE-CRITICAL: contract-fixtures (dual-sided) | PASS | AC-11 dual-sided (Rust↔TS); AC-06 dual-direction both runtimes for all 4 fields. |
| GATE-CRITICAL: transcript-retention | PASS | validate() REJECTS RetainDays (enterprise-only, not range); bare u32 rejected; merged config re-validated (#3905). |
| GATE-CRITICAL: ts-rs codegen | PASS | 6 exported types; dev-only footprint; CI diff-gate generate-before-diff ordering + correct path. |

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: The pseudocode OVERVIEW Component Map (5 components → D1..D4) matches ARCHITECTURE's Component Breakdown exactly:
- D1 ts-rs codegen + fixtures → `ts-rs-codegen.md` + `contract-fixtures.md`; ADR-001/ADR-002.
- D2 /observe negotiation → `observe-content-negotiation.md`; ADR-003.
- D3 transcript_delta guard → `transcript-delta-guard.md`; ADR-004.
- D4 transcript_retention → `transcript-retention.md`; ADR-005.
Technology choices are consistent with the ADRs: ts-rs as `[dev-dependencies]` only (ADR-001), fixture-as-authority (ADR-002), `Accept`-before-`into_parts` + allowlist `{Entries,BriefingContent}` (ADR-003), typed `TranscriptDeltaPayload` + early-return guard (ADR-004), OSS-rejects-`RetainDays` (ADR-005). Component boundaries (HTTP-only for D2, config-only for D4, `wire.rs #[cfg(test)]` + `bindings/` for D1) match the architecture's stated boundaries.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: Every FR has corresponding pseudocode:
- FR-01..FR-07 → ts-rs-codegen.md (derives, codegen test, CI gate) + contract-fixtures.md (fixtures, node harness, dual-direction None-vs-omission, flatten).
- FR-08..FR-10 → observe-content-negotiation.md (text branch allowlist, JSON-unchanged path, Pong/Ack/Error stay JSON).
- FR-11 → contract-fixtures.md (dual-sided delta) + ts-rs-codegen.md (6th binding).
- FR-12 → transcript-delta-guard.md.
- FR-13/FR-14 → transcript-retention.md (enum + 4 touchpoints + merge).
- FR-15 (precedence note) → ts-rs-codegen.md doc-comment on `ImplantEvent`, documentary only.
NFRs addressed: NFR-01/AC-15 dev-only footprint (cargo tree/metadata/audit); NFR-02 additive HTTP-only; NFR-03 no new wire variant (binding diff empty for existing types); NFR-04 SessionWrite inherited (guard after capability check); NFR-06 payload ceiling unchanged; NFR-07 500-line rule (additions slot into existing sections). No scope additions: pseudocode explicitly defers buffering/distillation/GC/merge-logic to #670 (SR-05) and the 64 KiB cap to F2.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: Test-plan OVERVIEW Risk→AC→Test mapping table covers R-01..R-14, each with at least one concrete scenario in a component plan:
- R-01/R-02 (Critical, serde fidelity) → contract-fixtures.md per-variant + dual-direction + non-trivial.
- R-03/R-04 (Critical, GATE) → transcript-delta-guard.md zero-rows on HTTP+UDS+batch.
- R-05/R-06/R-07 (High) → observe-content-negotiation.md byte-identity (truncation), allowlist, content-type-at-boundary.
- R-08 (High) → contract-fixtures.md binding cross-check vs ass-069 Q2/Q7 list.
- R-09/R-11 → transcript-retention.md four-touchpoint + PartialEq + merge re-validation.
- R-12 → ts-rs-codegen.md cargo tree/metadata/audit.
- R-14 (High meta-gate) → ts-rs-codegen.md gate self-test (mutate→fail, restore→pass, ordering+path).
- R-10/R-13 (Low) → residual TOML + precedence-note reviewer checks.
Risk priorities are reflected in emphasis: AC-12 GATE prerequisite is called out across OVERVIEW + the guard plan; integration risks (dispatch convergence, #1266 collision, merge re-validation, budget coupling) each appear in the relevant plan. Edge cases from the risk strategy (offset:0/empty bytes, malformed payload, empty Entries→204, multi-value Accept, RetainDays(0)) are enumerated in the component plans.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: OVERVIEW defines the two shared symbols once and all consumers reference them identically:
- `TranscriptDeltaPayload { offset: u64, bytes: String }` — Component 1 emits as 6th binding, Component 2 round-trips dual-sided, Component 4 parses into it. The guard plan and fixtures plan explicitly note they must assert the SAME struct shape (a divergence is flagged as a defect).
- `TRANSCRIPT_DELTA_EVENT` const in `wire.rs` — referenced by Component 4 on both `RecordEvent` and `RecordEvents` arms.
Signatures verified against live source:
- `format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>` at hook.rs:1047 matches the pseudocode exactly; `MAX_INJECTION_BYTES = 1400` (hook.rs:29) is confirmed the production constant used at hook.rs:979/1031 — the pseudocode's budget-parity resolution (R-05 OQ) is correct against real source.
- `observe_response_to_http(resp, wants_text)` signature change threads from the single caller (router.rs:250).
- `ConfigError::RetentionFieldOutOfRange { path, field: &'static str, value: String, reason: &'static str }` (config.rs:1975) matches the retention pseudocode's reuse exactly; the `reason: &'static str` accepts the enterprise-only static message. No contradictions found across component files.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: All design-phase agent reports contain a `## Knowledge Stewardship` block:
- Architect (active-storage): `Stored:` entries #4712–#4716 (ADRs) + design-review corrections #4718–#4721 + deprecation of #4717 via `context_correct`/`context_deprecate`. Compliant.
- Risk-strategist (active-storage): `Stored: nothing novel to store -- {reason}` with a concrete reason (existing patterns #4711/#3557/#1266/#4070 apply; candidate generalization deferred until a 2nd feature repeats it). Compliant — reason present, so no WARN.
- Pseudocode (read-only): `Queried:` entries (#4720/#4721/#4711/#1266/#3255/#4714/#4718) + deviations-none note. Compliant.
- Spec (read-only): `Queried:` entry (#4721/#4720 via briefing). Compliant.
- Test-plan (read-only): `Queried:` entries + `Stored: nothing novel -- {reason}`. Compliant.

### GATE-CRITICAL — transcript-delta-guard
**Status**: PASS
**Evidence**: `pseudocode/transcript-delta-guard.md` places the guard AFTER the SessionWrite capability check (:737) + `sanitize_session_id` (:747-757) and BEFORE col-022 routing (:767), feature-extraction (:793), and `insert_observation` (:849), with an explicit early `RETURN HookResponse::Ack`. It includes a dedicated CONTRAST table vs the col-022 #1266 specialize-then-fall-through pattern, stating persistence is unreachable for a delta. Both transports covered (HTTP via router.rs:234 → dispatch_request; UDS via listener loop) plus the `RecordEvents` batch arm via a `.filter(event_type != TRANSCRIPT_DELTA_EVENT)` before `obs_batch` construction (delta dropped, rest persist). Payload is parsed into typed `TranscriptDeltaPayload` (not raw `serde_json::Value`), with parse-failure logging at debug and control-flow independence (still `Ack`). Test plan asserts zero-durable-rows on all three arms as the gate prerequisite, plus structure/anti-pattern + auth-inheritance (NFR-04) scenarios. All sub-requirements of the gate-critical check satisfied.

### GATE-CRITICAL — contract-fixtures (dual-sided)
**Status**: PASS
**Evidence**: `test-plan/contract-fixtures.md` asserts AC-11 dual-sided: Rust→TS (Rust round-trip into the struct) AND TS→Rust (node --test deserializes into the `TranscriptDeltaPayload` binding, catching client-side offset/bytes drift), explicitly stating "a Rust-emit-only check does NOT satisfy AC-11." AC-06 is dual-direction (emit-absent + parse-default) in BOTH Rust and Node for all four `skip_serializing_if` fields with a non-trivial-value round-trip guard, with a hard coverage requirement rejecting a 4-field×1-direction matrix.

### GATE-CRITICAL — transcript-retention
**Status**: PASS
**Evidence**: `pseudocode/transcript-retention.md` + `test-plan/transcript-retention.md`: OSS `validate()` REJECTS `RetainDays(_)` for any N (incl. 0) with an enterprise-only error naming `RetainDays` — explicitly "NOT a generic range error / no range-check arm." Bare `u32` is rejected at the deserialization level (enum is the only accepted shape). Merged config is re-validated (#3905) so a merged `RetainDays` is still rejected. All four touchpoints (defaulter, Default impl, validate, project-wins merge) + literal-construction grep covered.

### GATE-CRITICAL — ts-rs codegen
**Status**: PASS
**Evidence**: `pseudocode/ts-rs-codegen.md` + `test-plan/ts-rs-codegen.md`: exactly 6 exported types (5 wire + `TranscriptDeltaPayload`), `[dev-dependencies]` only, dev-only footprint proven via cargo tree --edges normal + cargo metadata + cargo audit. CI diff-gate ordering is load-bearing: `cargo test` (generate) BEFORE `git diff --exit-code crates/unimatrix-engine/bindings/`, with an ordering+path assertion in the self-test (mutate→non-zero, restore→zero).

## Notes / Non-blocking observations (carry to delivery — not FAILs)

- **OQ resolution captured**: the R-05 budget question is resolved in pseudocode to `MAX_INJECTION_BYTES (=1400)`, verified against live hook.rs (979/1031). Delivery should reference the single shared constant (no re-declaration) so future drift cannot occur silently.
- **TS→Rust direction mechanics**: contract-fixtures pseudocode describes the TS→Rust leg as a Rust test parsing the same committed fixture rather than a payload originated by the TS runtime. The intent (both parse directions into the typed struct, both runtimes asserting the `{offset,bytes}` shape) is sound for F1's no-TS-package constraint; delivery should ensure the node harness asserts the binding-declared shape (exactly `["bytes","offset"]` keys) so a client-side shape drift is genuinely caught, as the plan states. Non-blocking; the plan already specifies this assertion.
- **u64 precision**: both D1 and D2 flag the `offset` ts-rs `bigint`/`number` mapping; the fixture value must stay within 2^53 or use the chosen override. Already an explicit delivery-confirmed open question; non-blocking.
- **ts-rs export API + post-merge validate() call site + validate error-variant choice**: all three are explicitly flagged as delivery-confirmed with a stable contract; none block design approval.

## Rework Required

None.

## Scope Concerns

None. Scope is correctly frozen to F1; #670/F2/F5 pull-forward is explicitly excluded and the pseudocode/test-plans honor it (SR-05).
