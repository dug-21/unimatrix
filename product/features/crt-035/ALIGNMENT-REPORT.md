# Alignment Report: crt-035

> Reviewed: 2026-03-30
> Artifacts reviewed:
>   - product/features/crt-035/architecture/ARCHITECTURE.md
>   - product/features/crt-035/specification/SPECIFICATION.md
>   - product/features/crt-035/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly serves the W1-1 graph integrity gap marked Fixed; PPR coverage is a core intelligence pipeline quality issue |
| Milestone Fit | PASS | Cortical phase (crt-) fulfilling an ADR-006 follow-up contract from crt-034; no future-milestone capabilities added |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in the three source documents |
| Scope Additions | WARN | ARCHITECTURE.md references `crates/unimatrix-engine/src/graph_ppr_tests.rs` — a crate not referenced in SCOPE.md; plus the AC-12 test placement is contradicted between architecture and spec |
| Architecture Consistency | WARN | AC-12 test description in ARCHITECTURE.md (Component 5, SR-06) is contradicted by SPECIFICATION.md AC-12 and RISK-TEST-STRATEGY.md R-07; architecture doc contains a stale in-memory fixture description |
| Risk Completeness | PASS | RISK-TEST-STRATEGY.md enumerates 10 risks, 3 integration risks, 6 edge cases, and 2 security risks; all SCOPE-RISK-ASSESSMENT.md items are addressed |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `crates/unimatrix-engine` crate reference | ARCHITECTURE.md Component 5 places AC-12 PPR test in `crates/unimatrix-engine/src/graph_ppr_tests.rs`. SCOPE.md only references `unimatrix-server` and `unimatrix-store`. The spec (AC-12) resolves this differently: it places the test in `typed_graph.rs` inside `unimatrix-server` (or unimatrix-store's graph layer). Architecture doc references a crate not mentioned in SCOPE.md or spec dependencies. |
| Simplification | OQ-03 left as delivery gate | SCOPE.md open question 2 (`db.rs` path) is closed. However OQ-03 (index coverage for NOT EXISTS) is punted to a delivery-gate check rather than resolved at design time. SCOPE-RISK-ASSESSMENT.md SR-04 flagged this as Low/Low. The risk strategy treats it as R-01 (Med/Med, High priority). The escalation in priority from scope to risk is noted but is internally consistent — not a gap. |
| Simplification | `bootstrap_only = 0` hardcoded on back-filled reverse edges | SCOPE.md §Back-fill SQL Design notes `bootstrap_only = 0` for all back-filled rows. The specification and architecture both preserve this. This means bootstrap-era reverse edges will be included in `build_typed_relation_graph` reads (which exclude `bootstrap_only = 1`). This is the explicitly intended behaviour per SCOPE.md and ARCHITECTURE.md §Component 2, SR-SEC-02. Rationale: documented. |

---

## Variances Requiring Approval

### VARIANCE 1: AC-12 Test Placement Contradiction Between Architecture and Specification

**What**: ARCHITECTURE.md (Component 5, SR-06) describes the AC-12 test as using an in-memory `TypedRelationGraph` built via `make_graph_with_edges`, bypassing SQLite entirely. It names the file `crates/unimatrix-engine/src/graph_ppr_tests.rs` and proposes a test named `test_reverse_coaccess_high_id_to_low_id_ppr_regression`. SPECIFICATION.md (AC-12, line 218) explicitly contradicts this: the test must use a real SQLite-backed `SqlxStore`, not an in-memory synthetic fixture, and is placed in `typed_graph.rs` with a `SqlxStore::open` call. RISK-TEST-STRATEGY.md (R-07) flags this directly as a "contradiction" at Med severity / Med likelihood.

**Why it matters**: The delivery agent has two conflicting authoritative documents. The spec resolves the question ("spec is authoritative" is stated in R-07), but the architecture document has not been updated to reflect this resolution. A delivery agent reading Component 5 of the architecture in isolation will build the wrong fixture and miss the `GRAPH_EDGES → TypedRelationGraph → PPR` integration path that the spec mandates. The architecture also references `crates/unimatrix-engine`, a crate not listed in the spec's Dependencies section; the spec lists the test in `typed_graph.rs` without specifying a crate path unambiguously.

**Recommendation**: Update ARCHITECTURE.md Component 5 and the SR-06 section to align with SPECIFICATION.md AC-12: remove the in-memory `TypedRelationGraph` description, replace with the `SqlxStore`-backed path, and remove or correct the `crates/unimatrix-engine` reference to the correct crate/file location. The spec is correct; the architecture must be brought into sync before delivery begins.

---

### WARN 1: Architecture Doc References `unimatrix-engine` Crate Not Present in Dependencies

**What**: ARCHITECTURE.md Component 5 names `crates/unimatrix-engine/src/graph_ppr_tests.rs` as the file for the AC-12 test. No such crate is listed in SPECIFICATION.md's Dependencies section, SCOPE.md's file references, or the project's crate layout (which defines `unimatrix-{store,vector,embed,core,server}`). The spec places the test in `typed_graph.rs` without a fully qualified crate path, and the crate containing `TypedGraphState` / `typed_graph.rs` is left ambiguous between the architecture and specification.

**Why it matters**: A delivery agent following the architecture will attempt to write to a non-existent crate location. This is blocked by the spec (which doesn't reference `unimatrix-engine`), but the contradiction wastes time and creates confusion.

**Recommendation**: Confirm the actual crate location of `typed_graph.rs` (expected: `unimatrix-server` or `unimatrix-store`) and update ARCHITECTURE.md Component 5 to use that path. This is a documentation fix, not a scope or design change.

---

## Detailed Findings

### Vision Alignment

crt-035 directly serves the product vision's Intelligence & Confidence section. The vision's critical gap table records "Co-access and contradiction never formalized as graph edges" as **Fixed** by W1-1 (`crt-021`). crt-034 then promoted co-access pairs into the graph as typed edges. crt-035 completes the structural correctness requirement: PPR traverses `Direction::Outgoing` exclusively (per the vision's graph architecture), so unidirectional CoAccess edges silently halve the effective coverage of the co-access signal for half of all queries.

The vision's core framing — "It is not a retrieval engine with additive boosts. It is a session-conditioned, self-improving relevance function" — depends on the graph being structurally correct. A graph where seeding the higher-ID entry of any co-access pair finds no path back to the lower-ID entry is a structural defect in the intelligence pipeline. crt-035 fixes a correctness bug in the W1-1 work, not a new capability.

No future-milestone capabilities are introduced. The feature touches only `co_access_promotion_tick.rs`, `migration.rs`, and their tests. No new tools, config fields, or public APIs are added.

### Milestone Fit

crt-035 is a Cortical phase feature fulfilling the explicit follow-up contract in ADR-006 (Unimatrix #3830), which was approved as part of crt-034. The feature scope is narrow: two code changes (tick bidirectionality + migration back-fill) plus a Unimatrix knowledge update. This is appropriate scope for a follow-up contract delivery — not a standalone design feature.

The schema version bump (v18→v19) is consistent with the current baseline (crt-033 set v18 per SCOPE.md §Migration Framework). No leap in schema version complexity is introduced.

### Architecture Review

The architecture document is well-structured and covers all five components. The three integration points (Tick → GRAPH_EDGES, Migration → GRAPH_EDGES, TypedGraphState ← GRAPH_EDGES) are correctly identified with no code changes required to PPR or cycle detection.

The atomicity decision (ADR-001, per-direction independence with eventual consistency) is explicitly documented and consistent with the infallible-tick constraint from SCOPE.md. The SR-05 blast radius table in the architecture is detailed and agrees with the specification's T-BLR sections.

The single significant defect is in Component 5 and SR-06: the architecture describes an in-memory test fixture path that the specification explicitly overrules. This creates a direct contradiction on the AC-12 test that must be resolved before delivery.

The index coverage discussion in Component 2 (SR-04) reaches the correct conclusion (UNIQUE constraint covers the NOT EXISTS sub-join) but defers final confirmation to a delivery-gate EXPLAIN QUERY PLAN check. This is consistent with SCOPE.md decision D4 and is an acceptable deferral given the one-time nature of the migration.

### Specification Review

The specification is thorough and internally consistent. All 14 acceptance criteria are present and have explicit verification methods. The 8 T-BLR test update entries plus 3 T-NEW tests plus 7 MIG-U cases give complete test coverage specification.

OQ-01 and OQ-02 are closed with explicit resolutions. OQ-03 is elevated to a delivery gate (correct handling for a performance confirmation that cannot be spec'd without running against a real database).

The T-BLR-08 handling is notably careful: the spec lists this test in both the "no change needed" table (with a CAUTION note) and as an explicit T-BLR-08 entry. The risk strategy (R-02) flags this double-listing as a delivery risk. The specification text is correct but the structure (appearing in both sections) is a documentation quality issue that the risk strategy correctly anticipates. No scope variance here, but the delivery agent must read both sections.

FR-06 mandates the `promote_one_direction` helper as a structural requirement to enforce the 500-line file limit (C-03, NFR-03). This is a design constraint correctly carried through all three documents.

The spec places AC-12 in `typed_graph.rs` with a concrete test name `test_ppr_reverse_coaccess_edge_seeds_lower_id_entry` and specifies the full SQLite-backed fixture path (steps 1–6). This conflicts with the architecture's in-memory description as noted above.

### Risk Strategy Review

The risk strategy is comprehensive: 10 risks across Critical/High/Medium/Low priority, 3 integration risks, 6 edge cases, and 2 security surface assessments.

Critical risks (R-02, R-08) are both about test blast radius correctness — the most likely delivery failure mode for a feature that inverts an existing test contract. The gate-3b grep requirements (scan for "no duplicate" string, scan for odd `count_co_access_edges` values) are concrete and non-negotiable.

R-07 explicitly identifies the AC-12 contradiction between architecture and specification. The risk strategy correctly names this as Med/Med and provides a gate check (grep `typed_graph.rs` for `SqlxStore::open`). This variance is flagged here for human attention as it is unresolved at the document level.

R-09 (migration rollback loop) is acknowledged as Low/Low with adequate coverage rationale (MIG-U-06 idempotency test is sufficient). No additional test required.

The security assessment correctly concludes there are no new external input surfaces or injection risks. The `bootstrap_only = 0` hardcoding (SR-SEC-02) is correctly identified as intended behavior, not a security concern.

The knowledge stewardship section in the risk strategy references prior lessons (#3548, #3579, #2758) that directly informed the R-02 and R-08 severity assessments. This is the correct use of the pattern store.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — found #2298 (config key semantic divergence), #3742 (optional future branch must match scope intent), #3337 (architecture diagram headers diverge from spec), #3426 (formatter regression), #181 (adaptive embedding). Pattern #3742 (architecture and risk diverging from scope deferral) and #3337 (architecture diverging from spec causing downstream test confusion) are directly applicable to the VARIANCE found here.
- Stored: nothing novel to store — the architecture-spec contradiction pattern on AC-12 test fixture is already generalized in #3337. The specific instance (in-memory vs SQLite-backed fixture for a graph traversal regression test) is feature-specific to crt-035 and does not add a new cross-feature pattern beyond what #3337 already captures.
