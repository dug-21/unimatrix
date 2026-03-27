# Test Plan: store-cohesion-query

Component: `Store::compute_graph_cohesion_metrics()` in `crates/unimatrix-store/src/read.rs`

---

## Scope

Seven mandatory unit tests for `compute_graph_cohesion_metrics()` per AC-13. All tests live
in the existing `#[cfg(test)] mod tests` block in `read.rs`, using the existing
`open_test_store()` helper and `create_graph_edges_table()` helper.

These tests constitute the primary risk coverage for R-01 (Critical), R-02, R-03, R-05 (all
High) and R-08 (Medium).

---

## Test Infrastructure

```rust
// Available in the existing #[cfg(test)] mod tests block:
use crate::test_helpers::open_test_store;
// create_graph_edges_table(pool: &SqlitePool) is defined in the same test module

// Pattern for all tests:
let dir = tempfile::TempDir::new().expect("tempdir");
let store = open_test_store(&dir).await;
// open_test_store() creates a full schema store including GRAPH_EDGES (v13).
// All seven tests use the same setup pattern.
```

**Note on schema:** `open_test_store()` applies all migrations including v13 (GRAPH_EDGES).
The `create_graph_edges_table()` helper exists for pre-v13 tests that lack it; the new tests
must NOT call `create_graph_edges_table()` — they operate against the fully-migrated schema
from `open_test_store()`. This distinction is critical (entry rows have `category` column
only in the migrated schema).

### Entry Insert Helper Pattern

All tests that need entries in `entries` table must insert rows with `status = 0` (Active)
and a `category` value. Use direct `sqlx::query` inserts consistent with the existing test
style in `read.rs`:

```rust
sqlx::query(
    "INSERT INTO entries (id, title, content, category, topic, trust_source, status, created_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, '', 'human', 0, 0, 0)"
)
.bind(entry_id as i64)
.bind(title)
.bind(content)
.bind(category)
.execute(&store.write_pool)
.await
.expect("insert entry");
```

### Edge Insert Helper Pattern

```rust
sqlx::query(
    "INSERT INTO graph_edges (source_id, target_id, relation_type, weight, created_at,
     created_by, source, bootstrap_only)
     VALUES (?1, ?2, ?3, 1.0, 0, 'test', ?4, ?5)"
)
.bind(source_id as i64)
.bind(target_id as i64)
.bind(relation_type)  // e.g., "Supports"
.bind(source)         // e.g., "", "nli"
.bind(bootstrap_only as i64)  // 0 or 1
.execute(&store.write_pool)
.await
.expect("insert edge");
```

---

## Test Functions

### test_graph_cohesion_all_isolated

**Covers:** AC-02, AC-07 (zero case), R-05 (division guard)

**Arrangement:**
- Insert 3 active entries (any category) into `entries` with `status = 0`.
- Insert 0 non-bootstrap edges (no rows in `graph_edges` with `bootstrap_only = 0`).

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.connectivity_rate, 0.0)` — no connected entries
- `assert_eq!(metrics.isolated_entry_count, 3)` — all three are isolated
- `assert_eq!(metrics.cross_category_edge_count, 0)`
- `assert_eq!(metrics.supports_edge_count, 0)`
- `assert_eq!(metrics.mean_entry_degree, 0.0)` — explicit zero, not NaN/inf (R-05)
- `assert_eq!(metrics.inferred_edge_count, 0)`
- `assert!(!metrics.mean_entry_degree.is_nan())` — NaN guard
- `assert!(!metrics.mean_entry_degree.is_infinite())` — inf guard
- `assert!(!metrics.connectivity_rate.is_nan())` — NaN guard
- `assert!(!metrics.connectivity_rate.is_infinite())` — inf guard

**R-05 note:** The assertions on `is_nan()` and `is_infinite()` are the explicit division-by-
zero guards required by R-05. An implementation that returns `0 as f64 / 0 as f64` would fail
these assertions.

---

### test_graph_cohesion_all_connected

**Covers:** AC-03, AC-06, AC-07 (non-zero case), R-01 (chain topology)

**Arrangement:**
- Insert 3 active entries: A (id=1, category="decision"), B (id=2, category="decision"),
  C (id=3, category="decision").
- Insert edges: A→B (`relation_type='Supports'`, `source='', bootstrap_only=0`) and
  B→C (`relation_type='Supports'`, `source='', bootstrap_only=0`).
  This is the chain topology for R-01: B appears as both target of A→B and source of B→C.

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.connectivity_rate, 1.0)` — all 3 entries connected; any double-count
  in `connected_entry_count` would give rate > 1.0, catching R-01
- `assert!(metrics.connectivity_rate <= 1.0)` — explicit bound check per R-01 scenario 3
- `assert_eq!(metrics.isolated_entry_count, 0)`
- `assert_eq!(metrics.supports_edge_count, 2)` — both edges are Supports (AC-06)
- `assert!((metrics.mean_entry_degree - (4.0_f64 / 3.0_f64)).abs() < 1e-10)` — (2*2 edges)/3 entries = 4/3
- `assert_eq!(metrics.inferred_edge_count, 0)` — source='' edges are not NLI-inferred

**R-01 note:** B appears as both `target_id` of A→B and `source_id` of B→C. Any naïve
`COUNT(DISTINCT source_id) + COUNT(DISTINCT target_id)` implementation counts B twice,
giving `connected_entry_count = 4`, `connectivity_rate = 4/3 ≈ 1.33`. The assertion
`connectivity_rate == 1.0` and `connectivity_rate <= 1.0` both catch this.

---

### test_graph_cohesion_mixed_connectivity

**Covers:** AC-08, R-01 (partial case), R-08 (deprecated endpoint)

**Arrangement:**
- Insert 4 active entries: A (id=1, category="decision"), B (id=2, category="decision"),
  C (id=3, category="pattern"), D (id=4, category="pattern").
- Insert 1 deprecated entry: E (id=5, status=1, category="convention").
- Insert edges:
  - A→B (`bootstrap_only=0`, `source=''`) — both active, connects A and B
  - C→E (`bootstrap_only=0`, `source=''`) — active→deprecated; does NOT connect C (R-08)
- D has no edges — D is isolated.

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.connectivity_rate, 0.5)` — 2 of 4 active entries connected (A and B)
- `assert_eq!(metrics.isolated_entry_count, 2)` — C and D are isolated (C's edge is to deprecated)
- `assert!(metrics.connectivity_rate <= 1.0)` — bounds check
- `assert_eq!(metrics.cross_category_edge_count, 0)` — A and B same category; C→E edge excluded
  because E is deprecated (R-02 NULL guard)
- `assert!((metrics.mean_entry_degree - 0.5_f64).abs() < 1e-10)` — (2*1 edge)/4 active = 0.5

**R-08 note:** The C→E edge has an active source and deprecated target. Active entries count
for `isolated_entry_count` only based on their own edges to other active entries. Entry C with
only a deprecated-target edge must still count as isolated in the connectivity sense — the
`connected_entry_count` sub-query joins back to `entries` with `status=0`, so E is excluded
and C does not appear in the connected set.

---

### test_graph_cohesion_cross_category

**Covers:** AC-04, R-02 (deprecated endpoint NULL guard)

**Arrangement:**
- Insert 3 active entries: A (id=1, category="decision"), B (id=2, category="pattern"),
  C (id=3, category="decision").
- Insert 1 deprecated entry: D (id=4, status=1, category="convention").
- Insert edges:
  - A→B (`bootstrap_only=0`, `source=''`) — cross-category (decision vs pattern); should count
  - A→C (`bootstrap_only=0`, `source=''`) — same-category (both decision); should NOT count
  - A→D (`bootstrap_only=0`, `source=''`) — active→deprecated; D's category is NULL from LEFT
    JOIN because `tgt_e.status = 0` fails; should NOT count (R-02 NULL guard)

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.cross_category_edge_count, 1)` — only A→B counts
- `assert_eq!(metrics.connectivity_rate, 1.0)` — A, B, C all connected (3/3 active entries)
- `assert_eq!(metrics.isolated_entry_count, 0)`

**R-02 note:** The A→D edge has a deprecated target. In the LEFT JOIN to `tgt_e` with
`status=0`, D does not match, so `tgt_e.category` is NULL. The CASE guard
`tgt_e.category IS NOT NULL` prevents this from counting as cross-category. This is the
direct test of ADR-004's correctness.

---

### test_graph_cohesion_same_category_only

**Covers:** AC-04 (same-category exclusion)

**Arrangement:**
- Insert 3 active entries: A (id=1, category="decision"), B (id=2, category="decision"),
  C (id=3, category="decision") — all same category.
- Insert edges: A→B, B→C (both `bootstrap_only=0`, `source=''`).

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.cross_category_edge_count, 0)` — all same category
- `assert_eq!(metrics.connectivity_rate, 1.0)` — all three connected via chain
- `assert_eq!(metrics.isolated_entry_count, 0)`
- `assert_eq!(metrics.supports_edge_count, 0)` — relation_type is not 'Supports'
- `assert_eq!(metrics.inferred_edge_count, 0)`

---

### test_graph_cohesion_nli_source

**Covers:** AC-05, R-03 (bootstrap_only=1 NLI edge)

**Arrangement:**
- Insert 2 active entries: A (id=1, category="decision"), B (id=2, category="pattern").
- Insert 3 edges:
  - A→B: `source='nli'`, `bootstrap_only=0` — real NLI inferred edge, should count
  - A→B: `source='nli'`, `bootstrap_only=1` — bootstrap NLI edge, must NOT count (R-03)
  - Note: UNIQUE constraint on (source_id, target_id, relation_type) — use different
    `relation_type` values (e.g., 'Supports' for NLI real, 'CoAccess' for bootstrap NLI)

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.inferred_edge_count, 1)` — only the `bootstrap_only=0` NLI edge counts
- `assert_eq!(metrics.connectivity_rate, 1.0)` — both entries connected via the non-bootstrap edge
- `assert_eq!(metrics.cross_category_edge_count, 1)` — decision vs pattern, non-bootstrap edge

**R-03 note:** This test directly verifies AC-16. The `bootstrap_only=1` edge with `source='nli'`
must produce `inferred_edge_count = 1` (not 2). The `bootstrap_only = 0` filter in Query 1's
WHERE clause is the guard.

---

### test_graph_cohesion_bootstrap_excluded

**Covers:** AC-16, R-03 (complete bootstrap exclusion)

**Arrangement:**
- Insert 3 active entries: A (id=1), B (id=2), C (id=3), all category="decision".
- Insert edges — ALL with `bootstrap_only=1`:
  - A→B: `source='nli'`, `relation_type='Supports'`, `bootstrap_only=1`
  - B→C: `source='bootstrap'`, `relation_type='Supersedes'`, `bootstrap_only=1`

**Action:** `store.compute_graph_cohesion_metrics().await`

**Assertions:**
- `assert_eq!(metrics.connectivity_rate, 0.0)` — all bootstrap edges, no real connections
- `assert_eq!(metrics.isolated_entry_count, 3)` — all 3 active entries isolated from real edges
- `assert_eq!(metrics.cross_category_edge_count, 0)`
- `assert_eq!(metrics.supports_edge_count, 0)` — bootstrap Supports edges do not count
- `assert_eq!(metrics.mean_entry_degree, 0.0)` — no non-bootstrap edges
- `assert_eq!(metrics.inferred_edge_count, 0)` — NLI source but bootstrap_only=1; must be 0

**AC-16 note:** This is the definitive test that `source='nli'` AND `bootstrap_only=1` does not
appear in `inferred_edge_count`. This scenario is explicitly called out in AC-16.

---

## Additional Coverage Notes

### Empty Store (zero active entries)

The `all_isolated` test uses 3 active entries. The R-05 edge case of zero active entries
(denominator = 0) is also exercised by this scenario: with 0 edges, `total_edges = 0`,
`active = 3`, so the active-entry-zero branch is NOT hit. A separate sub-scenario should be
considered for completeness, though R-05 states "Open a test store with no entries and no
edges." The `all_isolated` test with `mean_entry_degree = 0.0` covers the NaN guard via
integer arithmetic path (0 edges, non-zero active), but not the `active = 0` branch.

**Recommended additional assertion within `all_isolated`:** Add a second sub-scenario or
extend the test to also open a fresh store with zero inserts and call the function, asserting
`connectivity_rate = 0.0` and `mean_entry_degree = 0.0`. This ensures the `if active > 0`
guard is exercised on both branches.

### Supports Edge Count

AC-06 coverage is verified via `test_graph_cohesion_all_connected` (where both edges are
`relation_type='Supports'`, asserting `supports_edge_count = 2`). The `bootstrap_excluded`
test also verifies that bootstrap Supports edges are excluded.
