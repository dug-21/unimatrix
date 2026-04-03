# crt-044 Implementation Brief
# Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-044/SCOPE.md |
| Architecture | product/features/crt-044/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-044/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-044/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-044/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| migration_v19_v20 | pseudocode/migration_v19_v20.md | test-plan/migration_v19_v20.md |
| graph_enrichment_tick_s1_s2_s8 | pseudocode/graph_enrichment_tick_s1_s2_s8.md | test-plan/graph_enrichment_tick_s1_s2_s8.md |
| graph_expand_security_comment | pseudocode/graph_expand_security_comment.md | test-plan/graph_expand_security_comment.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Make S1 (tag co-occurrence), S2 (structural vocabulary), and S8 (co-access tick) graph edges fully
bidirectional by back-filling reverse edges for all existing single-direction rows via a v19→v20
schema migration and updating all three tick functions to write both directions going forward. A
secondary change adds a `// SECURITY:` comment at the `pub fn graph_expand` signature, making the
quarantine obligation visible at every IDE call site. This is the hard prerequisite for the crt-042
eval gate (`ppr_expander_enabled`) to produce meaningful P@5 improvements.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Migration structure: one, two, or three SQL statements | Two statements: one for `source IN ('S1','S2')` scoped to `relation_type='Informs'`, one for `source='S8'` scoped to `relation_type='CoAccess'`. Three is redundant (S1/S2 share identical logic); one is impossible (relation_type values differ). | ADR-001, entry #3889 | product/features/crt-044/architecture/ADR-001-migration-strategy.md |
| Forward-write bidirectionality: modify SQL query shape vs. two write_graph_edge calls | Two `write_graph_edge` calls per pair with swapped source_id/target_id, matching the `co_access_promotion_tick.rs` two-call pattern. SQL query shapes unchanged. | ADR-002, entry #4041 | product/features/crt-044/architecture/ADR-002-forward-write-pattern.md |
| Security comment form: `#[doc]` attribute, unit test, or inline `// SECURITY:` comment | Inline `// SECURITY:` comment immediately before `pub fn graph_expand(`. Visible in IDE hover at every call site; no logic change; consistent with existing `// SECURITY:` convention at `graph_enrichment_tick.rs:155`. | ADR-003 | product/features/crt-044/architecture/ADR-003-security-comment-approach.md |
| pairs_written counter: per-pair vs. per-edge semantics | Per-edge (individual INSERT attempts returning true). Each direction call counted independently, consistent with `run_co_access_promotion_tick`. New pair increments counter by 2. Semantic change must be documented in PR description. | ADR-002, OQ-1 resolved | product/features/crt-044/architecture/ADR-002-forward-write-pattern.md |
| Migration filter discriminator: source vs. created_by | Filter by `source` field, NOT by `created_by`. `created_by` alone misses tick-era edges. | ADR-001, entry #3889 | product/features/crt-044/architecture/ADR-001-migration-strategy.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/migration.rs` | Modify | Add `if current_version < 20` block with two back-fill SQL statements; bump `CURRENT_SCHEMA_VERSION` from 19 to 20 |
| `crates/unimatrix-store/src/graph_enrichment_tick.rs` | Modify | Add second `write_graph_edge` call per pair in `run_s1_tick`, `run_s2_tick`, and `run_s8_tick` with swapped source_id/target_id |
| `graph_expand.rs` (crate TBD — see Delivery Note 1) | Modify | Add two-line `// SECURITY:` comment immediately before `pub fn graph_expand(` signature; no logic change |
| Migration test file (existing) | Modify | Add S1 back-fill test, S2 back-fill test, S8 back-fill test, two-run idempotency test, exclusion test (co_access, nli, cosine_supports) |
| `graph_enrichment_tick` test file (existing) | Modify | Add per-source bidirectionality tests for run_s1_tick, run_s2_tick, run_s8_tick; add false-return steady-state test; add pairs_written counter assertion |

---

## Data Structures

### GRAPH_EDGES Table (unchanged structure)

```
source_id       u64    FK → ENTRIES.id
target_id       u64    FK → ENTRIES.id
relation_type   TEXT   'Informs' | 'CoAccess' | 'Supports' | 'Supersedes' | 'Contradicts'
weight          REAL
created_at      INTEGER (unix timestamp)
created_by      TEXT
source          TEXT   'S1' | 'S2' | 'S8' | 'co_access' | 'nli' | 'cosine_supports' | ...
bootstrap_only  INTEGER (0 = live, 1 = bootstrap-only)

UNIQUE(source_id, target_id, relation_type)
```

### Migration Back-fill Logic

Two SQL statements in `if current_version < 20` block, both using the same template:

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
SELECT
    g.target_id, g.source_id, g.relation_type, g.weight,
    strftime('%s','now'), g.created_by, g.source, 0
FROM graph_edges g
WHERE g.relation_type = '<type>'
  AND g.source IN ('<sources>')
  AND NOT EXISTS (
    SELECT 1 FROM graph_edges rev
    WHERE rev.source_id = g.target_id
      AND rev.target_id = g.source_id
      AND rev.relation_type = g.relation_type
  )
```

Statement A: `relation_type = 'Informs'`, `source IN ('S1', 'S2')`
Statement B: `relation_type = 'CoAccess'`, `source = 'S8'`

---

## Function Signatures

### write_graph_edge (existing, called twice per pair)

```rust
async fn write_graph_edge(
    store: &Store,
    source_id: u64,
    target_id: u64,
    relation_type: &str,
    weight: f32,
    created_at: u64,
    source: &str,
    metadata: &str,
) -> bool
```

Returns `true` if row inserted (rows_affected = 1); `false` if UNIQUE conflict (silent, Ok path)
or SQL error (warned inside write_graph_edge, not to be double-logged by callers).

### graph_expand (existing, comment added only)

```rust
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
// returned IDs into result sets. graph_expand performs NO quarantine filtering.
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64>
```

### Constants used (existing)

```rust
EDGE_SOURCE_S1: &str = "S1"
EDGE_SOURCE_S2: &str = "S2"
EDGE_SOURCE_S8: &str = "S8"
CURRENT_SCHEMA_VERSION: u64 = 20  // bumped from 19 by this feature
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | Filter back-fill by `source` field (`'S1'`, `'S2'`, `'S8'`), NOT by `created_by`. `created_by` alone misses tick-era edges (entry #3889). |
| C-02 | Use `INSERT OR IGNORE` semantics. The existing `UNIQUE(source_id, target_id, relation_type)` constraint provides idempotency. No schema change required. |
| C-03 | S1+S2 are `relation_type='Informs'`; S8 is `relation_type='CoAccess'`. MUST be separate WHERE clauses — do not combine. |
| C-04 | `source = 'nli'` and `source = 'cosine_supports'` Informs edges MUST NOT be back-filled. The `source IN ('S1','S2')` filter provides this exclusion implicitly; do not relax it. |
| C-05 | Both `INSERT OR IGNORE` AND `NOT EXISTS` MUST be present — defence-in-depth matching the v18→v19 pattern. |
| C-06 | `run_s8_tick`'s `pairs_written` counter counts per-edge (individual INSERT attempts returning true). New pair = counter += 2. Semantic change MUST be documented in PR description. |
| C-07 | `graph_expand.rs` is a pure function. The security comment is documentation only. No logic change to `graph_expand`. |
| C-08 | Migration transition is `CURRENT_SCHEMA_VERSION` 19 → 20. `migrate_if_needed` MUST check `current_version < 20`. |
| C-09 | `write_graph_edge` returning `false` on the second direction call is correct and expected. MUST NOT trigger a warning, log at warn/error level, or increment any error counter. |

---

## Dependencies

### Crate Dependencies

| Crate | Role | Change |
|-------|------|--------|
| `unimatrix-store` | Owns `migration.rs` and `graph_enrichment_tick.rs` | Modified |
| `unimatrix-engine` | Owns `graph_expand.rs` | Modified (comment only) |

No new crate dependencies are introduced (NFR-04).

### Internal Component Dependencies

| Component | File | Dependency Type |
|-----------|------|----------------|
| `migrate_if_needed` | `crates/unimatrix-store/src/migration.rs` | Extended with v19→v20 block |
| `run_s1_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call per pair) |
| `run_s2_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call per pair) |
| `run_s8_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call per pair) |
| `write_graph_edge` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Used unchanged; called twice per pair |
| `pub fn graph_expand` | `crates/unimatrix-engine/src/graph_expand.rs` | Comment added at function signature; no logic change |
| `GRAPH_EDGES` table | SQLite schema | New reverse-edge rows inserted; structure unchanged |
| `UNIQUE(source_id, target_id, relation_type)` | `GRAPH_EDGES` index | Relied upon for idempotency; not modified |

### Feature Dependencies

| Feature | Status | Dependency |
|---------|--------|-----------|
| crt-041 | Shipped | Source of S1/S2/S8 single-direction write pattern being fixed |
| crt-042 | Shipped | `graph_expand` Outgoing-only traversal; crt-044 is a prerequisite for the crt-042 eval gate |
| crt-035 | Shipped | Established back-fill template (entry #3889) |
| crt-043 | In delivery | Treats v20 as migration baseline (v20→v21); crt-044 MUST merge before crt-043, or crt-043 must renumber to v21 |

---

## NOT In Scope

- `co_access_promotion_tick.rs` / `source='co_access'` edges — already bidirectional since crt-035
- NLI Informs edges (`source='nli'`) — intentionally unidirectional per col-030 ADR; must not be back-filled
- Cosine Supports edges (`source='cosine_supports'`) — directionality out of scope
- Supersedes and Contradicts edges — directional by design
- Enabling `ppr_expander_enabled=true` as default — post-eval decision owned by crt-042 delivery team
- Running or evaluating the crt-042 eval gate (`run_eval.py`) — crt-042 delivery team's responsibility
- Any change to `graph_expand` traversal logic, BFS depth, or candidate cap
- New columns or UNIQUE constraint changes on `GRAPH_EDGES`
- Deduplicating S1/S2 Informs edges against NLI Informs edges

---

## Alignment Status

**Overall: PASS with one WARN (accepted)**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — Graph bidirectionality correctness is a prerequisite for the intelligence pipeline's PPR expander to produce meaningful P@5 improvements |
| Milestone Fit | PASS — Wave 1A prerequisite work, squarely within the Cortical phase |
| Scope Gaps | PASS — All six SCOPE.md goals are fully addressed |
| Architecture Consistency | PASS — Internal consistency confirmed; two-statement back-fill and two-call tick pattern match established precedents |
| Risk Completeness | PASS — All six scope risks traced; 10 novel risks documented with coverage |
| Scope Additions | WARN (accepted) — SPECIFICATION.md adds AC-12, AC-13, AC-14 beyond SCOPE.md, each traceable to SCOPE-RISK-ASSESSMENT SR-01, SR-02, SR-05 respectively. All three are additive, no functional scope change. Delivery agent must treat them as binding. |

**Architecture finding (resolved):** `graph_expand.rs` is in `crates/unimatrix-engine/src/graph_expand.rs`. Specification corrected before Session 2.

---

## Delivery Agent Notes (from Vision Guardian)

1. **AC-01 verification query should scope to `source IN ('S1','S2')` on both sides** to avoid
   false failures from NLI/cosine_supports Informs edges that are intentionally forward-only.
   The as-written AC-01 query counts ALL Informs edges without source filter, which would fail
   if any nli/cosine_supports forward-only rows exist.

2. **Pre-merge gate: confirm `CURRENT_SCHEMA_VERSION = 19` in target branch.** If crt-043 has
   merged first, renumber this migration to v21 (not v20) and update all version references
   accordingly (C-08). This is the highest-probability integration failure (R-02, Critical).

---

*Compiled by crt-044-synthesizer (claude-sonnet-4-6). Written 2026-04-03.*
