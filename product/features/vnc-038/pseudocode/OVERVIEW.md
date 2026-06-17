# vnc-038 Pseudocode Overview — Mandatory Project Identity at the Deployment Entrypoint

Component-interaction map, data flow, shared types, and the Rust↔JS `v:2` bundle contract.
Per-component pseudocode files implement the bodies; this file is the contract those files share.

> Reads from: ARCHITECTURE.md (Integration Surface), SPECIFICATION.md (FR/AC), RISK-TEST-STRATEGY.md (R-01..R-15), ADR-001..008 (#5080-5088). Every interface name below is traced to existing code (cited file:line) or to an ADR — none invented.

---

## Components (one pseudocode file each)

| # | Component | File | Pseudocode | ADR / AC |
|---|-----------|------|-----------|----------|
| 1 | Bundle codec (Rust, sole encoder) | `client_bundle.rs` | `bundle-codec-rust.md` | ADR-002 / AC-05 |
| 2 | Bundle decoder (JS) | `hook-client/bundle.js` | `bundle-decoder-js.md` | ADR-002 / AC-05 |
| 3 | Client attach (JS) | `init.js` | `client-attach-js.md` | ADR-001 / AC-05/07 |
| 4 | Hook transport (JS) | `hook-client/transport-http.js` | `hook-transport-js.md` | ADR-001 / AC-08 |
| 5 | Route grammar + resolver | `http/router/seam.rs`, `project_resolver.rs` | `route-grammar-resolver.md` | ADR-004 / AC-01/06/09 |
| 6 | Observe route + handler | `http/router.rs`, `main.rs` | `observe-route.md` | ADR-003 / AC-06/07/08 |
| 7 | Boot wiring | `main.rs` | `boot-wiring.md` | ADR-003/004 / AC-09 |
| 8 | register CLI | `projects.rs` | `register-cli.md` | ADR-007 / AC-02/03/04 |
| 9 | Reserved slugs | `infra/config.rs` | `reserved-slugs.md` | ADR-005 / AC-13(slug)/FR-13 |
| 10 | First-boot token (CI-1) | `http/token.rs` | `token-redaction.md` | ADR-008 / AC-11 |
| 11 | Local STDIO/UDS direct-binding guard (C-13) | `main.rs:859`/`:1158` | `local-binding-guard.md` | ADR-006 / AC-10 |
| 12 | Wave-1 cleanups (CI-2/CI-3) | `http/router.rs`, `public_url.rs` | `wave1-cleanups.md` | AC-12/AC-13 |

Component 11 is a **negative/guard** component: its pseudocode is "what must NOT change" plus the structure asserted to prove it. It mutates no production code.

---

## Data Flow (cloud/container HTTP surface)

```
PROVISIONING (one-time per project, restart-to-apply)
  operator ─register <slug>─► [projects.rs::register]
        State C: mkdir + genesis store  │  State B: open preserved store (re-attach)
                                        ▼
        [projects.rs::ensure_project_stanza]  ── atomic temp+fsync+rename ──► config.toml [[projects]]
                                        │
                              (daemon restart)
                                        ▼
BOOT  [main.rs] load_config_and_build_allowlist ─► project_slugs : Vec<ProjectSlug>
        if empty  ⇒ NOTHING servable, loud "register a project to begin"  (no resolver, no default)
        else      ⇒ build_unified_resolver(project_slugs) : Arc<dyn StoreResolver>   (slug-keyed only)
        observe handler is given Arc<dyn StoreResolver>  (NO boot-bound resolve_store)
        [LOCAL STDIO :1158 / UDS :859 boot paths run a SEPARATE direct path-hash open — untouched]

BUNDLE EMIT  operator ─client-bundle <slug>─► [client_bundle.rs::encode_bundle v:2]
        compose_route_urls(public_base, slug) ⇒ mcp_url, observe_url
        emit  unimatrix-bundle:<base64url({v:2, mcp_url, observe_url, token, fp})>
        token reaches client ONLY here (ADR-008); never stdout/logs

CLIENT ATTACH  operator pastes ─► [init.js::resolveRemoteTarget] ─► [bundle.js::decodeBundle]
        strict v:2 validate ⇒ {mcp_url, observe_url, token, fp}   (NO composition, ADR-001)
        store VERBATIM into .claude/settings.local.json: unimatrix.remote.{mcp_url, observe_url, token, fingerprint}

RUNTIME (two client→server arrows, both verbatim)
  MCP  ── POST mcp_url verbatim ───────►  /v1/{slug}/...      ┐
  OBS  ── POST observe_url verbatim ──►   /v1/{slug}/observe  ┘
                                          │
                                          ▼
                          [router.rs PathRouter] → parse_project_key(path) ⇒ ProjectKey::Slug(slug)
                                          │  (/health stays top-level; /observe top-level REMOVED)
                                          ▼
                          resolver.resolve_store(&key)  ── THE single funnel ──► Arc<Store> for {slug}
                                          │   (observe resolves per-request via the SAME funnel)
                                          ▼
                          per-slug McpAdapter (MCP)  /  per-request observe dispatch (OBS)
```

Both arrows carry **server-authored URLs the client never mutated** (ADR-001). Both resolve through the **one** `resolve_store` funnel keyed by `ProjectKey::Slug` only — provable at N=2. Local STDIO/UDS is not on this diagram and never enters the resolver (ADR-006).

---

## The Rust↔JS `v:2` Bundle Contract (the shared oracle — ADR-002)

This is the single most load-bearing cross-component contract. Components 1 and 2 MUST implement it byte-identically; the parity corpus is the shared oracle (R-03).

### Wire form (unchanged shape, new payload)
```
unimatrix-bundle:<base64url-nopad(canonical-json)>
```

### Canonical JSON — exact key order, exact key set (v:2)
```json
{"v":2,"mcp_url":"https://<base>/v1/<slug>","observe_url":"https://<base>/v1/<slug>/observe","token":"<64hex>","fp":"sha256:<64hex>"}
```

- **Field order = declaration order** in the Rust `Bundle` struct (serde serializes in declaration order; never a HashMap). Order: `v, mcp_url, observe_url, token, fp`.
- **Exact key set** (both sides reject missing OR extra): `{v, mcp_url, observe_url, token, fp}` — exactly 5 keys.
- `v` MUST equal `2` (u8). Unknown major rejects loud on both sides (R-04: a `v:1` bundle fails closed with a re-issue message).
- `mcp_url`, `observe_url` MUST be `https://` strings; the client posts them **verbatim** (no append, no normalization — R-01).
- `token` MUST match `^[0-9a-f]{64}$`; `fp` MUST match `^sha256:[0-9a-f]{64}$` (unchanged from v:1).

### Guard ordering (LOCKED, both sides — preserved from v:1, NFR-08)
```
1. LENGTH CAP   raw bytes ≤ MAX_RAW_LEN (4096), BEFORE any decode/parse
2. SCHEME       strip "unimatrix-bundle:" prefix
3. BASE64URL    decode no-pad (JS round-trip re-encode check kept)
4. JSON         parse to a generic object/Value
5. STRICT SCHEMA exact 5 keys, v==2, https URLs, token/fp grammar  ← load-bearing
```

### Atomicity rule (R-03 — the one-diff invariant)
Rust encoder + Rust `validate_schema` + `BUNDLE_VERSION` bump + JS `EXPECTED_KEYS` + JS `obj.v !== 2` + JS https checks + the parity corpus (hex vectors, re-exported `pub(crate)` oracle fns per #4956) ALL move in ONE diff. A single-side change is an integration break by construction (strict exact-key guard hard-rejects).

---

## Shared Types (introduced or modified)

### Rust (`unimatrix-server`)

```rust
// client_bundle.rs — v:2 (was {v, base_url, token, fp})
pub struct Bundle { pub v: u8, pub mcp_url: String, pub observe_url: String, pub token: String, pub fp: String }
pub const BUNDLE_VERSION: u8 = 2;        // was 1
pub const BUNDLE_SCHEME: &str = "unimatrix-bundle:";  // unchanged
pub const MAX_RAW_LEN: usize = 4096;     // unchanged
// BundleError variants unchanged: TooLong, BadScheme, BadBase64, BadJson, Schema(String)

// seam.rs — ProjectKey loses Default for served-project routing (ADR-004)
pub enum ProjectKey { Slug(ProjectSlug) }            // Default arm removed
pub struct ProjectSlug(String);                      // UNCHANGED (charset newtype)
pub enum RouteError { UnknownProject, InvalidSlug(String) }  // UNCHANGED
pub trait StoreResolver { fn resolve_store(&self, key:&ProjectKey)->Result<Arc<Store>,RouteError>;
                          fn adapter_for(&self, key:&ProjectKey)->Option<&McpAdapter>; }  // signatures UNCHANGED

// project_resolver.rs — MultiProjectRouter loses `default` (ADR-004)
struct MultiProjectRouter { slugs: HashMap<ProjectSlug, ProjectEntry> }   // `default` field removed
struct ProjectEntry { store: Arc<Store>, adapter: McpAdapter }            // UNCHANGED

// config.rs — value retained, derivation re-documented (ADR-005)
pub const RESERVED_SLUGS: [&str; 4] = ["v1","health","observe","tools"];  // VALUE unchanged

// Observe context — loses its pre-resolved single store (ADR-003)
// ObserveContext.store field is REPLACED by an Arc<dyn StoreResolver> held by the handler.
```

### JS (`packages/unimatrix/lib`)

```js
// bundle.js — v:2 decoder
const EXPECTED_KEYS = ["v", "mcp_url", "observe_url", "token", "fp"];   // was ["v","base_url","token","fp"]
const MAX_RAW_LEN = 4096;                                              // unchanged, GUARD 1
// decodeBundle(raw) -> { v:2, mcp_url, observe_url, token, fp }
// class BundleError extends Error  (unchanged)
// assertSlugAllowlist / SLUG_RE — RETIRED for the bundle path (ADR-001: no client slug branch)

// settings.local.json subtree (init.js writes; transport reads)
// unimatrix.remote = { mcp_url, observe_url, token, fingerprint? }   (was { url, token, fingerprint? })
```

---

## Sequencing Constraints (build order for Stage 3b)

1. **Bundle contract first (Components 1 + 2 + parity corpus, atomically).** Everything client-side and the bundle-emit path depend on the v:2 shape. R-03 forbids a single-side landing.
2. **Route grammar + resolver (Component 5)** before observe (6) and boot (7): observe re-uses `parse_project_key` + `resolve_store`; boot builds the unified resolver. ADR-004 deletions land here.
3. **Observe route (6)** depends on 5 (the funnel) and on the observe handler holding `Arc<dyn StoreResolver>` (boot wiring, 7).
4. **Boot wiring (7)** depends on 5 (resolver constructor shape) and on 8 (register writes `[[projects]]` that boot reads). Local guard (11) is verified against 7's diff.
5. **register CLI (8)** is independent of the bundle but must land before an N=2 isolation test (AC-06) can register two projects.
6. **Client attach (3) + hook transport (4)** depend on 2 (the decoder output shape).
7. **token (10), reserved slugs (9), wave-1 cleanups (12)** are low-coupling carry-items; 12's `router.rs` ≤500 outcome depends on 6's extraction.

---

## Cross-Cutting Error Boundaries (referenced by component files)

- **Bundle decode (JS trust boundary):** `BundleError` on any guard; hard reject, no partial accept; length cap first (R-03 ordering, security).
- **Route resolution (server):** `RouteError::UnknownProject` for unregistered slug; `RouteError::InvalidSlug` from the allowlist. NO `Default` fallback — unmatched/no-slug is a loud error, never a silent default store (R-10).
- **Config write (register):** atomic temp+fsync+rename → old OR new complete file, never partial; idempotent on existing stanza; State A loud error; State B re-attach (open, never genesis) (R-05/R-06).
- **First boot, empty `[[projects]]`:** nothing servable; loud actionable "register a project to begin" (R-10/AC-09).
- **Token (ADR-008):** the v:2 bundle is the sole token channel for cloud; never to stdout/`tracing`; redaction gated by deployment context so local (no bundle) is unchanged (R-14 ∩ R-13).

---

## Open Questions / Gaps (flagged, not papered over)

- **OQ-A (Component 3/4 wiring) — DESIGNED, confirm at impl.** Today `settings.local.json` stores a single `unimatrix.remote.url`, and `transport-http.post` composes `{url}/observe` (transport-http.js:84). v:2 carries TWO finished URLs. The pseudocode stores BOTH (`mcp_url`, `observe_url`) and feeds the hook transport `observe_url` as its post target with the `/observe` append deleted. Whether MCP requests are issued by this same Node client or only by Claude Code's MCP layer determines whether `mcp_url` is also consumed by transport — Component 3 stores it regardless (dumb-client: verbatim, no derivation). Impl confirms the MCP consumer.
- **OQ-B (token.rs print site is already redacted) — VERIFY not REMOVE (CI-1).** `token.rs` already emits only `render_first_boot_notice()` (token.rs:171), a pure builder that contains no token, no path, no secret. The architecture's "token.rs:101 print" predates this; the live code already satisfies AC-11/NFR-06. Component 10 therefore specifies the **assertion** (no token substring on stdout/`tracing`) and a guard against a future regression — it does not remove a print that no longer leaks. Flagged in `token-redaction.md`.
- **OQ-C (`tools` reservation) — KEEP, per ADR-005.** Conservatively retained; un-reserving is a one-line follow-up. No action in this feature beyond re-documenting the derivation.
