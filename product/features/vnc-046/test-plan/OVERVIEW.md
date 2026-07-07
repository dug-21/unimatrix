# vnc-046 — Test Strategy Overview

Per-slug state isolation for the cloud (HTTPS) observe path. This plan is rooted in
`RISK-TEST-STRATEGY.md` (R-01…R-16), `ACCEPTANCE-MAP.md` (AC-01…AC-10), and ADR-001…005.
Component test-plan files map 1:1 to the brief's Component Map.

## Test Philosophy — the durable guardrail

The **primary** gate is the bidirectional N≥2 behavioral suite through the public
`/v1/{slug}/…` interface, assembled production wiring (POST via `route_observe` → read via that
slug's `McpAdapter`). White-box units (boot assertion, `Arc::ptr_eq` wiring-pins, compile-fail
census) are **required complements, never substitutes** (AC-08/OQ-4). Two non-negotiables drive
every plan below:

1. **Bidirectional at N≥2, both directions asserted as distinct cases** (lesson #5348 /
   pattern #5347 / #5172). For every invariant: (A writes via `/v1/{A}/…` → present-in-A AND
   absent-in-B) **and** (B writes via `/v1/{B}/…` → present-in-B AND absent-in-A). A
   one-directional probe false-GREENs the symmetric reverse mis-route (R-01/SR-06) — the
   victim's own store stays correctly empty and a mis-resolved route still returns non-404.
   Neither direction may be inferred from the other. A `debug_assert!` for the un-probed
   direction is **zero release coverage** (NFR-2) and does not count.
2. **Assembled production wiring only** (R-02/#5285/#4974). No test hand-passes a
   `SessionRegistry`/`ServiceLayer` into `dispatch_request`, and no behavioral test seeds a
   server field. N=1 cannot distinguish a real funnel from a global-handle bypass — every
   isolation assertion runs at N≥2 registered slugs.

## Two Test Vehicles

| Vehicle | Location | Command | Covers |
|---------|----------|---------|--------|
| **Rust behavioral suite** | `crates/unimatrix-server/tests/project_routing_integration.rs` (EXTEND) | `cargo test -p unimatrix-server --test project_routing_integration` | AC-01…AC-08 in-process assembled wiring: `MultiProjectRouter` → `route_observe`/`SlugRouter` → `McpAdapter`; white-box pins + census |
| **#800 Python multi-slug HTTP fixture** | `product/test/infra-001/` (EXTEND, do not fork — SR-08) | `python -m pytest suites/test_project_isolation.py -v --timeout=90` | INV-C config-parity + INV-T/K isolation at the true MCP wire level over `/v1/{slug}/…` on ≥2 registered slugs |

Both vehicles are load-bearing. The Rust suite is the AC-01 vehicle and the fastest gate; the
#800 fixture proves the same invariants through the real HTTP transport (the surface a future
rewire would break) and is C6's single path to `proven`. Reuse existing helpers in each — the
Rust file already ships `build_server`, `wired_router(slugs)`, `drive(router, path)`,
`test_entry`, `entry_count`, and a config-parity section to extend.

## Risk → Test Mapping

| Risk | Priority | Where covered (component test plan) | Vehicle |
|------|----------|-------------------------------------|---------|
| R-01 one-directional false-GREEN | Critical | isolation-suite | both — bidirectional per invariant + negative-control meta-test |
| R-02 hand-passed handle bypass | Critical | isolation-suite | both — assembled wiring only; grep-gate AC-06 |
| R-03 census false-passes on threading | Critical | boot-assertion, isolation-suite | Rust white-box pin + behavioral back-stop |
| R-04 store_config/inference_config white-box gap | High | project-provisioner, boot-assertion | Rust bidirectional wiring-pin + coverage-enumeration |
| R-05 hold/registry pairing → OOM | High | project-provisioner, boot-assertion | Rust boot check + behavioral purge test |
| R-06 test-double bypass | High | resolution-funnel, project-resolver | Rust double audit + production-resolver pin |
| R-07 latent field ships global | High | boot-assertion | Rust compile-fail census + classification review |
| R-08 config seeded not derived | High | isolation-suite, project-provisioner | #800 derive-over-wire |
| R-09/R-15 knowledge-read leak + persistence | High | observe-handler, isolation-suite | both — INV-K2 bidi + persistence assertion |
| R-10 INV-T2 fold gap under identical name | High | isolation-suite | both — identical `{phase}-{NNN}`, count + distillation-input |
| R-11 hot-path latency | Medium | resolution-funnel | Rust cost-class review + parse-once assertion |
| R-12 #800 fixture reuse/owner | Medium | (this doc — Integration Harness Plan) | coordination |
| R-13 #925 not subsumed | Medium | (this doc — PR note) | doc/PR |
| R-14 500-not-404 mapping | Medium | observe-handler | Rust unit |
| R-16 UDS==HTTPS parity | Medium | isolation-suite | Rust/#800 parity, exclude wall-clock |

## Cross-Component Test Dependencies

- **resolution-funnel** (3 no-default trait methods) forces every `StoreResolver` double in
  `http/router/tests.rs` (~1982/2004/2472/2651) to implement `registry_for`/`pending_for`/
  `services_for` from its OWN `resolve_store` map (R-06). A lenient double re-admits the bypass
  inside the harness — audited in resolution-funnel + project-resolver plans.
- **project-provisioner** (`build_project_server` construction parity) is the precondition for
  every behavioral and boot-assertion test: if the registry+hold pair or the 5 config-snapshot
  fields are not wired, the funnel resolves handles that never diverge from default and the
  isolation suite has nothing to distinguish.
- **boot-assertion** `Arc::ptr_eq` pins depend on `ProjectEntry::from_server` cloning handles
  BEFORE `server` moves into `McpAdapter` (convergence-by-construction) — a shared integration
  point with project-resolver.
- **isolation-suite** consumes all of the above assembled; it is the back-stop that catches a
  set-but-not-threaded field the census (a source-assertion, #5427) cannot see.

## Integration Harness Plan (MANDATORY)

### Suites that apply (infra-001)

Per the suite-selection table, this feature touches server tool logic, store/retrieval,
confidence/config, and schema/storage. Stage 3c runs:

| Suite | Why |
|-------|-----|
| `smoke` (`-m smoke`) | **Mandatory minimum gate** — one critical path per capability; regression baseline |
| `protocol`, `tools` | Server tool logic + `ObserveContext`/`dispatch_request` reshape blast radius |
| `lifecycle` | Store→retrieval, restart persistence (schema/handle wiring change) |
| `edge_cases` | Unicode/boundary + empty-DB observe (empty-`signal_class_names`, zero-delta edge cases) |
| `confidence` | Config-driven scoring path (config-snapshot fields feed serving) |
| `test_project_isolation.py` (**NEW** — #800) | Per-slug MCP-level isolation + config-parity — the feature's own suite |

Full `suites/` run is the pre-merge gate. Existing suites are **regression guards** here: they
drive a single stdio server and must stay green (NG-4 — UDS/stdio construction untouched). Any
new failure in an existing suite is triaged per USAGE-PROTOCOL (feature-caused → fix;
pre-existing → GH issue + `xfail`).

### #800 Multi-Slug HTTP Fixture — FIRST-CLASS BUILD ITEM (in-scope, human directive)

The harness today drives a SINGLE stdio server with one `--project-dir` and has **no** multi-slug
fixture. This delivery BUILDS #800 by **extending** infra-001 (SR-08 — extend, never fork):

**Fixture scope (what Stage 3b/3c builds):**
1. **Multi-slug HTTP boot** — a `multi_slug_http_server` fixture (conftest) that writes a daemon
   `config.toml` declaring `[[projects]]` for ≥2 slugs (A, B) under a `base_dir`, enables the
   HTTP transport (`UNIMATRIX_HTTP_ENABLED=true` / `[http]`), boots `unimatrix … serve
   --foreground`, and reads the provisioned bearer token + served leaf cert from the data dir
   (reuse the existing cert/token plumbing the parity legs + `cert_provisioner` already use —
   do not invent a new TLS path).
2. **Per-slug `config.toml` placement** — under each slug's project dir, place a per-slug config
   with **genuinely different** declared values so isolation is distinguishable:
   `transcript_signal_class_names` (A vs B disjoint), `observation_registry` categories,
   `[retention]` cap, `store_config` byte-limit, `inference_config` blend. A "declared" slug and
   optionally a "not-declared" slug (deliberate default fallback — edge case).
3. **Per-slug MCP client** — extend `harness/client.py` with a client that speaks MCP JSON-RPC
   over HTTPS to `/v1/{slug}/tools/…` and observe deltas to `/v1/{slug}/observe`, bearer-authed,
   so per-slug behavior is assertable at the MCP level (mirror `client.py`'s tool methods,
   parameterized by slug/base URL). Reuse `uds_client`/`hook_client` framing patterns; no new
   transport path.

**New suite `suites/test_project_isolation.py`** — MCP-level assertions, all bidirectional at N≥2:

| Test (naming: `test_{concept}_{behavior}`) | Invariant | Assertion |
|--------------------------------------------|-----------|-----------|
| `test_transcript_fold_isolation_bidirectional` | INV-T2 (AC-02) | identical `{phase}-{NNN}` in A+B; A's `cycle_review` folds only A's; B only B's; both directions; count + distillation-input exclusion |
| `test_pending_entries_isolation_bidirectional` | INV-T3 (AC-03) | pending at A never visible via B; both directions |
| `test_knowledge_read_isolation_bidirectional` | INV-K2 (AC-04) | A's observe-path briefing/search returns none of B's; both directions; **persistence** assertion (durable store uncontaminated after distillation) |
| `test_signal_class_counts_reflect_slug_config` | INV-C1/C2 (AC-05) | A's declared `signal_class_names` → A's `signal_class_counts` (`cycle_review`); B's config never governs A's; derive over the wire, never seeded |
| `test_observation_status_reflects_slug_config` | INV-C1/C2 (AC-05) | A's `observation_registry` categories visible via A's `status`, not B's |
| `test_retention_purge_reflects_slug_config` | INV-C1/C2 (AC-05) | A's `[retention]` cap governs A's purge; B unaffected |
| `test_https_equals_uds_observe_fidelity` | AC-07 | same input via HTTPS vs UDS → equal `cycle_review` fold, exclude wall-clock (`computed_at`) |
| `test_unknown_slug_returns_404` | edge | unregistered slug 404s upstream of any write |

**Marker-keyed read-as-barrier** (pattern #5347): drive writes strictly sequentially per slug;
the own-read positive control is itself the durability barrier (bounded retry-until-present
keyed to that cell's marker), never a size-delta. Own-read timeout ⇒ INFRA (non-verdict);
cross-read marker present ⇒ RED (leak). Markers mutually non-substring, charset `[a-z0-9-]`.
Mark 2-3 of these `@pytest.mark.smoke` so the multi-slug path enters the minimum gate.

**GH coordination (R-12):** #800 is now IN-SCOPE TO BUILD in this PR (human directive) — the
brief's "OPEN dependency / unconfirmed owner" WARN is superseded. The PR resolves #800; note in
the PR that the fixture was built here, extending (not forking) infra-001.

### What NOT to add to infra-001
- Behavior already covered by the in-process Rust suite that has no distinct MCP-visible
  surface (the `Arc::ptr_eq` pins, compile-fail census) stays Rust-only — do not attempt to
  express a pointer-identity pin through MCP.
- No new suite for `store_config`/`inference_config` white-box exceptions — they are Rust
  wiring-pins (no clean public surface, AC-06 documented exception).

## #925 Reconciliation (R-13) — carry to PR

#925 (cycle-review foreign-session metrics sweep) is a **different plane × granularity**
(metrics-plane SQL, cross-feature within one slug) than INV-T2 (transcript-candidate plane,
cross-slug). It is **NOT** proven or subsumed by this suite. The RISK-COVERAGE-REPORT and PR
must state the distinction so no reviewer closes #925 as "subsumed" (ADR-005).

## Coverage Enumeration Contract (AC-06)

The behavioral suite ships an explicit coverage-enumeration table (comment or `docs` test) that,
per invariant, states **behavioral vs white-box** and names `store_config` + `inference_config`
as white-box-only documented exceptions. Absence of that enumeration is a gate failure. See
`isolation-suite.md`.

## Self-Check (Stage 3a)
- [x] OVERVIEW maps every risk (R-01…R-16) to a component plan + vehicle
- [x] Integration harness plan: suites to run, #800 fixture build as first-class item, new suite
- [x] Per-component plans (7) match the brief's Component Map 1:1
- [x] Every high-priority risk has ≥1 concrete test expectation
- [x] Bidirectional N≥2 + assembled-wiring constraints stated for every behavioral invariant
- [x] `store_config`/`inference_config` white-box exception enumerated, never omitted
- [x] Knowledge Stewardship block present

## Knowledge Stewardship
- **Queried:** `context_briefing` (vnc-046 test-plan task) + `context_search` — surfaced ADR-004
  (#5633, bidirectional N≥2 primary gate + #800 reuse), lesson #5348 (one-directional probe
  false-GREENs a reverse mis-route), pattern #5347 (bidirectional N×M tri-state read-as-barrier
  gate), #5172/#4974 (N=2 model-free / N=1 blindness), #5427 (source-assertion tests blind to
  argument threading — informs the census back-stop), #5285 (cloud parity must derive over the
  wire). Applied directly to the bidirectional/derive-over-wire/read-as-barrier requirements above.
- **Stored:** nothing novel at plan time — the governing patterns (#5347/#5348/#5172/#5427/#5285)
  already exist and are cited. A reusable "multi-slug HTTP MCP fixture" harness pattern is a
  candidate to store in Stage 3c **after** it is built and proven (topic `testing`, category
  `pattern`); deferred until it exists.
