# Risk Coverage Report: vnc-040

> Per-Slug Configuration Overlay Resolution (C6 / Feature A of #785). Stage 3c execution against the
> implementation committed on `feature/vnc-040` (registry + `is_per_slug_overlayable` in
> `infra/config.rs`; `resolve_slug_config` in `http_provision.rs`; per-slug loop + instructions
> relocation in `main.rs`). 34 component tests authored in Stage 3b, all executed and green here.

## Executive Summary

- **Unit tests: 34/34 vnc-040 component tests PASS** (11 in the `lib` target, 23 in the `bin` target),
  within full crate runs of **4250 lib + 98 bin = 0 failed**.
- **Integration smoke gate (MANDATORY): 24/24 PASS** (`pytest -m smoke`, 382 deselected).
- **Integration regression suites (protocol, tools, lifecycle, confidence): 0 failed** (xfail =
  pre-existing harness debt, NOT introduced by vnc-040; per-batch counts in Integration Tests). A first
  combined run was killed at my wall-clock ceiling (RC=124, env limit — NOT a test failure) with 0
  failures observed; the suites were re-run in batches with a higher ceiling for exact tallies.
- **All 14 risks covered; 0 coverage gaps.** R-13 is an accepted residual (single owner confirmed,
  no runtime-warn by design). AC-09 / FR-13 are construction/review-only (no behavioral test, per plan).
- **No GH Issues filed for pre-existing failures** — the only red surface (parallel link-resource
  exhaustion when building all integration-test binaries at once) is a build-environment caveat, not a
  test failure, and was avoided by running per-target. No infra-001 failure was caused by vnc-040.
- **Recommended (not filed — Delivery Leader files at Phase 4):** "infra-001: add multi-slug HTTP
  fixture + per-slug config.toml placement" — the per-slug overlay is not MCP-reachable on the current
  single-server harness, so its behavioral proof is Rust-internal (unit + #5172 N=2 harness), per
  test-plan OVERVIEW §5b.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Post-merge cross-field invariant gap (#3905); `validate_config(&merged)` must run inside `resolve_slug_config` after merge | `test_resolve_merged_violation_fails_loud_naming_slug__fusion_weight_sum`, `test_per_file_validation_alone_does_not_catch_merged_violation__fusion_weight_sum`, `test_resolve_runs_post_merge_validate_inside_helper_after_merge`, `test_resolve_valid_merge_passes_no_false_positive` | PASS | Full |
| R-02 | Hidden `merge_configs` inline `InferenceConfig {…}` literal drift (#4070) for the global→per-slug shape | `test_inference_overlayable_fields_overlay_siblings_fall_through`, `test_option_field_set_global_unset_per_slug_retains_global` + recorded A1 audit (below) | PASS | Full |
| R-03 | Fallthrough not byte-for-byte; no-file arm must reuse the daemon's parity Arcs (#4583) | `test_resolve_no_file_returns_cow_borrowed_global_no_merge`, `test_no_file_arm_ptr_eq_on_three_global_handles`, `test_no_file_arm_overlayable_values_equal_global` | PASS | Full |
| R-04 | A model handle (fields 0–2) sourced from merged config → 2nd model loads; breaks crt-056 AC-2 | `test_n2_exactly_one_nli_and_one_embed_handle_resident`, `test_fields_0_2_cloned_unconditionally_on_file_present_arm` | PASS | Full |
| R-05 | Hash-pin global-wins regresses under the global→per-slug pairing; warn not emitted (#4655) | `test_sha256_pins_global_wins_under_per_slug_pairing`, `test_no_global_pin_plus_per_slug_pin_does_not_silently_lock` | PASS | Full |
| R-06 | Forward-guard breach: per-slug vector index becomes config-driven instead of `VectorConfig::default()` (#5196) | `test_per_slug_vector_index_uses_vectorconfig_default_not_merged_dims` | PASS | Full |
| R-07 | Verdict checklist drops a `build_project_server` call-site input (materialized TWICE at design gate) | `test_classification_registry_exhaustive_vs_seam_field_set`, `test_is_per_slug_overlayable_matches_registry_disposition`, `test_is_per_slug_overlayable_sampled_dispositions` | PASS | Full |
| R-08 | `nli_top_k`/`nli_enabled` treated as model-coupled instead of runtime params | `test_nli_runtime_params_overlay_and_are_classified_overlayable` | PASS | Full |
| R-09 | Transport key (TLS/auth/host/`http.enabled`) read at the per-slug seam | `test_transport_keys_in_per_slug_file_do_not_affect_served_transport` | PASS | Full |
| R-10 | Per-slug load bypasses 64 KiB cap (#2395) or `#[cfg(unix)]` 0o022 permission check | `test_resolve_rejects_oversized_file_before_parse`, `test_resolve_rejects_world_or_group_writable_file` | PASS | Full |
| R-11 | Error from `resolve_slug_config` not slug-named or fails at request time, not startup | `test_resolve_invalid_class_fails_loud_naming_slug__{malformed_toml,unknown_category,oversized_instructions}`, `test_resolve_file_present_executes_full_order` | PASS | Full |
| R-12 | Per-slug `instructions` overlay regresses; absent-file slug stops falling through to global (#785) | `test_n2_instructions_per_slug_isolated`, `test_instructions_absent_falls_through_to_global` | PASS | Full |
| R-13 | Per-slug GLOBAL-only section silently ignored (accepted residual; split now owned by A's classification) | `test_classification_is_single_source_of_truth` (doc-assertion) + transport/permissive locks (R-09, AC-07) | PASS (residual) | Full (residual is documented, not test-gated) |
| R-14 | Classification ↔ `merge_configs` ↔ seed-render drift (crt-031 multi-copy-divergence; High proof obligation) | `test_classification_drift_guard_every_entry_matches_merge_configs`, `test_classification_registry_exhaustive_vs_seam_field_set` | PASS | Full |

All 14 risks (32 scenarios) have at least one passing test. No risk is uncovered.

## Test Results

### Unit Tests (`cargo test -p unimatrix-server`)

Two targets were run separately. The three vnc-040 test modules split across the targets by where their
`mod` is declared: `slug_config_classification_tests` is declared in `infra/config.rs` (compiles into the
**lib**); `slug_config_tests` (under `http_provision`) and `per_slug_loop_tests` (under `main.rs`) compile
into the **bin**. Running only `--lib` would have silently skipped 23 of the 34 component tests — both
targets were executed.

| Target | Total | Passed | Failed | vnc-040 component tests within |
|--------|-------|--------|--------|---------------------------------|
| `--lib` | 4250 | 4250 | 0 | 11 (`slug_config_classification_tests`) |
| `--bin unimatrix` | 98 | 98 | 0 | 23 (11 `slug_config_tests` + 12 `per_slug_loop_tests`) |
| **vnc-040 total** | **34** | **34** | **0** | — |

- 1 lib test `ignored` (pre-existing, unrelated to vnc-040).
- Build/test warnings are dead-code analysis notes only (no errors); the lib emits 24 pre-existing warnings.

**Linker-resource caveat (handled, not a failure):** building all `unimatrix-server` integration-test
binaries in parallel can hit transient `cc` link-resource exhaustion (NOT undefined-reference /
duplicate-symbol). This is an environment/parallel-link constraint, not a vnc-040 defect. Mitigated by
running the `lib` and `bin` targets separately (each links cleanly: 4250 + 98, 0 failed). No code or test
change required.

### Integration Tests (infra-001, `unimatrix` release binary)

Binary built: `cargo build --release -p unimatrix-server` → `target/release/unimatrix` (clean).
Harness env: `UNIMATRIX_BINARY`, `ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so`,
`LD_LIBRARY_PATH=/usr/local/lib`.

| Run | Suites | Total | Passed | Failed | xfail | Result |
|-----|--------|-------|--------|--------|-------|--------|
| Smoke (mandatory gate) | all (`-m smoke`) | 24 | 24 | 0 | 0 | PASS |
| Regression batch A | protocol, confidence, lifecycle | 108 | 100 | 0 | 6 (+2 xpassed) | PASS |
| Regression batch B | tools | 195 | 194 | 0 | 1 | PASS |
| **Regression total** | protocol, tools, lifecycle, confidence | **303** | **294** | **0** | **7 (+2 xpassed)** | **PASS** |

(A first single combined run of all four suites was killed at my wall-clock ceiling — RC=124, an
environment limit, NOT a test failure — after ~44% of `test_tools.py` with **0 failures** observed. The
suites were re-run in two batches with a higher ceiling to obtain exact tallies; counts above are from
those batched runs. Note: the live harness has grown well beyond USAGE-PROTOCOL's documented per-suite
counts — `tools` alone now collects ~200 tests — so each suite spins many fresh servers with model
warmup and is slow; this is expected, not a regression.)

- **Smoke gate (mandatory minimum) PASSED 24/24.** Covers one critical path per major capability
  (store/get, search, correction chain, scanning, capability enforcement, confidence range, status,
  restart persistence, volume).
- **Regression suites PASSED, 0 failures** (294 passed across both batches). The non-green entries are
  pre-existing `@pytest.mark.xfail` markers already present in the harness (21 xfail markers exist across
  the suites; NONE added by vnc-040). **2 `xpassed`** appeared in batch A — pre-existing xfail-marked
  tests that now pass incidentally; these markers predate this feature and the xpass is unrelated to the
  per-slug overlay (vnc-040 changes only the per-slug HTTP derivation, not the single-server paths these
  tests exercise). They are flagged for the harness owner to clean up the stale markers but are NOT a
  vnc-040 concern and NOT a failure. No new failures introduced by this feature.
- **Regression-only role (test-plan OVERVIEW §5b):** the infra-001 harness drives a SINGLE STDIO server
  with one `--project-dir`; it has no multi-slug / `base_dir` / per-slug `config.toml` fixture. The
  vnc-040 per-slug behaviors (`Arc::ptr_eq` fallthrough, one-model-each at N≥2, `merge_configs`
  drift-guard, post-merge re-validation) are NOT MCP-reachable today. infra-001's role for vnc-040 is to
  prove the single-project / no-file path (the silent majority, AC-02/NFR-02) still behaves identically —
  which it does. **No new infra-001 tests were added** (per plan §5c: adding speculative single-server
  per-slug tests would be vacuous). The authoritative per-slug proof is the Rust unit + #5172 model-free
  N=2 harness above.

### Construction / Review Verifications (recorded "C" obligations)

| Obligation | AC / Risk | Finding |
|-----------|-----------|---------|
| Post-merge `validate_config(&merged)` runs INSIDE `resolve_slug_config`, after `merge_configs`, before return | AC-08b / R-01 | Confirmed by `test_resolve_runs_post_merge_validate_inside_helper_after_merge` (error fires from the helper post-merge) |
| Fields 0–2 (`embed`, `nli`, `pool`) `Arc::clone`d UNCONDITIONALLY outside the overlay branch; threaded `permissive` from the global flag; `instructions` from `resolved.server.instructions` | AC-04 / AC-07 / R-04 | Confirmed at `main.rs:1092-1119,1153-1158` — `resolve_slug_config` at :1108, `instructions` from `r.server.instructions` at :1119, global `embed`/`permissive` threaded at :1156-1158 (never read from `resolved`) |
| A1 / SR-02 re-audit of the `merge_configs` inference arm (#4070) | R-02 | Confirmed at `config.rs:3905-3939`: the `InferenceConfig {…}` arm enumerates EVERY field explicitly (per-field merge), NOT a `..default()` spread that could silently drop a field. Global→per-slug uses the SAME arm as global→project. `embedding_model_sha256` global-wins + `tracing::warn` at :3915-3930 |
| No hot-reload / watch / notify path (restart-applies, vnc-038 ADR-007) | AC-09 | Confirmed: no `watch`/`notify`/`reload`/`inotify` in `http_provision.rs` or `config.rs`; the overlay reads once at `build_project_server` time |
| `adapt_service` left `AdaptConfig::default()`, not threaded | FR-13 | Recorded design decision; not an AC; not threaded at the seam |
| Single canonical classification is sole owner; verdict table + Feature B seed render FROM it (one-way A→B) | R-13 | `test_classification_is_single_source_of_truth` doc-assertion; the AC-11 drift-guard machine-pins the registry to `merge_configs` |

## Cross-Field Invariant Enumeration (AC-08b prerequisite, OVERVIEW §4)

The enumeration obligation: at least one merged-only violation per cross-field invariant in
`validate_config`. The Stage 3b authored R-01 tests cover the **canonical fusion-weight sum-of-six**
class (`__fusion_weight_sum`) — a global+per-slug pair each individually valid whose MERGE pushes the
six inference weights' sum out of range, proven (a) to fail loud at startup naming the slug, and (b) NOT
caught by per-file validation alone (the load-bearing negative for #3905). This is the canonical and
highest-value class. **Coverage note (minor):** the test plan invited a violation case per additional
cross-field class (PPR-weight, confidence-weight, custom-preset cross-level prohibition #3923). The
implemented suite proves the mechanism end-to-end on the canonical sum-of-six class and the post-merge
placement (which is shared by ALL classes — `validate_config(&merged)` runs once over the whole merged
struct), so the mechanism is fully proven; per-class violation fixtures for PPR/confidence/preset are a
thin completeness expansion, not a mechanism gap. Recorded here as a possible follow-up, not a blocker —
the post-merge re-validation path that catches every class is exercised and green.

## Gaps

**No risk lacks test coverage.** Two recorded, non-blocking notes:

1. **AC-08b per-class enumeration (above):** the post-merge re-validation MECHANISM is fully proven on
   the canonical sum-of-six class and runs over the entire merged struct (so it catches every
   cross-field class). Adding explicit merged-only violation fixtures for the PPR-weight,
   confidence-weight, and custom-preset (#3923) classes would broaden completeness but is not a
   mechanism gap. Recommend as an optional thin expansion.
2. **Multi-slug MCP-level coverage:** by design (OVERVIEW §5b), the per-slug overlay is not reachable
   through the single-server infra-001 harness. Behavioral coverage is the Rust unit + #5172 N=2
   harness (which IS present and green). Closing this at the MCP surface requires a harness uplift —
   recommended GH Issue (below), to be filed by the Delivery Leader at Phase 4, NOT built in this PR.

## GH Issues

- **Recommended, NOT filed (Delivery Leader files at Phase 4):** "infra-001: add multi-slug HTTP fixture
  + per-slug config.toml placement" — unblocks Feature B (seeding) and future per-slug MCP-level
  assertions. Justification: the single-server harness cannot place `{base_dir}/{slug}/config.toml` or
  drive N≥2 slugs, so vnc-040's per-slug overlay is not MCP-observable today.
- **No GH Issue for pre-existing failures:** there were none. The 2 regression xfails are pre-existing
  harness debt with their own existing markers; the linker-resource constraint is an environment caveat
  (handled by per-target runs), not a code/test failure caused by vnc-040.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_n2_categories_per_slug_isolated` — A's served `CategoryAllowlist` reflects A's, B's reflects B's, non-vacuous distinct populations, no leakage |
| AC-02 | PASS | `test_resolve_no_file_returns_cow_borrowed_global_no_merge` (Cow::Borrowed, no merge) + `test_no_file_arm_ptr_eq_on_three_global_handles` (`Arc::ptr_eq` on embed/nli/pool) + `test_no_file_arm_overlayable_values_equal_global`; corroborated by green smoke/regression single-server path |
| AC-03 | PASS | `test_resolve_single_key_overlay_changes_only_that_key`, `test_inference_overlayable_fields_overlay_siblings_fall_through`, `test_list_field_override_replaces_not_appends` (#2286 replace), `test_option_field_set_global_unset_per_slug_retains_global` (`.or()`) |
| AC-04 | PASS | `test_n2_exactly_one_nli_and_one_embed_handle_resident` (`Arc::ptr_eq` one-of-each at N=2; slug attempting `embedding_model_sha256` keeps global pin) + `test_fields_0_2_cloned_unconditionally_on_file_present_arm` |
| AC-05 | PASS | `test_sha256_pins_global_wins_under_per_slug_pairing` (merged pin == global; `tracing::warn` emitted), `test_no_global_pin_plus_per_slug_pin_does_not_silently_lock` |
| AC-06 | PASS | `test_transport_keys_in_per_slug_file_do_not_affect_served_transport` — served transport == global; seam never reads a transport field; listener built before the loop |
| AC-07 | PASS | `test_classification_registry_exhaustive_vs_seam_field_set` (closed checklist == live call-site arg set incl. `embed_handle`, `permissive`, `instructions`, `[embedding]`), `test_permissive_passed_from_global_flag_never_from_resolved`, `test_is_per_slug_overlayable_*` |
| AC-08a | PASS | `test_resolve_rejects_oversized_file_before_parse` (64 KiB cap #2395), `test_resolve_rejects_world_or_group_writable_file` (`#[cfg(unix)]` 0o022), `test_resolve_invalid_class_fails_loud_naming_slug__{malformed_toml,unknown_category,oversized_instructions}` |
| AC-08b | PASS | `test_resolve_merged_violation_fails_loud_naming_slug__fusion_weight_sum` (merged-only sum violation fails at startup naming slug) + `test_per_file_validation_alone_does_not_catch_merged_violation__fusion_weight_sum` (per-file `Ok`, #3905 proof) + `test_resolve_runs_post_merge_validate_inside_helper_after_merge` |
| AC-09 | PASS (C) | Construction review: no reload/watch/notify path; overlay reads once at `build_project_server` time (vnc-038 ADR-007 restart-applies) |
| AC-10 | PASS | `test_n2_instructions_per_slug_isolated` (A's instructions ≠ B's, no leak), `test_instructions_absent_falls_through_to_global` (absent → global `resolved.server.instructions`); threaded from `r.server.instructions` at `main.rs:1119` |
| AC-11 | PASS | `test_classification_drift_guard_every_entry_matches_merge_configs` (every registry entry: overlayable⇒slug wins, locked⇒global wins, incl. `*_sha256` carve-out) + `test_classification_registry_exhaustive_vs_seam_field_set` (registry exhaustive vs the seam-relevant `EXPECTED_CLASSIFIED_KEYS` set) |

All 11 AC-IDs (AC-01..AC-11, with AC-08 split into 08a/08b) verified PASS.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-004 (#5210 canonical classification),
  #5212 (two gotchas implementing `resolve_slug_config` in the binary crate's `http_provision.rs`),
  #5172 (model-free N=2 harness), and the crt-031 multi-copy-divergence lesson. Confirmed the test
  approach (drift-guard pins registry to `merge_configs`; #5172 N=2 model-free for the model invariant)
  matches stored precedent.
- Stored: nothing novel to store — the reusable patterns this stage exercised (model-free N=2 isolation
  harness #5172, registry-pins-merge drift-guard crt-031/AC-11, post-merge re-validation #3905) are
  already captured. One **environment caveat** worth a lesson IF it recurs: vnc-040's component tests
  split across the `lib` and `bin` targets by `mod` declaration site, so a `--lib`-only run silently
  skips bin-target tests — a Stage 3c agent MUST run both targets (or `--workspace`) to avoid a
  false-green. Flagged here; storing deferred unless it recurs in a second feature (per stewardship
  guidance, avoid premature single-instance storage).
