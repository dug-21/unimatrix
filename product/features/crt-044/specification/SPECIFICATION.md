# SPECIFICATION: crt-044 — Bidirectional S1/S2/S8 Edge Back-fill and graph_expand Security Comment

## Objective

crt-041 writes S1 (tag co-occurrence), S2 (structural vocabulary), and S8 (co-access tick) graph edges
using a lower-ID-first convention, producing only one directed edge per pair. With crt-042's
Outgoing-only `graph_expand` traversal, seeds in the higher-ID position cannot reach their lower-ID
partners — confirmed by 0 bidirectional Informs pairs in the crt-042 SR-03 gate check. This feature
back-fills the missing reverse edges via a v19→v20 schema migration and updates all three tick
functions to write both directions going forward. A secondary change adds a `// SECURITY:` comment
to the `pub fn graph_expand` signature, making the caller quarantine obligation visible at every IDE
usage site.

---

## Functional Requirements

### FR-M: Migration (v19 → v20)

**FR-M-01** — The `migration.rs` file MUST contain a `current_version < 20` block in
`run_main_migrations` that executes two SQL statements:

1. Back-fill reverse S1+S2 Informs edges: `INSERT OR IGNORE ... SELECT (swap source_id/target_id)
   FROM graph_edges WHERE relation_type='Informs' AND source IN ('S1','S2') AND NOT EXISTS(reverse)`.
2. Back-fill reverse S8 CoAccess edges: same swap pattern with `relation_type='CoAccess' AND
   source='S8' AND NOT EXISTS(reverse)`.

**FR-M-02** — `CURRENT_SCHEMA_VERSION` in `migration.rs` MUST be incremented from 19 to 20.

**FR-M-03** — Both `INSERT OR IGNORE` statements MUST use `NOT EXISTS` reverse-edge guards as a
defence-in-depth measure, in addition to the `UNIQUE(source_id, target_id, relation_type)` constraint
that provides the primary idempotency guarantee (C-05).

**FR-M-04** — Back-fill MUST filter by `source` field, not by `created_by`. Filtering by `created_by`
alone misses tick-era forward-only edges (C-01, entry #3889).

**FR-M-05** — The S1+S2 back-fill block MUST use `source IN ('S1', 'S2')` in a single combined
statement. The S8 back-fill MUST be a separate statement with `source = 'S8'`. These MUST NOT be
combined into a single statement because the `relation_type` values differ (`'Informs'` vs
`'CoAccess'`), per C-03.

**FR-M-06** — The migration MUST NOT touch edges with `source = 'nli'` or
`source = 'cosine_supports'`. The `source IN ('S1', 'S2')` and `source = 'S8'` filters provide
this exclusion implicitly but the constraint is explicit — see C-04.

**FR-M-07** — The new migration block executes within the existing outer transaction managed by
`migrate_if_needed`. No additional `BEGIN`/`COMMIT` is required. If any statement fails, the entire
transaction rolls back including the schema_version bump, matching the pattern established in
crt-043 ADR-003.

### FR-T: Tick Forward Writes

**FR-T-01** — `run_s1_tick` MUST call `write_graph_edge` twice per qualifying pair:
- Call 1: `write_graph_edge(lower_id, higher_id, "Informs", weight, source, ...)`
- Call 2: `write_graph_edge(higher_id, lower_id, "Informs", weight, source, ...)`

The SQL query shape (the `t2.entry_id > t1.entry_id` join convention) does NOT change.

**FR-T-02** — `run_s2_tick` MUST call `write_graph_edge` twice per qualifying pair, following the
identical two-call pattern as FR-T-01 with swapped `source_id`/`target_id`.

**FR-T-03** — `run_s8_tick` MUST call `write_graph_edge` twice per qualifying pair:
- Call 1: `write_graph_edge(*a, *b, "CoAccess", weight, source, ...)`  (where `a = min(ids)`)
- Call 2: `write_graph_edge(*b, *a, "CoAccess", weight, source, ...)`

The `valid_ids` guard and pair construction (`a = min`, `b = max`) do NOT change.

**FR-T-04** — The `pairs_written` counter in `run_s8_tick` MUST count individual edge INSERT
attempts that return `true` (i.e., per-edge, not per logical pair). Each of the two
`write_graph_edge` calls per pair is counted independently when it returns `true`. This matches the
counting semantics of `run_co_access_promotion_tick`. For a new pair that has no prior edges, the
counter increments by 2. This is a deliberate semantic change from the prior per-pair counting; see
AC-12 for the documentation requirement.

**FR-T-05** — `write_graph_edge` returning `false` on the second direction call for an
already-bidirectional pair (e.g., a pair whose reverse edge was back-filled by the migration) is
correct and expected behavior — the `UNIQUE` conflict is silently ignored by `INSERT OR IGNORE`. The
implementation MUST NOT treat this `false` return as an error, a warning, or a reason to skip the
budget increment for the first call. See C-09 and SR-02.

**FR-T-06** — Budget counters in all three tick functions MUST be incremented only when
`write_graph_edge` returns `true`. Each call's return value is independently valid.

### FR-S: Security Comment

**FR-S-01** — The `pub fn graph_expand(` signature in `graph_expand.rs` MUST carry an inline
`// SECURITY:` comment immediately preceding or on the signature line, stating the caller quarantine
obligation. The required text (from GH#495) is:

```
// SECURITY: caller MUST apply SecurityGateway::is_quarantined() before inserting
//            returned IDs into result sets.
```

**FR-S-02** — No logic change to `graph_expand` is permitted. This is documentation only (C-07).

---

## Non-Functional Requirements

**NFR-01 — Idempotency**: The v19→v20 migration block MUST be safe to run multiple times. Running it
twice on the same database MUST produce the same row count as running it once and MUST NOT raise
errors.

**NFR-02 — Zero regression**: `cargo test --workspace` MUST pass with no test failures before and
after the change.

**NFR-03 — No schema column changes**: The migration adds rows only. No new columns, no index
changes, no UNIQUE constraint changes to `GRAPH_EDGES`. Schema structure is unchanged.

**NFR-04 — No new crate dependencies**: This feature introduces no new entries in any
`Cargo.toml` `[dependencies]` section.

**NFR-05 — Migration performance**: The back-fill SQL statements operate on the full `GRAPH_EDGES`
table. No performance budget is formally specified, but both statements use the existing
`UNIQUE` index and `source`/`relation_type` columns; the `NOT EXISTS` sub-select traverses the same
index. Performance is acceptable for the expected DB size (thousands of entries).

**NFR-06 — Tick counter semantics change is documented**: The `pairs_written` semantic change in
`run_s8_tick` (per-pair → per-edge, values double for new pairs) MUST be noted in the PR description.
This is a cosmetic change to log output, not a behavioral defect.

---

## Acceptance Criteria

All AC-IDs from SCOPE.md are carried forward unchanged. Three additional criteria (AC-12 through
AC-14) address risks identified in the scope risk assessment.

### From SCOPE.md

**AC-01** — After applying the v19→v20 migration, the query
```sql
SELECT COUNT(*) FROM GRAPH_EDGES g1
WHERE g1.relation_type = 'Informs'
  AND EXISTS (
    SELECT 1 FROM GRAPH_EDGES g2
    WHERE g2.source_id = g1.target_id
      AND g2.target_id = g1.source_id
      AND g2.relation_type = 'Informs'
  )
```
returns a non-zero count equal to the total Informs edge count (every forward Informs edge has a
reverse partner).
*Verification*: SQL query run against a DB that has undergone the v19→v20 migration. Must return
`COUNT(*) > 0` AND `COUNT(*) = (SELECT COUNT(*) FROM GRAPH_EDGES WHERE relation_type = 'Informs')`.

**AC-02** — After applying the v19→v20 migration, the equivalent query for
`relation_type='CoAccess' AND source='S8'` returns a count equal to the S8 CoAccess edge count
(every forward S8 CoAccess edge has a reverse partner).
*Verification*: Same query pattern as AC-01, scoped to `relation_type='CoAccess'` with an additional
`source='S8'` filter on both `g1` and `g2`. Must return `COUNT(*) > 0` AND equal to total S8
CoAccess edge count.

**AC-03** — `run_s1_tick` writes two rows per qualifying pair going forward: `(lower_id, higher_id,
'Informs', ...)` and `(higher_id, lower_id, 'Informs', ...)`.
*Verification*: Integration test with a two-entry fixture; after tick run, assert both `(a→b)` and
`(b→a)` rows exist in `GRAPH_EDGES` with `source='S1'` and `relation_type='Informs'`.

**AC-04** — `run_s2_tick` writes two rows per qualifying pair going forward (same pattern as AC-03).
*Verification*: Integration test with a two-entry fixture; after tick run, assert both `(a→b)` and
`(b→a)` rows exist with `source='S2'` and `relation_type='Informs'`.

**AC-05** — `run_s8_tick` writes two rows per qualifying pair going forward: `(*a, *b, 'CoAccess',
...)` and `(*b, *a, 'CoAccess', ...)`.
*Verification*: Integration test with a two-entry fixture; after tick run, assert both `(a→b)` and
`(b→a)` rows exist with `source='S8'` and `relation_type='CoAccess'`.

**AC-06** — `CURRENT_SCHEMA_VERSION` is incremented to 20 in `migration.rs`.
*Verification*: `grep 'CURRENT_SCHEMA_VERSION' migration.rs` returns `= 19` before and `= 20` after.

**AC-07** — The v19→v20 migration block uses `INSERT OR IGNORE` and is idempotent: running it twice
produces no duplicate rows and no errors.
*Verification*: Migration test that applies the v19→v20 block twice in sequence; assert row counts
are identical after the first and second run, and no SQL error is returned. This MUST be an
explicit test, not just an assertion comment.

**AC-08** — The `pub fn graph_expand(` line in `graph_expand.rs` carries a `// SECURITY:` comment
stating the caller quarantine obligation, matching the text specified in GH#495 (FR-S-01).
*Verification*: `grep '// SECURITY:' graph_expand.rs` is non-empty; the comment appears on the line
immediately preceding `pub fn graph_expand(` or inline on that line.

**AC-09** — All existing migration tests pass. The new migration block has at least one test
asserting that a forward-only S1 Informs edge and a forward-only S8 CoAccess edge each gain a
reverse partner after the v19→v20 migration runs.
*Verification*: Test creates a v19-state fixture with one S1 Informs forward edge and one S8
CoAccess forward edge. After `migrate_if_needed`, asserts both reverse edges exist.

**AC-10** — All existing `graph_enrichment_tick` tests pass. New tests for `run_s1_tick`,
`run_s2_tick`, and `run_s8_tick` assert that both `(a→b)` and `(b→a)` edges exist after the tick
runs on a two-entry fixture.
*Verification*: Three separate test cases (one per tick function). Each creates a minimal two-entry
store, runs the relevant tick, then queries `GRAPH_EDGES` for both edge directions. This is a
per-source bidirectionality assertion (SR-06). These tests serve as regression guards if any
individual tick's two-call pattern is broken in a future change.

**AC-11** — `cargo test --workspace` passes with no regressions.
*Verification*: Full workspace test run returns exit code 0.

### Additional Criteria (from Scope Risk Assessment)

**AC-12** — The PR description MUST document that `run_s8_tick`'s `pairs_written` counter now counts
per-edge (individual INSERT attempts returning `true`), not per logical pair. For a new pair with
no prior edges, the counter value is 2× what it was before this change. This is correct and
expected, not a defect.
*Verification*: Reviewer confirms PR description contains this semantic change documentation before
merge. The implementation test for AC-05 verifies the count equals 2 for a single new pair.

**AC-13** — `write_graph_edge` returning `false` on the second direction call (UNIQUE conflict for
an already-existing reverse edge) MUST NOT trigger a warning, log message at warn/error level, or
increment any error counter. The `false` return for the second call after the v19→v20 migration
has back-filled the reverse edge is the normal post-migration steady state.
*Verification*: Run tick on a two-entry fixture whose reverse edge already exists (simulate
post-migration state). Assert no error log entries and `pairs_written` increments by 1 (first
call returns `true`, second call returns `false`).

**AC-14** — The migration test suite includes a two-run idempotency test: applying the v19→v20
migration block to a database that already has both forward and reverse edges produces the same
row count as a single-run migration. No error is returned on the second run.
*Verification*: Test creates a DB with a pre-existing reverse edge alongside a forward-only edge.
Runs migration twice. Asserts `GRAPH_EDGES` row count is identical after first and second run, and
no SQL error is raised. This is separate from the AC-07 test and focuses on partial-bidirectionality
input state. (SR-05)

---

## Domain Models

### Ubiquitous Language

| Term | Definition |
|------|-----------|
| **GRAPH_EDGES** | SQLite table storing directed weighted edges between knowledge entries. Columns include `source_id`, `target_id`, `relation_type`, `weight`, `source`, `created_by`, `bootstrap_only`. |
| **relation_type** | The semantic type of a graph edge. Relevant values: `'Informs'` (knowledge adjacency), `'CoAccess'` (co-retrieval affinity). |
| **source** | The string field on a `GRAPH_EDGES` row identifying which tick or process wrote the edge. Values relevant to this feature: `'S1'`, `'S2'`, `'S8'`. `'nli'` and `'cosine_supports'` are explicitly excluded. |
| **S1 edge** | An `Informs` edge written by `run_s1_tick` (tag co-occurrence). Prior to this feature, written with `source_id = lower_id`, `target_id = higher_id` only. |
| **S2 edge** | An `Informs` edge written by `run_s2_tick` (structural vocabulary overlap). Same single-direction convention as S1. |
| **S8 edge** | A `CoAccess` edge written by `run_s8_tick` (co-access tick promotion). Prior to this feature, written with `a = min(ids)`, `b = max(ids)` only. Not the same as `source = 'co_access'` edges from `run_co_access_promotion_tick`. |
| **bidirectional edge** | A pair of `GRAPH_EDGES` rows where row A has `(source_id=X, target_id=Y)` and row B has `(source_id=Y, target_id=X)`, same `relation_type`. A symmetric relationship is fully bidirectional when both directions exist. |
| **back-fill** | A migration operation that inserts the missing reverse direction for existing forward-only edges. Uses `INSERT OR IGNORE` with swapped `source_id`/`target_id`. |
| **forward-only edge** | A `GRAPH_EDGES` row whose reverse direction `(target_id → source_id)` does not exist. The pre-crt-044 state for all S1, S2, and S8 edges. |
| **write_graph_edge** | The Rust function in `graph_enrichment_tick.rs` that executes a single `INSERT OR IGNORE INTO GRAPH_EDGES` statement. Returns `true` if the row was inserted (`rows_affected() > 0`), `false` if the `UNIQUE` constraint caused the insert to be ignored. |
| **graph_expand** | `pub fn graph_expand(...)` in `graph_expand.rs`. BFS traversal that follows Outgoing edges from seed entries to expand the candidate pool. Quarantine obligation: caller MUST filter returned IDs through `SecurityGateway::is_quarantined()`. |
| **pairs_written** | Counter in `run_s8_tick` tracking successful edge INSERT calls. After crt-044, counts per-edge (each direction independently), not per logical pair. 2× previous values for new pairs is correct. |
| **UNIQUE constraint** | `UNIQUE(source_id, target_id, relation_type)` on `GRAPH_EDGES`. Enforces no duplicate directed edges. `INSERT OR IGNORE` silently skips conflicting inserts. |
| **v19→v20 migration** | The migration block in `migration.rs` added by this feature. Back-fills reverse S1/S2 Informs and S8 CoAccess edges. Owned entirely by crt-044. crt-043 uses v20 as its baseline and migrates to v21. |
| **SecurityGateway** | Module enforcing quarantine filtering. `SecurityGateway::is_quarantined(id)` returns true if an entry must be excluded from result sets. |

### Entity Relationships

```
GRAPH_EDGES
  source_id  →  ENTRIES.id  (FK)
  target_id  →  ENTRIES.id  (FK)
  relation_type: 'Informs' | 'CoAccess' | 'Supports' | 'Supersedes' | 'Contradicts'
  source: 'S1' | 'S2' | 'S8' | 'co_access' | 'nli' | 'cosine_supports' | ...
  UNIQUE(source_id, target_id, relation_type)

Bidirectionality requirement:
  S1 Informs: (a,b) MUST imply (b,a)
  S2 Informs: (a,b) MUST imply (b,a)
  S8 CoAccess: (a,b) MUST imply (b,a)
  nli Informs: (a,b) does NOT imply (b,a) — intentionally unidirectional
  cosine_supports: out of scope for directionality
```

---

## User Workflows

### Workflow 1: Database Upgrade (v19 → v20)

1. Operator upgrades the binary containing crt-044 changes.
2. On first `Store::open()`, `migrate_if_needed` detects `schema_version = 19 < 20`.
3. The `current_version < 20` block executes inside the outer transaction:
   a. `INSERT OR IGNORE` back-fills reverse S1+S2 Informs edges.
   b. `INSERT OR IGNORE` back-fills reverse S8 CoAccess edges.
   c. `schema_version` is bumped to 20.
4. All subsequent `graph_expand` traversals from any seed ID reach both lower-ID and higher-ID
   partners via Outgoing edges.

### Workflow 2: Forward Tick Run (post-migration)

1. `run_s1_tick` fires on its normal schedule.
2. For each qualifying `(lower_id, higher_id)` pair from the tag co-occurrence query:
   a. `write_graph_edge(lower_id, higher_id, ...)` — returns `true` (new edge) or `false` (exists).
   b. `write_graph_edge(higher_id, lower_id, ...)` — returns `true` (new reverse) or `false`
      (reverse already exists from migration or a prior tick run).
3. Budget counter increments once per `true` return across both calls.
4. Same flow for `run_s2_tick` and `run_s8_tick`.

### Workflow 3: IDE Navigation to graph_expand

1. Developer hover/ctrl-click on `graph_expand` call site in IDE.
2. IDE shows function signature with `// SECURITY:` comment inline.
3. Quarantine obligation is visible without navigating to the module header.

---

## Constraints

All constraints from SCOPE.md are carried forward. C-06 is expanded to reflect OQ-1 resolution.

| ID | Constraint |
|----|-----------|
| **C-01** | Filter back-fill by `source` field (`'S1'`, `'S2'`, `'S8'`), NOT by `created_by`. `created_by` alone misses tick-era edges (entry #3889). |
| **C-02** | Use `INSERT OR IGNORE` semantics. The existing `UNIQUE(source_id, target_id, relation_type)` constraint provides idempotency. No schema change required. |
| **C-03** | S1+S2 are `relation_type='Informs'`; S8 is `relation_type='CoAccess'`. These MUST be separate `WHERE` clauses. Do not combine into one statement. |
| **C-04** | `source = 'nli'` and `source = 'cosine_supports'` Informs edges MUST NOT be back-filled. The `source IN ('S1','S2')` filter provides this exclusion; do not relax it. |
| **C-05** | The `NOT EXISTS` reverse-edge guard in the migration SQL is defence-in-depth. Both `INSERT OR IGNORE` AND `NOT EXISTS` MUST be present, matching the v18→v19 pattern. |
| **C-06** | `run_s8_tick`'s `pairs_written` counter counts per-edge (individual INSERT attempts returning `true`). Each direction write is independent. A new pair increments the counter by 2. This matches `run_co_access_promotion_tick` semantics. The semantic change from per-pair to per-edge MUST be documented in the PR description (OQ-1 resolved). |
| **C-07** | `graph_expand.rs` is a pure function. The security comment is documentation only. No logic change to `graph_expand`. |
| **C-08** | Migration transition is `CURRENT_SCHEMA_VERSION` 19 → 20. The `migrate_if_needed` function MUST check `current_version < 20` in the new block. |
| **C-09** | `write_graph_edge` is idempotent (`INSERT OR IGNORE`). The second direction call per pair requires no special handling for already-existing reverse edges. A `false` return is correct and must not be treated as an error. |

---

## Dependencies

### Crate Dependencies

| Crate | Role | Change |
|-------|------|--------|
| `unimatrix-store` | Owns `migration.rs` | Modified |
| `unimatrix-server` | Owns `graph_enrichment_tick.rs` | Modified |
| `unimatrix-engine` | Owns `graph_expand.rs` | Modified (comment only) |

No new crate dependencies are introduced.

### Internal Components

| Component | File | Dependency Type |
|-----------|------|----------------|
| `migrate_if_needed` | `crates/unimatrix-store/src/migration.rs` | Extended with v19→v20 block |
| `run_s1_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call) |
| `run_s2_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call) |
| `run_s8_tick` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Modified (second write_graph_edge call) |
| `write_graph_edge` | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` | Used unchanged; called twice per pair |
| `pub fn graph_expand` | `crates/unimatrix-engine/src/graph_expand.rs` | Comment added at function signature (post-crt-042 state) |
| `GRAPH_EDGES` table | SQLite schema | Rows added by migration; structure unchanged |
| `UNIQUE(source_id, target_id, relation_type)` | `GRAPH_EDGES` index | Relied upon for idempotency; not modified |

### Feature Dependencies

| Feature | Status | Dependency |
|---------|--------|-----------|
| crt-041 | Shipped | Source of S1/S2/S8 single-direction write pattern being fixed |
| crt-042 | Shipped | `graph_expand` Outgoing-only traversal; this feature is a prerequisite for the crt-042 eval gate |
| crt-035 | Shipped | Established back-fill template (`INSERT OR IGNORE` + swap + `NOT EXISTS`); entry #3889 |
| crt-043 | In delivery | Uses v20 as its migration baseline (v20→v21). crt-044 MUST ship before crt-043 to avoid a version conflict. If delivery order cannot be guaranteed, the crt-043 implementation agent must coordinate schema version numbering. |

---

## NOT In Scope

The following items are explicitly excluded to prevent scope creep:

- **`co_access_promotion_tick.rs` / `source='co_access'` edges** — already bidirectional since crt-035.
- **NLI Informs edges (`source='nli'`)** — intentionally unidirectional per col-030 ADR; must not be back-filled.
- **Cosine Supports edges (`source='cosine_supports'`)** — directionality is out of scope.
- **Supersedes and Contradicts edges** — directional by design; excluded.
- **Enabling `ppr_expander_enabled=true` as default** — post-eval decision owned by crt-042 delivery team.
- **Running or evaluating the crt-042 eval gate (`run_eval.py`)** — crt-042 delivery team's responsibility.
- **Any change to `graph_expand` traversal logic, BFS depth, or candidate cap** — logic is unchanged.
- **New columns or UNIQUE constraint changes on `GRAPH_EDGES`** — schema structure is unchanged.
- **Deduplicating S1/S2 Informs edges against NLI Informs edges** — first-writer-wins via UNIQUE is unchanged.
- **Any change to `graph_expand.rs` logic** — the security comment is documentation only.

---

## Open Questions

None. All open questions from SCOPE.md are resolved:

- **OQ-1 (resolved)**: `pairs_written` counts per-edge. See C-06 and AC-12.
- **OQ-2 (resolved)**: S1+S2 use a combined `WHERE source IN ('S1','S2')` statement; S8 is a separate statement. See FR-M-05 and C-03.
- **OQ-3 (resolved)**: AC text references function name `pub fn graph_expand`, not line number. See AC-08.

One delivery-sequencing note for the implementation agent: crt-043 is in delivery and treats v20
as its migration baseline. crt-044 must land first (or the crt-043 implementation agent must
increment their target version to v21+1 if crt-044 already shipped v20). This is a merge-order
concern, not a specification ambiguity.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 13 entries; entries #4078 (S8 bidirectionality gap pattern), #3889 (back-fill filter by source pattern), and #4069 (crt-043 migration atomicity ADR) were directly applicable.
- Entry #4069 was an early-design ADR describing crt-043 as v19→v20; the IMPLEMENTATION-BRIEF confirms crt-043 is v20→v21 (dependent on crt-044 landing v20 first). No correction action taken — the ADR reflects its own feature's design intent and is consistent with crt-043's delivered brief.

---

*Specification authored by crt-044-agent-2-spec (claude-sonnet-4-6). Written 2026-04-03.*
