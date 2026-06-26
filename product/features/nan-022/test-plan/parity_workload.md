# Test Plan: C4′ — `harness/parity_workload.py` (extended)

Covers **R-13 (Medium, one-identity/token/barrier)**, **R-06 (High, corpus depth)**,
**R-15 (Medium, no-seed audit)**, **R-12 (Medium, stale-token)**, and the ingest half of
**R-09 (High, bundle contract)**. The consumed manifest is AUGMENTED with a deterministic
seed-corpus + query phase; `load_https_vector` is generalized to `load_https_bundle`;
`assert_no_seed_reachable` coverage is extended to all net-new modules. This file EXTENDS the
existing `suites/test_parity_workload.py` (the off-Docker `test_parity_workload.py` precedent).

Surface under test (new/extended):
- augmented `default_workload()` — seed-corpus + query phase, ONE manifest/identity/token preserved
- `load_https_bundle(out_path, expected_run_token) -> dict[str, Any]`  (missing/stale/empty → InfraError)
- `assert_no_seed_reachable(*source_paths)` — extended coverage; `FORBIDDEN_SEED_SITES`
- consumed verbatim: `ParityWorkload`, `ToolCall`, `durability_barrier`, `observe_count`,
  manifest `to_json`/`from_json`/`write_manifest`/`read_manifest`

Tier: **A (off-Docker unit)** — extends `suites/test_parity_workload.py`.

## Unit Test Expectations

### ONE-identity/ONE-token/ONE-barrier invariant post-augmentation (R-13 scenarios 1–2, AC-01)
- `test_default_workload_single_parity_workload_object`: the augmented `default_workload()`
  returns a SINGLE `ParityWorkload` (seed + query + cycle phases live in ONE object, not split
  sub-workloads).
- `test_run_token_equals_session_id`: `run_token == workload.session_id` — the single stable
  correlation token is preserved after augmentation (the SR-05/#832 defense).
- `test_one_barrier_helper_after_augmentation`: the augmented workload uses the ONE shared
  `durability_barrier` helper — no second/forked barrier.
- `test_augmented_manifest_round_trip_stable`: `to_json`/`from_json` round-trip of the augmented
  manifest is byte-stable (both legs replay the SAME manifest byte-identically).

### Seed-corpus depth — non-degenerate ranking (R-06 scenarios 1,3, AC-02/AC-05, OQ-C)
- `test_seed_corpus_size_yields_prefix_floor_gt_one`: assert the seed-corpus size + query set
  yield a ranking depth such that the NFR-7 stable-prefix floor N is achievable AND N > 1
  (resolves the concrete OQ-C numbers — a Stage-3a test-design call). A single-hit ranking is a
  vacuous pass (#5177); this asserts the corpus is deep enough that the stable prefix is a real
  signal.
- `test_seed_corpus_query_set_non_trivial`: the query set issues > 1 distinct retrieval/briefing
  query so the ranking is exercised, not a single trivial hit.

### Seed writes CONTENT only, never compared outputs (R-15 scenarios 1–2, AC-01, #5285)
- `test_seed_writes_via_context_store_path_only`: assert the seed phase writes corpus CONTENT via
  the normal `context_store` tool-call path — NOT SQL/struct injection (couples to R-06 identical-
  corpus-both-legs and R-15).
- `test_seed_does_not_set_topic_signal`: assert no seed call sets `topic_signal` or any compared
  OUTPUT (MetricVector fields, edge IDs, topic_signal) — `topic_signal` stays DERIVED over the
  wire (nan-021 ADR-004 #5289 / #5285). The forbidden seed sites
  (`_seed_observation_sql_lifecycle`, `_seed_attributed_observations_832`,
  `make_stamped_event(...,topic_signal)`) remain unreachable.

### No-seed static audit extended to ALL net-new modules (R-15 scenario 1, AC-01)
- `test_assert_no_seed_reachable_covers_all_net_new_modules`: `assert_no_seed_reachable` is invoked
  over EVERY net-new module (`parity_dimensions.py`, `parity_comparator.py`, `ranking_tolerance.py`,
  `parity_outcome.py`, `transport_health.py`) AND the seed-corpus loader AND the extended
  `parity_legs.py`/`parity_workload.py` — assert the forbidden seed sites are unreachable from each.
- `test_forbidden_seed_sites_referenced_from_one_definition`: `FORBIDDEN_SEED_SITES` is referenced
  from the single definition (pairs with `parity_comparator.FORBIDDEN_SEED_SITES`); assert no
  module carries a private copy (grep/import-graph assertion). R-05/R-15 single-source.

### load_https_bundle — token-guarded, never-empty ingest (R-09 scenarios 1–2, R-12, AC-08)
- `test_load_https_bundle_missing_capture_key_raises_infra`: a bundle missing ANY required
  `capture_key` → `InfraError`, never a partial pass (R-09 scenario 1).
- `test_load_https_bundle_null_capture_non_precompact_raises_infra`: a `null` capture for a
  non-PreCompact dimension → `InfraError` (only D5 `restored_payload` may be null, and only with
  `measurable=False`). R-09 scenario 2.
- `test_load_https_bundle_stale_token_raises_infra`: a bundle whose `run_token` != expected →
  `InfraError` (the nan-021 stale-token guard generalized). **R-12 — phantom-data guard.**
- `test_load_https_bundle_empty_or_truncated_raises_infra`: an empty / truncated / malformed-JSON
  bundle → `InfraError`, never partial-parse into an empty-pass (Security Risks: deserialization).
- `test_load_https_bundle_well_formed_returns_dimension_bundle`: a well-formed bundle with the
  correct run_token returns the `dimension_bundle` dict intact.

### Consumed-verbatim regression (ensure augmentation didn't break nan-021 surfaces)
- The existing `test_parity_workload.py` MetricVector/barrier/comparator tests MUST still pass
  unchanged (R-14 family) — assert no regression to `durability_barrier`/`observe_count`/
  `compare_metric_vectors` from the augmentation.

## Coverage Requirement
One manifest/identity/token/barrier preserved post-augmentation, asserted structurally (R-13);
the corpus produces a non-degenerate ranking depth ≥ N > 1 (R-06); the seed writes content only
via `context_store`, never a compared output, and the no-seed static guard covers EVERY net-new
module + the seed loader (R-15); `load_https_bundle` rejects missing/null/empty/stale-token
bundles as INFRA-ERROR, never a vacuous pass (R-09, R-12).
