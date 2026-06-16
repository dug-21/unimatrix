# crt-055 Agent 3 — Component 2: store_cycle_review() extension

**Agent ID**: crt-055-agent-3-store_cycle_review
**Scope**: unimatrix-store — single-writer INSERT/UPDATE bind extension for the 16 v5 columns.

## 1. Files modified
- `/workspaces/unimatrix/crates/unimatrix-store/src/cycle_review_index.rs`

## 2. Tests
- 35 passed / 0 failed (lib filter `cycle_review_index`, `--features test-support`).
- 5 new tests added:
  - `test_record_roundtrip_all_v5_columns` — INSERT binds every v5 column (AC-05 store side).
  - `test_update_path_binds_all_v5_columns` — UPDATE binds every v5 column (a missing UPDATE bind leaves stale values); `first_computed_at` preserved.
  - `test_basis_points_roundtrip` (AC-20) — context_reload_pct i64 bps verbatim: 3750 / 1 / 10000.
  - `test_signal_class_counts_json_roundtrip_and_coalesce` — non-empty map round-trips; empty String coalesces to "{}" on write+read (ADR-007).
  - `test_no_clobber_store_layer_contract` — the three #5022 assertions at the store layer (AC-17): (a) honest non-zero values persist at schema v5; (b) no implicit write retains bytes; (c) force re-review with real values does not clobber v5 columns to zero.
- Existing `sqlite_parity` schema tests still green; migration_v29_to_v30 binary compiles/loads.

## 3. Issues / blockers
- None blocking.

## Implementation notes
- **What was already landed (Component 1)**: SUMMARY_SCHEMA_VERSION=5, the 16 CycleReviewRecord fields, the migration (v29→v30), db.rs fresh-create DDL, AND the `get_cycle_review()` SELECT + row-mapping (read-back) were all already present. The ONLY gap was the INSERT (~:335) and UPDATE (~:369) binds — that is what this component added.
- **INSERT**: extended column list + VALUES (?13..?28) + 16 binds in the fixed order matching the SELECT mapping.
- **UPDATE**: extended SET clause (?12..?27) + 16 binds. `first_computed_at` deliberately kept OUT of the SET clause (ADR-001, unchanged). Every v5 column is bound on UPDATE so a force re-review writes real values, never leaving the first write's defaults.
- **coalesce_json helper**: added one module-level `fn coalesce_json(&str) -> &str` ("" → "{}") used by both the INSERT/UPDATE binds for `signal_class_counts_json` and the read mapper (replaced the inline coalesce), satisfying the TEXT NOT NULL DEFAULT '{}' contract (ADR-007).
- **Basis-points clamp placement (confirm-at-impl item resolved)**: the store layer persists `context_reload_pct` as a verbatim i64. The 0–10000 clamp / fraction→bps encode (round(fraction × 10000)) lives UPSTREAM in the reckoning component (Component 4) per the pseudocode binds (§2a binds `record.context_reload_pct` directly) and the data-flow diagram. The "persist boundary" in ADR-005 = the record-build boundary, not the SQL writer. The store stays dumb — no clamp, no encode, no is_finite guard (there is no float at this layer). `test_basis_points_roundtrip` pins verbatim persistence.
- **Single-writer / no-zero-clobber discipline (ADR-002 #5037, lesson #5022)**: untouched at the store layer. The four-return discipline (memo-hit / purged-retain / force+purged DO NOT write) is enforced by the handler in unimatrix-server (Component 9), outside this component's scope. The store remains the single dumb writer — the new binds add no new write site and no new failure mode (integers cannot trip the 4MB ceiling).
- **File length**: cycle_review_index.rs is ~2470 lines (pre-existing condition; was ~2153 before this change). Splitting the module is a separate refactor out of scope for this targeted bind extension; not undertaken to avoid churn on a shared checkout mid-wave.

## Knowledge Stewardship
- Queried: context_get #4750 (four success returns), #5022 (the #750 empty-clobber three assertions), #5037 (ADR-002 single writer / no zero-clobber). context_search was unavailable under that name; used ToolSearch + context_get directly. Findings applied: store-layer binds only; four-return gating + clamp are upstream; no second writer near the memo site.
- Stored: nothing novel to store — the implementation is a mechanical bind extension that follows existing patterns (crt-047 two-step upsert, ADR-001 first_computed_at exclusion) and the already-captured #5022 lesson / #5037 ADR. The one mildly non-obvious point (basis-points clamp lives upstream, store persists verbatim) is already documented in ADR-005 (#5047) and the pseudocode; re-storing it would duplicate, not add.
