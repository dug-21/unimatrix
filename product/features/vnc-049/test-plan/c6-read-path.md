# Test Plan — C6 Read-Path Attribution (prefer stored, legacy fallback)

Surfaces:
- `crates/unimatrix-server/src/services/observation.rs` — `parse_observation_rows` (:576, `mod
  tests` :658): SELECT the new cols, PREFER stored `source_domain`, surface `model_id`;
  `DEFAULT_HOOK_SOURCE_DOMAIN` (:572) becomes legacy-row fallback ONLY.
- `crates/unimatrix-store/src/observations.rs` — SELECT new cols in `fetch_observations_since`
  (:44), `load_observations_for_sessions` (:124), `load_observation_session_stats` (:173). This
  file has **no `mod tests`** today (coverage is indirect via
  `crates/unimatrix-server/tests/project_routing_integration.rs`).

Risks owned: **R-02 (read fork), R-04.2 (legacy-NULL fallback)**. ACs: contributes AC-03, AC-06
(surfacing model_id on the queried record).

Test surface: Rust `#[cfg(test)]` in `observation.rs mod tests` (:658), plus new direct coverage for
the store SELECTs (see the `load_observation_session_stats` gap below).

## The read-path fork (R-02, R-04.2) — BOTH branches asserted

`parse_observation_rows` gains a fork: if the row's stored `source_domain` is present, PREFER it;
if NULL (legacy row), fall back to today's registry/DEFAULT resolution. Per #5427, both branches of
the fork must be exercised — a test that hits only one vacuously routes through the common path.

- `test_parse_rows_prefers_stored_source_domain_when_present` — a row with stored
  `source_domain="opencode"` reads back `source_domain="opencode"` (NOT re-derived from event_type,
  which would give the claude-code default for opencode's canonical event names). This is the
  read-side half of AC-03.
- `test_parse_rows_falls_back_to_registry_when_source_domain_null` — a legacy row with NULL
  `source_domain` reads back via the registry/`DEFAULT_HOOK_SOURCE_DOMAIN` path EXACTLY as today
  (R-04.2 legacy branch). This preserves backward compatibility for all pre-migration rows.
- `test_parse_rows_surfaces_model_id` — the queried record surfaces the stored `model_id` (the
  read-side of AC-06; the distinctness assertion itself is the c5 assembled-path test).
- `test_parse_rows_null_model_id_surfaces_none` — a row with NULL `model_id` surfaces `None`
  cleanly (cloud/legacy rows).

## T-SEC-12/13 unchanged (R-05)
- `test_parse_rows_unknown_event_type_passthrough` (:1360) and `test_parse_rows_hook_path_always_
  claude_code` (:1390) MUST re-run green, unchanged. The prefer-stored fork only changes behavior
  for rows that HAVE a stored `source_domain` (opencode-only at write); NULL rows keep the exact
  legacy path these tests assert. Requirement: these tests are not edited.

## Store-layer SELECT coverage (R-04, R-17) — closes a known gap
The three store fns must SELECT the new columns. `load_observation_session_stats` (:173) has NO
direct test today — since C6 changes its SELECT, add direct coverage:
- `test_fetch_observations_since_selects_new_columns` — the projection includes `source_domain`,
  `model_id` and returns their stored values (exercised today only indirectly via
  `project_routing_integration.rs:1719/1729` — add a direct assertion here or extend that
  integration test).
- `test_load_observations_for_sessions_selects_new_columns` — same for the sessions loader
  (currently exercised only via `services/behavioral_signals.rs:453`).
- `test_load_observation_session_stats_selects_new_columns` — NEW direct coverage for the stats
  loader (no test exists today); assert it does not break on the added columns and surfaces them
  where applicable. Adding a `mod tests` to `observations.rs` is acceptable (cumulative, not a fork).

## Edge cases
- Mixed table: some rows stored `source_domain`, some NULL — a single query returns each row
  resolved via its correct branch (no cross-contamination).
- Row with stored `source_domain` but NULL `model_id` (opencode event without a resolved backend
  model) — reads back opencode + None.

## Cross-references
- The authoritative AC-03/AC-06 assertions are the c5 assembled-path tests (dispatch→insert→SELECT).
  c6 proves the read fork in isolation; c5 proves it end-to-end from the entry point.
