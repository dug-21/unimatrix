# Test Plan: Cross-Language Bundle Contract (R-09)

Covers **R-09 (High)** — the dimension bundle is a real cross-language contract: JS/shell
(`cloud_cycle_gates`, `bridge-cycle-driver.js`) EMIT `{run_token, dimension_bundle:{...}}`;
Python INGESTS it. A key typo, missing dimension key, or `null` where a dict is expected yields
a `None`/absent capture that — unguarded — reads as empty-equals-empty PARITY-PASS. This is the
highest-value integration seam (Integration Risks §). The Python-side ingest guards live in
`parity_workload.md` (`load_https_bundle`); this plan owns the **schema round-trip** that both
sides must satisfy.

This is a cross-cutting plan — no single module owns it. File: `suites/test_parity_bundle_contract.py`.

Tier: **A (off-Docker)** for the Python-side schema round-trip + fixture bundle; the live JS-emit
half is in `test_https_uds_parity.md` (Tier C).

## The contract (the on-disk schema both sides emit identically)

```
{"run_token": str,
 "dimension_bundle": {
   "retrieval":  {"queries":[{"tool","args","result_ids","scores"}...], "capture_2":[...]},
   "behavioral": {"topic_signals":[...]},
   "analytics":  {"metric_vector":{...}, "informs_edges":[...], "phase_signal":{...}},
   "proactive":  {"briefing_ids":[...], "briefing_scores":[...], "injection_set":[...], "capture_2":{...}},
   "precompact": {"restored_payload":{...}|null, "measurable":bool, "host_side_gap":str|null},
   "isolation":  {"slug_a_writes_visible_to_b":bool, "landed_only_in_a":bool}}}
```
Only `precompact.restored_payload` may be null (and only with `measurable=False`).

## Unit Test Expectations

### Schema round-trip both directions (R-09 scenario 3)
- `test_bundle_fixture_round_trips_through_python_ingest`: a golden fixture bundle matching the
  documented schema round-trips through `load_https_bundle` and EVERY dimension's comparator
  without a `KeyError`/shape error. The fixture is the single canonical schema artifact both the
  off-Docker test and the live JS emit assert against.
- `test_bundle_capture_keys_match_registry_capture_keys`: the fixture's `dimension_bundle` keys
  EXACTLY equal the registry `capture_key`s — no orphan key, no unhandled dimension (couples to
  the drift guard's capture_key↔schema check in `parity_comparator.md`).
- `test_each_capture_shape_matches_documented_shape`: parametrized per dimension — assert each
  capture entry carries the documented sub-keys (e.g. retrieval has `queries`+`capture_2`,
  isolation has both booleans, precompact has `measurable`+`host_side_gap`).

### intra-check dimensions carry capture_2 (couples to R-07)
- `test_intra_check_dimensions_carry_capture_2`: retrieval + proactive captures include the
  `capture_2` double-capture field; a missing `capture_2` on an intra-check dimension → the
  classifier cannot run the intra-stability check → INFRA-ERROR, not a half-classify.

### Malformed / partial bundle → INFRA, never partial-parse (Security Risks: deserialization)
- `test_malformed_bundle_errors_not_partial_pass`: a truncated/garbage bundle is rejected (INFRA),
  never partial-parsed into an empty-pass (delegates to `load_https_bundle`; asserted here at the
  schema-contract level for completeness).

## Live half (Tier C — in test_https_uds_parity.md)
- The JS/shell-emitted bundle for a real Docker run satisfies the SAME schema this off-Docker
  contract test asserts (R-09 scenario 4) — proving the cross-language emit matches the ingest.

## Coverage Requirement (from R-09)
Every required capture_key present and non-empty or INFRA-ERROR; the on-disk bundle schema is
contract-tested both off-Docker (Python ingest + golden fixture) and live (JS emit); the
bundle keys exactly match the registry capture_keys; only D5 may carry a justified null.
