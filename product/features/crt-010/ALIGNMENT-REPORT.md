# Alignment Report: crt-010

> Reviewed: 2026-03-06
> Artifacts reviewed:
>   - product/features/crt-010/architecture/ARCHITECTURE.md
>   - product/features/crt-010/specification/SPECIFICATION.md
>   - product/features/crt-010/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves "trustworthy, correctable, and auditable" — ensures stale knowledge doesn't outrank current knowledge |
| Milestone Fit | PASS | Cortical phase (M4) feature building on crt-001–005; retrieval-layer improvement fits Learning & Drift |
| Scope Gaps | PASS | All 6 SCOPE components addressed in architecture and specification |
| Scope Additions | ACCEPTED | Two additions: VectorIndex changes (SCOPE says "no changes"), compute_briefing_boost extension — accepted by human |
| Architecture Consistency | ACCEPTED | R-08 design tension resolved — compaction already excludes deprecated (col-013), C6 reduced to verification test |
| Risk Completeness | PASS | 12 risks, 41 test scenarios, all 9 scope risks traced to architecture decisions |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `VectorIndex::get_embedding` + `AsyncVectorStore::get_embedding` | SCOPE Affected Crates table states `unimatrix-vector: No changes`. Architecture adds two new methods for successor similarity computation. SCOPE Component 2 implies this change ("fetch their embedding from the vector index"), creating an internal SCOPE inconsistency — architecture correctly resolves it. |
| Addition | `compute_briefing_boost` deprecated filtering | SCOPE Component 3 mentions only `compute_search_boost`. Architecture C3 and open question #1 identify `compute_briefing_boost` as an additional caller requiring the same `deprecated_ids` parameter. |
| Addition | `cosine_similarity` helper in `confidence.rs` | Not in SCOPE. Architecture adds a pure function for cosine similarity. Reasonable implementation detail flowing from SCOPE Component 2's design decision (a). |
| Simplification | C7 (Penalty Constants) as separate component | SCOPE embeds penalty constants within Component 1 description. Architecture extracts to separate component C7. Organizational — no functional difference. |

## Variances Requiring Approval

### 1. VectorIndex "No Changes" Contradicted

**What**: SCOPE Affected Crates table explicitly states `unimatrix-vector | No changes (compact already accepts filtered embeddings)`. Architecture adds `VectorIndex::get_embedding()` and `AsyncVectorStore::get_embedding()` — two new public methods in unimatrix-vector and unimatrix-core respectively.

**Why it matters**: The SCOPE contains an internal inconsistency. Component 2 describes option (a) as "fetching their embedding from the vector index and computing cosine similarity" which necessarily requires a vector index API addition. The Affected Crates table contradicts this. Architecture correctly resolves the inconsistency by choosing option (a) per ADR-002, but the SCOPE's explicit "no changes" claim for unimatrix-vector is violated.

**Recommendation**: Accept. The architecture's resolution is sound — SCOPE Component 2 anticipated this change even though the Affected Crates table didn't reflect it. No SCOPE amendment needed; the ADR-002 decision documents the rationale.

### 2. Post-Compaction Supersession Injection Unreachable (R-08) — RESOLVED

**What**: Risk R-08 identified a design tension: Component 6 (vector index pruning) removes deprecated entries from HNSW during compaction, making supersession injection dead code for pruned entries.

**Resolution**: Investigation during human review revealed that C6 is **already satisfied** by existing infrastructure. Since col-013, `maintain: true` is silently ignored — maintenance runs on a background tick (`background.rs:234-257`). The background tick already filters entries to `Status::Active` at `status.rs:175-181` before passing to `VectorIndex::compact()`. Deprecated/quarantined entries are already excluded from HNSW rebuilds.

The R-08 tension still exists conceptually (post-compaction, supersession injection cannot fire for pruned entries), but it's not a *new* behavior introduced by crt-010 — it's how compaction already works. Pre-compaction, supersession injection provides a recall boost. Post-compaction, deprecated entries are gone entirely. Net effect is positive. C6 scope reduced to a verification test only.

**Status**: Accepted. No design change needed.

## Detailed Findings

### Vision Alignment

The product vision states: *"Unimatrix ensures what agents remember is trustworthy, correctable, and auditable."* crt-010 directly serves this by preventing deprecated and superseded knowledge from outranking current knowledge in retrieval results.

The vision's emphasis on **invisible delivery** ("knowledge arrives as ambient context, injected by hooks before the agent sees the prompt") makes UDS strict mode particularly important — agents receiving silent injection cannot evaluate what they receive, so delivering deprecated entries is actively harmful. SCOPE goal 1 ("Wrong information is worse than no information") aligns with the vision's trust guarantee.

The **auditable knowledge lifecycle** (hash-chained correction histories, supersession chains) is leveraged here as a retrieval signal. The `superseded_by` field — part of the correction chain infrastructure — becomes a first-class input to search ranking rather than being ignored.

**Status: PASS** — Feature directly strengthens the vision's core value proposition.

### Milestone Fit

crt-010 sits in the Cortical phase (Milestone 4: Learning & Drift). The product vision describes M4's goal as *"Turn passive knowledge accumulation into active learning."* This feature makes the knowledge lifecycle signals (deprecation, supersession) actively influence retrieval — a direct fit.

The feature builds on completed M4 features:
- crt-002 (confidence scoring pipeline — unchanged, penalties applied at retrieval layer)
- crt-004 (co-access boosting — extended with deprecated exclusion)
- crt-005 (coherence gate / compaction — extended with deprecated/quarantined pruning)

No future-milestone capabilities are built. The non-goals explicitly exclude items that would belong to future work (multi-hop supersession, runtime-configurable penalties, restore workflows).

**Status: PASS** — Appropriate milestone targeting with no scope creep.

### Architecture Review

The architecture is well-structured with 7 components mapping cleanly to 6 SCOPE components (C7 is an extraction of penalty constants from C1). The `RetrievalMode` enum (ADR-001) is a clean design — it controls all downstream status behavior from a single parameter, addresses SR-09 backward compatibility, and avoids boolean flag proliferation.

Key design decisions are all documented as ADRs:
- ADR-001: RetrievalMode enum on ServiceSearchParams
- ADR-002: Cosine similarity from stored embedding (not re-embedding)
- ADR-003: Single-hop supersession only
- ADR-004: HashSet<u64> interface for co-access filtering (engine crate decoupling)
- ADR-005: Named constants for penalty multipliers

**Integration surface** is well-defined with exact type signatures. The cross-crate boundary (server → engine) is minimal and type-safe (`HashSet<u64>`).

**Open questions** are appropriately flagged:
1. BriefingService co-access — identified as needing implementation audit (scope addition, see above)
2. hnsw_rs point data retrieval — flagged for implementation verification with fallback plan

**Error boundaries** are comprehensive — all failure modes produce graceful degradation (skip injection, return empty, log warning), never panic or fail the search.

**Status: PASS** — Sound architecture. R-08 tension resolved (C6 already satisfied by col-013 background tick). All concerns addressed.

### Specification Review

The specification faithfully translates SCOPE components into functional requirements (FR-1 through FR-6) with 16 acceptance criteria covering the behavioral matrix across modes, statuses, and injection scenarios.

Non-functional requirements add appropriate constraints:
- NFR-1.1: p95 latency regression <15% (addresses SR-01)
- NFR-2: Named constants, not magic numbers (addresses SR-02)
- NFR-3: No schema changes (addresses SR-06)
- NFR-4: Backward compatibility (addresses SR-09)

FR-6.2 adds a behavioral detail not explicit in SCOPE: "When the agent provides an explicit `status` parameter, that filter is honored as-is. No penalties are applied." SCOPE says "honor the agent's choice" — FR-6.2 interprets this as penalty bypass. This is a reasonable interpretation and R-07 in the risk strategy covers the interaction with supersession injection.

AC-16 (latency benchmark) may be difficult to verify as an automated test. The specification notes "Benchmark test or manual measurement" — acceptable for a non-functional requirement.

**Status: PASS** — Complete, traceable to SCOPE, with appropriate non-functional constraints.

### Risk Strategy Review

The risk strategy is thorough:
- 12 risks identified (2 Critical, 4 High, 6 Medium)
- 41 test scenarios mapped to risks
- All 9 scope risks (SR-01 through SR-09) traced to architecture decisions
- Edge cases enumerated (self-referential supersession, circular chains, zero vectors, concurrent deprecation, fan-in)
- Security risks assessed (no new attack surface, penalty bypass by design)
- Failure modes with expected behaviors documented

R-08 (post-compaction supersession unreachable) is correctly rated Critical and is the most important finding. The test scenario ("full cycle — insert, deprecate, supersede, compact, search — verify Active successor still appears") is the right validation.

R-01 (hnsw_rs API availability) is correctly rated Critical with a clear fallback plan (re-embedding via EmbedService).

The self-referential supersession edge case (entry where `superseded_by == self.id`) is identified but not explicitly covered by an acceptance criterion. FR-2.3 conditions would handle it (successor is itself superseded → skip), but a dedicated test would be prudent. This is minor — not a gap, just a suggestion.

**Status: PASS** — Comprehensive risk coverage with appropriate severity ratings.
