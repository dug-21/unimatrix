# Agent Report — vnc-049 C5: Ingest Attribution Persistence

**Agent:** vnc-049-agent-3-c5-ingest-persistence (uni-rust-dev)
**Component:** C5 — ingest attribution persistence (schema + write path)
**Feature:** vnc-049 (#986) | **Commit:** 12df44ac | **Branch:** feature/vnc-049

## Summary

Persist `source_domain` and `model_id` at ingest so a stored OpenCode record reads back
`source_domain="opencode"` and a queryable `model_id`, instead of the read-derived
`claude-code` default. This is the AC-03/AC-06 mechanism (ADR-001 provider-first
opencode-only stamp, fail-loud; ADR-002 model_id column). All technical checks green.

## Files Modified

- `crates/unimatrix-store/src/db.rs` — `observations` CREATE gains `source_domain TEXT`,
  `model_id TEXT` (nullable, after `topic_source`).
- `crates/unimatrix-store/src/migration.rs` — `CURRENT_SCHEMA_VERSION` 31→32; intra-stamped
  the now-prior v30→v31 block (#5052); added v31→v32 additive ALTER block — both
  `pragma_table_info` pre-checks run before either ALTER (#4092 multi-column rule), wrapped
  in an `observations`-table-existence guard, no backfill (legacy rows stay NULL, R-05).
- `crates/unimatrix-server/src/uds/listener.rs` — write-path `ObservationRow` gains
  `source_domain`/`model_id`; new `derive_source_domain` + `resolve_model_id` helpers;
  populated at `extract_observation_fields` (pinned ingest site) with fail-loud
  `debug_assert`; both `insert_observation` (:3383) and `insert_observations_batch` (:3416)
  INSERTs extended to `?11`/`?12` (parameterized binds only); context-search query row and
  the `blank_row` test helper set both to `None`; registered `mod opencode_persistence`.
- `crates/unimatrix-server/src/uds/listener/tests/opencode_persistence.rs` — NEW test
  submodule (child of `listener::tests`, shares its dispatch helpers).
- `crates/unimatrix-store/tests/migration_v31_to_v32.rs` — NEW R-04 cascade test file
  (cloned from `migration_v30_to_v31.rs`).
- `crates/unimatrix-store/tests/migration_v30_to_v31.rs` — prior-version exact `== 31`
  assertions relaxed to `>= 31` (three sites; #4373 cascade).
- `crates/unimatrix-store/tests/sqlite_parity.rs` — `test_schema_version_is_31` →
  `_is_32`, asserts 32.

## Tests (all green, 0 failures)

- **AC-03 (R-02):** `test_opencode_event_stored_source_domain_is_opencode` — real assembled
  dispatch→insert→SELECT; stored `source_domain == "opencode"` AND mandatory negative
  `!= "claude-code"`. PASS.
- **AC-06 GATING (R-01, authoritative, non-proxy):**
  `test_local_vs_cloud_model_distinct_on_queried_row` (queried rows distinguishable on
  `model_id`, local `ollama/qwen3-coder` vs cloud `anthropic/claude-3.5-sonnet`),
  `test_two_local_models_mutually_distinguishable`, and anti-tautology
  `test_opencode_model_id_survives_wire_to_insert` (asserts queried `model_id` equals the
  wire value → fails if the `?12` bind is dropped/NULLed). All assert on the QUERIED row,
  not the in-memory `ImplantEvent`; flow through production `insert_observation`. PASS. This
  is the lesson #5670 client→listener→DB `model_id` crossing test.
- **R-05 opencode-only:** `test_source_domain_stamp_fires_only_for_opencode` —
  claude-code/gemini-cli/codex-cli/absent-provider all leave `source_domain` NULL at write.
  PASS.
- **R-15 charset:** `test_persist_rejects_model_id_violating_format` — illegal chars and
  over-length (>128) drop to NULL, never raw to SQL. PASS.
- **ADR-001 fail-loud:** `test_derive_source_domain_never_silently_defaults_to_claude_code`
  — opencode → `Some("opencode")` (never None, never claude-code); non-opencode → None. PASS.
- **T-SEC-12/13 UNCHANGED:** `test_parse_rows_hook_path_always_claude_code` and
  `test_parse_rows_unknown_event_type_passthrough` re-ran green with NO edits (their
  `DEFAULT_HOOK_SOURCE_DOMAIN == "claude-code"` assertion is unaffected — non-opencode rows
  keep the read-derived path).
- **Migration cascade (`migration_v31_to_v32`, 6/6):** fresh-create has both columns;
  v31→v32 ALTER adds both; fresh-vs-migrated column parity; idempotent re-run (no double
  column); populated v31 data intact (legacy row → both columns NULL).

Run counts: store `migration_v31_to_v32` 6/6, `migration_v30_to_v31` 8/8, `sqlite_parity`
59/59; server `opencode_persistence` 7/7, full `uds::listener` 278/278,
`services::observation` 41/41. `cargo build -p unimatrix-store -p unimatrix-server` exit 0.
Clippy clean on all touched files (remaining `-D warnings` hits are pre-existing in
`transcript_hold.rs`/`verbosity.rs`/`main.rs`, outside C5 scope).

## Open Question Resolutions

- **Resolved schema version: 32** — live-resolved against `CURRENT_SCHEMA_VERSION`=31 today;
  registered v31→v32 block + bumped constant. Not hardcoded.
- **OQ-4 (derivation-site pin):** the stored opencode `source_domain` originates at the
  **write path** — `derive_source_domain` invoked in `extract_observation_fields`
  (listener.rs), the ADR-001 provider-first ingest site. It returns only `Some("opencode")`
  or `None`; it can never emit `"claude-code"` (fail-loud by construction, guarded by a
  `debug_assert`). `resolve_source_domain` (C7) remains authoritative ONLY for legacy NULL
  rows. Pinned by `test_derive_source_domain_never_silently_defaults_to_claude_code` so an
  edit to the read site cannot reintroduce the default for opencode rows.
- **OQ-A (which `ObservationRow`):** the **write-path** struct in `listener.rs` (fields
  `topic_signal`/`phase`/`topic_source`, bound at :3383/:3416) gains `source_domain`/
  `model_id`. Confirmed distinct from the read struct in `observations.rs:14` and from
  `format.rs`'s public `ObservationRow`. Constructed from `ImplantEvent` in
  `extract_observation_fields`; the only other two constructors (context-search query row,
  `blank_row` test helper) are not provider ingest events and set both fields to `None`.

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_briefing` + two `context_search` calls +
  `context_get` — surfaced ADR-001 (#5748), ADR-002 (#5739), migration-cascade checklist
  (#4373), idempotent ALTER guard (#4092), and lesson #5670 (hook-frame field must cross
  client→listener→DB). All applied: pragma pre-checks before ALTER, `is_valid_model_id`
  IMPORTED from wire.rs (not redefined), crossing test through production insert, `>=`
  cascade sweep on prior migration test.
- **Stored:** entry **#5752** "ALTER-COLUMN migration block needs a table-existence guard or
  minimal forward-only test fixtures fail 'no such table'" via `context_store` (category
  pattern, topic unimatrix-store). Novel gotcha that bit this session: the v31→v32 block
  initially crashed the *v30→v31* test suite because that minimal fixture has no
  `observations` table; fixed with a table-existence guard (same shape as v29→v30).
