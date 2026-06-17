# vnc-038 Architecture — Mandatory Project Identity at the Deployment Entrypoint

> Revises the vnc-034 personal-cloud deployment contract. Closes #766 by construction via the **dumb-client invariant**: the server is the sole authority on route shape; the client posts to server-composed URLs verbatim. Design artifacts only — no code in this phase.

## System Overview

vnc-038 sits at the deployment seam of the personal-cloud destination (goal #4934): the path from `client-bundle` (server emits) → `init --bundle` (client attaches) → MCP + observe traffic (client posts). It revises four vnc-034 contracts:

- **C1 bundle** (vnc-034 ADR-001 / #4954) → `v:2` bundle carrying server-composed MCP and observe URLs.
- **C4 routing seam** (vnc-034 ADR-003 / #4950) → unified resolver (cloud/container HTTP only, `ProjectKey::Slug` only), default route deleted, observe folded onto the per-request funnel. Local STDIO/UDS is NOT routed through this resolver — it keeps its direct path-hash store binding (ADR-006).
- **C5 register/attach** (vnc-034 ADR-004 / #4951) → `register` writes routing intent instead of printing it.
- **ADR-005 default alias** (#4949) → deleted; single project is N=1.

The change is a coordinated dual-side (Rust server + JS client) cut. The spine that ties every component together is the **dumb-client invariant** (ADR-001): all route grammar lives server-side; the client carries no route logic and composes no paths.

## Component Breakdown

| Component | Responsibility | Crate / Package | Changed by |
|-----------|----------------|-----------------|------------|
| **Bundle codec (Rust)** | Sole encoder of the `v:2` bundle; composes MCP+observe URLs from route grammar | `unimatrix-server/src/client_bundle.rs` | ADR-002 |
| **Bundle decoder (JS)** | Decodes + strict-validates `v:2`; returns finished URLs | `packages/unimatrix/lib/hook-client/bundle.js` | ADR-002 |
| **Client attach** | Reads `mcp_url` from bundle, posts MCP verbatim; no path composition | `packages/unimatrix/lib/init.js` | ADR-001 |
| **Hook transport** | Posts observe to bundle's `observe_url` verbatim; no `/observe` append | `packages/unimatrix/lib/hook-client/transport-http.js` | ADR-001 |
| **Route grammar** | `parse_project_key`: one rule, `/v1/{slug}/...` → `Slug`; no default | `unimatrix-server/src/http/router/seam.rs` | ADR-004 |
| **Unified resolver** | Sole `StoreResolver`; slug-keyed map; no `Default` arm | `unimatrix-server/src/http/router/project_resolver.rs` | ADR-004 |
| **Observe route + handler** | Per-slug `/v1/{slug}/observe` through the resolver; no boot-bound store | `unimatrix-server/src/http/router.rs`, `main.rs` | ADR-003 |
| **Boot wiring** | Build the unified resolver from `[[projects]]`; empty ⇒ nothing servable | `unimatrix-server/src/main.rs` | ADR-003, ADR-004 |
| **Local STDIO/UDS binding** | Opens path-hash store directly at boot, threads `Arc<Store>` to handlers; NOT routed through the resolver — untouched by vnc-038 | `unimatrix-server/src/main.rs` (`:859` UDS, `:1158` STDIO) | ADR-006 (unchanged) |
| **First-boot token print** | Redacted/gated so the bearer token is never emitted to stdout/logs; bundle is the sole token channel (cloud/container) | `unimatrix-server/src/http/token.rs:101` | ADR-008 |
| **register CLI** | Write `[[projects]]` atomically; re-attach-safe; binary-only | `unimatrix-server/src/projects.rs` | ADR-007 |
| **Reserved slugs** | Re-derived from the new grammar | `unimatrix-server/src/infra/config.rs` | ADR-005 |
| **client-bundle CLI** | Per-project bundle emit (takes a slug); composes URLs + carries token | `unimatrix-server/src/client_bundle.rs` / `main.rs` | ADR-002, ADR-008 |
| **Parity corpus** | Pins `v:2` Rust↔JS byte-equality (hex, re-exported oracle fns) | server `tests/` + JS test | ADR-002 |

## Component Interactions / Data Flow

```
                    provisioning (one-time, per project, restart-to-apply)
   operator ──register <slug>──► [projects.rs] ── atomic write ──► config.toml [[projects]]
                                                                         │
                                                              (daemon restart)
                                                                         ▼
   boot: load_config_and_build_allowlist ── project_slugs ──► build UNIFIED RESOLVER (slug→{store,adapter})
                                                                         │  (empty ⇒ nothing servable, loud msg)
                                                                         ▼
   operator ──client-bundle <slug>──► [client_bundle.rs encode_bundle v:2]
        composes mcp_url = {base}/v1/{slug}, observe_url = {base}/v1/{slug}/observe
                                   │  unimatrix-bundle:<base64url({v:2,mcp_url,observe_url,token,fp})>
                                   ▼
   operator pastes bundle ──► init --bundle ──► [bundle.js decodeBundle]
        strict v:2 validate ──► {mcp_url, observe_url, token, fp}  (NO composition)
                                   │ store verbatim into client config
            ┌──────────────────────┴───────────────────────┐
            ▼ (MCP, per request)                            ▼ (observe: init Ping + every hook)
   POST mcp_url verbatim                          POST observe_url verbatim
   /v1/{slug}/...                                 /v1/{slug}/observe
            │                                                │
            └──────────────► PathRouter ◄────────────────────┘
                                   │  parse_project_key(path) -> ProjectKey::Slug(slug)
                                   ▼
                         UNIFIED RESOLVER.resolve_store(&key)  ── THE single funnel ──► Arc<Store> for {slug}
                                   │  (observe resolves per-request via the SAME funnel — no boot-bound handle)
                                   ▼
                         per-slug McpAdapter / observe handler  (sole dispatch route; no default fallback)
```

The two client→server arrows (MCP and observe) carry **server-authored URLs the client never mutated** (ADR-001 invariant). Both resolve through the **one** `resolve_store` funnel (ADR-003/004), keyed by `ProjectKey::Slug` only, so a request bound to project B can never reach project A's store — provable at N=2 (#4974 guard). This diagram is the **cloud/container HTTP** surface. **Local STDIO/UDS is not shown and not involved**: it opens its path-hash store directly at boot and threads `Arc<Store>` to its handlers without entering the resolver (ADR-006, AC-10).

## Technology / Design Decisions (ADRs)

| ADR | Title | Drives |
|-----|-------|--------|
| ADR-001 | The Dumb-Client Invariant — server sole authority on route shape | SR-01; Goal 1/6; AC-05/07/08 |
| ADR-002 | `v:2` Bundle Carries Server-Composed MCP + Observe URLs — atomic dual-side | SR-02; RD-2; Goal 3; AC-05 |
| ADR-003 | Per-Slug Observe on the Per-Request Funnel — sole route, no boot fallback | SR-03; RD-3; Goal 5; AC-06/07/08 |
| ADR-004 | Delete the Default — unified resolver (cloud/container HTTP only), single = N=1 | SR-04; RD-5; Goal 1; AC-01/09 (cloud-only hard cut; local unaffected per ADR-006/AC-10) |
| ADR-005 | Reserved-Slug Re-Derivation under the new route grammar | SR-05 |
| ADR-006 | Local STDIO/UDS Keeps Its Direct Path-Hash Store Binding — Not Routed Through the Resolver (AC-10 ↔ RD-5) | SR-04; AC-10; RD-1/RD-5 |
| ADR-007 | `register <slug>` writes `[[projects]]`; restart applies — atomic, re-attach-safe | SR-07; RD-4; Goal 2/4; AC-02/03/04 |
| ADR-008 | First-Boot Token Delivered Only via the `v:2` Bundle — Never to Stdout/Logs | #735 CI-1; NFR-06; AC-11 |

## Integration Points / Dependencies

- **vnc-034** — direct input; this feature deprecates ADR-001 (#4954) and ADR-005 (#4949) via `context_correct`, and prunes ADR-003 (#4950) / ADR-004 (#4951).
- **#766** — triggering bug; its repro is AC-07's test.
- **#735 — folded into vnc-038** (no longer a sequencing dependency). Its three carry-items land on surfaces vnc-038 already reworks: **CI-1** token-to-stdout → ADR-008 / AC-11 / NFR-06 (redact the `token.rs:101` print; bundle is the sole token channel); **CI-2** `router.rs` ≤500 lines → a natural outcome of the route-grammar rewrite, not separate work (AC-12); **CI-3** stale `#![allow(dead_code)]` at `public_url.rs:19` → trivial cleanup (AC-13). vnc-038 closes #735; there is no longer a same-surface adjacency gate.
- **#767 / #768 / #769** — out of scope (RD-6 / fast-follow / separate).
- **Parity-corpus infra (#4956)** — reuse: hex (not base64) fixtures, `pub use` re-export of oracle fns for `tests/`.

## Integration Surface

Exact current signatures and the planned `v:2` shape, so downstream agents invent no names.

### Bundle codec — Rust (sole encoder)

| Item | Current (v:1) | Source |
|------|---------------|--------|
| Struct | `pub struct Bundle { v: u8, base_url: String, token: String, fp: String }` | `client_bundle.rs:52-68` |
| Version | `pub const BUNDLE_VERSION: u8 = 1;` | `client_bundle.rs:40` |
| Scheme | `pub const BUNDLE_SCHEME: &str = "unimatrix-bundle:";` | `client_bundle.rs:37` |
| Encoder | `pub fn encode_bundle(v: u8, base_url: &str, token_hex: &str, fp: &str) -> Result<String, ServerError>` | `client_bundle.rs:211` |
| URL derive | `derive_public_url(&Env::from_process()).base_url` | `client_bundle.rs:133` |
| Strict validate | `validate_schema` — exactly 4 keys | `client_bundle.rs:268-334` |

**Planned v:2:** `Bundle { v: u8, mcp_url: String, observe_url: String, token: String, fp: String }`; `BUNDLE_VERSION = 2`; `encode_bundle` composes both URLs from `{public_base}` + route grammar; `validate_schema` exact keys `{v, mcp_url, observe_url, token, fp}`.

### Bundle decoder — JS

| Item | Current (v:1) | Source |
|------|---------------|--------|
| Decode | `function decodeBundle(raw)` → `{v, base_url, token, fp}` | `lib/hook-client/bundle.js:58-127` |
| Length cap | `const MAX_RAW_LEN = 4096;` enforced FIRST on raw paste (GUARD 1) | `bundle.js:21,67-69` |
| Exact keys | `const EXPECTED_KEYS = ["v","base_url","token","fp"]; keysAreExactly(...)` | `bundle.js:24,107` |
| Version pin | `if (obj.v !== 1) throw BundleError(...)` | `bundle.js` |
| Error type | `class BundleError extends Error` | `bundle.js:30-35` |

**Planned v:2:** `EXPECTED_KEYS = ["v","mcp_url","observe_url","token","fp"]`; `obj.v !== 2`; validate `mcp_url`/`observe_url` are `https://` strings; return `{v, mcp_url, observe_url, token, fp}`. Guard ordering (length→scheme→base64url→JSON→strict schema) preserved.

### Client path-composition sites to DELETE (ADR-001 closed set)

| Site | Current code | Source |
|------|--------------|--------|
| C-1 slug append | `remote = endpointBase + "/v1/" + options.slug` | `init.js:305` |
| C-2 default append | `remote = endpointBase + "/v1"` | `init.js:307` |
| C-3 observe append | `u.pathname.replace(/\/+$/,"") + "/observe"` | `transport-http.js:84` |

After vnc-038: `init` stores `mcp_url` verbatim; `transport-http` posts to `observe_url` verbatim.

### Route grammar + resolver — Rust

| Item | Current | Source |
|------|---------|--------|
| `ProjectKey` | `enum ProjectKey { Default, Slug(ProjectSlug) }` | `seam.rs` |
| Parse | `pub(crate) fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError>` — 3 arms (tools→Default, slug, _→Default) | `seam.rs:178-194` |
| Resolver trait | `trait StoreResolver { fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>; }` | vnc-034 ADR-003 |
| Wave-2 resolver | `struct MultiProjectRouter { default: Option<ProjectEntry>, slugs: HashMap<ProjectSlug, ProjectEntry> }` | `project_resolver.rs:86-111` |
| Entry | `struct ProjectEntry { store: Arc<Store>, adapter: McpAdapter }` | `project_resolver.rs:39-84` |
| Build | `fn from_servers(default_store, default_server, slug_servers: Vec<ProjectServerInput>, max_body, allowed_origins) -> Result<Self, String>` | `project_resolver.rs:155` |
| Dispatch | `fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>` (Default + Slug arms) | `project_resolver.rs:212` |
| Resolve | `fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>` (Default + Slug arms) | `project_resolver.rs:199` |
| Boot swap | `if project_slugs.is_empty() { DefaultResolver::with_adapter(...) } else { MultiProjectRouter::from_servers(...) }` | `main.rs:1004-1042` |

After vnc-038: `ProjectKey::Default` removed for served-project routing; `parse_project_key` → single `/v1/{slug}/...` rule (loud error otherwise); `MultiProjectRouter` loses `default` field + `from_servers` default params + `Default` arms; boot builds the unified resolver from `project_slugs` only (empty ⇒ nothing servable). The resolver handles **only `ProjectKey::Slug`** (AC-09, cloud/container HTTP).

**Local STDIO/UDS is OUT of this surface (ADR-006, AC-10).** The local transports open the path-hash store directly and never reach the resolver:

| Item | Current (unchanged by vnc-038) | Source |
|------|--------------------------------|--------|
| Local UDS boot bind | opens `~/.unimatrix/{hash}/unimatrix.db` directly, threads `Arc<Store>` to handler | `main.rs:859` |
| Local STDIO boot bind | opens `~/.unimatrix/{hash}/unimatrix.db` directly, threads `Arc<Store>` to handler | `main.rs:1158` |

Delivery MUST NOT route these through the unified resolver; the ADR-004 deletions are HTTP-cloud/container-only (AC-10 regression boundary).

### Observe — Rust

| Item | Current | Source |
|------|---------|--------|
| Boot bind | `let served_store = resolver.resolve_store(&ProjectKey::Default)?` (ONCE at boot) | `main.rs:1045-1052` |
| Context | `ObserveContext { store, embed_service, vector_store, entry_store, adapt_service, server_version, session_registry, pending_entries_analysis, services }` | `main.rs:1058-1072` |
| Route | top-level `(Method::POST, "/observe")` split before slug routing | `router.rs:188` |

After vnc-038: top-level `/observe` removed; observe becomes `/v1/{slug}/observe` resolved per-request via `resolver.resolve_store(&key)`; boot-bound `resolve_store(Default)` deleted; the handler holds the `Arc<dyn StoreResolver>` and resolves per call (no pre-resolved `store` field).

### register / config — Rust

| Item | Current | Source |
|------|---------|--------|
| register | `fn register(&self, raw_slug: &str) -> Result<(), ServerError>` — State A/B/C | `projects.rs:264-337` |
| State B print | `eprintln!("re-add to config.toml ... [[projects]] slug = ...")` | `projects.rs:302-304` |
| State C print | `eprintln!("add to config.toml ... [[projects]] slug = ...")` | `projects.rs:335` |
| Boot read | `load_config_and_build_allowlist(&paths.data_dir) -> (..., project_slugs)` | `main.rs:627`, `:1616-1703` |
| Reserved | `pub const RESERVED_SLUGS: [&str; 4] = ["v1","health","observe","tools"];` + `is_reserved_slug` | `config.rs:2483-2492` |

After vnc-038: State B/C prints replaced by an atomic read-modify-write (temp+fsync+rename) of the `[[projects]]` stanza, idempotent and re-attach-safe; `RESERVED_SLUGS` value retained, derivation re-documented (ADR-005).

### First-boot token (#735 CI-1, ADR-008)

| Item | Current | Source |
|------|---------|--------|
| Token print | bearer token printed to first-boot stdout | `http/token.rs:101` |

After vnc-038: the token print is redacted/gated so the token never reaches stdout or `tracing` logs (NFR-06, AC-11). The token is delivered to the remote client **only** inside the `v:2` bundle (`{..., token, fp}`, ADR-002) — the bundle is the sole token channel for the cloud/container HTTP surface. Local STDIO/UDS has no bundle and its token handling is part of the unchanged direct-binding path (ADR-006); if `token.rs:101` is reachable on the local path, the redaction is gated by deployment context so local is left functionally unchanged (AC-10).

### #735 cleanup carry-items (CI-2, CI-3)

| Item | Current | After vnc-038 | AC |
|------|---------|---------------|----|
| Router size | `http/router.rs` ~562 lines (> 500-line guideline) | The route-grammar rewrite (default-alias removal, per-slug observe, `parse_project_key` simplification) extracts `router.rs` to at/under the 500-line guideline — a natural outcome of the rewrite, not separate work | AC-12 |
| Stale dead_code | module-level `#![allow(dead_code)]` + "until wiring lands" comment | both removed (the wiring landed; the allow no longer applies) — trivial cleanup | AC-13 (`public_url.rs:19`) |

## Error Boundaries

- **Bundle decode (client trust boundary):** `BundleError` on any guard (length, scheme, base64url, JSON, strict v:2 schema). Hard reject — no partial accept. Length cap runs first on the raw paste.
- **Route resolution (server):** `RouteError::UnknownProject` for an unregistered slug; `RouteError::InvalidSlug` from the allowlist. No `Default` fallback — an unmatched path is a loud error (AC-01/09), never a silent default store.
- **Config write (register):** atomic temp+rename; a crash yields the old or new complete file, never partial. Idempotent on existing stanza; State A is a loud error; State B re-attaches (open, never genesis).
- **First boot, empty `[[projects]]`:** nothing servable; loud actionable "register a project to begin" (AC-09).

## Open Questions for the Human / Downstream

1. **`tools` un-reservation (ADR-005).** This design keeps `tools` reserved (conservative, minimizes cutover/doc-window blast radius). If the human wants `tools` to be a registerable slug now, that is a one-line change + test — confirm preference.
2. **`token.rs:101` print scope (ADR-008, #735 CI-1).** Delivery must confirm whether the first-boot token print is HTTP-first-boot-only or shared with the local STDIO/UDS path. If shared, the redaction is gated by deployment context (not removed outright) so local stays functionally unchanged (AC-10). Spec/impl-level confirmation, not a re-litigation.

Resolved during this revision (no longer open):
- **Local-UDS store binding (ADR-006).** RESOLVED by the GATE-2 code-cited analysis: local STDIO (`main.rs:1158`) / UDS (`main.rs:859`) keep their **direct path-hash store binding** and are NOT routed through the unified resolver. The prior "materialize a path-hash key in the resolver map" question is moot — local never enters the resolver. Delivery must not route local through it (AC-10 regression boundary).
- **"Zero existing served users" / data-loss (RD-1).** RESOLVED SAFE by GATE-2: the hard cut (AC-09) is **cloud/container-HTTP only** — of which there are currently zero deployments. The deletions and the `v:2` bundle touch only the HTTP path; local STDIO/UDS never touches `parse_project_key`, the resolver, `ProjectKey::Default`, or a bundle, so local stores are unaffected with no migration and no operator action (AC-10).
