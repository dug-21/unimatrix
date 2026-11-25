# vnc-050 Implementation Brief: Headless Automation Access

GH #1010. Regenerated after design rework 1 (HUMAN-DECISIONS.md decisions 1-5; decision 5 supersedes decision 1's descope).

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-050/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-050/SCOPE-RISK-ASSESSMENT.md |
| Human Decisions | product/features/vnc-050/HUMAN-DECISIONS.md |
| Architecture | product/features/vnc-050/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-050/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-050/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-050/ALIGNMENT-REPORT.md |

## Goal

Let non-interactive clients (GitHub Actions, Claude Code cloud routines, a Cloudflare-hosted site) attach to a personal-cloud Unimatrix server from one secret (`UNIMATRIX_BUNDLE`) with least-privilege, slug-bound `read` or `read-write` tokens minted by the existing `client-bundle` command. The bridge validates TLS against standard CAs when no fingerprint is present (pinned behavior unchanged otherwise), carries caller-owned extra headers, and the server enforces token mode as a capability ceiling intersected with registry grants, plus per-token slug binding. Existing clients, bundles, `remote.json` files and the legacy token keep working.

## First Delivery Step (gates all ceiling work)

R-02 step 0: over real HTTP in a stateful MCP session, prove `BearerPrincipal` (inserted by the auth layer) is visible in the tool's `RequestContext` via `Parts.extensions` for POST (`initialize` then `tools/call`). If absent, implement the ADR-003 fallback (`McpAdapter::call` re-inserts `BearerPrincipal` before `StreamableHttpService`) and re-run. The test stays in the suite permanently (rmcp upgrade guard).

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 Token table (`http/token_table.rs`) | pseudocode/token-table.md | test-plan/token-table.md |
| C2 Scoped validator (`http/auth/scoped.rs`, `http/auth.rs`) | pseudocode/scoped-validator.md | test-plan/scoped-validator.md |
| C3 Slug guard + access audit (`http/router/slug_guard.rs`) | pseudocode/slug-guard.md | test-plan/slug-guard.md |
| C4 Principal mapping (`http/principal.rs`) | pseudocode/principal.md | test-plan/principal.md |
| C4b Capability ceiling (`mcp/cap_ceiling.rs`, `server.rs`, `mcp/context.rs`, `tools.rs`, `services/usage.rs`) | pseudocode/cap-ceiling.md | test-plan/cap-ceiling.md |
| C5 Legacy-token config (`infra/legacy_token_config.rs`, `infra/config.rs`) | pseudocode/legacy-token-config.md | test-plan/legacy-token-config.md |
| C6 Observe gate (`http/router/handlers.rs`) | pseudocode/observe-gate.md | test-plan/observe-gate.md |
| C6c Observe 403 client handling (`lib/hook-client` observe POST) | pseudocode/observe-403-client.md | test-plan/observe-403-client.md |
| C7 client-bundle CLI (`client_bundle/{mod,args,tokens_cli}.rs`, `main.rs`) | pseudocode/client-bundle-cli.md | test-plan/client-bundle-cli.md |
| C8 JS bundle decoder (`lib/hook-client/bundle.js`) | pseudocode/bundle-decoder.md | test-plan/bundle-decoder.md |
| C9 Remote resolver (`lib/hook-client/remote-resolve.js`) | pseudocode/remote-resolve.md | test-plan/remote-resolve.md |
| C10 Bridge / transports (`mcp-bridge.js`, `mcp-bridge/http-session.js`, `transport-http.js`, `config.js`) | pseudocode/bridge-transports.md | test-plan/bridge-transports.md |
| C11 Docs + smoke (remote-access guide, scripted smoke) | pseudocode/docs-smoke.md | test-plan/docs-smoke.md |
| Backward-compat matrix (cross-component, FR-34..39) | pseudocode/OVERVIEW.md (section) | test-plan/backward-compat.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

Unimatrix ADR IDs: ADR-001 #5814, ADR-002 #5806, ADR-003 #5819, ADR-004 #5820, ADR-005 #5815, ADR-006 #5818, ADR-007 #5817.

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Token storage | `{data_dir}/tokens.json` 0600 `{"v":1,"tokens":[{label,sha256,mode,allowed_slugs,created_at,revoked_at}]}`; SHA-256 of 32 raw bytes; label `^[a-z0-9][a-z0-9-]{0,62}$`; strict parse (unknown key, duplicate label, empty slugs reject file) | SCOPE OQ-4 | architecture/ADR-001-scoped-token-table.md |
| Constant-time compare | Legacy `ct_eq`, then `ct_eq` against every entry incl. tombstones; no early exit; no hash-keyed map; tombstone match = 401 after full scan | SR-02 | architecture/ADR-001-scoped-token-table.md |
| Revoke semantics | Tombstone: set `revoked_at`, never delete; label unique forever; re-mint of any existing label fails | HD 3a | architecture/ADR-001-scoped-token-table.md |
| Re-revoke | Already-revoked, unknown, or other-slug label: error, no state change (file byte-identical) | ADR-001/005 (resolves FR-07 either/or) | architecture/ADR-005-client-bundle-headless.md |
| Revocation propagation | Per-request `stat` stamp `(mtime_ns,len,inode)`; reload on change; corrupt file fails closed for scoped tokens, legacy unaffected; never serve last-good; missing file = empty table | Spec OQ-1, SR-03 | architecture/ADR-001-scoped-token-table.md |
| Writer concurrency | Exclusive `flock` on `tokens.lock`; tmp 0600, fsync, rename | NFR-03 | architecture/ADR-001-scoped-token-table.md |
| Legacy disable | `http.legacy_token_enabled` (serde default true) + `UNIMATRIX_LEGACY_TOKEN_ENABLED` override after file merge; `true|1|false|0`; empty = no override; other value = boot error; disabled → 401 `legacy token disabled` | HD 3c, PL-2 | architecture/ADR-001-scoped-token-table.md |
| Slug enforcement | One `slug_guard::check` in `PathRouter::call` before observe/MCP split and before `resolve_store`; 403 `token not permitted for project`; missing principal → 500; no default-store alias | SR-10 | architecture/ADR-002-single-slug-guard.md |
| Access audit | One `http_access` AuditEvent per HTTP request (label or `legacy`, never secret); denials logged to bound slug's log | AC-11 | architecture/ADR-002-single-slug-guard.md |
| Capability model | Effective caps = registry(self-asserted `agent_id`) ∩ token ceiling. read = {Read, Search}; read-write = {Read, Write, Search, SessionWrite}; never Admin; legacy/stdio/UDS = `None` (no ceiling); HTTP with `Parts` but no principal = empty ceiling (deny) | HD 5, PL-4, #4453 | architecture/ADR-003-token-mode-capability-ceiling.md |
| Check point | `require_cap(&self, ctx: &ToolContext, cap)` checks `cap_ceiling::permits` then unchanged registry check; 15 mechanical call-site swaps in `tools.rs` | #4453 | architecture/ADR-003-token-mode-capability-ceiling.md |
| agent_id | Stays self-asserted (attribution, registry lookup); bearer-identity path (`server.rs:541`) dormant; residual spoofing gap within read-write accepted (R-20) | HD 1, HD 5 | architecture/ADR-003-token-mode-capability-ceiling.md |
| Observe for read tokens | Server 403 `token lacks SessionWrite` before dispatcher on observe and session telemetry routes | HD 2 (V-1 rejected) | architecture/ADR-004-read-token-learning-suppression.md |
| Learning suppression | `records_learning(ctx.cap_ceiling)` (None → true; ceiling → contains Write); `UsageContext.learning_allowed` early return in `record_access`; query_log insert guarded; gate before enqueue | AC-15, HD 5 | architecture/ADR-004-read-token-learning-suppression.md |
| Client observe 403 | One stderr warning per process, latch off further observe POSTs, hook exit/stdout unchanged; other failures unchanged | HD 2 | architecture/ADR-004-read-token-learning-suppression.md |
| CLI shape | `client-bundle <slug> [--headless --mode M --label L] [--trust pin|ca] | --list | --revoke L`; flag matrix validated before I/O, exit 2 | SR-05, SR-18 | architecture/ADR-005-client-bundle-headless.md |
| Bundle format | Stays v:2, exact key set; `fp` becomes `string|null` (null = CA trust via `--trust ca`); old clients reject `fp:null` loudly; `init --bundle` accepts CA bundle, stores `fingerprint:null` | SR-06, Spec OQ-2/OQ-4 | architecture/ADR-005-client-bundle-headless.md |
| Resolver precedence | `UNIMATRIX_BUNDLE` > env pair > credential store; per-consumer tiers; MCP env tier active only when `UNIMATRIX_REMOTE_MCP_URL` set | SR-13/15 | architecture/ADR-006-credential-resolution-and-ca-trust.md |
| Bundle + env conflicts | Reject bundle + `UNIMATRIX_REMOTE_MCP_URL` / `_URL` / `_TOKEN` / `_FINGERPRINT` (exit 1, names var); allow bundle + `UNIMATRIX_REMOTE_HEADERS` | HD 3b | architecture/ADR-006-credential-resolution-and-ca-trust.md |
| Empty-string env vars | Every `UNIMATRIX_*` empty value = unset (no trimming). Bundle + empty conflict var = no conflict. One pair half set + other empty = partial-pair error (FR-23): exit non-zero, stderr names the empty and the set variable, never prints any credential; both empty = pair unset, store used | HD 7.2, HD 8 | specification/SPECIFICATION.md (FR-40, FR-23) |
| CA trust | No fp → plain `https` options, no `rejectUnauthorized` key, no `cert-pin.js` agent; guard throws if `rejectUnauthorized:false` reaches unpinned request; zero app bytes on verify failure; private CA via `NODE_EXTRA_CA_CERTS` only | SR-01 | architecture/ADR-006-credential-resolution-and-ca-trust.md |
| vnc-039 ADR-005 (#5119) | Corrected via `context_correct`: cloud MCP = bundle, env pair, or store; unpinned = CA trust | SCOPE | architecture/ADR-006-credential-resolution-and-ca-trust.md |
| Extra headers | Strict JSON object ≤32 entries; RFC 7230 names; string values ≤8 KiB, no CR/LF/NUL; reserved (case-insensitive): authorization, mcp-session-id, host, content-length, content-type, transfer-encoding, connection, accept; case-duplicates rejected; applied on initialize, POST, SSE GET, DELETE, observe, `pingForInit`; never logged | SR-04, SR-16 | architecture/ADR-007-extra-request-headers.md |
| Auth-layer identity | `token:<label>` is the auth-layer/`http_access` audit identity only; never a tool-call agent_id, never enrolled. Omitted tool-call agent_id = today's default (`anonymous`). If `resolve_or_enroll` grants anonymous Write, a RW token with no agent_id writes as anonymous (R-22, accepted, pre-existing) | HD 7.1, FR-16, R-13, R-22 | architecture/ADR-003-token-mode-capability-ceiling.md |
| Old-client source (compat) | `@dug-21/unimatrix@0.11.3` npm tarball with integrity hash; fallback `git archive v0.11.3 packages/unimatrix`; bump to N-1 on ship | RTS OQ-7 | — (RISK-TEST-STRATEGY.md R-21) |
| Multi-slug mint, `--list --json` | Deferred; schema allows `allowed_slugs[]`; one slug per mint; `--list` stderr table only | HD smaller questions | architecture/ARCHITECTURE.md (Open Questions) |

## Files to Create / Modify

Rust (`crates/unimatrix-server/src/`):
- `http/token_table.rs` (new) — tokens.json schema, strict parse, locked atomic writes, stat-stamp snapshot.
- `http/auth/scoped.rs` (new) — `TokenTableValidator: BearerValidator`; legacy check + constant-time full scan.
- `http/auth.rs` — wire `TokenTableValidator`; insert `BearerPrincipal` + `ResolvedIdentity`; legacy-disabled 401 body.
- `http/principal.rs` (new) — `TokenMode`, `BearerPrincipal`, `auth_identity`.
- `http/router/slug_guard.rs` (new) — post-parse slug check + `http_access` audit.
- `http/router.rs` — one guard call in `PathRouter::call` before observe/MCP split; ADR-003 fallback re-insert only if step 0 fails.
- `http/router/handlers.rs` — observe/session 403 without `SessionWrite`.
- `mcp/cap_ceiling.rs` (new) — `CapCeiling`, `ceiling_from`, `permits`, `records_learning` + tests.
- `mcp/context.rs` — `ToolContext.cap_ceiling` field.
- `server.rs` — one line in `build_context_with_external_identity`; `require_cap` signature + ceiling call.
- `mcp/tools.rs` — mechanical: 15 `require_cap` call sites; `learning_allowed` per MCP `UsageContext`; query_log guard.
- `services/usage.rs` — `UsageContext.learning_allowed`; early return in `record_access`.
- `infra/legacy_token_config.rs` (new) — env override parser + tests.
- `infra/config.rs` — `HttpConfig.legacy_token_enabled` field, default, 3-line override step.
- `client_bundle.rs` → `client_bundle/mod.rs`, `client_bundle/args.rs` (new), `client_bundle/tokens_cli.rs` (new) — flags, mint/list/revoke, nullable fp.
- `main.rs` — `ClientBundle(ClientBundleArgs)` variant swap + dispatch.

JS (`packages/unimatrix/lib/hook-client/`):
- `remote-resolve.js` (new) — `resolveRemote`, `parseExtraHeaders`, `ResolveError`.
- `bundle.js` — accept `fp:null`; reject all else as today.
- `mcp-bridge.js`, `mcp-bridge/http-session.js` — consume resolver; pinned vs CA branch; extra headers.
- `transport-http.js` — CA branch, extra headers on observe POST and `pingForInit`.
- `config.js` — resolve via `remote-resolve.js`; observe 403 warn-once latch.

Docs / tests:
- `docs/` remote-access guide (direct-HTTPS curl + generic MCP client recipe, GH Actions example).
- Tailscale findings doc: post-delivery operator deliverable (AC-14), not produced in delivery.
- Scripted smoke (local CA + server) executing doc recipe verbatim; compat matrix harness; Rust↔JS bundle parity corpus additions.

## Data Structures

```rust
pub(crate) enum TokenMode { Read, ReadWrite }            // fn capabilities(self) -> Vec<Capability>
pub(crate) enum BearerPrincipal {                         // request extension
    Legacy,
    Scoped { label: String, mode: TokenMode, allowed_slugs: Vec<ProjectSlug> },
}
pub(crate) struct CapCeiling(Vec<Capability>);
// ToolContext:   pub cap_ceiling: Option<CapCeiling>
// UsageContext:  pub(crate) learning_allowed: bool
// HttpConfig:    legacy_token_enabled: bool  (serde default true)
```

`tokens.json`: `{"v":1,"tokens":[{"label":String,"sha256":"<64hex>","mode":"read"|"read-write","allowed_slugs":[ProjectSlug],"created_at":u64,"revoked_at":u64|null}]}`; lock `{data_dir}/tokens.lock`.

Bundle v:2: `{v, mcp_url, observe_url, token, fp: "sha256:<64hex>" | null}`.

Audit row: `AuditEvent{operation:"http_access", credential_type:"static_token"|"scoped_token", agent_id:"token:<label>"|"http-bearer", metadata:{token_label, method, route}}`.

## Function Signatures

```rust
impl TokenTable {
    fn open(data_dir: &Path) -> Self;
    fn lookup_ct(&self, presented: &[u8; 32]) -> Option<ScopedEntry>;
    fn mint(&self, label, mode, slug) -> Result<[u8; 32], ServerError>;
    fn revoke(&self, label, slug) -> Result<(), ServerError>;   // tombstone
    fn list_for(&self, slug) -> Vec<EntryView>;
}
fn TokenTableValidator::new(legacy: Option<[u8; 32]>, table: Arc<TokenTable>) -> Self; // impl BearerValidator
pub(crate) fn auth_identity(p: &BearerPrincipal) -> ResolvedIdentity;
pub(crate) fn check<B>(req: &Request<B>, audit_sink: &dyn AccessAudit)
    -> Result<(), Response<BoxBody<Bytes, Infallible>>>;                   // slug_guard
pub(crate) fn ceiling_from(rc: &RequestContext<RoleServer>) -> Option<CapCeiling>;
pub(crate) fn permits(c: Option<&CapCeiling>, cap: Capability) -> bool;
pub(crate) fn records_learning(c: Option<&CapCeiling>) -> bool;
pub(crate) async fn require_cap(&self, ctx: &ToolContext, cap: Capability)
    -> Result<(), rmcp::ErrorData>;                                       // server.rs:631
pub(crate) fn resolve_legacy_token_override(v: Option<&str>) -> Result<Option<bool>, ConfigError>;
```

```js
resolveRemote(env, { projectHash }) -> { mcp: {url, token, pinnedFp}|null,
                                         observe: {url, token, pinnedFp}|null,
                                         headers: Array<[string, string]> }   // throws ResolveError (token-free)
parseExtraHeaders(raw: string|undefined) -> Array<[string, string]>         // throws ResolveError
HttpSession.create({ ..., pinnedFp: string|null, extraHeaders })             // null = CA branch
```

Wire contracts: observe 403 body `{"error":"token lacks SessionWrite"}`; slug 403 `{"error":"token not permitted for project"}`; legacy disabled 401 `legacy token disabled`; CLI usage error exit 2; client config error exit 1.

## Constraints

- Human constraint (SR-05): extend `client-bundle`; no `token` command, mint API, MCP admin tool, second bundle format or decoder. Gate fail on violation.
- Pinned trust (vnc-039 ADR-001 #5115, vnc-034 ADR-002 #4948) byte-identical; existing pin test files have empty diff. CA path never `rejectUnauthorized:false`.
- One capability check point (`require_cap`), one learning predicate, same `ctx.cap_ceiling` (#4453). Observe ceiling and tool ceiling derive from the same mode → capability table.
- Intersection only narrows; absent principal on HTTP = empty ceiling, never `None`.
- `/health` only unauthenticated route (#5571).
- No new npm runtime deps; no new crates (`sha2`, `subtle` present; `flock` via existing dep or `File::try_lock`).
- Modularity (#693): `tools.rs`, `server.rs`, `main.rs`, `config.rs` mechanical/minimal touches only; logic in new files; no mid-delivery decomposition.
- No change to UDS trust model or `bridge.rs`.
- Tests at the real boundary (socket, HTTP, spawned process) for all Critical risks (#4970); each rejection test has a success companion (#4416).
- Delivery covers only the scripted local CI equivalent of AC-13. Live AC-13 (GH Actions) and AC-14 (Tailscale) are post-delivery operator checks, never delivery gates (HD 9). vnc-050 contains no GH Actions workflow/infra and no Tailscale work. C19 (#5792) stays unproven until the operator runs both.

## Dependencies

- Crates: `sha2`, `subtle`, `rmcp` 1.7.0 (`Parts.extensions` propagation), existing `clap`, `serde`.
- Node stdlib: `https`, `tls`, `crypto`.
- Compat test artifact: `@dug-21/unimatrix@0.11.3`.
- External (post-delivery operator validation only): Tailscale, GitHub Actions; Cloudflare Access (docs).

## NOT in Scope

- claude.ai connector OAuth; JWT/OIDC; token expiry/rotation automation.
- Closing `agent_id` spoofing within a read-write/legacy token (R-20, FR-16a); bearer-identity path stays dormant.
- Label reuse or hard deletion of token records.
- `token` command, server mint API, MCP admin tool, token UI.
- New bundle version or second decoder.
- Per-tool ACLs beyond read vs read-write; multi-slug mint; `--list --json`.
- GH Actions workflow/infrastructure; Tailscale work (HD 9).
- UDS/local trust or `bridge.rs` changes; Tailscale/Cloudflare relay infra; plain-HTTP listener.
- Custom CA config beyond `NODE_EXTRA_CA_CERTS`; observe wire-format changes.

## Alignment Status

Vision alignment PASS (personal-cloud, integrity, Principle 3). Original report: PASS 3, WARN 1, VARIANCE 2, FAIL 0. Both variances are now resolved by HUMAN-DECISIONS.md:

- **V-1 (read-bundle observe)** — resolved by HD 2: server 403 pre-dispatch; client warns once, never fails the job (FR-14a, FR-24a, AC-18; ADR-004).
- **V-2 (spec/arch contradictions)** — resolved by HD 3, spec wins on revoke: (a) tombstone revoke, label never reusable (FR-06/06a, AC-17; ADR-001); (b) explicit bundle+env reject list, bundle+headers allowed (FR-20; ADR-006); (c) `http.legacy_token_enabled` + `UNIMATRIX_LEGACY_TOKEN_ENABLED` (FR-17, AC-19; ADR-001). The report's recommendation to adopt delete-on-revoke and the env-only `UNIMATRIX_LEGACY_TOKEN` was rejected.
- HD 4 adds backward compatibility (FR-34..39, AC-20..24, R-21). HD 5 restores the capability ceiling (FR-14b, AC-08/15/25/26, ADR-003); residual attribution gap R-20 accepted.
- WARN additions accepted: `--trust pin|ca`, per-request `http_access` audit (one row per HTTP request, not per SSE event), header limits (owned by ADR-007).
- Alignment report predates rework 1; its references to `UNIMATRIX_LEGACY_TOKEN`, `PrincipalCaps` overlay and identity replacement are superseded.

## Open Questions

None. Omitted agent_id and empty env semantics resolved by HUMAN-DECISIONS 7-8.
