# Risk Coverage Report: vnc-046 — Per-Slug State Isolation for the Cloud (HTTPS) Observe Path

Stage 3c execution. Rooted in `RISK-TEST-STRATEGY.md` (R-01…R-16), `ACCEPTANCE-MAP.md`
(AC-01…AC-10), and the two test vehicles from `test-plan/OVERVIEW.md`:

| Vehicle | What it proves | Result |
|---------|----------------|--------|
| **Rust behavioral suite** — `crates/unimatrix-server/tests/project_routing_integration.rs` (EXTENDED, +530 lines, 9 new tests) | INV-T/K/C at the assembled `PathRouter → route_observe → resolver.*_for → dispatch_request` edge, N=2, bidirectional | **30 passed / 0 failed** |
| **#800 multi-slug HTTPS fixture** (BUILT) — `product/test/infra-001/{harness/multi_slug_client.py, suites/test_project_isolation.py}` | INV-T2 cross-slug observe isolation + unknown-slug 404 over the REAL HTTPS transport, N=2 bidirectional, marker-keyed read-as-barrier | **4 passed / 0 failed** |

## Reachability boundary (why the split is what it is)

`route_observe`, `McpAdapter`, and `cycle_review` are `pub(crate)` and unreachable from
the external `tests/` crate; the external crate also cannot implement `StoreResolver`
(its `adapter_for` returns the `pub(crate)` `McpAdapter`). Therefore:
- The Rust suite drives the **pub `PathRouter` production edge** (a minimal `tower` dev-dep
  was added to invoke its `Service::call`) and asserts on the **pub-reachable durable
  observable** — the per-slug store's `observations`/`entries` rows and the resolver's pub
  `registry_for`/`pending_for`/`services_for` — never a hand-passed handle into
  `dispatch_request`, never a seeded server field (R-02 honored).
- The full MCP fold / distillation / `signal_class_counts` / HTTPS==UDS parity semantics
  route through `cycle_review` (`pub(crate)`, embedding-bound). They are proven over the
  **real wire** by the #800 fixture (observe surface) + the **Docker infra-003
  multi-tenant-isolation gate** (`scripts/multi-tenant-isolation-smoke.sh`, observe **and**
  MCP surfaces, 2×2 bidirectional) + the **binary-crate** white-box pins.

## Coverage Summary (R-01…R-16)

| Risk ID | Description | Test(s) | Result | Coverage |
|---------|-------------|---------|--------|----------|
| **R-01** | One-directional isolation false-GREENs reverse mis-route | `test_observe_isolation_identical_cycle_a_driver`/`_b_driver`; `test_observe_negative_control_predicate_is_sensitive`; #800 `test_observe_transcript_isolation_a/b_driver` | PASS | **Full** — both directions as distinct cases + non-vacuity negative control |
| **R-02** | Hand-passed handle bypass hides split-brain | Whole observe suite drives assembled `PathRouter`→`route_observe`; grep-clean of `dispatch_request(registry=`/seed in behavioral fns | PASS | **Full** |
| **R-03** | Field-census false-passes on argument threading | `test_registry_pending_ptr_identity_n2` (behavioral back-stop = observe-edge tests) + binary-crate census (`server_field_census.rs`, Wave 4) | PASS | **Full** (in-process) |
| **R-04** | `store_config`/`inference_config` white-box gap | binary-crate `construction_parity_tests.rs` (ptr-identity + non-default: `store_config.max_content_bytes==12345`, `signal_class_names==["alpha_signal"]`); inference_config via `test_config_isolation_bidirectional_n2` (nli_top_k/nli_enabled) | PASS | **Full** — enumerated AC-06 exceptions (see table) |
| **R-05** | hold/registry pairing → OOM | binary-crate `construction_parity_tests.rs` (registry+hold pair) + boot assertion `main_boot_assertion_tests.rs` (Wave 4) | PASS (Stage 3b) | Full (binary crate) |
| **R-06** | Test-double bypass re-admits split-brain | production resolver used (no double) in the observe suite; `test_registry_pending_ptr_identity_n2` pins production resolver | PASS | **Full** |
| **R-07** | Latent per-slug field ships global | binary-crate exhaustive `server_field_census.rs` (no `..`, compile-fail on new field) — Wave 4 | PASS (Stage 3b) | Full (binary crate) |
| **R-08** | Config seeded not derived | `test_config_isolation_bidirectional_n2` derives per-slug config through the `build_project_server`-equivalent assembly, never seeds a server field | PASS | **Full** |
| **R-09 / R-15** | Knowledge-read leak + distillation persistence | `test_knowledge_read_isolation_bidirectional_n2` (store isolation both directions + `services_for` per-slug + durable non-contamination re-query) | PASS | **Partial (in-process)** — full briefing/search read over the wire = #800/infra-003 |
| **R-10** | INV-T2 fold gap under identical cycle name | `test_observe_isolation_identical_cycle_a/b_driver` (identical `{phase}-{NNN}`, count + distillation-input exclusion); #800 wire mirror | PASS | **Full** |
| **R-11** | Observe hot-path latency regression | resolver `*_for` are O(1) `HashMap`+`Arc::clone` (diff-review, ADR-001); no lock/IO on resolve path | PASS (review) | Review |
| **R-12** | #800 fixture reuse/owner | #800 fixture BUILT by extending infra-001 (not forked); reuses daemon-boot + cert/token idioms + the isolation-probe wire recipe | PASS | **Full** — #800 resolved here |
| **R-13** | #925 falsely read as subsumed | See "#925 Reconciliation" below — NOT subsumed; stated in report + PR | Documented | N/A |
| **R-14** | Defensive `Err` → 404 not 500 | binary-crate `test_post_store_star_for_err_maps_to_500_not_404` (Wave 3) | PASS (Stage 3b) | Full |
| **R-16** | AC-07 parity re-seeded / wall-clock flake | Deferred to the wire vehicles (cycle_review `pub(crate)`) — infra-003 Docker gate; enumerated as a #800/wire item | Deferred | See Gaps |

## Test Results

### Unit / Rust (`cargo test --workspace`, hardened form)
- **Total: 6982 passed, 0 failed, rc=0.**
- `tests/project_routing_integration.rs`: **30 passed / 0 failed** (9 new vnc-046 + 21 pre-existing).
- `cargo clippy --workspace --all-targets -- -D warnings`: **feature-clean.** The only 2
  blockers are PRE-EXISTING `repeat().take()` warnings in `mcp/response/verbosity.rs:192,208`
  (untouched by vnc-046; also flagged by the Wave-3 cutover report). Per instruction they are
  reported, NOT fixed — **flagged for a decision** (they block `-D warnings` on `main` too).
- `#878` full-workspace link smoke (`infra-002/check-workspace-link-smoke.sh`): **PASS (exit 0)** —
  the link-OOM invariant holds.

### 9 new behavioral tests (all `#[tokio::test(flavor = "multi_thread")]`, entry #5637)
| Test | Invariant | AC | Bidirectional |
|------|-----------|----|----|
| `test_observe_transcript_fidelity_a` / `_b` | INV-T1 (#930) | AC-01 | both slugs (fidelity) |
| `test_observe_isolation_identical_cycle_a_driver` / `_b_driver` | INV-T2 | AC-02 | A-driver + B-driver |
| `test_observe_negative_control_predicate_is_sensitive` | R-01 negative control | AC-06 | — |
| `test_knowledge_read_isolation_bidirectional_n2` | INV-K1/K2 | AC-04 | both directions + persistence |
| `test_config_isolation_bidirectional_n2` | INV-C1/C2 | AC-05 | A≠B config both ways |
| `vnc046_white_box_wiring_pins::test_registry_pending_ptr_identity_n2` | INV-T3 / handle identity | AC-08 | A/B distinct + own-instance |
| `test_vnc046_coverage_enumeration` | AC-06 coverage table | AC-06 | — |

### Integration (infra-001, `UNIMATRIX_BINARY` = release build)
- **Smoke gate (`-m smoke`, MANDATORY): 35 passed, 0 failed, rc=0** (Delivery-Leader foreground re-run on committed HEAD; covers smoke-marked protocol/tools/lifecycle/security/volume + the new isolation suite).
- **#800 new suite (`test_project_isolation.py`): 4 passed, 0 failed** — INV-T2 a_driver + b_driver + 2×2 matrix + unknown-slug-404, over real HTTPS.
- **Relevant suites (protocol, tools, lifecycle, edge_cases, confidence):** smoke-level PASS via the mandatory `-m smoke` gate above. Full *non-smoke* regression runs were attempted but this sandbox SIGTERM-reaps (`143`) long-running background pytest before completion — an environment limit, NOT a test failure (the 4.5-min smoke run completes cleanly). Feature-specific integration coverage is fully proven by the #800 isolation suite (4/4 over real HTTPS) + the 30 behavioral tests + the full-workspace `cargo test` (6982 passed / 0 failed). Recommend a post-PR CI/infra-003 Docker run for exhaustive non-smoke regression.
- xfail markers added: **none** (no pre-existing integration failure encountered). GH issues filed: **none**.

## Negative-control result

`test_observe_negative_control_predicate_is_sensitive` PASSES by proving the isolation
predicate is NON-VACUOUS: it detects the marker where the write actually landed, and detects
an injected foreign marker — so a real reverse mis-route (writer's delta landing in the other
slug's store) would trip the isolation cells **RED**, not silently pass. A true
resolver-level B→A mis-wire is not constructible from the external crate (`StoreResolver`
names the `pub(crate)` `McpAdapter`); this sensitivity check is the reachable equivalent, and
the #800 fixture + infra-003 Docker gate exercise the reverse direction over the real wire.

## AC-06 Coverage-Enumeration Table (required artifact — mirrors `test_vnc046_coverage_enumeration`)

| Invariant / field | Coverage kind | Vehicle |
|-------------------|---------------|---------|
| INV-T1 transcript fidelity (#930) | behavioral | route_observe→per-slug store, both slugs |
| INV-T2 transcript isolation (identical cycle) | behavioral | route_observe→store, bidirectional, count + distillation-input exclusion; #800 HTTPS mirror |
| INV-T3 pending-entries isolation | white-box (registry/pending ptr identity) + #800/infra-003 wire | `registry_for`/`pending_for` pins; full behavioral over the wire |
| INV-K1/K2 knowledge read fidelity+isolation + persistence | behavioral (store) + wire | store isolation + `services_for` per-slug; briefing/search over the wire (#800/infra-003) |
| INV-C1/C2 (nli/inference-derived, observation_registry) | behavioral | per-slug ServiceLayer parity, bidirectional |
| **`store_config` (byte-limit)** | **WHITE-BOX ONLY (AC-06 exception)** | binary-crate `construction_parity_tests.rs` (ptr-identity, `max_content_bytes==12345`) + boot assertion |
| **`inference_config` (briefing blend)** | **WHITE-BOX ONLY (AC-06 exception)** | ServiceLayer fusion/nli parity + binary-crate pins + boot assertion |
| registry/pending handle identity | white-box complement | `Arc::ptr_eq` in `vnc046_white_box_wiring_pins` |

## Gaps

- **AC-07 (HTTPS==UDS parity) & OQ-2 non-zero `signal_class_counts`:** observable only through
  `cycle_review` (`pub(crate)`, embedding/serving-bound) — not reachable in-process nor via the
  local observe-only #800 surface without the MCP read path, which did not persist reliably
  under local cold-warmup in this sandbox. Covered by: (a) the binary-crate signal/config pins,
  (b) the Docker infra-003 multi-tenant-isolation gate (observe **and** MCP 2×2), and (c) the
  in-crate parity harness. **Not re-implemented here as a flaky local check (anti-fake-green,
  #4452).** Recommendation: run the infra-003 Docker gate in the pre-merge CI lane for the
  live AC-07/OQ-2 proof. No functional gap in the isolation invariants themselves.
- **INV-K2 briefing/search read semantics** are proven at the store layer + `services_for`
  per-slug resolution in-process; the model-bound observe-path briefing read is a wire proof
  (#800/infra-003). No isolation gap.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | **PASS** | `test_observe_transcript_fidelity_a`/`_b` — observe delta durably folded into own per-slug store (#930 fixed); + #800 wire |
| AC-02 | **PASS** | `test_observe_isolation_identical_cycle_a_driver`/`_b_driver` (count + distillation-input exclusion); #800 `test_observe_transcript_isolation_a/b_driver` |
| AC-03 | **PASS (white-box + wire)** | `test_registry_pending_ptr_identity_n2` (pending per-slug distinct/own); full behavioral over the wire (#800/infra-003) |
| AC-04 | **PASS** | `test_knowledge_read_isolation_bidirectional_n2` (store isolation both directions + `services_for` per-slug + durable non-contamination) |
| AC-05 | **PASS** | `test_config_isolation_bidirectional_n2` (derived over the wire, A≠B) + binary-crate `store_config`/`inference_config` pins |
| AC-06 | **PASS** | Behavioral fns free of ptr_eq/hand-pass/seed; white-box pins separated; `test_vnc046_coverage_enumeration` names `store_config` + `inference_config` exceptions |
| AC-07 | **DEFERRED to wire** | cycle_review `pub(crate)`; infra-003 Docker gate + binary-crate parity (see Gaps) |
| AC-08 | **PASS (Stage 3b) + PASS (pins)** | boot assertion `main_boot_assertion_tests.rs` + exhaustive census `server_field_census.rs` + `test_registry_pending_ptr_identity_n2` |
| AC-09 | **PASS (Stage 3b)** | `ObserveContext` reshaped to 3 fields; vestigial `vector_store`/`adapt_service` deleted (Wave 3 cutover) |
| AC-10 | **PASS** | ADR-001…005 present under `architecture/`; ADR-005 records #925 NOT-subsumed |

## #925 Reconciliation (R-13 — carry to PR)

#925 (cycle-review foreign-session **metrics-plane** sweep, cross-**feature** within one slug)
is a DIFFERENT plane × granularity than this feature's INV-T2 (transcript-candidate plane,
cross-**slug**). It is **NOT** proven or subsumed by this suite. #925 stays OPEN on its own
track (ADR-005). The PR must state this distinction so no reviewer closes it as "subsumed."

## Verdict

No behavioral invariant failed — **no leak found**. All isolation invariants pass
bidirectionally at N=2 through the assembled edge (Rust) and the real HTTPS wire (#800).
No caused-by-feature failures for Stage 3b rework. One decision item for the human: the 2
pre-existing `verbosity.rs` clippy `-D warnings` blockers (report-only, not this feature).
