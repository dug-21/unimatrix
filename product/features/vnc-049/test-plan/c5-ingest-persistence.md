# Test Plan — C5 Ingest Attribution Persistence (schema + write path)

The attribution spine and the home of the two authoritative non-proxy assertions (**AC-03**,
**AC-06 GATING**). Surfaces:
- `crates/unimatrix-store/src/db.rs` — CREATE gains `source_domain TEXT`, `model_id TEXT` (nullable).
- `crates/unimatrix-store/src/migration.rs` — additive ALTER for both columns; bump
  `CURRENT_SCHEMA_VERSION` (reads **31** @:26 today → next **32**; resolve live, do NOT hardcode).
- `crates/unimatrix-server/src/uds/listener.rs` — bind `source_domain`, `model_id` at
  `insert_observation` (:3383) and `insert_observations_batch` (:3416); source fields via
  `extract_observation_fields` (:3226). Over-cap file — minimal wiring only (ADR-005).

Risks owned: **R-01 (Critical, GATING), R-02 (Critical), R-04 (Critical), R-05 (Low, resolved),
R-17**. ACs: AC-03, AC-06, AC-02c (stored provider), contributes AC-01.

Test surface: Rust. Migration tests are external, one file per version step under
`crates/unimatrix-store/tests/` (clone `migration_v30_to_v31.rs` → `migration_v31_to_v32.rs`). The
assembled dispatch→insert→SELECT-readback tests go in `listener.rs mod tests` (:3456) following the
`listener/tests/foreign_domain.rs` pattern (`poll_exactly_one_row` :81, `test_foreign_pack_event_
stored_raw_and_read_resolves_to_sre` :117) — the one place that already does event→store→read-back.
NOTE: `dispatch_record_event_returns_ack` (:3767) stops at `Ack` and does NOT read back — the new
tests must SELECT the stored row.

## R-01 — AC-06 local-vs-cloud distinctness (GATING, authoritative, non-proxy)

The C18 `proven` blocker. Drive the REAL assembled path and assert on the QUERIED row.

- `test_opencode_local_model_event_stored_and_queried_distinct_from_cloud` — dispatch a
  RecordEvent for an opencode LOCAL-model event (`model_id="qwen3-coder"`, provider opencode)
  through `dispatch_request` → `insert_observation`; separately dispatch an opencode CLOUD-model
  event (a hosted `model_id`); read BOTH rows back (via the `foreign_domain.rs` read-back helper /
  `parse_observation_rows`); assert the two stored rows are distinguishable on `model_id`.
  **Distinctness asserted on the queried row, NOT on the in-memory `ImplantEvent`.**
- `test_opencode_model_id_dropped_between_wire_and_insert_fails` (anti-tautology negative,
  MANDATORY) — a variant that mutates the insert to drop/homogenize/NULL `model_id` makes the
  distinctness assertion FAIL. Proves the test is sensitive to the Wire→INSERT drop, not vacuous
  (#4177/#4876). Concretely: the positive test must fail if the `model_id` bind at :3383/:3416 is
  removed.
- `test_two_local_models_mutually_distinguishable` — `qwen3-coder` vs `llama3` stored rows are
  mutually distinct on query (R-01.3).

Forbidden as discharge (per ACCEPTANCE-MAP + #907/#918/#930): asserting `event.model_id.is_some()`,
asserting "the `--model` flag was passed", or seeding the row via a test-only INSERT helper. The
proof must flow through the production `insert_observation`/`_batch` and be read back.

## R-02 — AC-03 source_domain stored-record + mandatory negative (non-proxy)

- `test_opencode_event_stored_source_domain_is_opencode` — dispatch a real opencode RecordEvent
  through the assembled path; SELECT the stored row; assert `source_domain == "opencode"`.
- `test_opencode_event_stored_source_domain_not_claude_code` (MANDATORY negative, per AC-03) —
  same stored row asserts `source_domain != "claude-code"`. A test that stops at "`--provider
  opencode` was passed" is insufficient.
- **OQ-4 site pinning (#5427 per-site behavioral matrix, not a call-count).** Pin WHICH derivation
  site produces the stored value. Two candidate sites: listener Site A
  (`provider.unwrap_or("claude-code")`, #4306) vs `DomainPackRegistry` DEFAULT on the hook path
  (`DEFAULT_HOOK_SOURCE_DOMAIN` @observation.rs:572, consumed at listener.rs:2401 &
  background.rs:1662). Test `test_source_domain_produced_at_pinned_ingest_site` asserts the stored
  `source_domain="opencode"` originates at the ADR-001 provider-first ingest site, and — the
  behavioral-matrix requirement — that an edit to the OTHER site does not change the opencode
  outcome (assert the value comes from the pinned site, keyed on an observable, not on textual call
  presence which is blind to argument threading).
- `test_opencode_unresolvable_provider_stamp_fails_loud` — an opencode event whose provider-first
  stamp cannot resolve fails LOUD (error/explicit), never silently falls to `claude-code` (SR-11).

## R-03.4 / AC-02c — stored provider
- `test_opencode_event_stored_provider_is_opencode` — the stored row (or its read-back projection)
  carries `provider="opencode"`. (Canonicalization membership is c2; this is the persisted proof.)

## R-04 — migration cascade (schema bump, #4373 checklist, #378/#4153 old-schema DBs)

Clone `migration_v30_to_v31.rs` → `migration_v31_to_v32.rs`. Apply the #4373 cascade in full:

- `test_fresh_db_creates_source_domain_and_model_id_columns` — a fresh `db.rs` CREATE yields both
  columns (mirror `test_fresh_db_creates_schema_vNN`).
- `test_v31_to_v32_migration_adds_source_domain_and_model_id` — an old-schema (v31) DB ALTER-migrates
  to include both columns (mirror `test_vNN_migration_adds_column`, uses the `column_count(store,
  "observations")` helper).
- `test_fresh_and_migrated_column_parity` — column-count parity between fresh-create and
  ALTER-migrate for `observations` (the #4373 column-count parity assertion; also update
  `sqlite_parity.rs` `test_schema_column_count` + `test_schema_version_is_N`).
- `test_v31_to_v32_migration_idempotent` — re-run adds no duplicate column (mirror
  `test_vNN_migration_idempotent`); the `pragma_table_info` pre-check guards ADD COLUMN.
- **Cascade sweep (definitive gate, #4373):** update every prior migration test's exact-version
  assertion to a `>=` predicate; update `db.rs` hardcoded `schema_version` INSERT to 32; update
  `server.rs` `assert_eq!(version, N)` sites; rename the prior file's `test_current_schema_version_is_N`
  to `_is_at_least_N`. Requirement: `grep -rn 'schema_version.*== 31' crates/` returns zero after
  the bump, and `cargo test --workspace` is green (run immediately after the bump to surface all
  cascade breaks).
- **Legacy-NULL read fallback is asserted in c6** (both fork branches) — R-04.2. This plan owns the
  write/schema side; c6 owns the read fork.

## R-05 — opencode-only stamp (RESOLVED 2026-09-15; residual assertion)

- `test_source_domain_stamp_fires_only_for_opencode` — a `provider=="opencode"` event stamps
  `source_domain="opencode"` at write; a `gemini-cli`/`codex-cli`/`claude-code` event leaves
  `source_domain` NULL at write (→ read-derived fallback). Per-provider write matrix (#5427).
- **T-SEC-12/13 stay green unchanged** — `test_parse_rows_unknown_event_type_passthrough`
  (observation.rs:1360) and `test_parse_rows_hook_path_always_claude_code` (observation.rs:1390)
  MUST re-run green with no edits. Their `DEFAULT_HOOK_SOURCE_DOMAIN == "claude-code"` assertion
  (:1383-1384) is unaffected because non-opencode rows keep the read-derived path. Requirement:
  the change does not touch these tests; if it does, it is a scope violation for R-05.

## R-15 — untrusted model_id/source_domain at the bind site
- `test_persist_rejects_model_id_violating_format` — a `model_id` violating `^[a-z0-9_-]{1,64}$`
  (over-length, bad chars) is rejected/sanitized before the parameterized bind — never reaches SQL
  raw. (Binds are already parameterized; this asserts the validation contract, not SQL-escaping.)

## R-17 — over-cap wiring non-regression
- The pre-existing listener insert/dispatch tests (incl. `dispatch_record_event_returns_ack` :3767,
  `test_attribution_fallback_*` :5061-5098) re-run green — the added binds do not regress adjacent
  untested code. `cargo test --workspace` is the backstop. No ad-hoc mid-delivery carve of
  listener.rs.

## Lesson #5670 crossing test
The R-01 `test_opencode_local_model_event_stored_and_queried_distinct_from_cloud` IS the required
client→listener→DB crossing test carrying `model_id` end to end. It must drive the production insert
path, not a test-only helper.
