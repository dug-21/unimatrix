# Alignment Report: vnc-024

> Reviewed: 2026-06-05
> Artifacts reviewed:
>   - product/features/vnc-024/architecture/ARCHITECTURE.md
>   - product/features/vnc-024/specification/SPECIFICATION.md
>   - product/features/vnc-024/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-024/SCOPE.md
> Scope risk: product/features/vnc-024/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Strategic goals: #4710 (personal-cloud), #4678 (domain-agnostic), #4671 (root vision)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances personal-cloud (#4710): single-edge-language, remote fidelity, enterprise seams all directly served. Principle 8 (secrets) honored via accept-and-drop guard. |
| Milestone Fit | PASS | Correctly scoped as Chunk 1/F1; consumers (#670, F2–F5) explicitly OUT. No future-chunk pull-forward. |
| Scope Gaps | PASS | All five SCOPE goals + AC-01..AC-15 mapped into spec FR/AC. No dropped items. |
| Scope Additions | PASS | One additive item (`RecordEvents` batch-arm coverage) is implied by SCOPE constraint 3, not new scope. No unrequested surface. |
| Architecture Consistency | PASS | Architecture, spec, and risk strategy agree on all four deliverables and the principle-8 guard placement. |
| Risk Completeness | PASS | Secrets-to-disk (R-03/SR-07) elevated to gate prerequisite on both transports + batch arm. Covers all vision-critical risks. |
| Vision-Doc Currency | WARN | PRODUCT-VISION principle 6 still carries pre-ass-068 wording; Q6 recommended updating it. Documentation drift, not a vnc-024 defect. See Variance 1. |

PASS x6, WARN x1, VARIANCE x0, FAIL x0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE goal (1–5) and AC (01–15) traces into the spec; spec §Traceability is complete and accurate. |
| Addition | `RecordEvents` batch-arm guard coverage | Architecture (Deliverable 3) and risk strategy (R-04) require the accept-and-drop guard to also drop a `transcript_delta` element inside a `RecordEvents` batch. SCOPE constraint 3 forbids inheriting the generic-observation fall-through for deltas; the batch arm is another path to that same fall-through, so this is correct *coverage* of the stated guard, not new scope. Acceptable. |
| Simplification | No new wire variant for `transcript_delta` | Rationale (SCOPE Background Research / constraint 3): a new `event_type` string value on the existing `RecordEvent`/`ImplantEvent` keeps the change backward-compatible and codegen-stable. Documented and sound. |
| Simplification | Bindings at ts-rs default `crates/unimatrix-engine/bindings/`, not promoted to workspace-root | Rationale (OQ-02): CI-gated source of truth; F2/F5 vendor at build time. Documented. |

## Variances Requiring Approval

No VARIANCE or FAIL items. One WARN for human awareness (does not block vnc-024):

1. **PRODUCT-VISION.md principle 6 wording is stale (WARN, documentation drift)**
   - **What**: Principle 6 currently reads *"Single binary, zero required infrastructure. Container is optional. Daemon + UDS works without it."* ass-068 Q6 (FINDINGS.md:255, :351, :390) recommended updating it to *"Single binary server, zero required infrastructure. The client is an adapter — JS for hooks, the binary for MCP. Container is optional."* The recommendation has not been applied.
   - **Why it matters**: vnc-024 is the first chunk to operationalize the single-binary-server / client-is-an-adapter distinction — ts-rs codegen (Deliverable 1) and server-side `format_injection` (Deliverable 2) deliberately move logic into the server so the future TS client stays a thin adapter. The vnc-024 documents themselves consistently use the correct "adapter" framing; only the upstream vision doc lags. Leaving the old wording invites future readers to mis-read the client as infrastructure.
   - **Recommendation**: Accept for vnc-024 (the feature does not modify the vision doc and is not blocked by it). Separately apply the ass-068 Q6 wording change to PRODUCT-VISION.md principle 6 — a one-line edit, owned by the vision steward, not this delivery. This is the *only* outstanding action and it is editorial.

## Detailed Findings

### Vision Alignment

Goal #4710 (personal-cloud) is the primary driver and vnc-024 maps cleanly onto its success criteria:
- *"Single edge language — JS/TS hook client only; no Python, no second client runtime"* — vnc-024's entire premise. ts-rs generates the TS wire contract from `wire.rs` (the single Rust source of truth), CI-gated against drift (AC-01..AC-05). This is exactly the mechanism that makes a single JS/TS edge client trustworthy. No second client runtime is introduced; ts-rs is dev-only (AC-15, NFR-01). Aligned with the memory note "client is JS/TS only; Python rejected, never a 3rd edge language."
- *"Remote sessions have same intelligence pipeline fidelity ... via client-streamed transcript deltas (ass-069), NOT session hosting"* — Deliverable 3 lands the `transcript_delta` wire field so the streaming mechanism ships in the generated bindings from day one. F1 ships the wire carrier only; the consuming buffer is correctly deferred to #670.
- *"Enterprise extends, never re-architects ... data retention as a policy knob (OSS default = ephemeral; enterprise sets retain/encrypt/residency by config)"* — Deliverable 4 types `transcript_retention` as `PurgeOnCycleClose | RetainDays(u32)`. The architecture (ADR-005) and risk assessment (A2) both correctly identify the enum (not a bare `u32`) as the load-bearing enterprise seam: the OSS default is ephemeral purge-on-cycle-close, and the enum extends to retain/encrypt/residency without re-architecture. A bare `u32` would force the re-architecture the goal forbids. This is precise alignment with the enterprise-seam posture, and the documents defend it explicitly (SCOPE OQ-05, constraint 6, AC-13).

**Principle 8 ("No secrets in any database")** is the sharpest vision check and the documents handle it correctly. `transcript_delta.bytes` is raw conversation content that may contain secrets/keys. The naive path — inheriting the `RecordEvent` generic-observation fall-through (`listener.rs:849`) — would persist those bytes to SQLite, a direct principle-8 violation. All three documents converge on an explicit **accept-and-drop** guard (`return Ack`, persist nothing) placed *after* the capability check but *before* any persistence, on both transports plus the batch arm. The risk strategy (R-03) elevates the zero-durable-rows test to a **gate prerequisite** — green before any downstream AC is trusted — and explicitly rejects the false-safety framing ("doesn't error today" / "no client streams yet" are not safety properties, per #4711). This is the strongest possible posture for the principle and is correct.

No conflict with the other principles: hash-chain (1), audit-log (2), capability-at-service-layer (3 — guard sits after the `SessionWrite` check, NFR-04), graceful degradation (5), in-memory hot path (7) are unaffected. Principle 6 currency is the WARN above.

### Milestone Fit

vnc-024 is explicitly Chunk 1/F1 of the ass-068 five-chunk migration, and the milestone discipline is exemplary. The temptation in F1 is to build the #670 in-memory buffer "while we're here" because the accept-and-drop guard sits exactly where that buffer will live (risk SR-05/R-05). All three documents pre-empt this: AC-12 asserts **non-persistence (zero rows), never buffering**; the architecture boundary note states "It accumulates nothing in memory either — buffering is the re-scoped #670's job"; the spec NOT-in-scope and risk strategy both instruct the reviewer to reject any in-memory accumulation. The `transcript_retention` field is config-only with no GC consumer in F1 (deferred to #670/crt-036). Content negotiation is HTTP-only with UDS untouched (AC-10). Nothing from F2–F5 or #670 is pulled forward. This is correct milestone targeting — F1 builds the minimum that freezes the downstream contract, no more.

### Architecture Review

The architecture decomposes into four independent-but-themed deliverables ("freeze the F2/#670 interface now") and is internally consistent with the spec and risk strategy. ADR-001..ADR-005 each carry a rationale traceable to a SCOPE OQ and a risk ID. Two non-blocking delivery-time open questions are correctly surfaced (ts-rs externally-tagged enum representation for `RetainDays`; `format_injection` byte budget) — both are implementation details, not scope or vision questions, and do not affect alignment. The integration-surface table gives exact signatures and line anchors, supporting the "frozen contract" intent (SR-04/R-08).

### Specification Review

The spec's FR/AC/NFR set fully covers the five SCOPE goals with an accurate §Traceability matrix (SCOPE AC → spec AC, and Risk → coverage). Notable strengths in vision-critical areas:
- AC-12 is marked a **GATE** matching the risk strategy — the principle-8 enforcement is structurally prioritized, not just listed.
- AC-06/FR-07 enumerate the four `skip_serializing_if` fields by name with dual-direction assertions — closing the most-omitted serde-test gap (#885/#3557) that would otherwise ship the wire contract subtly wrong and propagate to the JS/TS client.
- AC-13 explicitly *rejects a bare `u32`*, codifying the enterprise-seam posture as a testable criterion.

No requirement exceeds SCOPE; the NOT-in-scope section mirrors SCOPE's Non-Goals exactly.

### Risk Strategy Review

The risk register covers every vision-relevant failure mode and prioritizes them correctly. The single highest-consequence risk (raw conversation bytes → durable storage, a permanent principle-8 violation surviving session end) is rated Critical, made a gate prerequisite, and tested on both transports plus the batch arm (R-03/R-04, SR-07). Enterprise-seam integrity (R-08/R-10/R-11 — retention enum completeness, TOML representation, `PartialEq` for merge) is covered so the "extend never re-architect" promise actually holds. The CI diff-gate self-test (R-14) is correctly elevated to High as the meta-gate protecting the codegen ACs the JS/TS client depends on. Security-risks and failure-modes sections are concrete and tie back to vision principle 8. No vision-relevant risk is missing.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search, category=pattern) for vision alignment / scope-addition / enterprise-seam patterns -- no relevant results (top match 0.33, unrelated config/field-wiring patterns). No prior vision-guardian pattern exists to apply.
- Stored: nothing novel to store -- vnc-024's alignment outcome is feature-specific (a clean PASS plus one editorial vision-doc-currency WARN). The one generalizable observation -- "a research spike that recommends a PRODUCT-VISION wording change leaves drift until separately applied; the first feature operationalizing the change should flag the unapplied recommendation" -- is a single occurrence; it becomes a storable cross-feature pattern only if it recurs. Noted here rather than stored prematurely.
