# Agent Report — vnc-034 Wave 2, Agent 6 (Tester, FINISH)

Agent ID: `vnc-034-wave-2-agent-6b-tester-finish`
Stage: 3c (Test Execution — finishing a prior tester's interrupted run)
Issue: #727

## Task

A prior tester wrote `crates/unimatrix-server/tests/project_routing_integration.rs`
(640 lines) but died (socket drop) leaving it NON-COMPILING (12 errors) with no
risk-coverage report. My job: fix it (do not rewrite), run the gates, write the
reports. Production code (`src/`) untouched; no commit (leader commits).

## What I Did

### 1. Fixed 12 compile errors (single root cause, consistent fix)

Root cause: `reached_mcp(resp: &(StatusCode, String))` is THE routing discriminator —
it must inspect the response **body** (`unknown project` / `invalid project slug` are
the only funnel-emitted bodies), so it needs the full tuple. Twelve call sites passed
only the `StatusCode` (`drive(...).await.0`, `s.status()`, or a `(status, _)` binding),
giving `expected &(StatusCode, String), found StatusCode`.

Fix (consistent, preserves the body-discriminator semantics — NOT reducing to status):
- Sites passing `drive(...).await.0` → `&drive(...).await` (pass the full tuple by ref).
- Sites with `let (status, _) = drive(...)` then `reached_mcp(status)` → bind the whole
  response (`let resp = drive(...)`), call `reached_mcp(&resp)`, use `resp.0` in the
  message. (Sites at R1, R2/CT-C4, non-v1-default.)
- The CT-C4 test compares Default-path status with/without projects: changed the
  `assert_eq!` to compare `with_resp.0 == without_resp.0`.
- The N-clients test builds its own requests with custom `mcp-session-id` headers, so
  it can't use `drive`. I extracted `drive`'s body-collection into a reusable
  `collect_resp(Response) -> (StatusCode, String)` helper and used it for both
  `route_mcp` results, then `reached_mcp(&s1) && reached_mcp(&s2)`.

No production code changed. No test coverage removed. No stubs/TODOs. The one new
helper (`collect_resp`) is a refactor of existing `drive` internals (cumulative, no
isolated scaffolding).

### 2. Ran the gates (actual numbers, let cargo finish)

- **HTTP integration:** `cargo test -p unimatrix-server --test project_routing_integration`
  → **10 passed, 0 failed** (0.20s).
- **infra-001 smoke** (backward-compat gate): built `target/release/unimatrix`, then
  `python -m pytest suites/ -m smoke --timeout=60`
  → **23 passed, 351 deselected** (199.40s). Green = resolver swap did not regress the
  single-project Default path. infra-001 CANNOT reach the `/v1/{slug}/` edge (single-
  project stdio); its role is strictly the Default-path regression gate — stated as
  such in the report.
- **Unit suite:** NOT re-run (leader-verified GREEN, 4002 passed at Gate 3b). I instead
  confirmed via Grep that every named D1/D4/D5/D6/funnel/OQ-CLI-7/seam/no-payload unit
  test cited in the report exists in source, and mapped the report to the
  **as-implemented** names (several differ from the Stage-3a plan working names).

### 3. Wrote `wave2/testing/RISK-COVERAGE-REPORT.md`

Maps every Wave-2 AC (R1–R6), CT-C4/CT-C6, all D1/D4/D5/D6 discriminators, the funnel
no-bypass invariant, and OQ-CLI-7 chain-preservation to its passing test (unit or
integration). Includes unit + integration + infra-001 counts and the infra-001
backward-compat role. Cannot-drive cases stated honestly.

## Results

- Integration test: **10 / 10 PASS**.
- infra-001 smoke: **23 / 23 PASS** (backward-compat gate green).
- Unit: 4002 PASS (leader-verified; named tests confirmed present).

## Cannot-Drive Cases (honest)

1. **AC-W2-R4 CLI lifecycle + D4/D5/D6 + OQ-CLI-7** — registry/CLI ops, no HTTP verb
   registers a project; covered by `src/projects/tests.rs` unit tests (all PASS).
2. **No-payload-project-field (R-06/FR-X2)** — unrepresentability invariant; no
   "mis-target a second slug" request is *constructible* (`ProjectKey` is path-derived
   only). Structural; the integration file proves the positive path-derived binding
   (session headers don't change the resolved slug).
3. **AC-CT-C6 seam-not-collapsed** — structural type/trait presence, not a single-
   request runtime behavior; unit-covered.
4. **infra-001 slug routing** — structurally unreachable (single-project stdio);
   by-design, slug routing proven by the Rust integration file.

No silent stubs; no TODOs.

## Issues

- None blocking. Two pre-existing flakes noted as NON-regressions (pass in isolation,
  unrelated to the resolver swap, out of scope to fix here):
  `http::token::tests::test_concurrent_creation_no_corruption`,
  `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous`. No xfail markers
  added; no GH issue required.
- Stage-3a plan test names drifted from as-implemented names (e.g.
  `test_no_payload_project_field` → covered by
  `test_per_request_slug_rejected_at_funnel_not_default_store`;
  `test_only_health_unauthenticated` → covered by the `health_*_no_bypass` family).
  Report cites the real names. Not a defect — naming-only drift.

## Files Touched

- `crates/unimatrix-server/tests/project_routing_integration.rs` (fixed 12 errors +
  `collect_resp` helper; coverage intent preserved).
- `product/features/vnc-034/wave2/testing/RISK-COVERAGE-REPORT.md` (new).
- `product/features/vnc-034/wave2/agents/vnc-034-wave-2-agent-6-tester-report.md` (this).

No `src/` changes. No commit.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced #4963 (vnc-034 build-but-
  unwireable seam trap / `PathRouter` resolver injection — directly relevant: the
  integration file drives the real `SlugRouter` over the injected `Arc<dyn StoreResolver>`),
  #4952 (ADR-006 wave→issue mapping), #4968 (Wave-1 split-defect: seam built but not
  dispatched). All consistent with the test's design; nothing contradicted.
- Stored: nothing novel to store. The discriminator-helper fix (body-not-status as the
  funnel signal, full-tuple by reference) is a local test-mechanics fix on a pattern
  already captured by #4963 (seam wired via injected resolver); the `collect_resp`
  refactor is a standard "extract the body-collection from the request-driver" move
  with no reuse value beyond this file. No new fixture, harness technique, or
  cross-feature integration pattern emerged that future agents would search for.
