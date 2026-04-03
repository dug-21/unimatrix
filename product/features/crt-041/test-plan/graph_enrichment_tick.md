# Test Plan: `graph_enrichment_tick` Component

**Source file:** `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
**Risk coverage:** R-01, R-02, R-04, R-05, R-06, R-07, R-10, R-11, R-12, R-13, R-14

---

## Unit Test Expectations

All tests in this module use `#[tokio::test]` (async). Tests use a real in-process SQLite
database initialized with the test schema (follow the pattern from `co_access_promotion_tick`
tests — build a `Store` in a temp dir, insert synthetic rows, call the tick function
directly, then query the DB to assert results).

### S1 Tick Tests

**`test_s1_basic_informs_edge_written`** — R-07, AC-01
- Arrange: two active entries sharing exactly 3 tags.
- Act: call `run_s1_tick(store, config)`.
- Assert:
  - `SELECT count(*) FROM graph_edges WHERE source_id=A AND target_id=B AND relation_type='Informs' AND source='S1'` returns 1.
  - `weight = 0.3` (3 * 0.1).
  - `created_by = 's1'`.
  - `bootstrap_only = 0`.

**`test_s1_excludes_quarantined_source`** — R-01, AC-03 (source_id position)
- Arrange: two entries sharing 3 tags; entry with lower ID has `status = 3` (Quarantined).
- Act: `run_s1_tick`.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S1'` returns 0.

**`test_s1_excludes_quarantined_target`** — R-01, AC-03 (target_id position)
- Arrange: two entries sharing 3 tags; entry with higher ID has `status = 3`.
- Act: `run_s1_tick`.
- Assert: zero S1 edges written. Both endpoint positions must be guarded.

**`test_s1_having_threshold_exactly_3`** — edge case
- Arrange: pair sharing exactly 3 tags (must qualify), pair sharing exactly 2 tags (must not).
- Act: `run_s1_tick`.
- Assert: exactly 1 edge written (the 3-tag pair only). Off-by-one check.

**`test_s1_idempotent`** — AC-02
- Arrange: two active entries sharing 3 tags.
- Act: call `run_s1_tick` twice.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S1'` returns 1 (not 2). INSERT OR IGNORE.

**`test_s1_weight_formula`** — AC-05
- Arrange: four pairs with 3, 5, 10, and 12 shared tags.
- Act: `run_s1_tick` with cap ≥ 4.
- Assert:
  - pair with 3 tags: `weight = 0.3`
  - pair with 5 tags: `weight = 0.5`
  - pair with 10 tags: `weight = 1.0` (exactly at cap)
  - pair with 12 tags: `weight = 1.0` (cap, not 1.2)

**`test_s1_cap_respected`** — AC-04
- Arrange: 10 pairs with distinct shared tag counts.
- Act: `run_s1_tick` with `max_s1_edges_per_tick = 3`.
- Assert: exactly 3 edges written; they correspond to the 3 highest-overlap pairs.

**`test_s1_cap_one`** — edge case (config §)
- Arrange: 5 qualifying pairs. Set `max_s1_edges_per_tick = 1`.
- Act: `run_s1_tick`.
- Assert: exactly 1 edge written; it is the highest-overlap pair.

**`test_s1_tick_completes_within_500ms_at_1200_entries`** — R-04, NFR-03
- Arrange: insert 1,200 active entries into `entries`, distribute ~5 tags each across
  ~20 distinct tag values so ~50 pairs share ≥3 tags. This tests the GROUP BY materialization
  boundary (OQ-01). Use a loop to insert via sqlx directly.
- Act: record wall-clock time; call `run_s1_tick`.
- Assert: elapsed < 500ms. If this assertion fails, the S1 GROUP BY query plan is not
  using the `idx_entry_tags_tag` index correctly and the delivery agent must restructure
  the query.

**`test_s1_empty_corpus_no_panic`**
- Arrange: empty DB (no entries, no tags).
- Act: `run_s1_tick`.
- Assert: no panic, no edges written, function returns.

**`test_s1_source_value_is_s1_not_nli`** — R-07
- Arrange: qualifying pair.
- Act: `run_s1_tick`.
- Assert: `SELECT source FROM graph_edges WHERE source='S1'` count = 1.
- Assert: `SELECT source FROM graph_edges WHERE source='nli'` count = 0.

---

### S2 Tick Tests

**`test_s2_empty_vocabulary_is_noop`** — R-14, AC-07
- Arrange: two active entries with overlapping content; `config.s2_vocabulary = vec![]`.
- Act: `run_s2_tick`.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S2'` = 0. No panic. No SQL error.

**`test_s2_basic_informs_edge_written`** — AC-06
- Arrange: two active entries; entry A title contains "migration schema", entry B content
  contains "schema migration"; vocabulary = `["schema", "migration"]`.
- Act: `run_s2_tick`.
- Assert: edge written with `source='S2'`, `relation_type='Informs'`,
  `weight = 0.2` (2 terms * 0.1), `created_by='s2'`.

**`test_s2_excludes_quarantined_source`** — R-01, AC-09 (source_id position)
- Arrange: vocabulary-matching pair; lower-ID entry has `status = 3`.
- Act: `run_s2_tick`.
- Assert: zero S2 edges.

**`test_s2_excludes_quarantined_target`** — R-01, AC-09 (target_id position)
- Arrange: vocabulary-matching pair; higher-ID entry has `status = 3`.
- Act: `run_s2_tick`.
- Assert: zero S2 edges.

**`test_s2_no_false_positive_capabilities_for_api`** — R-11, AC-10
- Arrange: vocabulary = `["api"]`; entry A title = "the api docs"; entry B content =
  "capabilities only, no other words".
- Act: `run_s2_tick`.
- Assert: no edge written between A and B. Space-padded matching must not match "api"
  inside "capabilities".

**`test_s2_no_false_positive_cached_for_cache`** — R-11
- Arrange: vocabulary = `["cache"]`; entry A content = "use cache"; entry B content =
  "cached data store" (suffix match — "cached" contains "cache" but space-padding should prevent).
- Act: `run_s2_tick`.
- Assert: no edge — "cached" does not match `' cache '` pattern.

**`test_s2_true_positive_api_in_title`** — R-11, AC-10 (positive case)
- Arrange: vocabulary = `["api"]`; entry A title = "the api is documented"; entry B
  title = "api schema reference".
- Act: `run_s2_tick`.
- Assert: edge IS written.

**`test_s2_sql_injection_single_quote`** — R-02, AC-11
- Arrange: vocabulary = `["it's"]`; two entries both containing "it's" in content.
- Act: `run_s2_tick`.
- Assert: no panic, no SQL error. Edge is written between the two matching entries (correct
  semantic result — the term matched despite the quote character). The push_bind path handles
  quoting transparently.

**`test_s2_sql_injection_double_dash`** — R-02
- Arrange: vocabulary = `["-- DROP TABLE graph_edges"]`.
- Act: `run_s2_tick`.
- Assert: no panic, no SQL error. Zero edges (the term is unlikely to match any entry content).
  The graph_edges table still exists (table drop did not execute).

**`test_s2_idempotent`** — AC-08
- Arrange: qualifying pair; vocabulary = `["schema"]`.
- Act: `run_s2_tick` twice.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S2'` = 1.

**`test_s2_cap_respected`** — AC-12
- Arrange: 10 qualifying pairs with distinct shared-term counts. `max_s2_edges_per_tick = 3`.
- Act: `run_s2_tick`.
- Assert: exactly 3 edges written, highest-overlap pairs selected.

**`test_s2_threshold_exactly_2_terms`** — edge case (spec §)
- Arrange: pair where entry A has 1 vocabulary term match and entry B has 1 different
  vocabulary term match (total = 2 across both sides). Vocabulary = `["schema", "cache"]`.
  Entry A: "schema design"; entry B: "cache strategy".
- Act: `run_s2_tick`.
- Assert: edge written (total ≥ 2 qualifies).

**`test_s2_one_term_each_side_qualifies`** — edge case (spec §)
- Arrange: same as above but verify a pair with 0+0 = 0 total does NOT qualify.
- Assert: no edge for the 0-match pair.

---

### S8 Tick Tests

**`test_s8_basic_coaccess_edge_written`** — AC-14
- Arrange: two active entries; insert audit_log row with
  `operation='context_search', outcome=0, target_ids='[<id_a>, <id_b>]'`.
- Act: `run_s8_tick` on a qualifying tick (tick=0, interval=10, or set interval=1).
- Assert: edge with `source='S8'`, `relation_type='CoAccess'`, `weight=0.25`,
  `created_by='s8'` written. Both endpoints active.

**`test_s8_watermark_advances_past_malformed_json_row`** — R-05, AC-20
- Arrange: insert audit_log row 1 (event_id=1, valid), row 2 (event_id=2,
  `target_ids='not-json'`), row 3 (event_id=3, valid with 2 active entry IDs).
  Set interval = 1.
- Act: `run_s8_tick`.
- Assert:
  - Row 1 pair is written as an S8 edge.
  - Row 3 pair is written as an S8 edge.
  - `SELECT value FROM counters WHERE name='s8_audit_log_watermark'` = 3 (past row 2).
  - Run again; zero new edges (idempotency + watermark advanced).

**`test_s8_watermark_written_after_edges`** — R-06, AC-16
- Arrange: valid audit_log rows; pre-existing watermark = 0.
- Act: `run_s8_tick`.
- Assert after function returns: edges are present AND watermark is updated. This validates
  the write ordering. A direct assertion is: if we query edges before and after the watermark
  counter in the same transaction, the count increased first. In practice, the unit test
  simulates re-run after "crash" by not calling watermark update (pre-state = edges written,
  watermark = 0). Call `run_s8_tick` again. Assert no duplicate edges (INSERT OR IGNORE).
  Watermark is now updated.

**`test_s8_excludes_briefing_operation`** — R-12, AC-17
- Arrange: audit_log row with `operation='context_briefing', outcome=0, target_ids='[1,2]'`
  where entries 1 and 2 are active.
- Act: `run_s8_tick`.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S8'` = 0.

**`test_s8_excludes_failed_search`** — R-12, AC-18
- Arrange: audit_log row with `operation='context_search', outcome=1, target_ids='[1,2]'`.
- Act: `run_s8_tick`.
- Assert: zero S8 edges.

**`test_s8_excludes_quarantined_endpoint`** — R-01, AC-19
- Arrange: active entry A (id=1) and quarantined entry B (status=3, id=2).
  audit_log row with `target_ids='[1,2]'`.
- Act: `run_s8_tick`.
- Assert: zero S8 edges. Both endpoint positions tested: also test with quarantined id=1.

**`test_s8_pair_cap_not_row_cap`** — R-10, AC-21
- Arrange: `max_s8_pairs_per_batch = 5`.
  Insert one audit_log row with `target_ids = '[1,2,3,4,5]'` (10 pairs).
  All 5 entries active.
- Act: `run_s8_tick`.
- Assert: `SELECT count(*) FROM graph_edges WHERE source='S8'` = 5 (pairs capped, not row).

**`test_s8_partial_row_watermark_semantics`** — R-10 (partial-row watermark)
- Arrange: `max_s8_pairs_per_batch = 3`.
  Row 1 (event_id=1): `target_ids = '[1,2,3,4]'` → 6 pairs.
  Row 2 (event_id=2): valid 2-entry pair.
  All entries active.
- Act: `run_s8_tick`.
- Assert: exactly 3 edges written (cap reached mid-row-1). Watermark is 0 if row 1 was not
  fully processed, OR 1 if the implementation writes partial row edges up to cap and records
  that row's event_id. Consult ADR-003 for the exact semantics: the watermark advances only
  to the last FULLY-processed row's event_id. If row 1 is partially consumed, watermark stays
  at 0 (pre-row-1). Row 2's edges NOT written (cap exhausted).

**`test_s8_singleton_target_ids_no_panic`** — edge case
- Arrange: audit_log row with `target_ids = '[42]'` (N=1, zero pairs).
- Act: `run_s8_tick`.
- Assert: no panic, zero edges, watermark advances past row.

**`test_s8_empty_target_ids_no_panic`** — edge case
- Arrange: audit_log row with `target_ids = '[]'` (empty array, zero pairs).
- Act: `run_s8_tick`.
- Assert: no panic, zero edges, watermark advances.

**`test_s8_duplicate_ids_deduplicated`** — edge case
- Arrange: audit_log row with `target_ids = '[1, 1, 2]'`. Entries 1 and 2 active.
  Pair (1,1) is invalid (a=b, not a<b). Pair (1,2) appears once.
- Act: `run_s8_tick`.
- Assert: exactly 1 edge written (not 2 for the duplicate id reference).

**`test_s8_watermark_persists_across_runs`** — AC-15
- Arrange: rows 1..5; run S8; watermark = 5. Insert row 6.
- Act: `run_s8_tick` again.
- Assert: only row 6's pairs produce new edges (not rows 1..5 reprocessed).

**`test_s8_gated_by_tick_interval`** — AC-13
- This is tested at the background level (see background.md). At the S8 unit level,
  verify that `run_s8_tick` itself does process rows when called (gate logic lives in
  `run_graph_enrichment_tick`, not inside `run_s8_tick`).

---

### `run_graph_enrichment_tick` Orchestration Tests

**`test_enrichment_tick_calls_s1_and_s2_always`** — AC-26
- Arrange: qualifying data for both S1 and S2. Set `s8_batch_interval_ticks = 10`.
- Act: call `run_graph_enrichment_tick(store, config, current_tick=0)`.
- Assert: S1 edges written, S2 edges written. At tick=0, 0 % 10 == 0 so S8 runs too.

**`test_enrichment_tick_skips_s8_on_non_batch_tick`** — AC-13
- Arrange: qualifying S8 data. `s8_batch_interval_ticks = 10`.
- Act: call `run_graph_enrichment_tick(store, config, current_tick=1)`.
- Assert: S1 and S2 edges may be written; S8 writes zero edges (1 % 10 != 0).

**`test_enrichment_tick_s8_runs_on_batch_tick`** — AC-13
- Act: call `run_graph_enrichment_tick(store, config, current_tick=10)`.
- Assert: S8 edges written (10 % 10 == 0).

---

### Error-Handling Tests (Infallible Tick Pattern)

**`test_s1_sql_error_logs_warn_no_panic`** — AC-25
- Arrange: valid store; then drop the `entry_tags` table to provoke an SQL error.
- Act: `run_s1_tick`.
- Assert: no panic, function returns. (Tracing output verification is optional — use
  `tracing_test` crate if available, otherwise assert no panic is the primary check.)

**`test_s2_sql_error_no_panic`** — AC-25
- Arrange: drop `entries` table.
- Act: `run_s2_tick` with non-empty vocabulary.
- Assert: no panic, returns cleanly.

**`test_s8_watermark_read_failure_no_panic`** — AC-25
- Arrange: drop `counters` table.
- Act: `run_s8_tick`.
- Assert: no panic.

---

## Integration Test Expectations (MCP Interface)

See OVERVIEW.md §New Integration Tests for the three tests to add to `test_lifecycle.py`.
These tests confirm the MCP-observable effect of S1/S2/S8:

1. After a tick, `cross_category_edge_count` or total non-bootstrap edge count increases.
2. `inferred_edge_count` does not change when only S1/S2/S8 run (backward compat R-13).
3. A quarantined entry is excluded from edge traversal (observable via search exclusion).

All three are marked `xfail` because the background tick interval (15 min by default) exceeds
the integration test timeout. The xfail reason must reference a GH Issue for CI tick interval
configuration if one is filed.

---

## Edge Cases Summary

| Edge Case | Test |
|-----------|------|
| S1 pair with exactly 3 shared tags | `test_s1_having_threshold_exactly_3` |
| S1 weight capped at 1.0 for 12 shared tags | `test_s1_weight_formula` |
| S1 cap = 1 | `test_s1_cap_one` |
| S2 empty vocabulary | `test_s2_empty_vocabulary_is_noop` |
| S2 "api" does not match "capabilities" | `test_s2_no_false_positive_capabilities_for_api` |
| S2 "cache" does not match "cached" | `test_s2_no_false_positive_cached_for_cache` |
| S2 single quote in vocabulary term | `test_s2_sql_injection_single_quote` |
| S8 singleton target_ids | `test_s8_singleton_target_ids_no_panic` |
| S8 empty target_ids array | `test_s8_empty_target_ids_no_panic` |
| S8 malformed JSON between valid rows | `test_s8_watermark_advances_past_malformed_json_row` |
| S8 duplicate IDs in target_ids | `test_s8_duplicate_ids_deduplicated` |
| S8 partial-row cap semantics | `test_s8_partial_row_watermark_semantics` |
