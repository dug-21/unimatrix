# SPECIFICATION: crt-013 Retrieval Calibration

## Objective

Resolve co-access signal overlap across four mechanisms consuming CO_ACCESS data, validate crt-010 status penalties with integration tests, make briefing semantic neighbor count configurable, and optimize the status scan from O(n) full-table iteration to targeted SQL aggregation. These changes improve retrieval quality, operational scalability, and pipeline clarity without altering the stored confidence formula or schema.

## Functional Requirements

### FR-01: Remove Episodic Augmentation Stub
Delete `crates/unimatrix-adapt/src/episodic.rs` and its `pub mod episodic` declaration in `lib.rs`. Remove all references to `EpisodicAugmenter` across the workspace. The module is a no-op stub (returns `vec![0.0; n]`) that would triple-count CO_ACCESS signal if activated.

### FR-02: Remove `co_access_affinity()` Dead Code
Delete the `co_access_affinity()` function at `confidence.rs:239-250`. This function computes a `[0.0, 0.08]` affinity value using `W_COAC` but is never called — the search pipeline uses `compute_search_boost()` from `coaccess.rs` instead.

### FR-03: W_COAC Constant Disposition (Option A)
Delete the `W_COAC: f64 = 0.08` constant at `confidence.rs:28`. The stored confidence formula sums to 0.92 from 6 factors (W_BASE through W_TRUST); this constant was reserved for query-time co-access but was never integrated into stored confidence. Removal is pure dead-code cleanup with zero behavioral change. Remove associated tests.

### FR-04: Co-Access Architecture ADR
Store an ADR in Unimatrix documenting the surviving two-mechanism architecture:
- **MicroLoRA adaptation** (pre-HNSW, embedding-level, `unimatrix-adapt/src/service.rs`) — shifts embeddings based on CO_ACCESS proximity
- **Scalar co-access boost** (post-rerank, score-level, `unimatrix-engine/src/coaccess.rs`) — additive boost capped at `MAX_CO_ACCESS_BOOST = 0.03`

The ADR must record why episodic augmentation and `co_access_affinity()` were removed, and note that double-counting between MicroLoRA and scalar boost is accepted by design (deferred to col-015 for empirical evaluation).

### FR-05: Status Penalty Integration Tests — Ranking Behavior
Write integration tests that validate **ranking outcomes** for crt-010 status penalties:
- **Flexible mode:** A deprecated entry ranks below an active entry when both match a query at comparable similarity. Assert relative order, not specific score values.
- **Flexible mode:** A superseded entry ranks below an active entry at comparable similarity.
- **Flexible mode:** A query matching only deprecated entries still returns results (degraded, not empty).
- **Strict mode:** Deprecated and superseded entries are excluded from UDS results entirely.
- **Co-access exclusion:** Deprecated entries do not receive or provide co-access boost (verified via `compute_search_boost()` with `deprecated_ids` populated).

### FR-06: Configurable Briefing Semantic Neighbor Count
Replace the hardcoded `k: 3` at `briefing.rs:228` with a configurable parameter. The `BriefingService` must accept a `semantic_k` value at construction time, defaulting to 3. Wire through to `ServiceSearchParams::k`.

### FR-07: Status Scan SQL Aggregation
Replace the `SELECT * FROM entries` full table scan in `status.rs:136-144` with targeted SQL queries returning pre-aggregated metrics:
- Correction chain counts: entries with `supersedes IS NOT NULL`, entries with `superseded_by IS NOT NULL`, `SUM(correction_count)`
- Trust source distribution: `GROUP BY trust_source` with counts
- Security metrics: entries where `created_by` is empty
- Active entries: `WHERE status = 'Active'` (still needed for lambda/coherence computation, but much smaller result set)

### FR-08: Status Aggregation Store Methods
Add new `Store` method(s) to `unimatrix-store` that return aggregated status metrics. Prefer a single method returning a struct over multiple fine-grained methods (reduces round-trips). The method executes SQL aggregation queries against existing schema — no new tables or columns.

## Non-Functional Requirements

### NFR-01: Zero Behavioral Change on Defaults
When briefing `semantic_k` is unconfigured, behavior is identical to current (k=3). Status scan optimization produces the same metrics as the current full-scan implementation. Dead code removal has no runtime effect.

### NFR-02: Test Determinism (SR-06 Mitigation)
Status penalty integration tests must use **injected pre-computed embeddings** with controlled cosine similarity, or assert on **relative ranking** (deprecated < active) rather than absolute score values. Tests must not depend on the ONNX embedding pipeline producing exact similarity thresholds.

### NFR-03: Behavior-Based Penalty Assertions (Human Framing Note 1)
Status penalty tests must assert ranking outcomes (e.g., "deprecated entry ranks below active entry"), not specific constant values (e.g., "score equals 0.598"). Graph Enablement will replace hardcoded constants with graph-topology-derived scoring — tests must survive that transition.

### NFR-04: Status Scan Equivalence (SR-03 Mitigation)
The SQL aggregation path must produce results equivalent to the Rust iteration path. "Equivalent" is defined as: identical counts for correction chains, identical trust source distribution, identical security metric counts. Known divergences (e.g., entries that fail deserialization in the old path but are counted by SQL) must be documented.

### NFR-05: No Schema Migrations
All changes operate on existing tables and columns. No new tables, columns, or indexes.

### NFR-06: Co-Access Architecture is Transitional (Human Framing Note 2)
The scalar co-access boost (`MAX_CO_ACCESS_BOOST = 0.03`) may be replaced by co-access transitivity via graph topology in a future Graph Enablement feature. The ADR (FR-04) must note this transitional status. Tests for co-access boost should test the mechanism's contract, not assume it is permanent.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | Episodic augmentation stub (`episodic.rs`) removed from `unimatrix-adapt`. Module declaration removed from `lib.rs`. All references cleaned up. | `grep -r "episodic" --include="*.rs"` returns zero hits. Workspace compiles. |
| AC-02 | `co_access_affinity()` function removed from `confidence.rs`. `W_COAC` constant removed. Tests for removed function removed. | `grep -r "co_access_affinity\|W_COAC" --include="*.rs"` returns zero hits. Workspace compiles. |
| AC-03 | ADR documenting the two-mechanism co-access architecture (MicroLoRA + boost) stored in Unimatrix. ADR notes transitional status per NFR-06. | ADR retrievable via `context_search`. |
| AC-04 | Integration test proves deprecated entry ranks below active entry at comparable similarity in Flexible mode. Test asserts relative ranking, not score constants (NFR-03). | `cargo test` — test passes. |
| AC-05 | Integration test proves deprecated and superseded entries excluded in Strict mode. | `cargo test` — test passes. |
| AC-06 | Integration test proves deprecated entries excluded from co-access boost computation. | `cargo test` — test passes. |
| AC-07 | Integration test proves deprecated-only query still returns results in Flexible mode. | `cargo test` — test passes. |
| AC-08 | Briefing `semantic_k` parameter configurable with default 3. Existing behavior unchanged when unconfigured. | Unit test with default k=3, unit test with k=5 returning more candidates. |
| AC-09 | Status scan no longer performs `SELECT * FROM entries`. Correction chain metrics, trust source distribution, and security metrics computed via SQL aggregation. | Code review confirms no full scan. `cargo test` passes. |
| AC-10 | `context_status` returns equivalent results before and after optimization. Comparison test runs both paths on same dataset and diffs output field-by-field (NFR-04). | Dedicated comparison integration test passes. |
| AC-11 | All existing tests pass with no regressions. | Full `cargo test` green. |

## Domain Models

### Entities

**EntryRecord** — Core knowledge entry stored in `entries` table. Fields relevant to crt-013:
- `id: u64` — unique identifier
- `status: Status` — Active, Deprecated, or Archived
- `superseded_by: Option<u64>` — pointer to successor entry
- `supersedes: Option<u64>` — pointer to predecessor entry
- `correction_count: u32` — number of corrections applied
- `trust_source: String` — origin trust level
- `created_by: String` — creator identity (empty = unattributed)
- `access_count: u64`, `helpful_count: u32`, `unhelpful_count: u32` — usage signals
- `confidence: f64` — stored composite confidence score

**CoAccessRecord** — CO_ACCESS table row tracking co-retrieval pairs:
- `entry_id_a: u64`, `entry_id_b: u64` — unordered pair
- `count: u32` — co-access frequency
- `last_updated: u64` — timestamp for staleness filtering

**RetrievalMode** — Enum controlling status filtering behavior:
- `Strict` — hard filter, drops non-Active and superseded entries (UDS path)
- `Flexible` — soft penalty, deprecated/superseded visible but penalized (MCP path)

**StatusAggregates** (new) — Struct returned by the new Store aggregation method:
- `supersedes_count: u64` — entries with `supersedes IS NOT NULL`
- `superseded_by_count: u64` — entries with `superseded_by IS NOT NULL`
- `total_correction_count: u64` — `SUM(correction_count)`
- `trust_source_distribution: HashMap<String, u64>` — `GROUP BY trust_source`
- `unattributed_count: u64` — entries where `created_by` is empty
- `active_entries: Vec<EntryRecord>` — subset for lambda/coherence computation

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **Co-access boost** | Post-rerank additive score adjustment (`[0.0, 0.03]`) based on CO_ACCESS pair frequency. Applied in `coaccess.rs`. |
| **MicroLoRA adaptation** | Pre-HNSW embedding shift using low-rank adaptation trained on CO_ACCESS proximity. Operates at vector level in `unimatrix-adapt`. |
| **Status penalty** | Multiplicative factor applied to rerank score in Flexible mode: 0.7 for deprecated, 0.5 for superseded. |
| **Strict mode** | UDS retrieval mode that hard-filters non-Active and superseded entries. |
| **Flexible mode** | MCP retrieval mode that penalizes but retains deprecated/superseded entries. |
| **Semantic k** | Number of nearest-neighbor candidates fetched during briefing semantic search. Currently hardcoded to 3. |
| **Lambda (λ)** | Composite health metric `[0.0, 1.0]` computed from 4 dimensions (freshness, graph, contradiction, embedding). Requires active entries. |

## User Workflows

### Agent Search (No Change to External Behavior)
1. Agent issues `context_search` via MCP → Flexible mode
2. Pipeline: embed query → HNSW search → filter → penalty → supersession injection → rerank → co-access boost → return
3. Deprecated entries appear in results but ranked lower due to multiplicative penalty
4. Co-access boost skips deprecated entries (both as anchor and partner)

### UDS Search (No Change to External Behavior)
1. Internal UDS listener triggers `context_search` → Strict mode
2. Same pipeline but hard-filters non-Active and superseded entries after HNSW

### Agent Briefing (Tunable Recall)
1. Agent issues `context_briefing` via MCP
2. BriefingService uses configurable `semantic_k` (default 3) for candidate retrieval
3. Token budget trimming unchanged — k controls recall before trimming

### Status Report (Faster Computation)
1. Agent or human issues `context_status`
2. StatusService executes SQL aggregation queries instead of full table scan
3. Active entries still loaded for optional lambda computation
4. Output format unchanged

## Constraints

### C-01: crt-011 Dependency (SR-02)
crt-011 (session count dedup) must be merged and CI-green before crt-013 implementation begins. Component 2 integration tests must use deterministic confidence values injected via test fixtures rather than relying on live confidence computation, isolating penalty validation from upstream data quality.

### C-02: No Schema Migrations (SR-07)
All SQL aggregation queries must operate on existing tables and columns. If needed indexes are missing, the optimization should still work correctly (sequential scan fallback). Adding indexes is out of scope.

### C-03: No Stored Confidence Formula Changes
The 6-factor stored formula (W_BASE=0.18, W_USAGE=0.14, W_FRESH=0.18, W_HELP=0.14, W_CORR=0.14, W_TRUST=0.14, sum=0.92) is unchanged. W_COAC removal is dead-code cleanup, not a formula change.

### C-04: Extend Existing Test Infrastructure
Use `TestServiceContext` and existing integration test patterns. No isolated scaffolding. Penalty validation tests should follow patterns in existing search integration tests.

### C-05: Dead Code Verification Before Removal (SR-01, SR-04)
Before removing `co_access_affinity()`, `W_COAC`, and `episodic.rs`, exhaustively verify zero callers across all crates including test code, build scripts, and feature gates. The Rust compiler catches missing imports, but pseudocode must enumerate all affected files.

### C-06: SQL Equivalence Contract (SR-03)
The comparison test (AC-10) must run both old (full-scan) and new (SQL aggregation) paths on the same dataset, diff field-by-field, and document any accepted divergences (e.g., entries failing deserialization). Both paths should be available during the transition for side-by-side validation.

### C-07: Briefing Config Minimalism (SR-05)
Briefing k configuration must follow existing server config patterns. If no externalized config struct exists, use a constructor parameter with env var override. Do not introduce a full config framework.

## Dependencies

| Dependency | Type | Crate | Notes |
|-----------|------|-------|-------|
| crt-011 (Session Count Dedup) | Hard | — | Must be merged before implementation. Confidence data integrity. |
| crt-010 (Status-Aware Retrieval) | Landed | `unimatrix-engine`, `unimatrix-server` | Penalties being validated. If crt-010 has bugs, crt-013 tests expose them. |
| `unimatrix-engine` | Modify | — | Remove `co_access_affinity()`, `W_COAC`, associated tests |
| `unimatrix-adapt` | Modify | — | Remove `episodic.rs` module and references |
| `unimatrix-server` | Modify | — | Briefing k config, status scan optimization, penalty integration tests |
| `unimatrix-store` | Modify | — | New SQL aggregation method(s) for status scan |

## NOT in Scope

- **No changes to the 6-factor stored confidence formula.** Weights W_BASE through W_TRUST are unchanged.
- **No changes to HNSW vector index structure.** Compaction was addressed by crt-010.
- **No new MCP tools or parameters.** Briefing k is internal configuration, not exposed via MCP.
- **No schema migrations.** All tables and columns exist.
- **No changes to CO_ACCESS pair recording.** Storage and `UsageDedup` are unmodified; only consumption/boosting mechanisms are evaluated.
- **No changes to MicroLoRA adaptation logic.** The embedding-level co-access mechanism (crt-006) is not modified.
- **No marker injection fix.** The `[KNOWLEDGE DATA]` marker escaping issue (#17 item 3) is a separate concern.
- **No empirical evaluation of MicroLoRA vs scalar boost overlap.** Deferred to col-015 (Wave 4).
- **No redistribution of W_COAC weight across other factors.** Option A (delete constant, keep formula at 0.92) only.

## Open Questions

1. **Status aggregation: single method vs multiple?** Recommendation from scope is single `StatusAggregates` struct. Architect to confirm during design.
2. **Active entries loading strategy:** The SQL aggregation path still needs active entries for lambda computation. Should the aggregation method include active entries in its return, or should StatusService make a separate `query_by_status(Active)` call? Trade-off: cohesion vs. separation of concerns.
3. **Briefing k env var naming:** What environment variable name for the semantic_k override? Suggestion: `UNIMATRIX_BRIEFING_SEMANTIC_K`. Architect to confirm.
