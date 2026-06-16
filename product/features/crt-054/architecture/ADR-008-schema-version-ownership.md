## ADR-008: crt-054 Owns Only the compaction_events Table and the Next CURRENT_SCHEMA_VERSION Bump; It Does NOT Touch SUMMARY_SCHEMA_VERSION or cycle_review_index

### Context
This ADR corrects the prior crt-054 ADR-008 (#5006), which was STALE on two counts (SCOPE line 93; SR-05): it claimed crt-054 "owns `SUMMARY_SCHEMA_VERSION` 4 and DB schema v29" on `cycle_review_index`. Both are false under the re-scope:
- **#758 (merged `7aca6c44`) owns `SUMMARY_SCHEMA_VERSION = 4`** — crt-054 is no longer first mover.
- **crt-054 no longer touches `cycle_review_index` or `SUMMARY_SCHEMA_VERSION` at all** — that ownership moved entirely to crt-055, which takes `SUMMARY_SCHEMA_VERSION` 4→5.

crt-054 now adds exactly one new table (`compaction_events`, ADR-007). `CURRENT_SCHEMA_VERSION` is currently 28 (`migration.rs:22`).

### Decision
crt-054 owns only its own DB schema bump for the new table:
- **`CURRENT_SCHEMA_VERSION` next bump (28 → 29 or 30).** crt-054 and crt-055 migrate **different tables** (`compaction_events` vs `cycle_review_index`), so there is no `ALTER`-collision and merge order is free — but the two migrations MUST take **distinct sequential** version numbers. Whichever merges first is 29; the other retroactively becomes 30 (SR-04; lesson #4095 mid-flight-merge hazard; the in-flight feature updates its migration block + pinned-version test at merge — an SM coordination point).
- **crt-054 does NOT bump `SUMMARY_SCHEMA_VERSION`** (`cycle_review_index.rs:49` stays crt-055's, 4→5). #758 owns 4; crt-055 owns 5; crt-054 owns neither.
- **No `cycle_review_index` ALTER. No `store_cycle_review` change. No `CycleReviewRecord` change.**

Three-path bump hygiene for the new table (#4153): the `create_tables_if_needed` fresh-create path (`db.rs`) includes `compaction_events`; the `run_main_migrations` upgrade block adds it under an `if current_version < N` guard via `CREATE TABLE IF NOT EXISTS` (idempotent); change the pinned `assert_eq!(version, N)` test to the new version; verify the cascade-file existence (#4484). Because it is a whole new table (not an `ALTER ADD COLUMN`), no `pragma_table_info` column pre-check is required — `CREATE TABLE IF NOT EXISTS` is the idempotency guard.

### Consequences
Easier: the SR-04 collision risk reduces to "pick the next free sequential number at merge" because the two features touch disjoint tables; crt-054's schema footprint is one self-contained new table; no coupling to the report's memoization version.

Harder: the merge-order version handshake with crt-055 must be honored at merge time (lesson #4095) — the second-merged feature edits its migration guard + pinned test in one change or leaves a DB path on the wrong schema (#4153/#4484).

Cross-refs: ADR-007 (the table this versions), #4153 (three-path bump), #4484 (cascade-file existence), #4095 (mid-flight merge hazard), crt-047 v23→v24 (the migration template), crt-055 SCOPE §migration (the `SUMMARY_SCHEMA_VERSION` 4→5 owner). Corrects prior ADR-008 (#5006): crt-054 owns neither `SUMMARY_SCHEMA_VERSION` nor any `cycle_review_index` migration.
