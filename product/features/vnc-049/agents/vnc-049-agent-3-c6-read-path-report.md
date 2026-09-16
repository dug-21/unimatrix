# C6 Read-Path Attribution — Agent Report (vnc-049-agent-3-c6-read-path)

Component: **C6 — Read-path attribution (prefer stored, legacy fallback)**
ADRs: ADR-001 (opencode-only ingest stamp + prefer-stored read fork), ADR-002 (model_id carrier).
Risks: R-02 (read fork), R-04.2 (legacy-NULL fallback), R-05 (opencode-only stamp / T-SEC-12/13).
Commit: `6a0add51` — `impl(c6-read-path): prefer stored source_domain + surface model_id on read (#986)`.

## Summary

Read half of the ADR-001 fork. `parse_observation_rows` now SELECTs the persisted
`source_domain`/`model_id` columns (added by C5) and PREFERS the stored `source_domain`
when non-NULL (opencode rows stamped at ingest). NULL rows — legacy pre-vnc-049 AND every
non-opencode row — fall through to the unchanged read-derived Approach A resolution
(`resolve_source_domain` → `DEFAULT_HOOK_SOURCE_DOMAIN`), preserving hook-path behavior
byte-for-byte. `model_id` is surfaced on the queried record for AC-06 local-vs-cloud
distinctness.

## Files Modified

Core C6 surfaces:
- `crates/unimatrix-server/src/services/observation.rs` — `parse_observation_rows`: SELECT
  `source_domain`/`model_id` at all 3 sites (idx 7/8); prefer-stored fork; surface `model_id`
  on `ObservationRecord`. `DEFAULT_HOOK_SOURCE_DOMAIN` doc updated to legacy-row-fallback-only.
  Added 5 unit tests.
- `crates/unimatrix-store/src/observations.rs` — new `source_domain`/`model_id` fields on the
  read `ObservationRow`; `fetch_observations_since` + `load_observations_for_sessions` SELECT
  and bind the columns (idx 8/9). `load_observation_session_stats` is a GROUP BY aggregate
  (projects `session_id, started_at, COUNT` — no per-row observation columns), documented as
  such and given NEW direct test coverage. Net-new `#[cfg(test)] mod tests` (3 tests).
- `crates/unimatrix-core/src/observation.rs` — added `model_id: Option<String>`
  (`#[serde(default)]`) to `ObservationRecord`.

Mechanical `model_id: None` compile-fixes forced by the required core field (see Boundary note):
- `crates/unimatrix-server/src/uds/listener.rs` (1 line), `.../background.rs`,
  `.../services/behavioral_signals.rs`, `.../mcp/knowledge_reuse.rs`, `.../mcp/tools.rs`,
  and 21 `unimatrix-observe` test-helper files. ~112 exhaustive struct literals total.

## Tests

- `unimatrix-store` `observations::tests` — 3/3 pass:
  `test_fetch_observations_since_selects_new_columns`,
  `test_load_observations_for_sessions_selects_new_columns`,
  `test_load_observation_session_stats_with_attribution_columns` (new direct coverage).
- `unimatrix-server` `services::observation` — 46/46 pass, incl. new:
  `test_parse_rows_prefers_stored_source_domain_when_present` (R-02.1/AC-03),
  `test_parse_rows_falls_back_to_registry_when_source_domain_null` (R-04.2),
  `test_parse_rows_surfaces_model_id` (AC-06),
  `test_parse_rows_null_model_id_surfaces_none`,
  `test_parse_rows_mixed_stored_and_null_no_cross_contamination`.
- **T-SEC-12/13 GREEN UNCHANGED** (not edited): `test_parse_rows_unknown_event_type_passthrough`
  and `test_parse_rows_hook_path_always_claude_code` both pass. Prefer-stored only changes
  behavior for rows with a stored `source_domain` (opencode-only at write); NULL rows keep the
  exact legacy path (R-05).
- `unimatrix-observe` full suite: 656 pass. Full workspace `cargo build` clean; clippy clean on
  C6 code (only the pre-existing out-of-scope `transcript_hold.rs` warning remains).
- Unrelated flake observed: `http::token::tests::test_concurrent_creation_forced_interleave_converges`
  failed once under full `--lib`, passes in isolation — concurrency test, no relation to C6.

## Boundary Exception (flagged)

The spawn file-boundary said "do NOT touch listener.rs (C5)". The pseudocode + test plan require
`model_id` on the shared core `ObservationRecord` (surfaced by `parse_observation_rows`, asserted
by `test_parse_rows_surfaces_model_id`). That struct has all-required pub fields and no `Default`
derive, so adding one field forces a `model_id: None` compile-fix at every exhaustive literal —
including 2 production DB-read sites: `listener.rs` `content_based_attribution_fallback` (a
DB-read attribution fallback) and `background.rs` `fetch_observation_batch`. The listener.rs
change is a single `model_id: None,` line on the core `ObservationRecord` literal; it does NOT
touch C5's write-path `ObservationRow` struct, `insert_observation*`, `derive_source_domain`,
`resolve_model_id`, or the migration. Pure mechanical compile-fix, not a logic change. Flagged
for gate acceptance.

Secondary note: pseudocode said "SELECT new cols in `load_observation_session_stats`", but that
query is a GROUP BY aggregate with no per-row projection. Implemented per the test plan's more
precise "surfaces them where applicable" wording — query unchanged, documented, direct coverage
added.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` + 2× `mcp__unimatrix__context_search` — surfaced
  ADR-001 (#5748, opencode-only prefer-stored fork; opencode rows never re-derive to claude-code),
  #4304 (`resolve_source_domain` returns "unknown" for events outside the builtin pack — use with
  fallback), #4308 (vnc-013 ADR-004 Approach A registry-with-fallback contract that T-SEC-12/13
  protect). Applied directly to the read-fork implementation and the byte-for-byte legacy path.
- Stored: entry **#5756** "Three distinct ObservationRow/ObservationRecord structs — adding a
  field has cross-crate blast radius" via `mcp__unimatrix__context_store` (pattern, topic
  `unimatrix-observe`). Captures the invisible-until-compiled ~112-site blast radius, the
  three-struct distinction (core `ObservationRecord` vs store read `ObservationRow` vs listener
  write `ObservationRow`), the `#[serde(default)]` mitigation, and the read-fork correctness pin.
