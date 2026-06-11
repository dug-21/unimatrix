# Risk Coverage Report: vnc-034 (Wave 1)

> Stage 3c test execution. Scope: **Wave-1 only** — single-project HTTPS serving (#726)
> + pure-JS remote client (#725) + the build-first C1/C2 connection contract. Wave 2
> (#727, AC-W2-R*) is OUT OF SCOPE and not tested here.
>
> Date: 2026-06-11 · Agent: vnc-034-agent-7-tester · Result: **PASS** (all Wave-1 risks
> covered; mandatory integration smoke gate green; one pre-existing flake confirmed
> unrelated, no GH Issue warranted).

## Coverage Summary (Wave-1 risks R-01..R-13 + rotation)

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 (Critical) | Deferred StoreResolver seam swap / store served around not through the funnel | `test_single_funnel...`, `test_per_request_funnel_consults_resolver_with_transport_key`, `test_path_router_mcp_edge_is_the_slug_router_seam`, `test_resolver_swap_requires_no_callsite_change`, `test_slug_key_under_default_resolver_returns_unknown_project`, `test_default_resolver_slug_returns_unknown_project`, `test_per_request_slug_rejected_at_funnel_not_default_store` (lib) | PASS | Full |
| R-02 | C2 fingerprint diverges Rust-oracle ↔ JS-client (DER vs PEM, casing) | `test_c2_fingerprint_golden_is_stable`, `test_fingerprint_hashes_der_not_pem`, `test_fingerprint_is_64_lowercase_hex`, `test_bundle_fp_equals_served_leaf_der_fingerprint` (fingerprint_parity.rs); JS `computeFingerprint parity` over committed corpus; `test_e2e_bundle_fp_equals_served_leaf_der` (NEW, client_bundle_e2e.rs) | PASS | Full |
| R-03 | Slug parser trust boundary (../ , encoded separators, over-length) | `test_projectslug_rejects_traversal_corpus`, `test_projectslug_over_length_boundary`, `test_projectslug_empty_rejected`, `test_projectslug_accepts_valid` (lib); JS `slug allowlist` accept/reject | PASS | Full (Wave-1 parser guard; routing escape AC-W2-R6 deferred) |
| R-04 | Local-UDS / cloud seam parity (NFR-10) | `test_local_install_resolves_path_hash_store_through_seam`, `test_default_resolver_is_the_same_trait_as_wave2_resolver` (lib) — AC-W1-X2 regression test IN the Wave-1 set | PASS | Full |
| R-05 | C1 bundle parser accepts malformed/oversized/extra-field input | `test_bundle_length_cap_before_decode`, `test_bundle_at_exactly_cap_boundary`, `test_bundle_strict_schema_reject_*`, `test_bundle_reject_*`, `test_bundle_parser_never_crashes_on_corpus` (bundle_codec.rs); JS `bundle decode — guard ordering / strict schema reject`; `test_e2e_emitted_blob_round_trips` (NEW) | PASS | Full |
| R-06 | 1:1 enforced at transport, not config (mis-target unrepresentable) | `test_project_identity_has_no_payload_carrier`-class lib seam tests + `test_per_request_funnel_consults_resolver_with_transport_key`; JS `initRemote — 1:1 unrepresentable` (`test_client_has_no_second_project_field`) | PASS | Full |
| R-07 | First-boot credential idempotence (restart loads, not regenerates) | `test_second_call_loads_byte_identical_no_rewrite`, `test_operator_override_honored_not_overwritten`, `test_concurrent_first_boot_converges`, `test_partial_state_*_errors_loud` (cert_provisioner.rs) | PASS | Full (fn-level boot-twice; container restart documented as env-limited, see Gaps) |
| R-08 | Production cert params (SAN/validity/0600, not test defaults) | `test_key_written_mode_0600`, `test_cert_san_set_...` (lib), `test_first_call_generates_both_files` (cert_provisioner.rs) | PASS | Full |
| R-09 | C3 single derivation desyncs cert SAN from bundle base-url | `test_derive_public_url_*`, `test_three_consumers_read_one_derivation`, `test_bundle_host_in_cert_sans`, `test_no_socket_autodetect` (public_url.rs lib) | PASS | Full |
| R-10 | Enterprise seams collapsed by a Wave-1 shortcut | `test_provisioned_cert_builds_tls_acceptor` (AC-CT-C6, cert_provisioner.rs) + `StoreResolver`/`ProjectKey`/`ProjectSlug` trait-present lib assertions | PASS | Full |
| R-11 | Fail-loud provisioning on unwritable /data | `test_unwritable_data_dir_returns_actionable_error`, `test_unreadable`/`test_partial_state_*_errors_loud` (cert_provisioner.rs) | PASS | Full (fn-level; container UID-mismatch documented as env-limited) |
| R-12 | Hard invariants: TLS-only port, token absent, install <250 KB, only /health unauth | `test_e2e_token_absent_from_stdout_and_stderr` (NEW), `test_client_bundle_token_absent...` (lib); JS `test_remote_install_under_250kb` + zero-deps + hook-client size gate; only-/health-unauth + plaintext-port = source/config-asserted (see Gaps for live-container probe) | PASS (partial on live-container probes) | Full at unit/e2e; container-runtime probes env-limited |
| R-13 | OQ-C additive addressing (Wave-1 client unchanged by Wave-2 /{slug}) | `test_route_v1_tools_maps_to_default`, `test_route_v1_slug_tools_parses_to_slug`, `test_resolver_swap_requires_no_callsite_change` (lib, AC-CT-C4 Wave-1 half) | PASS | Full |
| (rotation) | Rotate-without-rebundle diagnosable; rotate-with-rebundle reconnects | `test_e2e_rotation_changes_fp_old_pin_would_mismatch` (NEW, server half); JS `test_pin_mismatch_rejects_with_diagnosable_error` + `test_reinit_overwrites_pinned_fp_cleanly` (client half); runbook file-check (see AC-CT-ROT) | PASS | Full |

## Test Results

### Unit Tests (Rust lib — `cargo test -p unimatrix-server --lib`)
- Total: 3946
- Passed: 3945 (3946 with the one flake re-run in isolation)
- Failed: 0 attributable to vnc-034. 1 PRE-EXISTING FLAKE under full-suite parallel load
  (`http::token::tests::test_concurrent_creation_no_corruption`) — passes 1/1 in isolation
  (`--test-threads=1`), `token.rs` untouched by vnc-034. NOT a feature failure; NO GH Issue
  warranted (already a known flake flagged in the spawn brief).
- Ignored: 1 (oracle/regen-gated).
- Note: `cargo test --workspace` / bare `cargo test -p unimatrix-server` were NOT run — the
  in-binary `bin "unimatrix"` test target OOM-kills `ld` (signal 9) on the 113-object link in
  this container. Per the documented constraint, the lib suite + per-target integration tests
  were run individually (all link fine).

### Rust Integration Tests (per-target — `cargo test -p unimatrix-server --test <name>`)
- Total: 43 passed, 0 failed, 2 ignored (oracle-regen, intentional).
  | Target | Passed | Ignored | Covers |
  |--------|--------|---------|--------|
  | `fingerprint_parity` | 12 | 1 | C2 oracle + drift guard + served-cert linkage (R-02, AC-W1-S4) |
  | `bundle_codec` | 18 | 1 | C1 codec, guard ordering, strict schema, corpus drift (R-05, AC-W1-C9/C10) |
  | `cert_provisioner` | 9 | 0 | idempotence, override, 0600, fail-loud, acceptor seam (R-07/R-08/R-11) |
  | `client_bundle_e2e` (**NEW this stage**) | 4 | 0 | real-binary e2e: served-cert==fp, token-absent-fds, round-trip, rotation (R-02/R-05/R-12 + ROT) |

### JS Client Tests (`node --test`)
- Total: 841
- Passed: 840
- Failed: 0
- Skipped: 1
- Includes `remote-client.test.js` (34 tests): C2 pin parity vs committed Rust corpus, C1
  decode guard ordering, initRemote endpoint derivation, 1:1 unrepresentability, onboarding
  artifacts, token hygiene, <250 KB footprint, rotation overwrite.
- Auxiliary hard gates: `check-zero-deps.js` PASS (no runtime deps; 18 hook-client modules
  use only Node built-ins/relative); `check-hook-client-size.js` PASS (stripped 81038/100000,
  raw 138860/160000).

### Integration Smoke Gate (infra-001 stdio — MANDATORY minimum)
- `python -m pytest suites/ -m smoke --timeout=90` against the FRESH release binary
  (rebuilt 2026-06-11 14:19 with the Wave-1 SlugRouter seam wired): **23 passed,
  0 failed, 351 deselected (199s)**.
- Proves the SlugRouter/DefaultResolver seam insertion + cert provisioning + `client-bundle`
  subcommand did NOT break existing stdio tool dispatch, store/retrieval, or restart
  persistence — the regression-baseline role from test-plan OVERVIEW §4.1.

### Integration Regression Suites (infra-001 — recommended, env-limited)
- Partial runs of `protocol`, `lifecycle`, `edge_cases`, `tools` against the fresh binary
  showed all-pass progress (only pre-existing `xfail`/`xpass` markers, zero hard failures)
  but hit the timeout ceiling at ~50-55% completion. Cause is the per-`server`-fixture
  embedding-model COLD-START cost (~5-8s each × hundreds of tests), NOT any vnc-034 regression.
  The mandatory smoke gate (which exercises one critical path per capability through the same
  wired binary) passed fully and is the binding minimum gate per USAGE-PROTOCOL §"Minimum gate
  requirement". No failure was observed in any partial run.

## Integration Test Counts (MANDATORY)
- **Rust feature-local integration tests:** 43 passed / 2 ignored across 4 targets
  (1 NEW target, `client_bundle_e2e.rs`, 4 tests, added this stage).
- **infra-001 stdio smoke:** 23 passed / 0 failed (mandatory gate, fresh binary).
- **JS client integration (`remote-client.test.js`):** 34 tests within the 840-pass JS suite.

## xfail / GH Issues
- No new `@pytest.mark.xfail` markers added. No integration tests deleted or commented out.
- No GH Issue filed: the only red signal (`test_concurrent_creation_no_corruption`) is a
  known PRE-EXISTING parallel-load flake on untouched code (`token.rs`), confirmed passing in
  isolation; per the triage tree it is neither feature-caused nor a bad assertion, and it was
  already flagged as known — filing a duplicate Issue is not warranted.

## Gaps

Wave-1 risks all have at least Full unit/e2e coverage. The residual gaps are **live-container
integration probes** that require Docker-compose orchestration not runnable inside this
single-container session. Each degrades to a documented unit/e2e proof + a flagged manual
walkthrough (per test-plan OVERVIEW §4.2 / §"Per-OS note"), NOT a silent drop:

| Deferred-to-container probe | AC-ID | Substitute coverage that DID run |
|-----------------------------|-------|----------------------------------|
| sibling-container HTTPS reachability | AC-W1-S1 | (manual / CI compose) — listener wiring type-checked; provisioner+acceptor seam tested (`test_provisioned_cert_builds_tls_acceptor`) |
| plaintext-port refusal | AC-W1-S2 | (compose config) — `compose.yaml` publishes TLS port only (file-check, agent-5 report); no plaintext mode in OSS posture |
| boot-twice / unwritable-/data in a REAL container | AC-W1-S3, S8 | fn-level proven byte-identical (`test_second_call_loads_byte_identical_no_rewrite`) + fail-loud actionable error (`test_unwritable_data_dir_returns_actionable_error`) |
| only-/health-unauth live probe + no /metrics | AC-W1-S6 | source/config: auth `/health` bypass list unchanged (vnc-001/023 lib); no `/metrics` endpoint added (NOT in scope, brief §NOT in Scope) |
| per-OS `init --remote` live HTTPS call (macOS-arm/Windows) | AC-W1-C1, C7 | platform-independent pin/parse JS unit tests pass on Linux; per-OS = manual (AC-W1-C8 is `manual` by spec) |
| nan-014 hardening (UID 65532, distroless, ORT pin, /shared :ro) | AC-W1-S7 | image-inspection / shell (CI) — preserved per agent-5 container-posture report (file-check) |

These are container-runtime assertions; the cross-stack contract risks they would also touch
(R-02 served-cert==fp, R-12 token-absent) ARE proven here end-to-end through the real binary in
`client_bundle_e2e.rs`. The live-HTTPS reachability layer is the only thing not exercised, and
it is gated on compose infra, not on missing test logic.

## Acceptance Criteria Verification (Wave-1 AC-IDs)

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-W1-S1 | PARTIAL (container) | listener wiring + acceptor seam tested; live sibling-container HTTPS = CI compose / manual |
| AC-W1-S2 | PARTIAL (config) | `compose.yaml` TLS-port-only (file-check); no OSS plaintext mode |
| AC-W1-S3 | PASS (fn) / PARTIAL (container restart) | `test_second_call_loads_byte_identical_no_rewrite`, `test_operator_override_honored_not_overwritten` |
| AC-W1-S4 | PASS | `test_bundle_fp_equals_served_leaf_der_fingerprint` (fingerprint_parity) + `test_e2e_bundle_fp_equals_served_leaf_der` (NEW e2e through real binary) |
| AC-W1-S5 | PASS | `test_e2e_token_absent_from_stdout_and_stderr` (NEW, real fds) + lib token-redaction; token only inside the blob |
| AC-W1-S5b | PASS | `render_output` lib tests (stdout blob-only, stderr base-url+fp echo) + e2e fd capture |
| AC-W1-S6 | PARTIAL (probe) | `/health` auth-bypass unchanged (lib); no `/metrics` added (scope); live unauth probe = container |
| AC-W1-S7 | PARTIAL (image) | nan-014 hardening preserved per container-posture report (file-check); image probe = CI |
| AC-W1-S8 | PASS (fn) | `test_unwritable_data_dir_returns_actionable_error` — actionable msg names UID 65532 + path, no panic |
| AC-W1-S9 | PASS | `test_bundle_host_in_cert_sans`, `test_three_consumers_read_one_derivation` (public_url lib) |
| AC-W1-C1 | PARTIAL (per-OS) | Linux pin/parse + initRemote tests pass; macOS-arm/Windows live = manual (C8 manual by spec) |
| AC-W1-C2 | PASS | JS `test_pin_match_accepts`, `test_pin_mismatch_rejects_with_diagnosable_error` over committed corpus |
| AC-W1-C3 | PASS | JS `test_remote_install_under_250kb` + `check-hook-client-size.js` + `check-zero-deps.js` |
| AC-W1-C4 | PASS | JS `test_bad_slug_rejected_no_config_written` (client no-auto-create on parse-edge reject) |
| AC-W1-C5 | PASS | JS `test_client_has_no_second_project_field` — flat single-endpoint config, fan-out unrepresentable |
| AC-W1-C6 | PASS | JS `test_skills_copied`, `test_claudemd_block_not_appended`, `test_unimatrix_init_pointer_printed` |
| AC-W1-C7 | PASS | JS shared-codepath assertion (two CLIs, one bundle path); live data-sharing = AC-W2-R5 (Wave 2) |
| AC-W1-C8 | MANUAL (by spec) | onboarding walkthrough — `manual` verification method per ACCEPTANCE-MAP |
| AC-W1-C9 | PASS | bundle_codec strict-schema reject tests + JS strict-schema reject suite (load-bearing guard) |
| AC-W1-C10 | PASS | `test_bundle_length_cap_before_decode` (Rust) + JS `test_length_cap_before_decode` — length rejects BEFORE decode |
| AC-W1-X1 | PASS | single-funnel lib source/structural assertions + per-request-funnel test |
| AC-W1-X2 | PASS | `test_local_install_resolves_path_hash_store_through_seam` (local-UDS parity IN the Wave-1 set) |
| AC-W1-X3 | PASS | `test_per_request_funnel_consults_resolver_with_transport_key` — transport-derived key, no payload carrier |
| AC-CT-C2 | PASS | committed Rust-oracle corpus consumed byte-identically by Rust drift guard + JS pin test |
| AC-CT-C3 | PASS | `test_three_consumers_read_one_derivation`, `test_no_socket_autodetect` (public_url lib) |
| AC-CT-C4 | PASS (Wave-1 half) | `test_resolver_swap_requires_no_callsite_change`, additive route-shape tests |
| AC-CT-C6 | PASS | `StoreResolver`/`ProjectKey`/`ProjectSlug`/`TlsConfig` seam-present + acceptor-build test |
| AC-CT-ROT | PASS | runbook deliverable CONFIRMED (`docs/cert-rotation.md`, 3 steps + diagnosable rejection) + `test_e2e_rotation_changes_fp_old_pin_would_mismatch` (server) + JS diagnosable-mismatch + reinit-overwrite (client) |

> AC-CT-ROT runbook file-check: **CONFIRMED PRESENT** — `docs/cert-rotation.md` ships the
> required operator deliverable. It documents the three steps (rotate cert → re-run
> `client-bundle` → re-`init --remote`), the token-unchanged/fp-changed table, and the
> diagnosable stale-fingerprint rejection naming expected-vs-presented `sha256:`. The
> behavioral contract is verified end-to-end: server-side new-fp generation
> (`test_e2e_rotation_changes_fp_old_pin_would_mismatch`, NEW) + client-side diagnosable
> mismatch + reinit-overwrite (JS). Deliverable + behavior both satisfied.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001 (#4954 bundle wire form),
  ADR-006 (#4952 wave mapping), #4961 (C1 decoder guard ordering), #4956 (pub-use a pub(crate)
  fn to consume from a tests/ crate), #4962 (seam wireability lesson). Applied #4956's
  re-export guidance (consumed `Bundle`/`decode_bundle`/`load_or_generate_cert` from the lib
  surface in the new tests/ crate).
- Stored: entry #4964 "E2E testing a pre-tokio CLI subcommand by driving the real binary with
  HOME-relative data dir" via context_store (pattern) — the CARGO_BIN_EXE + HOME-relative
  data-dir + top-level-arg-ordering recipe for fd-split-contract integration tests.
