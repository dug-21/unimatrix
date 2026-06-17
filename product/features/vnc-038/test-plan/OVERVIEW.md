# vnc-038 Test Strategy Overview

> Mandatory Project Identity at the Deployment Entrypoint. Test plans rooted in RISK-TEST-STRATEGY.md (R-01..R-15) and ACCEPTANCE-MAP.md (AC-01..AC-13). The spine is the **dumb-client invariant** (ADR-001) and the **N=2 isolation proof** (C-11 / GATE-4, #4974 ceremonial-funnel precedent). N=1 green is NOT proof.

## Test Pyramid

| Level | Surface | Drives |
|-------|---------|--------|
| Unit (Rust) | `client_bundle.rs`, `seam.rs`, `project_resolver.rs`, `projects.rs`, `config.rs`, `token.rs` | parse/encode/validate logic, atomic write, reserved set |
| Unit (JS) | `bundle.js`, `init.js`, `transport-http.js` | strict decode, verbatim-post, empty-compose-set |
| Parity corpus (cross-lang) | `tests/bundle_codec.rs` ↔ `test/remote-client.test.js` via `tests/fixtures/c1c2-parity/bundle-golden.json` | `v:2` Rust-encode/JS-decode byte equality (R-03/R-04) |
| Integration (Rust) | `tests/project_routing_integration.rs` | N=2 MCP + observe isolation, loud-first-boot, local-bypass (R-02/R-07/R-09/R-10/R-13) |
| End-to-end (#766 repro) | `tests/client_bundle_e2e.rs` + JS init/hook transport | bundle → init --bundle → 200 over `/v1/{slug}/observe`; runtime hook 200 (R-12, AC-07/08) |

**Cumulative-infra rule (C-09/NFR-07):** every test EXTENDS an existing surface above — no isolated scaffolding. The bundle parity corpus is regenerated from the Rust oracle (`bundle-golden.json`, never hand-written JS). Existing N=2 MCP fixtures in `project_routing_integration.rs` are INVERTED (the `Default`-arm tests `test_v1_tools_default_unchanged_with_projects`, `test_non_v1_path_routes_default`, `test_default_and_slug_interleaved_no_cross_contamination` must be rewritten to assert loud-error, not Default dispatch).

## Risk-to-Test Mapping

| Risk | Pri | Test Plan File | Key Assertion | AC |
|------|-----|----------------|---------------|-----|
| R-01 missed client compose site | Crit | client-attach-js, hook-transport-js, bundle-decoder-js | client-side compose-site set is **empty**; outgoing URL == bundle field byte-for-byte | AC-05 |
| R-02 ceremonial observe funnel | Crit | observe-route, boot-wiring | observe holds `Arc<dyn StoreResolver>`, resolves per-call; no boot `resolve_store(Default)`; **N=2** counting-resolver | AC-06 |
| R-03 v:2 parity break | Crit | bundle-codec-rust, bundle-decoder-js | round-trip equality + strict-reject matrix both sides; guard ordering | AC-05 |
| R-04 stale v:1 hard-cut | Crit | bundle-decoder-js, bundle-codec-rust | `obj.v !== 2` → actionable `BundleError`; no v:1 fallback either side | AC-05 |
| R-05 genesis-clobber on re-register | High | register-cli | chain-head hash equal before==after re-register; idempotent stanza | AC-02 |
| R-06 partial config write | High | register-cli | atomic temp+fsync+rename; old-OR-new; additive into N stanzas | AC-02/03 |
| R-07 delete-default breaks MCP seam | Crit | route-grammar-resolver | `parse_project_key` no `tools→Default`/`_→Default`; resolver Slug-only; call-site audit | AC-01 |
| R-08 reserved-slug drift | High | reserved-slugs | every reserved name rejected at parse edge; set derived from grammar; `tools` pinned | AC-02 (FR-13) |
| R-09 cross-pollination N≥2 | Crit | route-grammar-resolver, observe-route | **N=2** B-write never reaches A (MCP AND observe); resolve==dispatch same map | AC-06 |
| R-10 loud-first-boot regression | High | boot-wiring, route-grammar-resolver | empty `[[projects]]` ⇒ nothing servable + actionable msg; no silent default | AC-01/09 |
| R-12 init-Ping vs hook asymmetry | Crit | observe-route (e2e), hook-transport-js | AC-07 #766 repro 200; AC-08 hook 200; BOTH verbatim observe_url | AC-07/08 |
| R-13 local routed through resolver | Crit | local-binding-guard | local STDIO `:1158`/UDS `:859` direct-bind; never `parse_project_key`/resolver/`Default`/bundle | AC-10 |
| R-14 token to stdout/logs | High | token-redaction | first-boot stdout+`tracing` carry no token substring; bundle carries it; local unaffected | AC-11 |
| R-15 #735 cleanup | Low | wave1-cleanups | `router.rs` ≤500 lines; `public_url.rs` no `dead_code`/"until wiring lands" | AC-12/13 |

> R-11 (#735 collision) is SUPERSEDED by fold-in — no test, traceability only.

## Cross-Component Test Dependencies

- **Parity corpus is the shared oracle (R-03).** Any `v:2` field change must move the Rust encoder (`client_bundle.rs`) + JS decoder (`bundle.js`) + golden corpus in ONE diff. A single-side `v:2` change is an integration break by construction — `bundle_codec.rs::test_c1_bundle_golden_is_stable` and the JS `remote-client.test.js` decode-of-golden both fail.
- **register → boot read is restart-mediated (R-06).** Test the full write→restart→resolve loop in the project-lifecycle fixture, not just the write: `register` (projects.rs) writes `[[projects]]`; `load_config_and_build_allowlist` re-reads at boot; the resolver maps the slug. AC-02/03/04 span register-cli + boot-wiring + route-grammar-resolver.
- **Observe folded onto the MCP funnel (R-02/R-09).** Observe shares `resolve_store(parse_project_key(path))` with MCP but enters from a different handler. The N=2 isolation proof must cover BOTH entry points through the ONE funnel.
- **Local is NOT a resolver seam (R-13).** Local-binding-guard tests assert local bypasses everything the route-grammar/resolver/observe plans cover. The ADR-004 deletions are HTTP-only; the cross-check lives in route-grammar-resolver (R-07 sc.2) and local-binding-guard (R-13 sc.4).
- **Token redaction ∩ local (R-14 ∩ R-13).** If `token.rs:101` is shared between cloud first-boot and local, the redaction is deployment-context-gated. token-redaction sc.3 cross-checks local-binding-guard.

---

## Integration Harness Plan (infra-001)

The `product/test/infra-001/` pytest harness exercises the compiled `unimatrix-server` over the MCP JSON-RPC protocol. **This feature rewrites the cloud/container HTTP route grammar, the bundle wire form, and the observe route** — surfaces the harness exercises end-to-end.

### Applicable Suites (run in Stage 3c)

| Suite | Why it applies to vnc-038 |
|-------|---------------------------|
| `smoke` | MANDATORY minimum gate — must stay green through the route-grammar rewrite. |
| `protocol` | Handshake + tool discovery still route over `/v1/{slug}/...`; the default-alias removal must not break MCP transport. |
| `tools` | All 9 tools dispatch through the unified resolver; verifies the per-request seam survives the `Default`-arm deletion. |
| `lifecycle` | register→restart→resolve flow, store→search persistence across the resolver rewrite (schema/boot-wiring change). |
| `security` | Slug-at-the-parse-edge (path traversal, reserved-name shadow, TOML injection); first-boot token not leaked. |
| `volume` | Boot-wiring/storage change — confirm N registered projects scale through the slug map. |

> `confidence`, `contradiction`, `edge_cases` are NOT primary targets (no scoring/negation/unicode change), but `edge_cases` SHOULD run as a regression sweep since boot/empty-DB ops are touched.

### Suite-to-Risk Coverage Map

| Existing suite covers | Risk |
|-----------------------|------|
| `tools` + `protocol` per-slug dispatch | R-07 (MCP seam unbroken) |
| `lifecycle` store→restart→search | R-06 (register→boot read loop) |
| `security` slug validation / token | R-08, R-14 (partial) |

### Gaps — New Integration Tests Required (Stage 3c)

The harness today does NOT validate the vnc-038-new behavior through the MCP interface. New tests to add:

1. **`suites/test_lifecycle.py::test_observe_per_slug_route_returns_200` (R-12/AC-07/AC-08).** Drive a `v:2` bundle → POST observe to `/v1/{slug}/observe` → assert 200 (the #766 repro through the live binary). Add a runtime-hook-event variant for AC-08. Fixture: `server` (registered slug). This is the harness-level #766 closure proof.
2. **`suites/test_lifecycle.py::test_no_slug_first_boot_fails_loud` (R-10/AC-09).** Boot the binary with empty `[[projects]]`; assert MCP and observe requests fail loud with "register a project to begin", never a 200/default. Fixture: a fresh server with no registered project.
3. **`suites/test_lifecycle.py::test_two_projects_observe_isolation_n2` (R-02/R-09/C-11).** Register A and B; POST observe to `/v1/{A}/observe` writes A only; `/v1/{B}/observe` writes B only — the **N=2 observe isolation proof through the live MCP surface**. Fixture: `admin_server` or two registered slugs. Complements the Rust-level counting-resolver test; together they discharge GATE-4.
4. **`suites/test_security.py::test_v1_tools_default_alias_gone` (R-07/AC-01).** POST to `/v1/tools/...` and a no-slug `/v1` path; assert loud error (no servable store), NOT a default-store 200. Fixture: `server`.
5. **`suites/test_security.py::test_reserved_slug_registration_rejected` (R-08/AC-02).** Attempt register against each of `["v1","health","observe","tools"]`; assert rejection at the parse edge. (May be CLI-level if the harness can invoke `register`; otherwise unit-tested in reserved-slugs and noted here.)
6. **`suites/test_security.py::test_first_boot_token_not_in_logs` (R-14/AC-11).** Capture first-boot stdout/stderr from the spawned binary; assert no token substring; assert the bundle carries the token. Fixture: fresh `server` spawn capturing boot output.

### When NOT to Add Integration Tests

- Pure codec parity (R-03/R-04) — covered by the cross-language corpus, not the MCP funnel; no harness test.
- Atomic config write internals (R-06 atomicity) — Rust unit test (interrupt simulation); the harness only validates the post-restart routable outcome.
- `router.rs` line count / `public_url.rs` dead_code (R-15) — file/grep checks, no harness.
- Local STDIO/UDS direct-binding (R-13) — local is NOT an HTTP/MCP-harness surface; structure/grep guard + existing local UDS/STDIO fixture only.

### Failure Triage (Stage 3c)

Per `USAGE-PROTOCOL.md`: a harness failure CAUSED by vnc-038 → fix code, re-run, document. PRE-EXISTING/unrelated → file GH Issue + `@pytest.mark.xfail(reason="Pre-existing: GH#NNN")`, do NOT fix in this PR. The route-grammar rewrite WILL legitimately change existing `tools`/`protocol` expectations that assumed the `/v1/tools→Default` alias — those are EXPECTED test-assertion updates (triage class 3: fix the test), document each in the report.

---

## Non-Negotiable Gates (must be green for Gate 3c PASS)

1. **N=2 isolation proof (C-11/GATE-4)** — MCP AND observe, two registered projects, each request resolves once to the matching store. N=1 green REJECTED.
2. **#766 closure end-to-end (AC-07/AC-08)** — init-Ping 200 + runtime-hook 200 over the real per-slug observe route.
3. **Dumb-client invariant (AC-05)** — client-side path-composition set empty; verbatim-post byte-for-byte.
4. **v:2 parity (R-03/R-04)** — corpus round-trip + strict-reject both sides; no single-side v:2 passes.
5. **Local-regression guard (R-13/AC-10)** — local STDIO/UDS direct-bind, never resolver/parse_project_key/Default/bundle.
6. **Token redaction (R-14/AC-11)** — no token substring in first-boot stdout/logs.
7. **Carry-items (R-15)** — `router.rs` ≤500; `public_url.rs` dead_code removed.

## Open Questions
- **OQ-3 `tools` reservation:** the reserved-slugs plan pins `tools` reserved (conservative). If the human un-reserves it, the registration-rejection table + grammar-coupling test change by one row. Tester locks the chosen state so a silent flip is caught.
- **OQ-2 `token.rs:101` print scope:** if shared between cloud first-boot and local, the redaction must be deployment-context-gated (token-redaction sc.3). Delivery confirms HTTP-first-boot-only vs shared; the test plan covers both via the local-non-regression assertion.
- **Harness `register` invocation:** whether infra-001 can drive the `register` CLI directly (for gaps #5) or only the served HTTP surface. If CLI-invocation is unavailable, reserved-slug rejection stays Rust-unit-level and the harness test is dropped (documented in Stage 3c).
