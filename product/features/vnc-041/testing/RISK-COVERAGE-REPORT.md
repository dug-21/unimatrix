# Risk Coverage Report: vnc-041

> Config seeding (global (a) + per-slug (b)) + seam-level WARN for global-locked keys.
> Capability C17 (Unimatrix #5214). Feature B of the vnc-040 split. GH #801.
> Crate under test: `unimatrix-server` (binary crate). Branch: `feature/vnc-041`.
> Sources: RISK-TEST-STRATEGY.md (R-01..R-14), ACCEPTANCE-MAP.md (AC-01..AC-06),
> test-plan/OVERVIEW.md (§5 integration harness plan), USAGE-PROTOCOL.md (triage).

## Execution Summary

| Layer | Command | Result |
|-------|---------|--------|
| Lib unit + in-crate integration | `cargo test -p unimatrix-server --lib` | 4280 passed, 0 failed, 1 ignored |
| Bin-target tests (main/per-slug-loop/global-serve-seed) | `cargo test -p unimatrix-server --bins` | 123 passed, 0 failed |
| infra-001 smoke (MANDATORY gate) | `pytest suites/ -m smoke --timeout=60` | 24 passed, 0 failed |
| infra-001 protocol (regression net) | `pytest suites/test_protocol.py` | all passed (in 87-pass protocol+lifecycle run) |
| infra-001 lifecycle (regression net) | `pytest suites/test_lifecycle.py` | passed; 5 xfailed, 2 xpassed (all PRE-EXISTING markers) |

Workspace note: `cargo test --workspace` is NOT used — it OOMs (signal 9, host memory)
while LINKING the `import_integration` test binary. Per the spawn instruction, the server
crate was tested per-crate (`--lib` + `--bins`), which is the load-bearing scope for this
feature (all five components + their tests live in `unimatrix-server`).

## Coverage Summary (R-01 .. R-14)

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Global seed fires on local serve path (wrong/missing `http.enabled` gate) | `global_serve_seed_tests::test_serve_seed_fires_with_http_enabled_and_base_dir_none`, `test_local_serve_writes_zero_new_config_files`, `test_seed_call_is_inside_http_enabled_block`, `test_only_one_serve_time_seed_call_site_for_a` | PASS | Full |
| R-02 | AC-06 sentinel green by reasoning not empirics | `test_local_serve_writes_zero_new_config_files` (delta==0), `test_container_serve_writes_one_config_file_negative_control` (delta>0), `test_local_serve_resolution_behavior_matches_pre_vnc041_baseline` | PASS | Full |
| R-03 | Seed write not atomic-no-clobber (clobbers operator config) | `config::tests::test_write_if_absent_*` (6: create-absent, no-overwrite, already-exists-noop, idempotent, swallows-failure, creates-parents); `projects::tests::test_register_does_not_clobber_pre_placed_b`, `test_register_twice_does_not_overwrite_b`; `global_serve_seed_tests::test_container_serve_does_not_clobber_operator_edited_a` | PASS | Full |
| R-04 | Locked surface hand-enumerated, drifts from A's classification | `config::tests::test_render_legend_covers_every_registry_entry`, `test_render_legend_lines_keyed_on_registry_disposition`, `test_render_legend_flips_when_disposition_flips`; `slug_config_tests::test_resolve_warns_when_per_slug_sets_global_locked_key`, `test_resolve_no_warn_when_per_slug_sets_overlayable_key`, `test_resolve_warn_behavior_flips_when_disposition_flips`, `test_resolve_no_hand_enumerated_locked_list_in_warn_pass`, `test_resolve_warns_for_unknown_key` | PASS | Full |
| R-05 | Per-slug seed touches shared (a)≡(c) / wrong base | `projects::tests::test_register_writes_b_at_per_slug_data_dir_path`, `test_register_b_path_is_sibling_not_inside_path_hash_dir`, `test_register_does_not_modify_shared_a_c_file`, `test_register_seed_does_not_create_config_in_path_hash_dir`, `test_register_seeded_b_path_is_the_resolver_formula`, `test_register_seeded_b_is_resolver_loadable` | PASS | Full (empirical-resolver round-trip: see Gaps) |
| R-06 | A→B classification drift over time | `config::tests::test_render_legend_flips_when_disposition_flips`, `test_render_legend_lines_keyed_on_registry_disposition`; `slug_config_tests::test_resolve_warn_behavior_flips_when_disposition_flips`; render `match` over `OverlayDisposition` is exhaustive (compile forcing function, ADR-003) | PASS | Full |
| R-07 | WARN raw-parse adds error path / alters resolution | `slug_config_tests::test_resolve_output_identical_with_and_without_warn_path`, `test_resolve_warn_pass_does_not_add_error_on_uninspectable_file`, `test_resolve_no_file_arm_unchanged_no_warn`, `test_resolve_empty_file_no_warn` | PASS | Full |
| R-08 | WARN granularity / dedup-state scope wrong | `slug_config_tests::test_resolve_repeated_calls_same_slug_key_warns_once`, `test_resolve_two_slugs_same_locked_key_warn_per_slug` | PASS | Full |
| R-09 | Signature / match-arm ripple breaks existing tests | Pre-existing `config::tests` `write_default_config_if_absent` arms still green (create-absent / no-overwrite / force-overwrite / silent-on-fail); `main_tests.rs` `Command` arms + register call-site tests all green (lib 4280 + bins 123, 0 failed) | PASS | Full |
| R-10 | Best-effort seed failure fails the command | `config::tests::test_write_if_absent_swallows_write_failure_no_panic`; `projects::tests::test_register_seed_write_failure_does_not_fail_register`; `global_serve_seed_tests::test_container_serve_seed_failure_does_not_abort_startup` | PASS | Full |
| R-11 | Dual (a) writers (`handle_version` + serve) conflict | `global_serve_seed_tests::test_init_then_container_serve_a_written_once`, `test_serve_seed_then_version_second_caller_noops`, `test_serve_seed_second_boot_does_not_overwrite` | PASS | Full |
| R-12 | Field-less / shape-mismatched locks mis-render / panic | `config::tests::test_render_fieldless_locks_render_managed_globally_no_knob`, `test_render_does_not_panic_for_any_registry_entry`, `test_render_legend_lists_exactly_registry_dotted_keys`; `slug_config_tests::test_resolve_warns_for_table_shaped_lock_tls` | PASS | Full |
| R-13 | Per-slug seed missed on State B (re-attach) | `projects::tests::test_register_state_c_genesis_writes_b`, `test_register_state_b_reattach_writes_b`, `test_register_state_a_already_routed_errors_no_seed` | PASS | Full |
| R-14 | Seeded (b) body not resolver-loadable | `config::tests::test_render_output_parses_as_valid_toml`, `test_render_output_deserializes_same_as_bare_template`; `projects::tests::test_register_seeded_b_is_resolver_loadable`, `test_register_seeds_b_with_rendered_classification_body` | PASS | Full |

All 14 risks: PASS. 13 Full / 1 Full-with-noted-substitution (R-05 empirical-resolver round-trip — see Gaps; the substitute proof is behaviorally equivalent to the resolver).

## Test Results

### Unit + In-Crate Integration Tests (`unimatrix-server`)

- Lib target: 4280 passed, 0 failed, 1 ignored.
- Bin target: 123 passed, 0 failed.
- vnc-041-specific additions (subset of the above, all PASS):
  - C1 seed-write primitive (`config.rs` `write_if_absent`): 6
  - C2 per-slug seed renderer (`config.rs` `render_per_slug_seed_toml`): 10
  - C3 per-slug seed writer (`projects/tests.rs` F-section, incl. round-trip in-scope half): 13
  - C4 global serve-time seed (`global_serve_seed_tests.rs`): 11
  - C5 locked-key seam WARN (`http_provision/slug_config_tests.rs`, vnc-041 subset): 14
  - vnc-041 new total: 54 (all PASS).

### Pre-existing flaky (NOT a vnc-041 regression)

`eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — confirmed PASS in
isolation on this branch (`cargo test -p unimatrix-server --lib <filter>` → 1 passed), and
also passed in the full `--lib` run this session (0 failed). Pre-existing parallelism-sensitive
flake on HEAD; explicitly NOT attributed to vnc-041. No action taken.

### Integration Tests (infra-001 — regression net per OVERVIEW §5)

vnc-041 adds filesystem provisioning (two seeded `config.toml` files) + a `tracing::warn`.
It introduces NO new MCP tool, parameter, schema, or resolution/behavior change, so per the
plan no new infra-001 tests were required — these suites run purely as a no-regression net
proving the new serve-time seed write does not perturb handshake, tool discovery, or restart.

- Smoke (MANDATORY gate): 24 passed, 0 failed, 0 error.
- Protocol: all passed (within the 87-passed protocol+lifecycle run).
- Lifecycle: passed; 5 xfailed, 2 xpassed.

#### xfail / xpass triage (USAGE-PROTOCOL.md)

All 5 xfailed and 2 xpassed are in `test_lifecycle.py` and carry **PRE-EXISTING** markers
authored before vnc-041 (reasons reference CI tick-interval config and bugfix-491 — graph
edge-count visibility / background dead-knowledge deprecation timing). They are NOT caused by
vnc-041 (a filesystem-seeding + log feature with no MCP-visible surface) and were NOT added by
this feature. Per the triage protocol, pre-existing markers with existing reasons are left
as-is; no new GH Issue is filed and no marker is changed in this PR. The 2 XPASS (e.g.
`test_inferred_edge_count_unchanged_by_cosine_supports`) are pre-existing markers whose tests
incidentally pass under this run's timing — flagged here for the human's periodic xfail review
(USAGE-PROTOCOL "Human" §2), but out of scope to remove in a feature PR that did not author them.

- Total: 111 (24 smoke + 87 protocol/lifecycle)
- Passed: 111 (+2 xpassed counted as not-failing)
- Failed: 0
- xfailed: 5 (pre-existing)

## Gaps

### Deferred gap (C3 / ADR-002 / AC-02) — EMPIRICAL register→resolve_slug_config round-trip

**Status: NOT closed by an empirical bin-target test — genuinely infeasible without a
production-visibility edit. The in-scope seam proof is the behaviorally-equivalent substitute
and is sufficient. Reasoning below.**

The empirical round-trip requires calling, in one test, BOTH `ProjectRegistry::register` (to
write file (b)) AND `resolve_slug_config` (to read it back). On `feature/vnc-041`:

1. `resolve_slug_config` lives in `http_provision.rs`, declared `mod http_provision;` in
   `main.rs` ONLY — it is a **binary-target** symbol, absent from the lib crate
   (`lib.rs` has no `http_provision`). So the lib-crate test module `projects/tests.rs`
   (where the C3 register tests live, reaching the private `register`) **cannot call it**.
2. `ProjectRegistry::register` is **private** (`fn register`, `projects.rs:273`) and the
   explicit-dirs constructor `ProjectRegistry::with_dirs` is **`#[cfg(test)]`-gated and
   private** (`projects.rs:193-194`). When the lib is compiled as a dependency of the
   bin-target test build, `#[cfg(test)]` is NOT active and these symbols do not exist — so
   the bin-target test module `per_slug_loop_tests.rs` (which CAN reach `resolve_slug_config`)
   **cannot construct a registry or call `register`** over explicit temp dirs.
3. The only `pub` register entry point, `run_project_command`, routes through
   `ProjectRegistry::resolve` → `project::ensure_data_directory`, which is HOME / `--project-dir`
   keyed — i.e. it requires HOME isolation, **forbidden under Rust 2024** (the original deferral
   cause).

Closing the round-trip in the bin target would therefore require a **non-test production edit**
to `projects.rs` — widening `with_dirs` (and exposing `register`) from `#[cfg(test)]` to
`#[cfg(any(test, feature = "test-support"))]`, the crate's established cross-target test-seam
idiom (cf. `lib.rs:40`, `infra/session.rs:1056`). That exceeds the Stage-3c test-execution
mandate ("edit ONLY the test file") and would inject production API-surface churn the
delivery/review gates did not sanction. It is left for a follow-up if an empirical bin-target
round-trip is later deemed required.

**Why the in-scope proof is sufficient (behavioral equivalence, not structural reasoning):**
`resolve_slug_config`'s two load-bearing behaviors are (i) the probe-path derivation
`base_dir.join(slug.as_str()).join(PROJECT_CONFIG_NAME)` (`http_provision.rs:318`) and (ii) the
file-present arm `load_single_config → validate_config → merge_configs → validate_config`
(`http_provision.rs:341-365`). The C3 tests exercise BOTH against the **actual file `register`
wrote** (zero hand-placement):
- `test_register_seeded_b_path_is_the_resolver_formula` asserts (b) lands at the resolver's
  literal probe formula byte-for-byte.
- `test_register_seeded_b_is_resolver_loadable` reproduces the resolver's exact file-present
  arm (the same four `pub` `infra::config` calls, in the same order) on the seeded (b),
  proving it parses, per-file-validates, merges, and post-merge-validates with no error.

Additionally, `resolve_slug_config` itself is independently and empirically covered against
real on-disk (b) files by `http_provision/slug_config_tests.rs` (27 tests) and
`per_slug_loop_tests.rs` (file-present + no-file arms). The only thing an end-to-end call would
add is threading the two through one function symbol — and each is covered. No risk-coverage gap
results: R-05 / AC-02 is Full.

### Other risk-coverage gaps

None. R-01..R-14 all have passing tests (table above).

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `global_serve_seed_tests::test_serve_seed_fires_with_http_enabled_and_base_dir_none` (seed fires with `base_dir = None`, gate is `http.enabled`; (a) at `paths.data_dir.join("config.toml")` with `DEFAULT_CONFIG_TOML` knobs); `test_serve_seed_second_boot_does_not_overwrite` (no clobber on re-boot). |
| AC-02 | PASS | `projects::tests::test_register_writes_b_at_per_slug_data_dir_path` + `test_register_seeded_b_path_is_the_resolver_formula` (b at the resolver's exact path); `test_register_seeded_b_is_resolver_loadable` (resolver's file-present arm reproduced on the register-written file, no hand-placement). Empirical end-to-end resolver call infeasible without a production-visibility edit — see Gaps; substitute proof is behaviorally equivalent. |
| AC-03 | PASS | `config::tests::test_render_legend_covers_every_registry_entry`, `test_render_overlayable_keys_render_editable_line`, `test_render_locked_keys_render_managed_globally_line`, `test_render_fieldless_locks_render_managed_globally_no_knob`, and the flip test `test_render_legend_flips_when_disposition_flips` (annotation proven, not restated). |
| AC-04 | PASS | `slug_config_tests::test_resolve_warns_when_per_slug_sets_global_locked_key`, `test_resolve_warn_names_key_and_slug_not_value` (key+slug, content-free), `test_resolve_repeated_calls_same_slug_key_warns_once` (once per boot), `test_resolve_two_slugs_same_locked_key_warn_per_slug` (per-slug isolation), `test_resolve_output_identical_with_and_without_warn_path` (WARN-only), `test_resolve_warn_behavior_flips_when_disposition_flips` (derived from `is_per_slug_overlayable`). |
| AC-05 | PASS | `config::tests::test_write_if_absent_does_not_overwrite_existing_file` + `test_write_if_absent_already_exists_is_silent_noop` (create_new no-clobber, no precheck); `projects::tests::test_register_does_not_clobber_pre_placed_b`; `global_serve_seed_tests::test_container_serve_does_not_clobber_operator_edited_a`. |
| AC-06 | PASS | `global_serve_seed_tests::test_local_serve_writes_zero_new_config_files` (delta == 0 empirically), `test_container_serve_writes_one_config_file_negative_control` (delta > 0 — sentinel not trivially passing), `test_local_serve_resolution_behavior_matches_pre_vnc041_baseline`, `test_seed_call_is_inside_http_enabled_block` (structural placement). |

All six ACs: PASS.

## GH Issues Filed

None. No vnc-041-caused integration failure occurred. The infra-001 xfails/xpasses are
pre-existing markers with existing reasons (no new issue warranted).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get(5241)` — surfaced ADR-005 (#5239),
  the resolve_slug_config gotchas (#5212), and the C5 raw-toml-flatten / bin-target-test-trap
  pattern (#5241, "`cargo test --lib` returns 0 tests for http_provision — run `--bins`"),
  which directly informed running `--lib` AND `--bins` and the infeasibility analysis of the
  deferred round-trip.
- Stored: nothing novel — the bin-vs-lib target split for `http_provision` tests is already
  captured in #5241; the deferred-round-trip infeasibility (resolve_slug_config bin-only +
  register/with_dirs `#[cfg(test)]`-private, meeting only via a `test-support` cfg-widen) is a
  feature-specific instance of that already-stored pattern, not a new cross-feature lesson.
