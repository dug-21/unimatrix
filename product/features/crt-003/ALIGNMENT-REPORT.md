# Vision Alignment Report: crt-003

## Assessment Summary

| Dimension | Result | Notes |
|-----------|--------|-------|
| Core Value Proposition | PASS | Contradiction detection is explicitly named in the value proposition: "contradiction detection across the knowledge base" |
| M4 Milestone Goal | PASS | "Contradictions surface" is the M4 deliverable for crt-003 |
| Security Cross-Cutting | PASS | crt-003 is the designated defense against semantic poisoning (M4 security row in vision) |
| Gaming Resistance | PASS | No automated actions from detection results; manual quarantine prevents DoS |
| Status Lifecycle | PASS | Quarantined status extends the lifecycle with clear semantics distinct from Deprecated |
| Retrieval Model | PASS | No changes to retrieval semantics; quarantine filtering is additive |

## Detailed Alignment

### 1. Core Value Proposition Alignment

The product vision states: "contradiction detection across the knowledge base" as part of the auditable knowledge lifecycle. crt-003 directly implements this. The architecture uses the existing HNSW index for efficient detection (no new infrastructure) and surfaces results through the existing `context_status` tool (no new API surface for detection).

**Result: PASS**

### 2. M4 Milestone Alignment

Vision: "Knowledge quality improves automatically. Unused entries fade. Helpful entries strengthen. Contradictions surface."

crt-003 delivers "contradictions surface" through:
- Contradiction detection with conflict heuristic (surfaces contradictions)
- Embedding consistency checks (surfaces potential hijacking)
- Quarantine mechanism (enables human action on surfaced issues)

**Result: PASS**

### 3. Security Alignment

Vision (Security by Milestone table): "M4 (crt-003): Semantic poisoning defense via contradiction detection, embedding consistency checks, entry quarantine."

All three are implemented:
- Semantic poisoning defense: conflict heuristic detects contradictory entries
- Embedding consistency checks: re-embed and compare to detect relevance hijacking
- Entry quarantine: new Status::Quarantined with Admin-only management tool

The vision also mentions "entry quarantine status in StatusIndex" -- implemented via STATUS_INDEX with Quarantined = 3.

**Result: PASS**

### 4. Architecture Constraints Alignment

| Vision Constraint | crt-003 Design | Aligned? |
|-------------------|---------------|----------|
| No hardcoded agent roles | No new role references | Yes |
| Generic query model | QueryFilter unchanged | Yes |
| Domain-agnostic categories | No new categories required | Yes |
| `#![forbid(unsafe_code)]` | All new code is safe Rust | Yes |
| No new crate dependencies | Uses existing regex, embedding, HNSW | Yes |

**Result: PASS**

### 5. Warnings

**W1: Conflict heuristic accuracy**

The vision references "ReasoningBank's contradiction pipeline" which uses NLI models. crt-003 uses a lighter-weight rule-based heuristic (ADR-003). This is a deliberate trade-off: zero additional model dependencies at the cost of lower accuracy. The tunable sensitivity threshold and pluggable signal architecture allow future NLI integration without rearchitecture. This is a scope decision, not an alignment issue.

**Severity: INFO** -- No action required. The heuristic is explicitly designed to be upgradeable.

**W2: No automated quarantine**

The vision mentions "entry quarantine status" which could be interpreted as automatic quarantine of flagged entries. crt-003 explicitly makes quarantine manual (Admin-only) to prevent a DoS vector. This is the correct design decision -- automated quarantine would be a new attack surface. The SCOPE.md non-goal is well-justified.

**Severity: INFO** -- Intentional design constraint, not misalignment.

## Conclusion

crt-003 is fully aligned with the product vision. It delivers the three M4/crt-003 security capabilities (contradiction detection, embedding consistency, quarantine) using the established architecture patterns (Status enum, combined transactions, post-search filtering). No variances require human approval. The two informational notes (W1, W2) are deliberate scope decisions documented in SCOPE.md and ADRs.

**Overall: 6 PASS, 0 WARN, 0 FAIL**
