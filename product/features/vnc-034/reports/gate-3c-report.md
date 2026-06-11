# Gate 3c Report: vnc-034 (Wave 1)

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-11
> Scope: **Wave-1 ONLY** (#726 single-project HTTPS serving + #725 pure-JS remote client + the build-first C1/C2 connection contract). Wave 2 (#727, AC-W2-R1..R6) intentionally DEFERRED — absence NOT flagged.
> Branch: feature/vnc-034 (HEAD 04a1ec3b)
> Validator independently re-ran every load-bearing Rust/JS suite and re-read the seam source — claims confirmed against ground truth, not taken on report.
> Result: **PASS**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-13 + rotation) | PASS | Every Wave-1 risk maps to ≥1 passing test; RISK-COVERAGE-REPORT mapping verified against re-run results |
| 2. Test coverage completeness vs Risk Strategy | PASS | All Wave-1 risk-to-scenario mappings exercised; integration counts present; residual gaps are container-runtime-bound, each degraded to a documented unit/e2e proof |
| 3. Specification compliance (Wave-1 FRs) | PASS | FR-A1..A11, FR-B1..B9, FR-X1..X5 all covered & verified; ACCEPTANCE-MAP Wave-1 AC-IDs PASS or PARTIAL(env)/MANUAL(by-spec) |
| 4. Architecture compliance | PASS | C4 per-request seam funnel WIRED (PathRouter MCP edge → SlugRouter → resolve_store); C1/C2 contracts honored; C3 single derivation; C6 seam separation intact |
| 5. Knowledge stewardship (tester) | PASS | Tester report has `## Knowledge Stewardship` with `Queried:` + `Stored:` (#4964) entries |
| INTEGRATION: smoke gate | PASS (tester-attested) | 23 pass / 0 fail against fresh Wave-1 binary; validator confirms wiring genuine, harness external to this checkout |
| INTEGRATION: per-target suites | PASS (re-run) | 43 pass / 2 ignored across fingerprint_parity, bundle_codec, cert_provisioner, client_bundle_e2e |
| INTEGRATION: xfail / no-deletion hygiene | PASS | Zero `.py`/`suites/` changes on branch — no integration test deleted, commented, or xfail-marked |

## Detailed Findings

### Check 1 — Risk Mitigation Proof
**Status**: PASS
**Evidence (independently re-run, not report-trusted)**:
- **R-01 (Critical) — deferred seam swap / served-through-not-around**: `seam.rs` confirms `SlugRouter::route_mcp` calls `self.resolver.resolve_store(&key)` as THE funnel and `router.rs:282` wires it as `PathRouter`'s MCP edge — every MCP request flows `parse_project_key → resolve_store → dispatch`. `DefaultResolver` returns `UnknownProject` for any `Slug(_)`, never the default store (`seam.rs` RouteError doc + lib tests `test_slug_key_under_default_resolver_returns_unknown_project`). Resolver-swap test passes. **Served through the seam, not around it (FR-X5/A4) — verified in source + tests.**
- **R-02 — cert-fingerprint cross-stack parity**: `fingerprint_parity` re-ran **12 pass / 1 ignored** (`test_fingerprint_hashes_der_not_pem`, `test_fingerprint_is_64_lowercase_hex`, `test_bundle_fp_equals_served_leaf_der_fingerprint`); JS `remote-client.test.js` pin parity over the committed Rust-oracle corpus passes; `client_bundle_e2e::test_e2e_bundle_fp_equals_served_leaf_der` proves the bundle pins the SERVED leaf DER through the real binary.
- **R-03 — slug allowlist trust boundary**: `ProjectSlug::try_from` enforces `^[a-z0-9][a-z0-9-]{0,62}$` at the parse edge BEFORE any path join (`seam.rs:81-103`); `/v1/tools` matched before the slug arm so the reserved literal never becomes a slug. Traversal/encoded-separator/over-length/empty/uppercase all rejected (lib + JS suites). Escape is structurally impossible, not runtime-rejected.
- **R-04 — local-UDS/cloud seam parity (NFR-10)**: `test_local_install_resolves_path_hash_store_through_seam` is IN the Wave-1 lib set; one trait, two resolvers; no cloud-only isolation path.
- **R-05 — bundle parser trust boundary**: `bundle_codec` re-ran **18 pass / 1 ignored** (length-cap-before-decode, strict-schema reject, corpus-never-crashes); JS guard-ordering + strict-schema reject pass; e2e round-trip passes.
- **R-06 — 1:1 at transport, not config**: `ProjectKey` constructible only from transport (`seam.rs:30-45`); JS `test_client_has_no_second_project_field` — config bakes exactly one endpoint; mis-target unrepresentable.
- **R-07 — first-boot credential idempotence**: `cert_provisioner` re-ran **9 pass** (`test_second_call_loads_byte_identical_no_rewrite`, `test_operator_override_honored_not_overwritten`, `test_concurrent_first_boot_converges`, partial-state-errors-loud).
- **R-08 — production cert params**: `test_key_written_mode_0600`, SAN tests, `test_first_call_generates_both_files` pass.
- **R-09 — C3 single derivation**: `public_url` lib tests `test_three_consumers_read_one_derivation`, `test_bundle_host_in_cert_sans`, `test_no_socket_autodetect` pass.
- **R-10 — enterprise seam preservation**: `StoreResolver`/`ProjectKey`/`ProjectSlug`/`TlsConfig` seam-present + `test_provisioned_cert_builds_tls_acceptor`.
- **R-11 — fail-loud provisioning**: `test_unwritable_data_dir_returns_actionable_error` + partial-state-errors-loud; **no `.unwrap()` in non-test provisioning code** (validator grep of cert_provisioner/public_url/tls/client_bundle/http_provision/seam/default_resolver — only doc-comment mentions, zero code occurrences).
- **R-12 — hard invariants**: `client_bundle_e2e::test_e2e_token_absent_from_stdout_and_stderr` (real fds); JS `test_remote_install_under_250kb` + `check-zero-deps.js` PASS + `check-hook-client-size.js` PASS (stripped 81038/100000, raw 138860/160000).
- **R-13 — additive addressing (Wave-1 half)**: `test_resolver_swap_requires_no_callsite_change` + additive route-shape tests.
- **rotation**: `client_bundle_e2e::test_e2e_rotation_changes_fp_old_pin_would_mismatch` (server) + JS diagnosable-mismatch + reinit-overwrite (client) + runbook `docs/cert-rotation.md` (validator read it: 3 steps, token-unchanged/fp-changed table, expected-vs-presented `sha256:` mismatch message).

### Check 2 — Test Coverage Completeness
**Status**: PASS
**Evidence**: Every Risk-Strategy scenario for R-01..R-13 + rotation has a corresponding exercised test (mapping table in RISK-COVERAGE-REPORT verified row-by-row against re-run output). Integration test counts ARE present in the report (43 Rust feature-local / 2 ignored; 23 stdio smoke; 34 JS remote-client). Residual gaps are genuinely environment-bound live-container compose probes (AC-W1-S1/S2/S6/S7 + per-OS C1), each degraded to a documented unit/e2e substitute + flagged manual/CI follow-up — NOT silent drops, NOT masking feature bugs. The cross-stack contract risks those probes would also touch (R-02 served-cert==fp, R-12 token-absent) ARE proven end-to-end through the real binary in `client_bundle_e2e.rs`.

### Check 3 — Specification Compliance (Wave-1 FRs)
**Status**: PASS
**Evidence**: ACCEPTANCE-MAP Wave-1 AC-IDs resolve to PASS, or PARTIAL(container/per-OS) with a documented unit/e2e substitute, or MANUAL-by-spec (AC-W1-C8). FR-A5b stdout/stderr split with token-redaction verified (`test_e2e_token_absent...` + runbook Step 2). FR-B1/B8 pure-JS zero-dep < 250 KB verified by hard gates. FR-X1/X3/X5 single-funnel + sole-write-capability verified in `seam.rs`. No Wave-2 FR (FR-C*) validated — correctly out of scope.

### Check 4 — Architecture Compliance
**Status**: PASS
**Evidence**: The C4 per-request seam funnel is real and wired — `PathRouter` holds a `SlugRouter` and dispatches the MCP fall-through arm through it (`router.rs:107,143,282`); `SlugRouter::route_mcp` is the single `resolve_store` call site. C1 wire form (`unimatrix-bundle:<base64url(json)>`), C2 (`sha256:<lowercase-hex>` over leaf DER), C3 (single `derive_public_url`), C6 (token authorizes / slug scopes / cert secures — three seam interfaces present and documented-but-degenerate) all match ARCHITECTURE.md §3 and ADR-001..007. No architectural drift.

### Check 5 — Knowledge Stewardship (tester)
**Status**: PASS
**Evidence**: `vnc-034-agent-7-tester-report.md` contains a `## Knowledge Stewardship` block with `Queried:` (context_briefing surfacing ADR-001/006, #4961/#4956/#4962) and `Stored:` (#4964 "E2E testing a pre-tokio CLI subcommand…" pattern). Both required entry types present with reasons.

## Integration Test Validation (mandatory)

- **infra-001 smoke gate**: tester attests 23 pass / 0 fail against a binary rebuilt 2026-06-11 14:19 with the SlugRouter seam wired. The `suites/` harness is external to this Rust checkout, so the validator could not re-execute it here; the validator DID independently confirm the seam wiring is genuine (not a no-op) by reading `router.rs`/`seam.rs`, and confirmed the binary's lib + all per-target integration suites are green. **Accepted.**
- **Per-target Rust suites (validator re-ran)**: fingerprint_parity 12/1 · bundle_codec 18/1 · cert_provisioner 9/0 · client_bundle_e2e 4/0 = **43 pass / 2 ignored**. Matches the report exactly.
- **Lib suite (validator re-ran)**: `cargo test -p unimatrix-server --lib` → **3946 pass / 0 fail / 1 ignored**. The known `http::token::tests::test_concurrent_creation_no_corruption` parallel-load flake did NOT trigger in `--lib` isolation, confirming its pre-existing parallel-load nature (token.rs untouched by vnc-034) — correctly not GH-filed.
- **xfail / deletion hygiene**: branch diff (`main...feature/vnc-034`) touches **zero** `.py` / `suites/` files — no integration test deleted, commented out, or `@pytest.mark.xfail`-marked. Tester reported NONE added; confirmed.
- **RISK-COVERAGE-REPORT integration counts**: present (43 Rust / 23 smoke / 34 JS).
- **Residual gaps**: live Docker-compose multi-container probes (AC-W1-S1/S2/S6/S7) + per-OS C1 — each degraded to a documented unit/e2e proof + flagged manual/CI follow-up. Genuinely environment-bound, not silent drops, not masking feature bugs.

## Known Environment Constraints (per spawn brief — NOT failures)
- `bin "unimatrix"` TEST target cannot link here (`ld` OOM, signal 9). Validated via `--lib` + individual `--test <name>` targets + JS runner, per the documented constraint. `cargo test --workspace` / bare `-p unimatrix-server` correctly NOT run.
- `test_concurrent_creation_no_corruption` pre-existing flake — passes in isolation, not a regression, correctly not GH-filed.
- `router.rs` 562 lines (pre-existing over-guideline) + stale `public_url.rs` `#![allow(dead_code)]` — non-blocking cleanup follow-ups already noted at Gate 3b.

## Rework Required
None.

## Scope Concerns
None.
