# Agent Report: crt-056-agent-8-rework-tests

> Phase: Test Execution rework (Gate 3c REWORKABLE FAIL — AC-1/AC-2 vacuous-green coverage gap).
> Branch `feature/crt-056`. Builds on accessor commit `9ccde2a9`.

## Gap closed

The prior RISK-COVERAGE-REPORT marked AC-1 (8-field config parity) and AC-2 (one shared model)
PASS with NO implementing tests (#4202/#3935 vacuous-green). Both are now closed with real,
mutation-verified tests added cumulatively to
`crates/unimatrix-server/tests/project_routing_integration.rs` (reusing the existing
`build_server`/harness assembly — no isolated scaffolding).

`http_provision::build_project_server` lives in the binary crate (`main.rs` private mod) and is
unreachable from the external `tests/` crate, so both tests drive its EXACT public assembly path —
`ServiceLayer::new(<threaded resolved Arcs>)` → `UnimatrixServer::new(.., Some(layer))` (the literal
`build_project_server` body minus disk glue). Documented as such in the test (not claimed to call
the private fn).

## New tests (both PASS)

| Test | AC | Result |
|------|----|--------|
| `test_per_slug_service_layer_config_parity_8_fields` | AC-1 / R-05 | PASS |
| `test_shared_nli_model_across_n2_slugs` | AC-2 / R-12 / R-04 | PASS |

- AC-1: builds a per-slug server from an NLI-ENABLED, NON-DEFAULT resolved config; asserts ALL 8
  parity fields field-by-field (not a subset): `nli_enabled` (BOTH directions — enabled-cfg⇒on,
  disabled-cfg⇒off), `nli_top_k`=37, `nli_handle` (`Arc::ptr_eq`), `fusion_weights` (`==` + `!=
  default`), `confidence_params` (`Arc::ptr_eq` + value `!= default`), `category_allowlist`
  (`Arc::ptr_eq` + operator category present), `observation_registry`/domain packs (`Arc::ptr_eq`),
  `ml_inference_pool().pool_size()`=5. `session_capabilities` NOT asserted (OUT, ADR-006).
- AC-2: N=2 per-slug servers share the ONE `nli_handle` Arc — each `Arc::ptr_eq` the daemon's and
  each other's, `Arc::strong_count >= 3`. The shared Arc proves no per-slug `NliServiceHandle::new()`.

## Non-vacuity proof (mutation)

Threaded a fallback `nli_top_k` (20 instead of the resolved 37): the AC-1 test FAILS at
`assert_eq!(.., 37, "AC-1.2 ...")` with RC=101. Reverted. A degraded slug that fell back to a
test-default ServiceLayer would be caught. (Also confirmed `nli_enabled` is a SEPARATE scalar from
`inference_config.nli_enabled` in `ServiceLayer::new` — the test asserts the threaded scalar.)

## Test runs

- Targeted: `cargo test -p unimatrix-server --test project_routing_integration --jobs 1` →
  **21 passed; 0 failed** (was 19; +2). Hardened invocation.
- Regression: `cargo test -p unimatrix-server --jobs 1` (hardened, `setsid -w` + ceiling + file) →
  **RC=0**, no FAILED / panicked in captured output. `--jobs 1` per the sandbox OOM note.
- fmt: additions hand-formatted; `cargo fmt -- --check` clean within my added line range (did NOT
  fmt the whole file — would reflow unrelated existing tests). Clippy: no new warnings on the test.

## Docs corrected (truthful)

- `testing/RISK-COVERAGE-REPORT.md`: AC-1/AC-2 verification rows now cite the real tests by name;
  R-05 downgraded-from-"unit" to Full (field-by-field); R-04/R-12 cite the shared-handle test;
  behavioral-test table + counts (19→21, total 4438→4440) updated; Executive Summary notes the
  rework; new Gap 4 marks the **embedding-model** share **Partial (structural)** honestly (NLI
  handle is behavioral; embed handle is source-confirmed `Arc::clone` in `build_project_server`,
  model-free harness can't compare it in-test).
- New **Tick-path scope** section in the report + a **Scope correction** in
  `test-plan/wave2-gating-audit.md`: corrected the inaccurate "sole tick path / no longer wired /
  removed entirely" claim. Truthful statement (matching code `9ccde2a9`): the global-handle tick is
  **RETIRED on the multi-project HTTP daemon path**; the **stdio single-store path (N=1)** retains
  the legacy `spawn_background_tick` as an **accepted carve-out** (NFR-5 hazard requires N≥2 sharing
  global handles, impossible on a single store).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #4014/#4320/#3817/#4028 (InferenceConfig
  dual-default trap + multi-field struct edit batching), #2552 (Arc params-at-end on ServiceLayer),
  #4977/#4202/#3935 (silent-early-return / vacuous-pass lessons — directly motivated the mutation
  proof). Applied.
- Stored: entry **#5175** "Config-parity tests: drive the binary-crate provisioner's public
  assembly from the external test crate" via `/uni-store-pattern` (topic `testing`, category
  `pattern`) — the reusable technique for closing a vacuous-green AC when the provisioning
  entrypoint is binary-crate-private.
