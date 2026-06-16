# Test Plan — Activity-fold landing (read-before-purge, width conversion, JSON)

**Component**: `unimatrix-server/src/mcp/tools.rs` review pipeline — read `activity_snapshot()` per held session, sum, build `signal_class_counts_json`
**Risks**: R-03 (read-after-purge zeroes fold — CRITICAL), R-04 (held-route believable-zero — CRITICAL), R-09 (integer-width), R-11 (leak gate), R-12 (catalog index contract)
**ACs**: AC-07 (fold → columns), AC-08 (read-before-purge + inversion), AC-09 (silent-zero regression guard), AC-14, AC-19

> ADR-007: read `activity_snapshot()` strictly BEFORE the crt-052 `purge_cycle_transcripts`; sum across the cycle's held sessions; checked/saturating `u64/u32`→`i64`. Producer index contract (ADR-008): `class_counts[0]=error`, `[1]=refusal` by FIXED index — a producer reorder corrupts every column with no type error.

## Unit tests

### Fold → columns (AC-07)
- `test_fold_lands_bytes_and_delta` — known `ActivitySnapshot` → `transcript_bytes_total == bytes_total`, `transcript_delta_count == delta_count`.
- `test_fold_lands_class_counts_by_pinned_index` (R-12) — `transcript_error_count == class_counts[0]`, `transcript_refusal_count == class_counts[1]` by FIXED index (catalog reorder caught).
- `test_fold_sums_across_held_sessions` — multiple held sessions in one cycle → columns equal the SUM across sessions, not a single session.
- `test_signal_class_counts_json_matches_catalog` — `signal_class_counts_json` is the full `class_name → count` map serialized via a real JSON serializer (never string concat); round-trips to the same map; forward-compatible for classes beyond error/refusal (NFR-06).

### Width conversion (R-09, AC-14)
- `test_fold_width_conversion_saturates` — near-`u64::MAX` `bytes_total` / large `u32` counts → checked/saturating `i64`, never wrapped (delegates the persist-boundary contract to store_cycle_review.md).

### Leak gate (R-11, AC-19)
- `test_consumed_surface_is_metadata_only` — the consumed `ActivitySnapshot` exposes counters only; no `Display`/content serialization enters the persist path; `signal_class_counts_json` is a count map, not content bytes.

## Integration tests (MCP harness)

### Read-before-purge ordering + inversion (R-03, AC-08) — load-bearing
- `test_read_before_purge_ordering` (Rust integration, AC-08) — assert the `activity_snapshots_for_feature()` read site STRICTLY PRECEDES `purge_cycle_transcripts` in the review pipeline.
- `test_inverted_order_zeroes_columns` (R-03, AC-08) — the INVERSION test: a variant that reverses the order (purge first) yields ZEROED `transcript_*` columns. This proves the ordering assertion is load-bearing, not decorative. (Run at the Rust integration layer where call order is directly manipulable.)
- `test_cycle_review_read_before_purge_columns_nonzero` (harness, AC-08) — full review with held activity through the binary → `transcript_*` columns non-zero end-to-end (the purge does not zero them because the read precedes it).

### Held-route silent-zero regression guard (R-04, AC-09)
- `test_cycle_review_held_route_fold_nonzero` (harness, AC-09) — representative TS-client cycle with HELD activity → fold source non-empty, `transcript_bytes_total`/`_delta_count` non-zero. The #750 silent-zero class cannot recur for the held route (crt-054 ADR-001 #1 regression risk).
- `test_held_route_undeclared_does_not_zero_valid_sessions` (R-04) — a cycle with one UNDECLARED session among valid declared sessions → the valid sessions' fold is NOT zeroed by the undeclared one (per-session presence handled in the sum).

### Fold lands end-to-end (AC-07)
- `test_cycle_review_fold_lands_into_columns` (harness, AC-07) — known fold through the full pipeline → each column equals the summed snapshot field; JSON map matches catalog.

## Edge cases (from RISK-TEST-STRATEGY §Edge Cases / §Failure Modes)
- Undeclared-only cycle → transcript metrics "unavailable", never `0` (delegates rendering to fail_loud_guard.md / AC-01).
- Empty fold (no held sessions) → columns "unavailable" per-metric, distinct from a measured zero.
- `signal_class_counts_json` with a class_name from config → serialized safely via JSON serializer; round-trip integrity; class_names config-validated (bounded count, no duplicate).

## Expected behaviors / assertions summary
- Read strictly before purge; inversion zeroes columns (proves it's load-bearing).
- Fold sums across held sessions; class counts by pinned index `0=error`,`1=refusal`.
- Held-route fold non-zero for a representative cycle (silent-zero regression guard).
- Metadata-only consumed surface; no content on the persist path.
