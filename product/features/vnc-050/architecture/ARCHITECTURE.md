# vnc-050 Architecture: Headless Automation Access

## System Overview
vnc-050 adds least-privilege, non-interactive access to the personal-cloud HTTPS listener, and it does this by extending the existing setup flow (`register` → `client-bundle` → attach). It adds no new creation mechanism.

- **Server side**: a scoped token table sits behind the existing `BearerValidator` seam. One slug guard in `PathRouter` enforces `allowed_slugs`. HTTP layer enforces authentication, tombstone revocation, slug binding and the observe `SessionWrite` check. At the MCP layer the token mode is a capability ceiling intersected with the registry grants of the self-asserted `agent_id`; the intersection only narrows (ADR-003). Read-token tool calls record no learning (ADR-004).
- **Client side**: one resolver module serves both the MCP bridge and the observe path. It handles `UNIMATRIX_BUNDLE`, the env pair and the credential store. When a fingerprint is present, the client pins it exactly as today. When there is none, the client uses standard CA validation.
- Local STDIO/UDS and `bridge.rs` are unchanged.

## Component Breakdown

| # | Component | Location | Responsibility | ADR |
|---|---|---|---|---|
| C1 | Token table | `crates/unimatrix-server/src/http/token_table.rs` (new) | Defines the `tokens.json` schema. Parses and validates the file, does atomic locked writes, and keeps a live-reload snapshot. Revoke writes a tombstone (`revoked_at`); records are never deleted. | 001 |
| C2 | Scoped validator | `http/auth/scoped.rs` (new); `http/auth.rs` wires it | `TokenTableValidator: BearerValidator`. Checks the legacy token and does a constant-time full scan. Inserts `BearerPrincipal` and `ResolvedIdentity`. Honors `http.legacy_token_enabled`. | 001 |
| C3 | Slug guard + access audit | `http/router/slug_guard.rs` (new); one call in `http/router.rs` | Returns 403 for a slug outside the binding on every `/v1/` route. Writes the per-request `http_access` audit row. | 002 |
| C4 | Principal mapping | `http/principal.rs` (new) | `BearerPrincipal` → auth-layer `ResolvedIdentity` (`token:<label>`), mode → capabilities. Used by C3/C6, audit and C4b. | 003 |
| C4b | Capability ceiling | `mcp/cap_ceiling.rs` (new); 1 line in `server.rs` `build_context_with_external_identity`, `require_cap` signature + ceiling call; `ToolContext.cap_ceiling` field in `mcp/context.rs`; `tools.rs` call-site arg swap + `learning_allowed` lines; `UsageContext.learning_allowed` + early return in `services/usage.rs` | Reads `BearerPrincipal` from rmcp `Parts.extensions`, intersects with registry in `require_cap`, gates usage/co-access/query_log recording | 003, 004 |
| C5 | Legacy-token config | `infra/legacy_token_config.rs` (new); 3-line hook + 1 field in `infra/config.rs` | `http.legacy_token_enabled` (default true) + `UNIMATRIX_LEGACY_TOKEN_ENABLED` override | 001 |
| C6 | Observe gate | `http/router/handlers.rs` | 403 on observe/session routes without `SessionWrite` | 004 |
| C6c | Observe 403 handling (client) | `lib/hook-client` observe POST path | Warn once per credential, persistent marker latch, never fail the hook | 004 |
| C7 | client-bundle CLI | `client_bundle.rs` → `client_bundle/{mod.rs,args.rs,tokens_cli.rs}`; `main.rs` variant swap | Handles `--headless/--mode/--label/--trust`, `--list` and `--revoke`. Enforces the flag matrix. Makes the bundle's `fp` nullable. | 005 |
| C8 | JS bundle decoder | `packages/unimatrix/lib/hook-client/bundle.js` | Accepts `fp:null`. Rejects everything else exactly as today. | 005 |
| C9 | Remote resolver | `packages/unimatrix/lib/hook-client/remote-resolve.js` (new) | Applies the bundle > env pair > store precedence, fails loud on conflicts, parses headers | 006, 007 |
| C10 | Bridge / transports | `mcp-bridge.js`, `mcp-bridge/http-session.js`, `transport-http.js`, `config.js` | Consume C9. Split TLS into a pinned branch and a CA branch. Apply the extra headers. | 006, 007 |
| C11 | Docs + smoke | `docs/` remote-access guide; `product/features/vnc-050/` Tailscale findings; scripted smoke | Direct-HTTPS curl / generic MCP client recipe, GH Actions example, Tailscale findings | — |

## Component Interactions / Data Flow

**Mint**
`unimatrix client-bundle alpha --headless --mode read --label ci-read [--trust ca]`
1. C7 validates the flags.
2. It generates 32 random bytes and writes the entry under `flock` with tmp+rename (C1).
3. It encodes the v:2 bundle, using the new token and `fp` (or `null` for `--trust ca`).
4. The blob goes to stdout. The label, mode, slug and URLs are echoed to stderr.

**Attach**
`UNIMATRIX_BUNDLE=<blob> node mcp-bridge.js`
1. C9 resolves the credential: `decodeBundle` gives `{mcp_url, observe_url, token, fp}`, and the headers are parsed.
2. C10 `HttpSession` picks the pinned branch when `fp` is set and the CA branch when it is null.
3. Every request carries `Authorization` plus the extra headers.
4. The hook/observe path (`config.js`) resolves through C9 as well, so it posts to the bundle's `observe_url` using the same trust settings.

**Request**
1. TLS.
2. C2 auth layer: on failure, 401. On success it inserts `BearerPrincipal` and `ResolvedIdentity`.
3. `PathRouter::call` runs C3 `slug_guard::check`: `parse_project_key`, then the scoped binding check (403 if it fails), then the `http_access` audit row.
4. Routing:
   - Observe: `route_observe`. A missing `SessionWrite` is a 403 before the dispatcher (C6). The client warns once and stops posting (C6c).
   - MCP: `route_mcp` → rmcp (injects `Parts` incl. extensions) → `build_context_with_external_identity` sets `ToolContext.cap_ceiling` from `BearerPrincipal` → `require_cap(ctx, cap)` = ceiling ∩ registry(self-asserted `agent_id`) → tool; learning recorded only if `records_learning` (ADR-003/004).

**Revoke**
`client-bundle alpha --revoke ci-read`
1. C1 sets `revoked_at` on the record and rewrites the file. The record and label are retained forever.
2. On the next request (including the next request of an open MCP session), C2 sees a new stat stamp and reloads, so the revoked token gets 401.

## Technology Decisions
- SHA-256 via the existing `sha2` dependency and `subtle::ConstantTimeEq`, both already in the workspace. No new crates.
- `flock` through `libc`/`fs2`, whichever the workspace already uses. Pseudocode must confirm. If neither is present, use `std::fs::File::try_lock`, which is stable since Rust 1.89.
- JS: Node `https` and `crypto` only. No new npm dependencies.
- Live reload uses a per-request `stat`, not a file watcher. It is portable across container volumes and has no extra threads.

## Behavioral Outcome Coverage (from SCOPE-RISK-ASSESSMENT entry points)

| Entry point | How the outcome holds from the user's invocation path |
|---|---|
| `client-bundle <slug> --headless --mode M --label L` | The C7 mint path. The secret appears only in the stdout blob. The entry is visible to `--list`. |
| `--list` | C7 reads C1. Prints label, mode, slugs, created_at and revoked state, tombstones included. No hash or secret. |
| `--revoke L` | C1 tombstone → C2 stamp reload → 401 on the next call (ADR-001). Unknown or already-revoked label is an error. Label never reusable. |
| `UNIMATRIX_BUNDLE` alone | C9 tier 1 feeds both the bridge and the observe path (SR-15). Needs no `init` and no `remote.json`. |
| `UNIMATRIX_REMOTE_MCP_URL` + `_TOKEN` | C9 tier 2. A partial pair exits 1. Messages never include the token. |
| `UNIMATRIX_REMOTE_HEADERS` | ADR-007. Applied on initialize, POST, SSE GET, DELETE and observe. Reserved names fail loud. |
| Fingerprint present (bundle or env) | The existing pinned branch is untouched, and the existing pin tests pass unchanged (AC-05). |
| No fingerprint, bad cert | The CA branch rejects at the handshake, and the server receives zero app bytes (SR-01). |
| Read token → observe / session | 403 before the dispatcher (C6); client warns once, job continues (C6c). |
| Read token → MCP write tool | `CapabilityDenied` regardless of claimed `agent_id` (ceiling, ADR-003). |
| Read token → MCP read/search tool | Served; no usage, co-access or query_log rows (ADR-004). |
| Read-write token claiming `human` → Admin tool | `CapabilityDenied` (ceiling never includes Admin). |
| Token bound to A → `/v1/B/...` | The C3 guard returns 403 on every route (SR-10). |
| Legacy `{data_dir}/token` | All slugs, read-write, today's semantics. `http.legacy_token_enabled=false` or `UNIMATRIX_LEGACY_TOKEN_ENABLED=false` → 401 "legacy token disabled". |
| curl / generic MCP recipe | Exercised by the scripted smoke test (local CA plus test server), so AC-12 and the local equivalent of AC-13 are proven in CI (SR-08). The live tailnet run for AC-14 is marked operator-run. |

**Inputs that are accepted but not honored are rejected loudly:**
- `--mode`/`--label` without `--headless` (SR-18).
- Bundle combined with `UNIMATRIX_REMOTE_MCP_URL`, `UNIMATRIX_REMOTE_URL`, `UNIMATRIX_REMOTE_TOKEN` or `UNIMATRIX_REMOTE_FINGERPRINT` (SR-19). Bundle + `UNIMATRIX_REMOTE_HEADERS` is allowed.
- `UNIMATRIX_REMOTE_FINGERPRINT` without a URL.
- `UNIMATRIX_LEGACY_TOKEN_ENABLED` set to an unrecognized value (boot error).
- Bad header JSON (SR-04).
- `--revoke` of an unknown label.
- `--trust` combined with `--list`/`--revoke`.
- Minting a label held by any record, active or revoked.

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| Token file | `{data_dir}/tokens.json` 0600: `{"v":1,"tokens":[{"label":String,"sha256":"<64hex>","mode":"read"\|"read-write","allowed_slugs":[ProjectSlug],"created_at":u64,"revoked_at":u64|null}]}`; lock `{data_dir}/tokens.lock` | C1 (new) |
| `TokenMode` | `pub(crate) enum TokenMode { Read, ReadWrite }`; `fn capabilities(self) -> Vec<Capability>` | `http/principal.rs` |
| `BearerPrincipal` | `pub(crate) enum BearerPrincipal { Legacy, Scoped { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug> } }` (request extension) | `http/principal.rs` |
| `TokenTable` | `pub(crate) struct TokenTable`; `fn open(data_dir:&Path) -> Self`; `fn lookup_ct(&self, presented:&[u8;32]) -> Option<ScopedEntry>`; `fn mint(&self, label, mode, slug) -> Result<[u8;32], ServerError>`; `fn revoke(&self, label, slug) -> Result<(), ServerError>` (tombstone); `fn list_for(&self, slug) -> Vec<EntryView>` | C1 |
| `TokenTableValidator` | `impl BearerValidator`; `fn new(legacy: Option<[u8;32]>, table: Arc<TokenTable>) -> Self` | C2 |
| Legacy disable | `HttpConfig.legacy_token_enabled: bool` (serde default `true`); env `UNIMATRIX_LEGACY_TOKEN_ENABLED` = `true\|1\|false\|0`, empty = no override, other = boot error; `pub(crate) fn resolve_legacy_token_override(Option<&str>) -> Result<Option<bool>, ConfigError>` | C5 |
| Slug guard | `pub(crate) fn check<B>(req:&Request<B>, audit_sink:&dyn AccessAudit) -> Result<(), Response<BoxBody<Bytes,Infallible>>>` | C3 |
| Access audit row | `AuditEvent{operation:"http_access", credential_type:"static_token"\|"scoped_token", agent_id:"token:<label>"\|"http-bearer", metadata:{"token_label","method","route"}}` | C3, existing `AuditEvent` columns |
| Ceiling | `pub(crate) struct CapCeiling(Vec<Capability>)`; `pub(crate) fn ceiling_from(rc:&RequestContext<RoleServer>) -> Option<CapCeiling>`; `pub(crate) fn permits(c:Option<&CapCeiling>, cap:Capability) -> bool`; `pub(crate) fn records_learning(c:Option<&CapCeiling>) -> bool` | `mcp/cap_ceiling.rs` (C4b) |
| `ToolContext.cap_ceiling` | `pub cap_ceiling: Option<CapCeiling>` | `mcp/context.rs` |
| `require_cap` | `pub(crate) async fn require_cap(&self, ctx:&ToolContext, cap:Capability) -> Result<(), rmcp::ErrorData>` (was `agent_id:&str`) | `server.rs:631` |
| `UsageContext.learning_allowed` | `pub(crate) learning_allowed: bool` | `services/usage.rs` |
| Auth identity | `pub(crate) fn auth_identity(p:&BearerPrincipal) -> ResolvedIdentity` (`token:<label>` / existing `http-bearer`) | C4 |
| Observe 403 | status 403, body `{"error":"token lacks SessionWrite"}` | C6 |
| Client observe latch | `stateDir/observe-denied.json` (0600) holding SHA-256 of credential; latch only on 403 `token lacks SessionWrite`; one warning per credential across processes; no expiry; missing/corrupt marker = retry; write failure never changes exit/output | C6c |
| CLI | `client-bundle <slug> [--headless --mode read\|read-write --label <L>] [--trust pin\|ca] \| --list \| --revoke <L>`; usage error exit 2 | C7 |
| Bundle | v:2, keys unchanged `{v,mcp_url,observe_url,token,fp}`; `fp: "sha256:<64hex>" \| null` | C7/C8 |
| Env vars | `UNIMATRIX_BUNDLE`, `UNIMATRIX_REMOTE_MCP_URL` (new), `UNIMATRIX_REMOTE_URL` (observe; unchanged), `UNIMATRIX_REMOTE_TOKEN`, `UNIMATRIX_REMOTE_FINGERPRINT` (new), `UNIMATRIX_REMOTE_HEADERS` (new) | C9 |
| Resolver | `resolveRemote(env, {projectHash}) -> {mcp:{url,token,pinnedFp}\|null, observe:{url,token,pinnedFp}\|null, headers:Array<[string,string]>}`; throws `ResolveError` (token-free message) | C9 |
| Headers parser | `parseExtraHeaders(raw:string\|undefined) -> Array<[string,string]>`; throws `ResolveError` | C9 |
| `HttpSession.create` | adds `extraHeaders`; `pinnedFp` may be `null` (CA branch) | C10 |
| `transport-http` config | adds `extraHeaders`; `pinnedFp` null = CA branch | C10 |

## Error Boundaries
| Origin | Response | Leak guard |
|---|---|---|
| Unknown or malformed token, revoked token, disabled legacy token, corrupt table (scoped tokens) | 401 | The body never echoes the token |
| Slug outside the binding | 403, before `resolve_store` | Does not reveal whether the slug exists |
| Missing capability on a tool (ceiling or registry) | `CapabilityDenied` MCP error (existing type) | Does not reveal which set denied |
| Observe without `SessionWrite` | 403; client warns once, never fails | Warning carries no token/header |
| Client config (precedence conflict, partial pair, bad headers, bad bundle) | stderr message, exit 1 | Header values and tokens never printed |
| TLS verify failure | The socket is destroyed before any write; exit 1 | Zero app bytes reach the server |

## Modularity (#693, PL-10; cap = 1,000 code lines excluding tests)

| File | Lines now | Over cap? | vnc-050 treatment |
|---|---|---|---|
| `mcp/tools.rs` | 13,798 | Yes | **Mechanical only**: 15 `require_cap(&ctx.agent_id, …)` → `require_cap(ctx, …)`; one `learning_allowed` line per MCP `UsageContext` site; one guard on the query_log insert. No logic. |
| `server.rs` | 4,443 | Yes | **Minimal**: one `cap_ceiling` line in `build_context_with_external_identity`; `require_cap` signature + one `cap_ceiling::permits` call. Bearer-identity branch (`:541`) untouched and dormant. |
| `main.rs` | 2,460 | Yes | **Variant swap only**: `ClientBundle { slug }` → `ClientBundle(ClientBundleArgs)` and a dispatch line. The args struct lives in `client_bundle/args.rs`. Net change about 0 lines. |
| `infra/config.rs` | 12,863 | Yes | **Minimal touch, about 5 lines**: one `HttpConfig` field + doc, one `Default` line, a 3-line override step after step 3c calling `legacy_token_config`. Parser and tests live in new `infra/legacy_token_config.rs` (< 150 lines). Accepted growth; no logic added to `config.rs`. |
| `services/usage.rs` | — | — | One `UsageContext` field + early return in `record_access`. |
| `infra/registry.rs` | — | — | **Not modified**. |
| `mcp/cap_ceiling.rs` | new | No | All ceiling logic and tests (< 200 lines). |
| `client_bundle.rs` | 423 | No | Becomes `client_bundle/mod.rs`. Mint, list and revoke go in the new `tokens_cli.rs`, so each file stays under the cap. |
| `http/auth.rs`, `http/router.rs`, `handlers.rs` | 292 / 451 / 226 | No | Small edits. All stay under the cap. |
| JS: `config.js`, `mcp-bridge.js`, `http-session.js`, `transport-http.js` | ≤ 339 | No | Resolution logic moves to the new `remote-resolve.js` |

No decomposition of `tools.rs`, `server.rs`, `main.rs` or `config.rs` is planned in this feature. Those are pre-existing overages that this design does not grow materially. They remain on the #693 backlog and need a separately scheduled decomposition with a test plan. There is no mid-delivery refactor.

## Backward Compatibility (new server x existing clients)

No existing artifact needs to change. Every row below is a required test permutation (new server, unmodified existing client or file).

| Existing client / artifact | New-server behavior | Why it holds |
|---|---|---|
| Pinned v:2 bundle from `client-bundle <slug>` (legacy token, `fp:"sha256:..."`) via `init --bundle` / `remote.json` | Attaches and works unchanged | Bundle format, key set and fp validation unchanged (ADR-005); legacy token enabled by default; pinned TLS branch byte-for-byte (ADR-006) |
| Same pinned bundle via new `UNIMATRIX_BUNDLE` | Works, pinned | Same decoder, `fp` set → pinned branch |
| Old JS client given a new `fp:null` bundle | Fails loud ("fp must be sha256") | Intended: no silent pin downgrade (ADR-005) |
| Legacy `{data_dir}/token`, any slug, MCP + observe | 200, all slugs, today's semantics | `TokenTableValidator` checks legacy first; no slug binding for `Legacy`; `legacy_token_enabled` defaults true |
| Existing observe env path `UNIMATRIX_REMOTE_URL` + `UNIMATRIX_REMOTE_TOKEN` (no MCP URL, no fingerprint) | Observe POSTs as today; bridge still uses its prior source | Tier-2 observe meaning unchanged; missing `_MCP_URL` only affects the bridge, which falls through to the credential store (ADR-006). Unpinned env observe already used CA validation |
| `remote.json` / credential store with pinned fingerprint | Unchanged | Tier 3 unchanged |
| Stored credential with `fingerprint:null` | Now CA trust instead of refusal | Only behavior change; was previously an error state, so no working setup regresses |
| Existing config files (no `legacy_token_enabled` key) | Parse unchanged; legacy enabled | serde default; additive key under PL-2 versioned config |
| No `tokens.json` on disk | Empty table; only legacy accepted | Missing file = empty table (ADR-001) |
| stdio / UDS clients | Unchanged | No change to `bridge.rs`, identity or registry |
| Existing hooks with legacy credential | Observe accepted | Legacy has `SessionWrite` |

## Test Strategy Hooks
- Route × token matrix for the C3 guard: every route including observe, SSE GET and DELETE.
- Read token on observe and every session route: 403, and observations/sessions/signal_queue/cycle_events/entries tables unchanged after draining background channels (AC-15 observe scope, SR-17).
- JS: read-bundle hook run against a real server gets 403 → exactly one warning, hook exit 0, no second POST across N hook processes.
- R-01 first delivery step: `BearerPrincipal` visible at a tool via `Parts.extensions` on POST in a stateful session (else apply ADR-003 McpAdapter re-insert fallback).
- Ceiling matrix: {read, read-write, legacy, stdio} × {Read, Search, Write, SessionWrite, Admin} × claimed `agent_id` {human, registered restricted}; read + `human` write → denied; HTTP with Parts but no principal → denied.
- Read token search/lookup/get: usage, co_access and query_log tables unchanged after draining channels.
- Residual-gap pin: read-write token + `agent_id:"human"` non-Admin write succeeds; labeled as the accepted gap.
- Tombstone: revoke → 401; `--list` shows revoked; re-mint same label fails; audit rows for the label still resolve.
- Legacy config: file false, env true/false/garbage/empty matrix; absent key = enabled.
- Revoke-while-running: the mint → call → revoke → call sequence returns 200 then 401 against a running server (AC-16).
- Corrupt `tokens.json` produces 401 for scoped tokens while legacy keeps working.
- JS: an untrusted server and a hostname-mismatched server both record zero bytes received (SR-01). The existing pin tests run unchanged (AC-05). Header presence is checked on every request type, and a sentinel value never appears in stderr. The full precedence/conflict matrix is covered.
- Rust↔JS bundle parity corpus gains `fp:null` and still rejects non-null invalid `fp` values.
- Scripted smoke: a local CA, a server with a CA-signed cert, then a curl recipe and a generic MCP client run verbatim from the docs (AC-12, local AC-13). Operator-run: the real GH Actions job and the tailnet (AC-13/14).

## Open Questions
1. Residual gap (risk entry required): within a read-write token an agent can claim any `agent_id`; accepted until agent-identity work lands.
2. `--list` `--json`: not in scope (stderr table only), per reviewer recommendation.
3. Multi-slug mint: deferred; schema already allows it.
4. `http_access` audit: one row per HTTP request, not per SSE event; existing retention.
