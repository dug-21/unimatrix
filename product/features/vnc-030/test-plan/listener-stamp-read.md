# Test Plan — C6 `uds/listener.rs` stamp-read + topic_source + close inversion

Source: ADR-004, ADR-005. ACs: AC-04, AC-05. Risks: R-01, R-04, R-05, R-09, R-12, R-17. File: extend `crates/unimatrix-server/src/uds/listener.rs` integration tests + infra-001 `test_lifecycle.py` (MCP-visible). `cargo test -p unimatrix-server`.

Three record sites: single `~:719`, single `~:861`, batch `~:1042`. Both listener-local INSERTs (`:3015`, `:3055`) gain `topic_source` as `?10`. Close-path inversion flip `~:1950-1978`. One shared `apply_stamp_to_row`-style helper across all three sites (ADR-003 mandate).

## Three-site round-trip (R-01 — CRITICAL, see also seam-and-roundtrip.md §2)

Asserted **per site independently** (#3486 lesson — field-exists is insufficient):

### stamp_read_site_a_records_declared (~:719)
- Stamped `ImplantEvent{cycle_stamp{topic,phase}}` → row `topic_signal == topic`, `phase == stamp.phase`, `topic_source == 'declared'`; `apply_stamp` set `Declared`; `record_topic_signal` tally NOT grown; `enrich_topic_signal` SKIPPED (FR-14).

### stamp_read_site_b_records_declared (~:861)
- Same assertions through the second single site, independently.

### stamp_read_batch_n_declared_rows (~:1042, R-06)
- A `RecordEvents` batch of N stamped events → N rows each `topic_source == 'declared'`.

### unstamped_frame_legacy_chain_all_sites (negative)
- `cycle_stamp: None` through all three → legacy chain, NOT `declared`.

## Stamp attribution regardless of registry state (FR-14)

### stamped_event_declared_even_with_empty_registry
- Stamped event against an empty/post-restart registry → still lands `declared` (the stamp, not the registry, is the source of truth post-restart). `apply_stamp` covers the no-post-restart-cycle_start case.

### stamped_event_skips_enrich_and_vote_tally (R-05 server side, FR-14)
- A stamped event does NOT feed `enrich_topic_signal` or `record_topic_signal` — assert the vote tally does **not** grow on stamped traffic. (The client strips `topic_signal`; the server must also skip — the client↔server contract boundary.)

## Close-path inversion flip (R-04, FR-17, ~:1950-1978)

### close_declared_beats_contradicting_vote
- `process_session_close`: snapshot `feature_source` via the existing `get_state` capture (:1892); a `Declared`-and-present session short-circuits vote and content fallback → declared wins over a contradicting vote (mirror of the sweep fix).

### close_inferred_session_uses_vote_then_content_then_registry (floor preserved, R-09)
- An `Inferred` session → vote → content fallback → registry feature, today's order (NULL-gated). No never-declare regression.

### close_inversion_minimal_diff (R-18, gate diff review)
- One short-circuit before the existing vote chain, nothing else; zero changes to adjacent crt-052 functions.

## topic_source per write site (R-12, AC-05, FR-21 — one source per value)

One integration case per value asserting the column matches the writing code path:

### topic_source_declared_from_stamp_path (FR-14)
### topic_source_extracted_from_unstamped_with_signal
- Unstamped event arriving with `topic_signal` set → row `topic_source == 'extracted'`.
### topic_source_registry_fill_from_enrich
- `enrich_topic_signal` NULL-fill from registry state → `topic_source == 'registry-fill'`.
### topic_source_vote_from_vote_path (ties OQ-A)
- Per FR-21/OQ-A: assert at the exact code site whether any path writes vote-derived attribution at row level, or record that `vote` rows are reachable only via the `Inferred(Voted)` registry-fill path. **Delivery resolves OQ-A**; the test asserts whichever the resolution pins (one source per write site holds either way).
### topic_source_null_when_unattributed
- No mechanism attributed the row → `topic_source IS NULL`.

### both_local_inserts_carry_topic_source (R-12)
- Both `:3015` and `:3055` INSERTs include `?10`. Grep-audit `INSERT INTO observations` confirms every record-path site extended; non-record-path sites (store-crate `insert_observation`, analytics/export/background) stay NULL-source by design (ADR-005 §4). (Delivery performs the grep-audit.)

## Never-declare floor (R-09, FR-19)

### never_declare_session_attributes_as_today
- No tracker/stamp → extraction → fill/vote attributes exactly as today; rows carry `extracted`/`registry-fill`/`vote`/NULL. No heuristic deleted; demotion changes precedence order only.

## Security — topic content (CRITICAL)

### topic_with_sql_metachars_binds_parameterized
- A stamped topic containing SQL metacharacters lands as a **literal** column value via the parameterized `?10` bind — never string interpolation.

## Coverage requirement
One assertion per record site that the stamp was read AND applied; the shared helper exercised by all three; one `topic_source` integration case per value; the source value computed by the same decision tree that sets `topic_signal` (source/signal cannot disagree); grep-audit at delivery; close inversion minimal-diff; topic binds parameterized.

## Open question for delivery
**OQ-A** — pin the `topic_source='vote'` row-level write site (or record `vote` is reachable only via `Inferred(Voted)` registry-fill) before closing Gate 3; confirm FR-21's one-source-per-write-site rule holds.
