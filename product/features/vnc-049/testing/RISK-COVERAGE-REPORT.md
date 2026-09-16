# Risk Coverage Report: vnc-049

OpenCode observation harness (C18) — behavioral-signal parity + local-model
attribution. Stage 3c test execution against branch `feature/vnc-049`
(schema v32; `source_domain`/`model_id` persist opencode-only at ingest).

Binary under test: `target/release/unimatrix` (fresh build, exit 0).
Integration harness: `product/test/infra-001` (compiled-binary MCP + UDS-observe).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | AC-06 GATING — local-vs-cloud model distinctness on the assembled path (not a seam) | Rust `listener.rs`: `test_local_vs_cloud_model_distinct_on_queried_row`, `test_two_local_models_mutually_distinguishable`, anti-tautology `test_opencode_model_id_survives_wire_to_insert`; **NEW** infra-001 `test_opencode_local_vs_cloud_model_distinct_on_queried_row`, `test_opencode_two_local_models_mutually_distinct` | PASS | Full |
| R-02 | Silent `source_domain=claude-code` default | Rust `test_opencode_event_stored_source_domain_is_opencode` (+neg); **NEW** infra-001 `test_opencode_event_stored_source_domain_is_opencode` (positive + mandatory negative NOT claude-code) | PASS | Full |
| R-03 | Split-brain drift (hook.rs / normalize.js / parity corpus) | C8 `parity_corpus_opencode` + `opencode-arm-goldens.json` (drift-fail); JS hook-client opencode arm (829 pass); `KNOWN_PROVIDERS∋opencode`; **NEW** infra-001 `test_opencode_provider_evidenced_by_stored_source_domain` (AC-02c) | PASS | Full |
| R-04 | Migration cascade incomplete (schema bump 31→32) | Rust `migration_v31_to_v32` (6 tests) + `sqlite_parity` (59); **cascade gap found + fixed**: stale HEAD-pin `verify_integration.rs::test_schema_version_still_31` → 32 (see Findings) | PASS | Full |
| R-05 | Existing-provider `source_domain` regression (opencode-only stamp) | Rust `test_source_domain_stamp_fires_only_for_opencode`; T-SEC-12/13 (`test_parse_rows_hook_path_always_claude_code`, `test_parse_rows_unknown_event_type_passthrough`) GREEN unchanged; **NEW** infra-001 `test_source_domain_stamp_fires_only_for_opencode` (gemini-cli → NULL, opencode → stamped) | PASS | Full |
| R-06 | Subagent orphaning / alignment (AC-04) | C1 plugin `subagent.test.js` (child session.created + parentID + validated agent + ordering) | PASS | Full (plugin+Rust; harness gap noted) |
| R-07 | `session.agent` on a spoofable channel | C1 plugin: validated `session.agent` rides `extra.agent_type`; conflicting tool-args value does NOT override | PASS | Full |
| R-08 | C10 retrieval regression on install | C9 `opencode-install.test.js`/`init.test.js` (byte-for-byte preserve, plugin provisioned); **NEW** infra-001 `test_context_retrieval_unaffected_by_attribution_columns`; protocol+tools suites green | PASS | Full |
| R-09 | Installer non-idempotence | C9 `test_append_plugin_entry_idempotent`, merge-helper idempotence tests | PASS | Full |
| R-10 | model_id carrier blast radius | Rust serde back-compat (`deserializes_without_model_id`, `skip_serializing_if` byte-stability); crossing test; **NEW** infra-001 `test_opencode_cloud_event_without_model_id_stored_null` (frame w/o model_id stores clean) | PASS | Full |
| R-11 | `session.idle`→Stop over-count | C1 plugin `stop-guard.test.js` (repeated idle → exactly one Stop per window; re-arm) | PASS | Full |
| R-12 | PreCompact experimental API | C1 plugin `precompact.test.js` (present→lands; disabled→gated no-op; hostile payload fail-safe) | PASS | Full |
| R-13 | Plugin transport fail-open | C1 plugin `failopen.test.js` (emit failure → session survives) | PASS | Full |
| R-14 | Bus-derived field synthesis | C1 plugin `events.test.js` (cwd/worktree from PluginInput; degraded honesty) | PASS | Full |
| R-15 | Untrusted model_id/source_domain input | Rust `test_persist_rejects_model_id_violating_format`; C4 charset unit; **NEW** infra-001 `test_model_id_invalid_charset_not_persisted_raw` (SQL-ish model_id dropped to NULL, table intact) | PASS | Full |
| R-16 | AC-01 false-pass on parity gap | C1 plugin `entry.test.js`/`events.test.js` (7-event map; SubagentStart injection recorded as measured parity gap, not silent drop) | PASS | Full |
| R-17 | Over-cap wiring surgery | Full `cargo test --workspace` (6307 pass, 0 fail); adjacent listener/hook/observation/db/migration tests green | PASS | Full |

## Test Results

### Unit Tests — Rust workspace
- Command: hardened `setsid -w timeout … cargo test --workspace` (file-not-pipe).
- Total: 6307 passed, 0 failed, 31 ignored (across 29 test binaries + lib tests).
- One failure on the first run — `test_schema_version_still_31` (stale HEAD-tracking
  schema-version pin) — fixed to `_32` as an R-04 cascade completion; re-run GREEN.
- Full-workspace LINK smoke (#878 guard, `infra-002/check-workspace-link-smoke.sh`):
  PASS — profile-presence OK, `--workspace --no-run` link completed at configured
  parallelism; #878 invariant holds.
- Known pre-existing parallel-only flakes (`test_ac14_correlated_sweep_non_vacuous`,
  `test_record_access_mcp_feature_recording`, `http::token…forced_interleave_converges`,
  `test_run_scenarios_does_not_write_to_snapshot`) did **not** trigger this run.

### Unit Tests — JS/TS
- C1 OpenCode plugin (`opencode-plugin/`, `node --test`): 38 passed, 0 failed.
- C9 installer (`opencode-install.test.js` + `init.test.js`): 38 passed, 0 failed.
- C3 normalize.js mirror + hook-client (`npm run test:hook-client`): 829 passed,
  1 skipped, 0 failed (includes the opencode canonicalization arm).
- JS total: 905 passed, 1 skipped, 0 failed.

### Integration Tests (infra-001, compiled binary)
- **Smoke gate (MANDATORY):** `pytest -m smoke` — 37 passed, 0 failed. Includes the
  two new opencode smoke tests (source_domain, local-vs-cloud distinctness).
- **lifecycle + security suites:** 136 passed, 0 failed, 6 xfailed, 1 xpassed
  (all xfail/xpass are pre-existing markers, e.g. GH#406 — none vnc-049).
  One transient failure on first run was my own test-assertion bug
  (`assert_search_contains` expects an entry_id, not the content string) — fixed,
  re-run GREEN. Not a feature defect.
- **tools + protocol + edge_cases (schema-migration regression sentinels):**
  264 passed, 0 failed, 2 xfailed (pre-existing). Confirms the new columns did not
  regress MCP tool responses / storage / restart persistence.
- New vnc-049 integration tests added: **8**, all GREEN (see below).

#### New integration tests (extend the GH#819 `daemon_server` + `UnimatrixHookClient` + row-count model; real UDS ingest → read-back SELECT; anti-seed #5285)
| Test | File | AC/Risk |
|------|------|---------|
| `test_opencode_event_stored_source_domain_is_opencode` | test_lifecycle.py | AC-03 / R-02 (+ mandatory negative) |
| `test_opencode_local_vs_cloud_model_distinct_on_queried_row` | test_lifecycle.py | AC-06 GATING / R-01 |
| `test_opencode_two_local_models_mutually_distinct` | test_lifecycle.py | R-01.3 |
| `test_opencode_provider_evidenced_by_stored_source_domain` | test_lifecycle.py | AC-02c / R-03.4 |
| `test_opencode_cloud_event_without_model_id_stored_null` | test_lifecycle.py | R-10.2 |
| `test_context_retrieval_unaffected_by_attribution_columns` | test_lifecycle.py | AC-05c / R-04 |
| `test_source_domain_stamp_fires_only_for_opencode` | test_security.py | R-05 |
| `test_model_id_invalid_charset_not_persisted_raw` | test_security.py | R-15 |

Harness support (cumulative — extended existing helpers, no new scaffolding):
- `harness/hook_client.py`: `record_event`/`record_post_tool_use` gained
  `provider`/`model_id` kwargs (the flattened `ImplantEvent` attribution carriers).
- `harness/assertions.py`: `read_observation_attribution(store_dir)` — SELECT
  `source_domain, model_id, tool` read-back (companion to `_observation_row_count`).

**Anti-seed compliance (#5285):** every new attribution test drives the event over
the real UDS hook wire carrying `provider`/`model_id`, exactly as
`unimatrix hook --provider opencode --model <…>` populates them; the daemon DERIVES
`source_domain` (`derive_source_domain`, opencode-only) and validates+binds
`model_id` at `insert_observation`. There is no `source_domain` wire field to seed —
attribution flows wire→INSERT and is asserted on the QUERIED stored row.

## Findings

1. **R-04 cascade gap found and closed (Stage 3c).** The 31→32 schema bump left one
   stale HEAD-tracking version pin: `crates/unimatrix-server/tests/verify_integration.rs::test_schema_version_still_31`
   (`assert_eq!(CURRENT_SCHEMA_VERSION, 31)`). Gate-3b's cascade grep
   (`schema_version.*== 31`) did not match the comma-form `assert_eq!(…, 31)`, so it
   slipped through. Fixed to `test_schema_version_still_32` / expected `32` with an
   updated HEAD-tracking comment — the correct cascade completion (#4373), test-code
   only, no production change. Re-run GREEN. All other schema pins already use `>=`
   predicates and pass at 32; migration.rs's `value = 31` references are the
   intentional intra-stamp fixture (#5052) for the v31→v32 block, not stale.

## Gaps

- **AC-04 subagent alignment / AC-07 seam — not driven end-to-end through infra-001.**
  The plugin (TS) subagent-correlation and observe-channel provenance are not
  reachable from the harness (per OVERVIEW §4.5 OQ: the harness drives the CLI→store
  segment, not the plugin→CLI segment). These are fully covered by the C1 node plugin
  tests (`subagent.test.js`: child `session.created`+`parentID`+validated agent,
  spoof rejection, ordering; 38 pass) and the Rust crossing/structural tests. No
  fabricated harness proxy was written for them (anti-proxy discipline). This is a
  known, documented harness-reach boundary, not a coverage hole in the behavior.
- No other uncovered risks: R-01..R-17 all carry passing coverage.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | C1 plugin 7-event map + parity-gap recording (`entry`/`events`/`stop-guard`/`precompact` tests); SubagentStart injection recorded as measured gap, no silent drop |
| AC-02 | PASS | `KNOWN_PROVIDERS∋opencode`; C8 parity corpus drift-fail (mutation-verified both arms); stored source_domain=opencode evidences provider (infra-001 `test_opencode_provider_evidenced_by_stored_source_domain`) |
| AC-03 | PASS | Rust `test_opencode_event_stored_source_domain_is_opencode` (+neg) AND infra-001 `test_opencode_event_stored_source_domain_is_opencode` — stored-record positive + mandatory negative (NOT claude-code), derivation site pinned (ADR-001 ingest) |
| AC-04 | PASS | C1 plugin `subagent.test.js` — child `session.created`+`parentID`+validated `session.agent` on `extra.agent_type` (observe channel), ordering, spoof rejection (harness-reach gap noted above) |
| AC-05 | PASS | C9 installer 38 tests (byte-for-byte preserve, provisioning, idempotence) + infra-001 retrieval sentinel + tools/protocol regression green |
| AC-06 | PASS | **C18 GATING satisfied** — Rust `test_local_vs_cloud_model_distinct_on_queried_row` + infra-001 `test_opencode_local_vs_cloud_model_distinct_on_queried_row` assert distinctness on the QUERIED stored row through the compiled binary; anti-tautology (exact stored values; fails on any drop/homogenize/NULL) |
| AC-07 | PASS | `build_context_with_external_identity` seam untouched/callable; validated `session.agent` not folded into spoofable tool args (C1 spoof-rejection); no enforcement built |

## GH Issues Filed
None. No genuinely pre-existing integration failures surfaced this run; the sole unit
failure was a vnc-049 cascade item (fixed in-PR, not xfailed). No new xfail markers
added. The 6 xfailed / 1 xpassed in lifecycle+security and 2 xfailed in the
regression suites are all pre-existing markers unrelated to vnc-049 (the single
xpass is a signal for that marker's owner to review, out of scope here).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: vnc-049 Stage 3c execution) —
  surfaced ADR-001 (#5748), ADR-007 (#5744), ADR-009 (#5746), hook-client UDS pattern
  (#4823); applied the anti-seed derive-don't-seed contract (#5285) and the GH#819
  row-count durability model.
- Stored: entry #5758 — "infra-001 UDS-observe attribution read-back pattern (anti-seed)"
  via context_store (topic: testing, category: pattern).
