# Alignment Report: crt-022

> Reviewed: 2026-03-19
> Artifacts reviewed:
>   - product/features/crt-022/architecture/ARCHITECTURE.md
>   - product/features/crt-022/specification/SPECIFICATION.md
>   - product/features/crt-022/RISK-TEST-STRATEGY.md
> Scope reviewed:
>   - product/features/crt-022/SCOPE.md
>   - product/features/crt-022/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Infrastructure feature directly serving the W1-2 milestone in the product roadmap |
| Milestone Fit | PASS | Correctly positioned as W1-2; no future-wave content introduced |
| Scope Gaps | WARN | SPECIFICATION.md carries a forward reference to `spawn_with_timeout` as a blocking gate (C-11) that is unresolved within the spec itself, with resolution deferred entirely to ARCHITECTURE.md |
| Scope Additions | WARN | ARCHITECTURE.md introduces `RayonError::TimedOut` variant and `spawn_with_timeout` method not present in SCOPE.md; RISK-TEST-STRATEGY.md covers them thoroughly, but the variant was not in the original scope |
| Architecture Consistency | PASS | All four SCOPE.md open questions (OQ-1, OQ-2, SR-01, SR-03, SR-04, SR-06) are resolved with explicit ADR references; component breakdown is internally consistent |
| Risk Completeness | PASS | Risk register covers all scope risks SR-01 through SR-07; adds R-02, R-03, R-08 for implementation-time concerns not visible at scope time; security risks addressed |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `RayonError::TimedOut(Duration)` variant | SCOPE.md defines `RayonError { Cancelled }` (one variant). ARCHITECTURE.md and SPECIFICATION.md expand this to `{ Cancelled, TimedOut(Duration) }`. The addition is a direct consequence of the OQ-2 timeout decision (ADR-002) and is well-motivated, but was not enumerated in the original scope. |
| Addition | `RayonPool::spawn_with_timeout` method | SCOPE.md defines only `RayonPool::spawn`. ARCHITECTURE.md adds `spawn_with_timeout<F,T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>`. Again motivated by ADR-002; the scope explicitly deferred this decision. The addition is in-bounds by the spirit of SCOPE.md's OQ-2 but not its letter. |
| Simplification | Pool floor raised 2 → 4 | SCOPE.md proposes `max(num_cpus / 2, 2).min(8)`. ARCHITECTURE.md raises the floor to 4 (`max(num_cpus / 2, 4).min(8)`) after SR-04 analysis. Rationale documented in §pool-sizing (ADR-003). This is a design improvement, not a deviation. |
| Gap | SPECIFICATION FR-06 default formula inconsistency | FR-06 specifies the default as `(num_cpus / 2).max(2).min(8)` — the old SCOPE.md formula — while ARCHITECTURE.md §pool-sizing and ADR-003 raise the floor to 4. NFR-04 then references the old formula as "the specified baseline". SPECIFICATION.md and ARCHITECTURE.md are inconsistent on the pool floor default. |

---

## Variances Requiring Approval

### 1. SCOPE ADDITION: `RayonError::TimedOut` and `spawn_with_timeout`

**What**: The scope defined a `RayonPool` with a single `spawn` method and a `RayonError` with one variant (`Cancelled`). The architecture and specification add a second public method (`spawn_with_timeout`) and a second error variant (`TimedOut(Duration)`), expanding the public API surface.

**Why it matters**: Scope additions change the delivery contract — more API surface means more implementation work, more test surface (R-02 scenarios require the `TimedOut` path), and a permanent API commitment. This is a material API expansion, even though it is well-motivated. Adding API surface beyond what scope asked for requires human sign-off.

**Recommendation**: Accept. The addition is a direct, necessary resolution of SCOPE.md's own OQ-2 (which was explicitly flagged as load-bearing and blocking). ADR-002 documents the rationale. The scope itself acknowledged this decision was deferred — the addition closes a gap the scope knew existed. Human should confirm acceptance of the two-method API before implementation begins.

---

### 2. WARN: SPECIFICATION.md FR-06 and NFR-04 carry the old pool floor (2), conflicting with ARCHITECTURE.md (4)

**What**: SPECIFICATION.md FR-06 specifies `(num_cpus / 2).max(2).min(8)` as the default pool size formula. NFR-04 also references this formula as "the specified baseline." ARCHITECTURE.md §pool-sizing explicitly supersedes this with a floor of 4, justified by the contradiction scan + quality-gate monopolisation analysis (SR-04 / ADR-003). The two source documents disagree on a key configuration default.

**Why it matters**: An implementer reading only the specification will produce a floor-2 default. An implementer reading only the architecture will produce a floor-4 default. This is an internal inconsistency between source documents — exactly the kind of discrepancy that produces implementation bugs. The pool floor directly affects the SR-04 monopolisation risk.

**Recommendation**: The specification must be updated to reflect the floor-4 formula before implementation begins. This is a document correction, not a design dispute — ARCHITECTURE.md's reasoning is sound. The specification writer should update FR-06 and NFR-04 to read `(num_cpus / 2).max(4).min(8)`. This is a WARN, not a FAIL, because the architecture is clear and correct; the specification has a copy-paste regression from the SCOPE.md value.

---

## Detailed Findings

### Vision Alignment

The product vision (Wave 1 — Intelligence Foundation) explicitly calls out W1-2:

> "W1-2: Rayon Thread Pool + Embedding Migration ... Establish a dedicated rayon::ThreadPool in unimatrix-server for all CPU-bound ML inference, bridged to tokio via oneshot channel."

crt-022 directly implements this roadmap item. The architecture preserves all non-negotiables from the vision:
- Hash chain integrity: no new write paths introduced; no schema changes.
- Audit log: not affected.
- Single binary: rayon is an additive dependency, not a new service.
- In-memory hot path: the rayon pool does not touch analytics-derived search data.
- Domain agnosticism: the pool is named `ml_inference_pool` (generic), and the `[inference]` config section is named to accommodate future non-dev models (W1-4 NLI, W2-4 GGUF).

The vision's "Critical Gaps" section identifies "Process exits on session end — background tick, write queue, ML inference stop" as Critical and "Single SQLite writer — MCP requests compete with all background work" as High. This feature does not address those gaps directly (W0-0 daemon mode is the fix), but it does address the ML inference pool saturation issue that compounds those problems. Alignment is appropriate.

### Milestone Fit

SCOPE.md correctly identifies this as W1-2. No Wave 2 or Wave 3 content is introduced:
- W2-4 GGUF pool: explicitly excluded by SCOPE.md Non-Goals §8 and SPECIFICATION.md C-05.
- W1-4 NLI: explicitly excluded; `[inference]` section naming is forward-compatible but NLI types are not introduced.
- W3-1 GNN: no GNN infrastructure added.

The architecture acknowledges integration points for W1-4, W2-4, and W3-1 without building them — appropriate forward planning, not premature implementation.

### Architecture Review

The architecture is thorough and internally consistent. All scope risk questions are resolved:

- **SR-01 (OrtSession thread safety)**: Resolved in §thread-safety. `OnnxProvider` wraps `Mutex<Session>`; `test_send_sync` asserts `Send + Sync` at compile time. The `Mutex<Session>` correctly serialises concurrent rayon callers. No change needed.
- **SR-03 (timeout coverage gap)**: Resolved in §timeout-semantics (ADR-002). `spawn_with_timeout` is the correct selection over per-call-site `tokio::time::timeout`.
- **SR-04 (contradiction scan monopolisation)**: Resolved in §pool-sizing (ADR-003). Scan stays as single task; floor raised to 4.
- **SR-06 (ad-hoc pool re-instantiation)**: Resolved via `AppState` distribution (ADR-004).
- **OQ-1 (naming)**: Resolved as `ml_inference_pool` (human-approved per SPECIFICATION.md).
- **OQ-2 (timeout semantics)**: Resolved by ADR-002 / §timeout-semantics.

The call-site migration pattern (§Call-Site Migration Pattern) is precise and machine-verifiable. The component interactions diagram is accurate. The `RayonPool` panic containment model (tx drop → rx.await Err) is correctly described with no need for `catch_unwind`.

One architectural note: the architecture correctly documents that `tokio::time::timeout` around `rx.await` cancels the async wait but does not terminate the rayon thread. This is stated explicitly in §timeout-semantics: "pool sizing ensures the pool is never fully consumed by hung threads under normal operation." This is a sound and honest tradeoff.

### Specification Review

The specification is detailed and traceable. All 11 acceptance criteria from SCOPE.md are carried forward. FR-01 through FR-11 and NFR-01 through NFR-07 cover all scope requirements. The call site inventory is precise and consistent with SCOPE.md's background research section.

**Key issue identified**: FR-06 and NFR-04 carry the old floor-2 formula. The architecture raised this to floor-4 after the SR-04 analysis. The specification does not reflect this change. This is a document inconsistency that must be resolved before implementation.

**C-11 (timeout semantics deferred to architecture)**: SPECIFICATION.md C-11 defers the timeout decision to ARCHITECTURE.md. The architecture has resolved this (ADR-002, `spawn_with_timeout`). C-11 should be updated to reflect the resolution rather than remaining as a blocking gate reference. This is a documentation hygiene issue — the gate is closed, but the spec still reads as if it is open.

**Domain model for `RayonError`**: SPECIFICATION.md §Domain Models defines `RayonError` with only the `Cancelled` variant. The architecture adds `TimedOut(Duration)`. The specification's domain model section should be updated to reflect the actual agreed-upon error type.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is well-structured and traces each risk to the architecture and specification. Coverage is appropriate for an infrastructure feature:

- R-01 through R-11: all scope risks traced; 3 additional implementation-time risks (R-02, R-03, R-08) beyond the SCOPE-RISK-ASSESSMENT.md scope risks.
- The security risks section (adversarial MCP input → pool exhaustion; mutex poisoning → embedding DoS) identifies a genuine mitigation gap: mutex poisoning triggers `Cancelled` at call sites, but `EmbedServiceHandle`'s retry state machine is triggered by `get_adapter()` failures, not by `Cancelled` responses from the bridge. This gap is identified and the recovery path described. No test scenario fully exercises the end-to-end recovery.
- The CI grep step bypass via macro expansion is identified as a residual risk with code review as the primary control. This is honest and appropriate for a grep-based enforcement mechanism.

The knowledge stewardship section correctly cross-references prior entries (#1688, #735, #2491, #2535, #2537) and confirms no novel patterns were generated — the patterns were already captured by prior agents in this feature's design phase.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found 2 results: #2298 (config key semantic divergence, dsn-001), #2063 (single-file topology vs vision language, nxs-011). Neither is directly applicable to crt-022 (different feature types). Supplementary search for crt-022-specific patterns found #2491, #2537, #2536 — all feature-specific patterns already stored.
- Stored: nothing novel to store — the spec/architecture inconsistency on pool floor (2 vs 4) is a document correction issue, not a generalizable alignment pattern. The scope addition pattern (OQ deferral → API expansion in architecture phase) could generalize, but it is already captured implicitly in prior vision alignment reviews. No new entry warranted.
