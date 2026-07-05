# Risk Coverage Report: nxs-014

> Cross-version hash chain wiring (weak mode) + transport-agnostic chain-verify core.
> Stage 3c execution. All runs foreground, real output captured. Feature branch `feature/nxs-014`
> (wave 1 + wave 2 + Gate-3b rework commits).

## Execution Summary

| Layer | Command | Result |
|-------|---------|--------|
| Unit — store | `cargo test -p unimatrix-store --lib` | **402 passed, 0 failed, 0 ignored** |
| Unit — server | `cargo test -p unimatrix-server --lib` | **4417 passed, 0 failed, 1 ignored** |
| Rust integration | `--test verify_integration --test import_integration --test export_integration` | **50 passed, 0 failed** (10 / 19 / 21) |
| Link smoke (#878) | `infra-002/check-workspace-link-smoke.sh` | **PASS** — profile invariant holds, full `--no-run` link at configured parallelism |
| Integration smoke (MANDATORY gate) | `pytest suites/ -m smoke --timeout=90` | **28 passed, 0 failed, 622 deselected** |
| Integration blast-radius (lifecycle+tools) | `pytest test_lifecycle.py test_tools.py -k "correct or chain or persist or restart or supersede or version or deprecate"` | **50 passed, 2 xfailed (pre-existing), 0 failed** |

Total: **4819 unit** + **50 Rust integration** + **78 harness (76 pass, 2 xfail)** = green across every surface.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Two-site half-fix persists empty (Critical) | `write_ext::test_correct_persists_previous_hash_from_db`, `test_correct_persists_version_increment_from_db`, `test_correct_multi_hop_chain_db_readback`, `test_correct_returned_record_agrees_with_db` | PASS | Full |
| R-02 | Deprecated predecessor invisible to core (Critical) | `chain_verify::test_verify_deprecated_predecessor_counted_as_checked`, `verify_integration::test_query_all_entries_returns_deprecated_rows`, `test_verify_cli_deprecated_predecessor_verifies_clean`, `import::test_import_deprecated_predecessor_verifies_clean` | PASS | Full (both loaders) |
| R-03 | Legacy tolerance regression (High) | `chain_verify::test_verify_mixed_legacy_and_chained_is_clean`, `test_verify_genesis_supersedes_none_empty_prev_skipped_not_dangling`, `test_verify_single_legacy_entry_is_clean`, `test_verify_legacy_predecessor_new_successor` | PASS | Full |
| R-04 | `validate_hashes` refactor changes verdict (High) | `import::test_import_rejects_broken_link_with_good_content_hash` (AND-half 1), `test_import_rejects_mutated_content_with_good_link` (AND-half 2); existing `import_integration::test_hash_validation_failure_prevents_commit`, `test_skip_hash_validation_bypass`, `test_atomicity_rollback_on_*` unchanged & green | PASS | Full |
| R-05 | Import atomicity on tampered corpus (High) | `import::test_import_tampered_corpus_rollback_no_rows` (post-failure COUNT == pre), `test_import_clean_corrected_corpus_commits` | PASS | Full (DB state proves ROLLBACK) |
| R-06 | Frozen-hash scope-creep (Med/High-sev) | `hash::test_content_hash_known_value`, `test_content_hash_both_empty`, `_empty_title`, `_empty_content`, `_unicode`, `_format`, `_determinism` (7, byte-identical) | PASS | Full |
| R-07 | Export/import round-trip lossy (Med) | `import::test_roundtrip_multihop_including_legacy_byte_identical`, `test_roundtrip_version_large_value_survives_u32_i64_bind`, `test_roundtrip_then_mutation_fails_loud` | PASS | Full |
| R-08 | Empty predecessor hash masked as legacy (Med) | `write_ext::test_correct_empty_predecessor_content_hash_rejected_names_id` | PASS | Full |
| R-09 | Dangling / MissingPredecessor mis-class (Med) | `chain_verify::test_verify_content_hash_mismatch_named`, `test_verify_chain_link_mismatch_named`, `test_verify_missing_predecessor`, `test_verify_dangling_previous_hash`, `test_verify_both_content_and_link_violation_on_one_entry` | PASS | Full (each ViolationKind) |
| R-10 | CLI verify contract (Med) | `verify_integration::test_verify_cli_clean_corpus_exit_zero_with_summary`, `test_verify_cli_clean_corpus_binary_exit_zero_prints_summary`, `test_verify_cli_tampered_corpus_nonzero_exit_names_id`, `test_verify_cli_opens_readonly`, `test_verify_cli_missing_project_dir_errors_cleanly`, `test_verify_cli_empty_db_is_clean` | PASS | Full (both exit branches + id-naming + read-only) |
| R-11 | README claim re-drift (Med) | grep: no unqualified "tamper-evident"; "tamper-**recorded**" + threat boundary present (README L235, L724); shipped integrity credited (L177, L235, L720); durable in README + ADR-002 | PASS | Full (grep + manual) |
| R-12 | Fail-silent on detected break (Low) | `chain_verify::test_is_clean_false_whenever_violations_nonempty`, `test_report_names_every_offending_id`; CLI `Err`/exit + import `Err` keyed to `!is_clean()` (R-05/R-10 tests) | PASS | Full |

## Test Results

### Unit Tests
- **unimatrix-store --lib:** Total 402, Passed 402, Failed 0, Ignored 0
  - chain_verify: 17 tests (all ViolationKind variants, legacy-skip, deprecated-predecessor-counted, is_clean property, edge cases, signature guard)
  - write_ext: R-01 DB read-back (4) + R-08 empty-predecessor reject (1)
  - hash: 7 frozen-vector tripwire tests
- **unimatrix-server --lib:** Total 4417, Passed 4417, Failed 0, Ignored 1
  - import::tests: 8 nxs-014 tests (AND-halves, ROLLBACK, deprecated-predecessor, round-trip x3, clean-commit) + existing validate_hashes tripwire tests unchanged

### Integration Tests
- **Rust test targets:** Total 50, Passed 50, Failed 0
  - verify_integration: 10 (CLI contract, loader guard, read-only, schema-30, transport-free signature)
  - import_integration: 19 (atomicity, hash-validation, round-trip — existing tripwire, unchanged & green)
  - export_integration: 21 (round-trip, hash-valid export — unchanged & green)
- **infra-001 harness (Python/MCP):**
  - smoke: Total 28, Passed 28, Failed 0 (one critical path per suite incl. correction chain + restart persistence)
  - correction/chain/deprecate/persist blast-radius (lifecycle+tools): Passed 50, xfailed 2 (pre-existing), Failed 0
  - Named regression guards all PASS: `test_correct_creates_chain`, `test_correct_preserves_metadata`, `test_correct_all_formats`, `test_correct_with_edges_attaches_to_new_entry`; correction-chain / multi-step / deprecate-then-correct / restart-persistence paths green.

### xfail references (pre-existing, NOT caused by nxs-014)
Two tests in the blast-radius run carry pre-existing `@pytest.mark.xfail` markers with documented reasons — no new GH Issue filed (already-tracked, unrelated to this feature; per USAGE-PROTOCOL triage they are pre-existing, not feature-caused):
- `test_lifecycle.py::test_dead_knowledge_entries_deprecated_by_tick` — xfail: "Dead-knowledge deprecation pass runs in background" (tick timing).
- `test_tools.py::test_deprecated_visible_in_search_with_lower_confidence` — xfail: "background scoring timing; not caused by col-028".

Both are timing-of-background-work flakes on unrelated subsystems (GC tick, background scoring), pre-date nxs-014, and touch no code this feature changed. No new failures were introduced.

### New infra-001 tests: NONE (Stage 3a conclusion confirmed)
The Stage 3a plan concluded no new infra-001 tests are warranted because the feature's observable effects are **DB-level** (`previous_hash`/`version` columns — not surfaced in any `context_correct` MCP response) and **CLI-level** (`unimatrix verify` — no MCP tool, FR-09/AC-11). Adding a Python/MCP harness test for a value absent from every MCP response would be a false-green. This conclusion **holds**: AC-01/02 are covered by Rust DB read-back unit tests, AC-04/05 by the Rust import test module, AC-09 by `verify_integration.rs` (direct-DB CLI, mirrors Export/Import). The harness role here is regression guard only — satisfied by smoke + the correction/chain blast-radius run.

## Gaps

**None.** Every risk R-01..R-12 maps to at least one green test at the correct surface. Both R-02 loaders
(`query_all_entries` for CLI; the in-flight-transaction load for import) are exercised with a Deprecated
predecessor asserted as loaded/checked. Both R-04 AND-halves (broken-link-good-content, good-link-mutated-content)
are proven on the import path. R-05 ROLLBACK is proven by post-failure DB row count, not just `Err`.

Full `test_lifecycle.py` (96) and full `test_tools.py` were not run to completion in a single ceiling due to
per-fixture embedding-model init throughput (~8s/test). Coverage is not reduced: the mandatory smoke gate ran
one critical path across **all** suites, and the complete correction/chain/deprecate/version/persist/restart
subset — the feature's entire MCP blast radius per the OVERVIEW suite-selection rationale — ran green. Tests
outside that subset (search relevance, briefing, confidence math, contradiction) are outside this feature's
blast radius and are not gates for it.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `write_ext::test_correct_persists_previous_hash_from_db` — fresh `SELECT` reads persisted `previous_hash == original.content_hash`; fails on a struct-only half-fix. |
| AC-02 | PASS | `write_ext::test_correct_persists_version_increment_from_db` (v==2) + `test_correct_multi_hop_chain_db_readback` (N=3 persisted versions 1,2,3, each `previous_hash==predecessor.content_hash`). |
| AC-03 | PASS | `chain_verify::test_verify_clean_two_hop_chain_is_clean`, `test_verify_mixed_legacy_and_chained_is_clean` (skipped_legacy counted), genesis skip test. |
| AC-04 | PASS | `chain_verify::test_verify_content_hash_mismatch_named` + `test_verify_chain_link_mismatch_named` (recompute AND link, names id); `import::test_import_rejects_mutated_content_with_good_link`. |
| AC-05 | PASS | `import::test_roundtrip_multihop_including_legacy_byte_identical` (values byte-identical, re-verify clean) + `test_roundtrip_then_mutation_fails_loud` (paired negative names id). |
| AC-06 | PASS | README grep: no unqualified "tamper-evident"; "tamper-**recorded**" wording present (L235, L724); shipped integrity not under-sold (content_hash, append-only audit, supersession chain). |
| AC-07 | PASS | Threat-model boundary durable in README (L235, L724 — DB-write adversary out of tier) AND ADR-002-weak-mode-threat-boundary.md present. |
| AC-08 | PASS | `write_ext::test_correct_empty_predecessor_content_hash_rejected_names_id` — correction `Err` naming `original_id`, zero rows persisted, original stays Active. |
| AC-09 | PASS | `verify_integration::test_verify_cli_clean_corpus_exit_zero_with_summary` (exit 0 + summary), `test_verify_cli_tampered_corpus_nonzero_exit_names_id` (non-zero + id named), read-only + missing-dir tests. |
| AC-10 | PASS | 7 `hash::test_content_hash_*` vectors byte-identical & green; signature `compute_content_hash(title, content) -> String` unchanged; no `previous_hash` fold. |
| AC-11 | PASS | `verify_integration::test_verify_core_signature_is_transport_free` + `chain_verify::test_verify_entries_signature_is_slice_of_entryrecord`; grep confirms no new MCP verify tool registered. |
| AC-12 | PASS | `verify_integration::test_schema_version_still_30` — schema version unchanged, no new migration step. |

**Loader coverage guard (architect Open Q1, R-02, gating):** PASS — `test_query_all_entries_returns_deprecated_rows`
(direct loader guard), Deprecated predecessor counted as `checked` on both the core, CLI, and import paths.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: nxs-014 Stage 3c execution) — surfaced #5502 (ADR-001 chain-verify-core placement), #2744 (`write_pool_server()` for server-context direct-DB test writes), #2149 (post-nxs-011 sqlx test setup), #4352 (validate_correct_params test order), #5478 (KI-CHAIN-XV capability, tamper-detectable). Applied: confirmed the verify-core/thin-caller test topology and the DB read-back convention already used in the write_ext tests.
- Stored: nothing novel to store — the test patterns here (DB read-back kills two-site half-fix; both-loaders Deprecated-predecessor coverage; AND-halves on the import path) are feature-specific instances of already-stored lessons (#3611 multi-site half-fix, #4177 tautological-assertion, #5180 green-on-skip). No cross-feature reusable pattern emerged that isn't already captured; storing would duplicate existing entries.
