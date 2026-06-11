# vnc-034 Agent 7 — Tester Report (Stage 3c, Wave 1)

> Agent: vnc-034-agent-7-tester (Test Execution) · Date: 2026-06-11 · Result: **PASS**
> Scope: Wave-1 only (#726 + #725 + C1/C2 contract). Wave 2 (#727) NOT tested.

## Outcome

All Wave-1 risks (R-01..R-13 + rotation) have Full unit/e2e coverage. The mandatory
integration smoke gate is green against a freshly-rebuilt Wave-1 binary. One new Rust
integration target was added to close the only-visible-through-the-binary contracts. The
single red signal is a confirmed pre-existing flake on untouched code — no GH Issue warranted.

Report: `product/features/vnc-034/testing/RISK-COVERAGE-REPORT.md`

## What I ran

| Layer | Command | Result |
|-------|---------|--------|
| Rust lib unit | `cargo test -p unimatrix-server --lib` | 3945 pass / 1 pre-existing flake / 1 ignored |
| Flake isolation | `... --lib http::token::...test_concurrent_creation... -- --test-threads=1` | 1 pass (confirms flake, not feature) |
| Rust integration (4 targets) | `cargo test -p unimatrix-server --test fingerprint_parity --test cert_provisioner --test bundle_codec --test client_bundle_e2e` | 43 pass / 0 fail / 2 ignored |
| JS client | `node --test` | 840 pass / 0 fail / 1 skip (incl. remote-client.test.js 34) |
| JS hard gates | `check-zero-deps.js`, `check-hook-client-size.js` | both PASS |
| Integration smoke (MANDATORY) | `pytest suites/ -m smoke --timeout=90` (fresh release binary) | **23 pass / 0 fail** |
| Integration regression (recommended) | `pytest protocol/lifecycle/edge_cases/tools` | all-pass progress, hit ceiling on embedding cold-start cost (env-limited, no failures) |

Container constraints honored: did NOT run `cargo test --workspace` or bare
`-p unimatrix-server` (bin test target OOM-links). Ran lib + per-target integration
individually (all link fine). Rebuilt the **release** binary so the smoke gate exercises the
actual Wave-1 SlugRouter seam (the committed release binary predated the seam wiring).

## New test target added: `crates/unimatrix-server/tests/client_bundle_e2e.rs` (4 tests)

Drives the REAL compiled `unimatrix` binary end-to-end — the contracts a unit test on the
`render_output` helper cannot prove because they live at the process fd boundary:

- `test_e2e_bundle_fp_equals_served_leaf_der` — AC-W1-S4/AC-CT-C2: emitted bundle `fp` ==
  independent SHA-256 of the SERVED leaf DER == production oracle. Proves the bundle pins the
  served cert, end-to-end through the subcommand.
- `test_e2e_token_absent_from_stdout_and_stderr` — AC-W1-S5/S5b/NFR-06: token appears in
  NEITHER captured stdout NOR stderr; stdout is the opaque blob only; token round-trips
  inside the blob only; stderr echoes base-url + fp.
- `test_e2e_emitted_blob_round_trips` — R-05.3: captured blob decodes via production
  `decode_bundle` back to canonical `{v, base_url, token, fp}`.
- `test_e2e_rotation_changes_fp_old_pin_would_mismatch` — AC-CT-ROT (server half): regenerate
  cert → NEW fp → a client pinned to the old fp mismatches; re-bundle restores the pin.

Hermetic via `HOME`→TempDir (`ensure_data_directory` resolves `$HOME/.unimatrix/{hash}`),
cert provisioned with the production `load_or_generate_cert`. `cargo fmt`/`clippy` clean.

Why no new infra-001 stdio tests: the Wave-1 transport/security surface is HTTPS/bundle-shaped,
NOT stdio-shaped (test-plan OVERVIEW §4.1). infra-001's role here is the regression baseline,
satisfied by the smoke gate.

## Triage of the one red signal

`http::token::tests::test_concurrent_creation_no_corruption` fails only under full-suite
parallel lib load; passes 1/1 in isolation; `token.rs` is untouched by vnc-034. Per the
USAGE-PROTOCOL triage tree this is neither feature-caused nor a bad assertion — it is a known
pre-existing flake (flagged in the spawn brief). NOT counted as a feature failure; NO GH Issue
(a duplicate of an already-known flake adds no signal). No `xfail` added, no test deleted.

## Risk coverage gaps

None at the unit/e2e layer — every Wave-1 risk is Full. Residual gaps are **live-container
Docker-compose probes** (AC-W1-S1/S2/S6/S7, per-OS C1) that cannot run inside a single-container
session. Each degrades to a documented unit/e2e proof + flagged manual/CI walkthrough (NOT a
silent drop), and the cross-stack contracts they would also touch (served-cert==fp, token-absent)
ARE proven end-to-end through the real binary here. See RISK-COVERAGE-REPORT §Gaps.

## AC verification

All 27 Wave-1 + cross-wave AC-IDs mapped in the report. 23 PASS, 1 MANUAL-by-spec (AC-W1-C8),
4 PARTIAL (container/per-OS live probes only, with running substitute coverage). AC-CT-ROT
runbook deliverable confirmed present at `docs/cert-rotation.md`.

## GH Issues filed

None.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001 (#4954), ADR-006 (#4952),
  #4961 (C1 decoder guard ordering), #4956 (re-export a pub(crate) fn to consume from a tests/
  crate), #4962 (seam wireability). Applied #4956 (consumed Bundle/decode_bundle/
  load_or_generate_cert from the lib surface in the new tests/ crate).
- Stored: entry #4964 "E2E testing a pre-tokio CLI subcommand by driving the real binary with
  HOME-relative data dir" via context_store (pattern) — CARGO_BIN_EXE + HOME-relative data-dir
  + top-level-arg-ordering recipe for fd-split-contract integration tests.
