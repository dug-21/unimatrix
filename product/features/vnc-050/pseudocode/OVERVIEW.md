# vnc-050 Pseudocode Overview — Headless Automation Access

Sources: ARCHITECTURE.md, SPECIFICATION.md, RISK-TEST-STRATEGY.md, ADR-001..007 (#5814 #5806 #5819 #5826 #5815 #5829 #5817), HUMAN-DECISIONS 1-9.
Rust root: `crates/unimatrix-server/src/`. JS root: `packages/unimatrix/lib/hook-client/`.

## Components

| ID | File | Pseudocode | Why involved |
|---|---|---|---|
| C1 | `http/token_table.rs` (new) | token-table.md | tokens.json schema, strict parse, flock+tmp+rename writer, stat-stamp snapshot |
| C2 | `http/auth/scoped.rs` (new), `http/auth.rs` | scoped-validator.md | `TokenTableValidator: BearerValidator`; inserts `BearerPrincipal` + `ResolvedIdentity` |
| C3 | `http/router/slug_guard.rs` (new), `http/router.rs` | slug-guard.md | one slug check before observe/MCP split; `http_access` audit |
| C4 | `http/principal.rs` (new) | principal.md | `TokenMode`, `BearerPrincipal`, `auth_identity`, single mode→caps table |
| C4b | `mcp/cap_ceiling.rs` (new) + 1-line touches | cap-ceiling.md | ceiling ∩ registry in `require_cap`; `records_learning` gate; R-02 step 0 |
| C5 | `infra/legacy_token_config.rs` (new), `infra/config.rs` | legacy-token-config.md | `http.legacy_token_enabled` + env override |
| C6 | `http/router/handlers.rs` | observe-gate.md | 403 pre-dispatch without SessionWrite |
| C6c | `lib/hook-client` (`index.js`, new `observe-latch.js`) | observe-403-client.md | warn once, latch, never fail hook |
| C7 | `client_bundle/{mod,args,tokens_cli}.rs`, `main.rs` | client-bundle-cli.md | `--headless/--mode/--label/--trust/--list/--revoke`; nullable fp |
| C8 | `bundle.js` | bundle-decoder.md | accept `fp:null` only |
| C9 | `remote-resolve.js` (new) | remote-resolve.md | precedence, conflicts, empty=unset, header parse |
| C10 | `mcp-bridge.js`, `mcp-bridge/http-session.js`, `transport-http.js`, `config.js` | bridge-transports.md | consume C9; pinned vs CA branch; extra headers |
| C11 | `docs/` guide + smoke script | docs-smoke.md | verbatim recipe executed by scripted smoke |

## Shared Types (defined once; every component file uses these exact names)

```rust
// http/principal.rs (C4)
pub(crate) enum TokenMode { Read, ReadWrite }               // fn capabilities(self) -> Vec<Capability>
#[derive(Clone, Debug)]
pub(crate) enum BearerPrincipal {                            // request extension
    Legacy,
    Scoped { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug> },
}
// http/token_table.rs (C1)
pub(crate) struct ScopedEntry { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug>, revoked: bool }
pub(crate) struct EntryView  { label, mode, allowed_slugs, created_at: u64, revoked_at: Option<u64> }
// mcp/cap_ceiling.rs (C4b)
pub(crate) struct CapCeiling(Vec<Capability>);
// mcp/context.rs:   ToolContext.cap_ceiling: Option<CapCeiling>
// services/usage.rs: UsageContext.learning_allowed: bool
// infra/config.rs:   HttpConfig.legacy_token_enabled: bool (serde default true)
```

```js
// remote-resolve.js (C9)
// Resolved = { mcp: {url, token, pinnedFp}|null, observe: {url, token, pinnedFp}|null,
//              headers: Array<[string,string]>, source: "bundle"|"env"|"store"|"none" }
// class ResolveError extends Error { code, varName }   // message token-free
```

Mode → capability table lives ONLY in `TokenMode::capabilities` (C4). C2 (auth-layer `ResolvedIdentity`, read by C6) and C4b (`ceiling_from`) both call it — one source (RTS integration risk).

## Data Flow

```
mint:    C7 args -> validate matrix (no I/O) -> C1.mint (flock, tmp, fsync, rename) -> encode v:2 (fp|null) -> stdout blob
request: TLS -> C2 validate (stat-stamp reload C1; legacy ct_eq; full ct scan) -> ext{ResolvedIdentity, BearerPrincipal}
         -> PathRouter::call -> C3 slug_guard::check (403 | audit http_access) -> split
            observe -> C6 SessionWrite check (403) -> dispatcher
            mcp     -> McpAdapter -> rmcp (Parts incl. extensions) -> build_context sets cap_ceiling (C4b)
                        -> require_cap(ctx,cap): permits(ceiling) && registry -> tool
                        -> UsageContext.learning_allowed = records_learning(ceiling); query_log guarded
client:  env -> C9 resolveRemote -> C10 bridge (HttpSession pinned|CA, headers) / config.js -> transport-http (observe)
         observe 403 SessionWrite -> C6c latch + one warning
```

## Wave / Dependency Ordering

| Wave | Items | Depends on |
|---|---|---|
| W0 (gate) | C4 types; C1 (read path only: `open`, snapshot, `lookup_ct`); C2 validator + layer wiring; `cap_ceiling::ceiling_from` + test probe; **R-02 step 0 test** (cap-ceiling.md §Step 0). If the test fails, apply ADR-003 fallback in `McpAdapter::handle` and re-run. | none |
| W1 | C4b rest (ToolContext field, `require_cap` signature, 15 call sites, `learning_allowed`, query_log guard); C3; C5; C6; C1 writers (`mint`, `revoke`, `list_for`) | W0 passing |
| W2 | C7 (needs C1 writers); C8; C9 | W1 (C7), none (C8, C9) |
| W3 | C10 (needs C8, C9); C6c (needs C10 config.js resolve) | W2 |
| W4 | C11 docs + smoke; compat matrix harness | all |

JS W2 (C8, C9) may run in parallel with Rust W1. C4b depends on W0 per brief.

## Backward-Compat Matrix (FR-34..39, AC-20..24, R-21)

Old client = `@dug-21/unimatrix@0.11.3` tarball (integrity hash, CI cache); fallback `git archive v0.11.3 packages/unimatrix` flagged in output.

| # | Client | Credential source | Legacy | Expected | Mechanism that holds it |
|---|---|---|---|---|---|
| 1 | old | v:2 pinned bundle via `init --bundle` | on | initialize + read + write OK (AC-20) | C2 checks legacy first → `BearerPrincipal::Legacy`; C3 no binding; ceiling `None` |
| 2 | new | same `remote.json` | on | OK, pinned; file byte-identical (AC-21) | C9 tier 3 unchanged; C10 pinned branch byte-identical |
| 3 | old | existing `remote.json` | on | OK; file byte-identical (AC-21) | server unchanged for legacy (FR-39) |
| 4 | old | env observe `UNIMATRIX_REMOTE_URL`+`_TOKEN` ± fp | on | events land (AC-22) | legacy has SessionWrite → C6 passes |
| 5 | new | env observe pair, no `_MCP_URL` | on | observe OK; bridge falls to store (AC-22) | C9 MCP env tier active only when `_MCP_URL` set |
| 6 | new | env observe pair + `UNIMATRIX_REMOTE_FINGERPRINT` | on | pinned observe | C9 env fp pins both URLs |
| 7 | new | pre-vnc-050 bundle via `UNIMATRIX_BUNDLE` | on | pinned attach (AC-24) | C8 unchanged for string fp |
| 8 | new | pre-vnc-050 bundle via `init --bundle` | on | pinned attach (AC-24) | same decoder |
| 9 | old | `UNIMATRIX_BUNDLE` | — | N/A: var unknown to 0.11.3 | — |
| 10 | old | new `fp:null` bundle via `init --bundle` | — | fails loud "fp must be sha256" (intended) | old decoder |
| 11-18 | rows 1-8 | same | off | 401 `legacy token disabled`; old client exits with existing auth error within timeout; observe non-fatal (AC-23) | C2 + C5 |
| 19 | any | no `tokens.json` on disk | on | only legacy accepted | C1 missing file = empty table |
| 20 | server | config file with no `legacy_token_enabled` key | — | parses; enabled | C5 serde default |
| 21 | stdio/UDS | — | — | unchanged | ceiling `None`; no bridge.rs change |

FR-39 diff test: fixed request set (initialize, tools/list, context_search, context_store, observe) with legacy token on old vs new server; status, header names, body shape identical except FR-11 disabled body. Every cell above is a test in test-plan/backward-compat.md or N/A with the stated reason.

## Cross-Cutting Invariants

- No secret, hash or header value in any stdout/stderr/log/audit/error body (R-09).
- Every empty `UNIMATRIX_*` value = unset; no trimming (FR-40) — applies in C5 and C9.
- Each rejection test has a success companion (#4416); Critical risks proven at the real boundary (#4970).
- `tools.rs`/`server.rs`/`main.rs`/`config.rs`: mechanical touches only (R-19).

## Test-Only Seams (Gate 3a P1)

Rule (same as cap-ceiling `PROBE`): every seam is `#[cfg(test)]` (item, field, or module). Release builds contain no symbol, field, branch or allocation from it; production control flow is identical. Global recorders are keyed by a per-test unique slug/label so parallel tests filter their own records (no `serial_test`). No other seams may be added in delivery; a missing one is a gate escalation.

| Seam | File | Shape | Used by (test-plan) |
|---|---|---|---|
| `PROBE` | `mcp/cap_ceiling.rs` | see cap-ceiling.md | cap-ceiling step 0 |
| `DISPATCH_SPY` | `http/router/handlers.rs` | `#[cfg(test)] pub(crate) static DISPATCH_SPY: Mutex<Vec<String>>`; `route_observe` pushes the request path's slug immediately before the existing `dispatch_request(` call (line ~170), inside `#[cfg(test)] { .. }` | observe-gate `read_token_observe_403_pre_dispatch`, `read_token_session_telemetry_routes_403` (assert no entry for test slug) |
| `RESOLVE_SPY` | `http/router/project_resolver.rs` | `#[cfg(test)] pub(crate) static RESOLVE_SPY: Mutex<Vec<String>>`; first statement of `ProjectResolver::resolve_store` pushes `format!("{key:?}")` under `#[cfg(test)]` | slug-guard `path_variants_never_200`, `guard_runs_before_resolve_store` |
| `StripPrincipalLayer` | `http/auth/test_layers.rs` (declared `#[cfg(test)] mod test_layers;` in `http/auth.rs`) | tower layer inserted by the test server builder between `StaticTokenAuthLayer` and `PathRouter`; `req.extensions_mut().remove::<BearerPrincipal>()` then forwards | slug-guard `missing_principal_returns_500`; cap-ceiling `empty_ceiling_denies_read_and_write`. Test server builder = existing test boot helper extended with `Option<StripPrincipalLayer>` param (cfg(test) fn, cumulative fixture) |
| `UsageService` test counters | `services/usage.rs` | `#[cfg(test)] test_hooks: Arc<UsageTestHooks>` field, `UsageTestHooks { enqueued: AtomicUsize, in_flight: AtomicUsize, idle: tokio::sync::Notify }`. `record_access` increments `enqueued` after the `learning_allowed` gate passes (i.e. only when work is scheduled); every spawned task (`tokio::spawn`/`spawn_blocking` at lines ~232, 301, 308, 341) increments `in_flight` before spawn and decrements + `notify_waiters` on completion (guard struct, also on panic). `#[cfg(test)] pub(crate) async fn flush(&self)` loops until `in_flight == 0`, 5 s timeout → panic. `#[cfg(test)] pub(crate) fn enqueued(&self) -> usize`. Accessor reachable from the in-process test server handle | cap-ceiling `gate_before_enqueue` (enqueued == 0), `read_token_learning_tables_unchanged` and observe-gate `read_token_observe_no_table_change` ("explicit flush hook"); background extraction tick uses the existing test tick trigger |
| Scan counter | `http/token_table.rs` | private `fn scan(&self, presented, visits: &mut usize) -> Option<ScopedEntry>` holds the single loop body (increments `*visits` per entry, no branch on it). `lookup_ct` = `scan(p, &mut 0)`. `#[cfg(test)] pub(crate) fn lookup_ct_counted(&self, p) -> (Option<ScopedEntry>, usize)`. Counter increment is a plain local add — no timing or result effect | scoped-validator `tombstone_match_rejected_after_full_scan`, `no_early_exit_scan_visits_all_entries` |
| `AFTER_TMP_CREATE` | `http/token_table.rs` | `#[cfg(test)] pub(crate) static AFTER_TMP_CREATE: Mutex<Option<Box<dyn Fn(&Path) + Send>>>`; `atomic_write` calls it right after `create_new(..).mode(0o600).open(tmp)` and before `write_all` | token-table `file_and_tmp_mode_0600` (hook stats tmp, asserts `0o600`) |

JS tests (C6c, C8–C10) use spawned processes, stub/capturing servers and the existing injectable `readStore`; no JS seams are added.
