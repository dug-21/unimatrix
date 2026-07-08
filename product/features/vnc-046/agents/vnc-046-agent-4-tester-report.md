# Agent Report — vnc-046 Agent 4 (Tester, Stage 3c)

**Agent:** vnc-046-agent-4-tester
**Phase:** Stage 3c — Test Execution + the durable-guardrail BUILD (Rust behavioral suite + #800 HTTPS fixture)

## What I built

1. **Rust bidirectional behavioral isolation suite** — extended
   `crates/unimatrix-server/tests/project_routing_integration.rs` (+530 lines, 9 new tests).
   Drives the REAL assembled edge: pub `PathRouter` → `route_observe` →
   `resolver.resolve_store/registry_for/pending_for/services_for(&key)` → `dispatch_request`
   for `POST /v1/{slug}/observe` (RecordEvent), N=2, bidirectional. Read side asserts on the
   pub-reachable durable observable (per-slug `observations`/`entries` rows + resolver pub
   `*_for`). INV-T1 fidelity (a/b), INV-T2 isolation (a_driver/b_driver, identical cycle name,
   count + distillation-input exclusion), INV-K1/K2 (store isolation + `services_for` + durable
   non-contamination), INV-C1/C2 (bidirectional A≠B config), a non-vacuity negative control,
   white-box registry/pending ptr-identity pins (INV-T3/AC-08), and the AC-06
   coverage-enumeration table (`test_vnc046_coverage_enumeration`). Added a `tower` dev-dep
   (minimal, test-only) to invoke the pub `PathRouter`'s `Service::call`.

2. **#800 multi-slug HTTPS fixture (BUILT, in-scope)** — extended infra-001 (SR-08, not forked):
   - `harness/multi_slug_client.py` — `MultiSlugHttpServer`: boots `serve --foreground` with
     `UNIMATRIX_HTTP_ENABLED` + 2 registered slugs on ONE instance; discovers token + leaf cert
     + per-slug store dirs; drives `/v1/{slug}/observe` over HTTPS (bearer + `--cacert`-pinned
     leaf cert via stdlib urllib+ssl, no new pip dep); sqlite read-as-barrier. Reuses the
     daemon-boot idiom (`conftest.py::daemon_server`) + the wire recipe from
     `scripts/isolation-probe-lib.sh`. SHORT `/tmp` HOME to keep the UDS socket path under SUN_LEN.
   - `harness/conftest.py` + `suites/conftest.py` — `multi_slug_http_server` fixture (module
     scope; skip-clean if the local HTTPS substrate is unavailable, mirroring the parity-leg pattern).
   - `suites/test_project_isolation.py` — INV-T2 observe isolation a_driver/b_driver (smoke) +
     2×2 matrix + unknown-slug-404 (smoke), all bidirectional, marker-keyed read-as-barrier (#5347).

## Results (real numbers)

- `cargo test --workspace` (hardened form): **6982 passed, 0 failed, rc=0.**
- `tests/project_routing_integration.rs`: **30 passed / 0 failed** (9 new + 21 pre-existing).
- `cargo clippy --workspace --all-targets -- -D warnings`: **feature-clean**; only blockers are 2
  PRE-EXISTING `repeat().take()` warnings in `mcp/response/verbosity.rs:192,208` (not vnc-046) —
  **flagged for a human decision, not fixed** (per instruction).
- `#878` link smoke: **PASS (exit 0).**
- infra-001 smoke (`-m smoke`, mandatory): **32 passed, 0 failed, rc=0.**
- #800 `test_project_isolation.py`: **4 passed, 0 failed** (over real HTTPS).
- Relevant suites (protocol, tools, lifecycle, edge_cases, confidence): **<PENDING>.**
- **Negative control:** passes — the isolation predicate is non-vacuous (detects where the write
  landed + an injected foreign marker), so a real reverse mis-route trips RED.
- **No leak found.** No caused-by-feature failures for Stage 3b rework. **No xfail markers / no
  GH issues filed** (no pre-existing integration failure encountered).

## Reachability decisions (why the coverage splits)

`route_observe`/`McpAdapter`/`cycle_review` are `pub(crate)`; the external crate can't implement
`StoreResolver` (its `adapter_for` returns `pub(crate)` `McpAdapter`). So: Rust suite drives the
pub `PathRouter` and asserts the durable store layer; the model-bound cycle_review semantics
(AC-07 parity, OQ-2 `signal_class_counts`, full briefing/search reads) are wire proofs —
delivered by the #800 fixture (observe surface) + the Docker infra-003 gate + binary-crate pins.
`store_config`/`inference_config` have no pub accessor externally → documented AC-06 white-box
exceptions, pinned in the binary crate (`construction_parity_tests.rs`, non-default + ptr-identity).
The local MCP-write surface did not persist reliably under cold-warmup in this sandbox, so it was
NOT shipped as a flaky check (anti-fake-green) — the infra-003 Docker gate is its live vehicle.

## #925

NOT subsumed (different plane × granularity: metrics-plane cross-feature vs transcript-candidate
cross-slug). Stays OPEN (ADR-005). Stated in RISK-COVERAGE-REPORT + to be stated in the PR.

## Files

- `crates/unimatrix-server/Cargo.toml` (M, +tower dev-dep)
- `crates/unimatrix-server/tests/project_routing_integration.rs` (M, +530)
- `product/test/infra-001/harness/multi_slug_client.py` (NEW)
- `product/test/infra-001/harness/conftest.py` (M, +fixture)
- `product/test/infra-001/suites/conftest.py` (M, +re-export)
- `product/test/infra-001/suites/test_project_isolation.py` (NEW)
- `product/features/vnc-046/testing/RISK-COVERAGE-REPORT.md` (NEW)

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_briefing` (vnc-046 Stage 3c) + `context_get(5637)` —
  surfaced ADR-004 (#5633 bidirectional N≥2 primary gate), #5637 (make_server/from_servers tests
  need `multi_thread` flavor, applied to all 9 new tests), #5348/#5347/#5172/#5427/#5285 (governing
  isolation-test patterns). Applied directly.
- **Stored:** entry #5641 "Prove per-slug HTTP isolation via the pub PathRouter edge + durable
  store read (external tests crate)" via `/uni-store-pattern` (topic `testing`, category `pattern`)
  — the reachable-layer decision (route_observe/McpAdapter are pub(crate); drive PathRouter with a
  tower dev-dep; read observations with a read-as-barrier for the async writer; short /tmp HOME for
  SUN_LEN in the multi-slug HTTPS fixture). Genuinely novel test-infra technique for this crate.
