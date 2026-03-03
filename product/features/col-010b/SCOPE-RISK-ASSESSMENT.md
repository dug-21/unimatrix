# Scope Risk Assessment: col-010b

Feature: Retrospective Evidence Synthesis & Lesson-Learned Persistence
Author: col-010b-agent-0-scope-risk
Date: 2026-03-02

---

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Fire-and-forget `tokio::spawn` for lesson-learned embedding may silently fail, leaving entries with `embedding_dim = 0` invisible to `context_search` | Med | Med | Architect: ensure graceful degradation path — entry queryable by metadata even without embedding. Spec: define recovery via supersede on next retrospective call. |
| SR-02 | ONNX embedding pipeline is synchronous (`spawn_blocking`); nested inside `tokio::spawn` creates two-layer async complexity | Med | Low | Architect: follow established pattern from col-009 signal writes. Keep nesting shallow. |
| SR-03 | `RetrospectiveReport` additive fields (`narratives`, `recommendations`) with `#[serde(default)]` may break existing callers if serialization format changes | Med | Low | Spec: use `skip_serializing_if = "Option::is_none"` for `narratives`; `skip_serializing_if = "Vec::is_empty"` for `recommendations`. Existing callers see no new fields when empty. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Evidence truncation (`evidence_limit`) changes default behavior — existing tests asserting exact evidence array lengths will break | High | High | Spec: R-09 blocking gate must be enforced. Audit all existing `context_retrospective` tests BEFORE implementing Component 1. |
| SR-05 | `hotspots: Vec<HotspotFinding>` type invariant means truncation happens server-side on the serialized output, not on the in-memory struct — dual representation risk | Med | Med | Architect: truncation must be a clone-and-truncate step, never mutating the original report. |
| SR-06 | Narrative synthesis scope — "deterministic heuristics only" is clear, but timestamp clustering and sequence extraction have edge cases (empty events, single event, non-monotone sequences) | Low | Med | Spec: define graceful defaults for all edge cases — `None` for missing patterns, empty `Vec` for no clusters. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `PROVENANCE_BOOST` must be applied at two callsites (`uds_listener.rs` and `tools.rs`) — divergence risk if one is missed | Med | Med | Architect: use a shared helper function or ensure the constant is referenced from `confidence.rs` at both sites. |
| SR-08 | Lesson-learned supersede uses `context_lookup` then `context_correct` — two separate store operations, not atomic. Concurrent retrospective calls may produce duplicates (inherited SR-09 from col-010) | Med | Low | Accepted known limitation per col-010 SCOPE.md. Document in spec. |
| SR-09 | `from_structured_events()` was delivered in col-010 P0 — col-010b extends it with narrative synthesis. Must verify P0's structured path is functional before adding synthesis | Med | Low | Architect: verify `from_structured_events()` exists and is called from the retrospective handler. Validate before extending. |

## Assumptions

1. **col-010 P0 is merged and stable** (SCOPE.md "Depends on"): `from_structured_events()`, SESSIONS, INJECTION_LOG are all functional. If P0 has latent bugs, col-010b synthesis will produce incorrect narratives.
2. **`lesson-learned` category is in CategoryAllowlist** (SCOPE.md Component 3): MEMORY.md and `categories.rs` confirm this. If the allowlist is poisoned at runtime, writes silently skip.
3. **ONNX embedding service is available** (SCOPE.md Component 3): The embed pipeline (`unimatrix-embed`) is stable. Fire-and-forget pattern handles failure gracefully.
4. **`hotspots` type is unchanged** (SCOPE.md constraint): `Vec<HotspotFinding>` with `evidence: Vec<EvidenceRecord>` is the existing structure. Truncation is server-side only.

## Design Recommendations

1. **SR-04 is the highest priority risk.** The R-09 blocking gate (audit existing tests) must be the first implementation step. Architect and spec should structure Component 1 to enforce this ordering.
2. **SR-07** requires the architect to define a single application pattern for `PROVENANCE_BOOST` that both callsites use identically. A shared helper or explicit code review checkpoint.
3. **SR-01/SR-02** are mitigated by the established fire-and-forget pattern in the codebase (col-009 signal writes, co-access pair recording). Follow the same pattern exactly.
