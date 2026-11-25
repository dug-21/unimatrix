# vnc-050 Test Plan Overview

Source: RISK-TEST-STRATEGY.md (R-01..R-22), ACCEPTANCE-MAP.md (AC-01..AC-29), ADR-001..007.

## Strategy

| Layer | Tooling | Purpose |
|---|---|---|
| Rust unit | `#[test]` in new modules (`token_table`, `scoped`, `slug_guard`, `principal`, `cap_ceiling`, `legacy_token_config`, `client_bundle/args`) | Pure logic: parse, predicates, flag matrix |
| Rust HTTP integration | `crates/unimatrix-server/tests/headless_access_http.rs` (new) — boots the real HTTPS listener on an ephemeral port with a temp `data_dir`, drives it with a raw hyper/reqwest client | Critical risks R-02, R-03, R-04, R-05, R-15, R-16, R-17 at the real socket |
| Rust CLI E2E | extend `tests/client_bundle_e2e.rs` — spawns the built binary | R-12, R-14, AC-16/17 |
| JS unit | `packages/unimatrix/test/hook-client/remote-resolve.test.js`, `bundle` corpus | R-07, R-08 parse, R-10, R-18 |
| JS spawned-process | spawn `mcp-bridge.js` / hook process with clean HOME against `test/helpers/real-server.js` or a byte-capturing TLS server | R-01, R-07, R-08, R-09, R-11, R-17 client side, R-18 |
| Compat harness | `packages/unimatrix/test/compat/` — `@dug-21/unimatrix@0.11.3` tarball vs new server | R-21, AC-20..24, FR-39 |
| Doc smoke | `scripts/` smoke extracting recipe blocks from the remote-access guide | AC-12, AC-13 (delivery) |
| infra-001 | `multi_slug_http_server` fixture, new `suites/test_headless_access.py` | MCP-visible ceiling/slug behavior through the harness |

Rules applied to every plan file:
- Critical risks are discharged only by real-boundary tests (socket / HTTP / spawned process). Seam tests (`resolveRemote`, `slug_guard::check`, `require_cap`) are supplementary and never discharge a behavioral-outcome row (#4970).
- Every rejection test has a named success companion (#4416).
- Sentinel secrets: tokens are generated as `ab` repeated hex sentinels (`SENTINEL_TOKEN`), header values `X-SENTINEL-<uuid>`; R-09 greps every captured stream for them.

## Gating order

1. **R-02 step 0 (permanent)**: `principal_visible_in_tool_request_context_over_http` (cap-ceiling.md). Runs before any other ceiling test; failure → ADR-003 fallback, re-run. Never removed (rmcp upgrade guard).
2. C1/C2/C5 unit → C3/C4/C4b/C6 HTTP integration → C7 CLI E2E.
3. C8/C9 unit → C10 spawned bridge → C6c hook → compat → docs smoke.

## Risk-to-Test Mapping

| Risk | Pri | Plan file(s) | Key boundary test |
|---|---|---|---|
| R-01 | Crit | bridge-transports | byte-capturing TLS server, zero app bytes (bridge, observe POST, pingForInit) + guard throws |
| R-02 | Crit | cap-ceiling, principal | step 0; write-tool × agent_id table over HTTP; rw companion; empty-ceiling via `StripPrincipalLayer`; Admin tools quarantine+enroll; narrow-only |
| R-03 | Crit | slug-guard | route × token × slug matrix; path variants; 403 body identical |
| R-04 | Crit | cap-ceiling | table snapshot before/after read batch + drain; rw/legacy/stdio companions |
| R-05 | Crit | token-table, scoped-validator, client-bundle-cli | mint→200→CLI revoke→401 no restart; open session; same-tick inode; corrupt/missing file |
| R-06 | Med | scoped-validator | index 0 / n-1 match; static no-HashMap/no-break check |
| R-07 | Crit | remote-resolve, bridge-transports | precedence matrix via resolver AND spawned bridge + config.js; bundle-only observe lands |
| R-08 | High | remote-resolve, bridge-transports | capturing proxy per request type; reject matrix |
| R-09 | High | bridge-transports, client-bundle-cli, slug-guard | sentinel grep of all outputs |
| R-10 | High | bundle-decoder | Rust↔JS parity corpus; `init --bundle` CA → `fingerprint:null` |
| R-11 | High | bridge-transports, backward-compat | `git diff main` empty on pin tests; 3 fp sources vs self-signed server |
| R-12 | High | token-table | 20 parallel mints; 0600; no tmp left; reader never sees partial |
| R-13 | High | principal | `token:ci` InvalidInput on stdio/UDS/HTTP-legacy/HTTP-scoped; no `token:` agent rows |
| R-14 | Med | client-bundle-cli | flag matrix exit 2, file mtime unchanged |
| R-15 | High | legacy-token-config | env parse matrix; config×env matrix; disabled 401 body |
| R-16 | Med | slug-guard | http_access rows per call; denial in bound slug log; sink failure non-fatal |
| R-17 | Crit | observe-gate, observe-403-client | `DISPATCH_SPY` empty; tables unchanged; spawned hook one warning across N spawns, per-credential marker latch (fail-open) |
| R-18 | High | remote-resolve, bridge-transports | bundle + each conflict var (bridge + hook); headers allowed; empty = unset; partial pair |
| R-19 | Low | OVERVIEW (gate) | `git diff --stat main` line delta for tools.rs/server.rs/main.rs/config.rs/usage.rs |
| R-20 | Acc | cap-ceiling | regression pin: rw + `human` writes; http_access has real label; Admin denied |
| R-21 | Crit | backward-compat | 0.11.3 matrix × legacy enabled/disabled; FR-39 response diff |
| R-22 | Acc | cap-ceiling, principal | rw + omitted agent_id → `anonymous`; read + omitted → denied |

## Cross-Component Dependencies

- C1 → C2 (validator reads `TokenTable` snapshot) → C4 (principal inserted) → C3 (guard reads principal) and C4b (ceiling reads principal from `Parts.extensions`) and C6 (observe reads `ResolvedIdentity`). C4b and C6 must derive from one `TokenMode::capabilities` table: test `observe_and_tool_ceiling_agree_per_mode`.
- C7 writes the file C1 reads: AC-16 E2E uses the binary for mint/revoke against a running server process.
- C8 ← C9 ← C10/config.js. C10 and config.js must call C9 (R-07 parity test spawns both).
- Slugs provisioned after boot: `ceiling_and_guard_apply_to_slug_provisioned_after_boot` (slug-guard.md).

## Integration Harness Plan (infra-001)

Feature touches server tool logic (capability check), HTTP auth/routing, and usage recording.

Existing suites to run in Stage 3c:
| Suite | Reason |
|---|---|
| `-m smoke` | mandatory gate |
| `test_tools.py`, `test_protocol.py` | 15 `require_cap` call-site swaps; `UsageContext` change |
| `test_security.py` | capability enforcement, `token:` reserved prefix input validation |
| `test_lifecycle.py`, `test_confidence.py` | learning gate must not change stdio/legacy behavior (usage/helpful counts still move) |
| `test_project_isolation.py`, `test_transport_health.py`, `test_https_uds_parity*.py` | HTTP path routing, slug, legacy token path, `/health` stays unauthenticated |
| `test_edge_cases.py` | input validation boundaries |

Gaps (MCP-visible behavior not covered by existing suites): scoped tokens on the HTTP transport.

New tests — `suites/test_headless_access.py`, reusing `multi_slug_http_server` (extend fixture with an optional `tokens` param that writes `{data_dir}/tokens.json` before boot via the real `client-bundle --headless` binary; no isolated scaffolding):
- `test_read_token_search_succeeds_store_denied` (success + rejection pair, `agent_id:"human"`)
- `test_read_write_token_store_succeeds_on_bound_slug`
- `test_scoped_token_other_slug_returns_403`
- `test_legacy_token_still_works_all_slugs`
- `test_revoked_token_returns_401_without_restart`
- `test_read_token_observe_returns_403`
- `test_token_prefix_agent_id_rejected` (also add stdio variant to `test_security.py`: `test_agent_id_token_prefix_reserved`)

Failures outside these behaviors follow USAGE-PROTOCOL.md triage (pre-existing → GH issue + xfail).

## Out of Delivery Scope

AC-13 live (GH Actions) and AC-14 (Tailscale) are post-delivery operator checks; no tests planned. Only the scripted local AC-13 equivalent (docs-smoke.md) is a delivery test.

## Test-Only Seams
All seams are those in pseudocode/OVERVIEW.md "Test-Only Seams" (`PROBE`, `DISPATCH_SPY`, `RESOLVE_SPY`, `StripPrincipalLayer`, `UsageService` flush/enqueued, `lookup_ct_counted`, `AFTER_TMP_CREATE`); spy assertions filter by per-test slug. JS tests use no seams. No others may be added.
