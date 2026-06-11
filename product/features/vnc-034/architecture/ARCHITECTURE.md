# vnc-034 Architecture — Personal-Cloud Multi-Project Serving

> Umbrella architecture for the personal-cloud serving arc: server serving (Group A), pure-JS remote client (Group B), and multi-project routing (Group C). Locks the six shared contracts C1–C6 coherently, then defines the Wave 1 / Wave 2 boundary at the interface level. Grounded in the existing `crates/unimatrix-server` code surface (router.rs, tls.rs, token.rs, auth.rs, listener.rs, config.rs, server.rs) and ADR-004/ADR-005 path-hash isolation.

## 1. System Overview

The Unimatrix container is the **server/operator side** of the personal cloud. It does not attach outward; clients attach **in**. The destination (goal #4934) is *one container, one bearer token, one command, serving N projects to N clients over HTTPS* — single tenant, full per-project isolation, multi-LLM clients.

Three gaps separate the shipped substrate from a reachable cloud, mapped to the three goal groups:

| Gap | Group | Wave |
|-----|-------|------|
| Container serves no reachable TLS endpoint; emits no connection bundle | A — Server serving | Wave 1 |
| No pure-JS remote init; no bundle ingestion; no attach-to-slug | B — Client | Wave 1 |
| One container serves at most one project's knowledge | C — Multi-project routing | Wave 2 |

The umbrella exists because these groups **share wire contracts** (the bundle, the cert fingerprint, the URL/slug grammar, the store-resolution seam). Designing them in isolation guarantees drift. This architecture locks C1–C6 once.

### 1.1 The central architectural spine: one isolation seam, two resolvers

Every read and write in the system funnels through a single store-resolution seam, `resolve_store`. It has **two resolvers behind one interface**:

- **Local UDS install** — resolves the daemon's path-hash store (ADR-004: `SHA-256(canonical_project_root)[..16]`). No slug. The slug-free path.
- **Cloud install** — resolves by URL slug (Wave 2) or the single default project (Wave 1).

This is the definitive answer to SR-07 / SR-08 / A2 / A4: the local install exercises the *exact* seam the cloud depends on, so the common case is the seam's proving ground. There is no cloud-only isolation path. Process-per-project is explicitly **not** required (C4) — safe Rust + the single-funnel invariant provide isolation in-process at 1× model memory.

### 1.2 Request lifecycle (end-to-end)

```
client (pure-JS, pinned TLS)
   │  HTTPS POST /v1/{slug?}/tools/...   Authorization: Bearer <hex>
   ▼
TCP accept → optional TLS handshake (listener.rs)
   ▼
StaticTokenAuth   (auth.rs)   constant-time bearer compare; GET /health bypass
   │  inserts ResolvedIdentity into request extensions
   ▼
PathRouter        (router.rs) /health, /observe, else → MCP
   ▼
SlugRouter [NEW, the resolve_store seam]   parse path → ProjectKey → resolve_store(key) → Arc<Store>
   │  Wave 1: default resolver returns the one store
   │  Wave 2: ProjectRouter resolver: slug → per-slug McpAdapter (own store/vector/hashchain)
   ▼
McpAdapter / StreamableHttpService (rmcp)  →  UnimatrixServer tool dispatch
   ▼
ToolContext built with external_identity; store handle is the resolved Arc<Store>
```

The slug is the **only** carrier of project identity (transport-derived, never request payload — C4 invariant 1, SR-06). The agent literally has no field in which to name another project, so mis-targeting is *unrepresentable*, not merely rejected.

## 2. Component Breakdown and Boundaries

| Component | Crate / location | Responsibility | Wave |
|-----------|------------------|----------------|------|
| **CertProvisioner** | `unimatrix-server/src/http/tls.rs` (promote test helper) | First-boot `load_or_generate_cert` → `{data_dir}/tls/{cert,key}.pem`, key `0600`, SAN derived from C3 | 1 |
| **FingerprintComputer** | `unimatrix-server/src/http/tls.rs` (new fn) | `sha256:<lowercase-hex>` over served leaf DER (C2) | 1 |
| **PublicUrl** | `unimatrix-server/src/http/` (new module or config) | Single derivation of base-url, allowed_hosts default, cert SAN from `UNIMATRIX_PUBLIC_URL` (C3) | 1 |
| **BundleCodec** | `unimatrix-server/src/client_bundle.rs` (new) + JS mirror | Encode/decode C1 bundle; the server side is a sync pre-tokio subcommand (C-10) | 1 |
| **SlugRouter (resolve_store seam)** | `unimatrix-server/src/http/router.rs` (new layer wrapping ProjectRouter) | THE isolation seam (C4). Parses path → `ProjectKey`; owns the `StoreResolver` handle | 1 (seam) / 2 (slug resolver) |
| **StoreResolver** trait | `unimatrix-server/src/http/router.rs` | `resolve_store(&ProjectKey) -> Result<Arc<Store>, RouteError>`. Two impls: `DefaultResolver` (Wave 1), `ProjectRouter` (Wave 2) | 1 / 2 |
| **ProjectRouter** | `unimatrix-server/src/http/router.rs` (extend existing pass-through) | Wave-2 `StoreResolver`: `[[projects]]` slug → per-slug `McpAdapter`; per-slug hot caches | 2 |
| **ProjectRegistry / lifecycle CLI** | `unimatrix-server/src/projects.rs` (new) | register / list / delete slugs; never client-auto-created (C5) | 2 |
| **RemoteClient** | `lib/hook-client/` (pure JS) | `init --remote <bundle>`: parse bundle, pin cert, append slug, copy skills, size-gate | 1 |
| **Container posture** | `Dockerfile`, `compose.yaml` (nan-014) | `EXPOSE`/`ports`, HTTP-enable env, bind-mount UID docs; hardening preserved | 1 |

### 2.1 Boundaries

- **Server ↔ Client boundary** is exactly C1 (bundle) + C2 (fingerprint). Nothing else crosses. The slug is **not** in the bundle — it is appended client-side at `init` (C1/C5).
- **Routing edge ↔ tool dispatch boundary** is the resolved `Arc<Store>`. Once resolved, no downstream path obtains a different store handle (C4 invariant 2).
- **OSS ↔ enterprise boundary** is held by seam interfaces only (`TlsConfig.enabled`, `BearerValidator`, `StoreResolver`, slug-as-scope). Enterprise extends; it never re-architects (SR-04).
- **Wave 1 ↔ Wave 2 boundary** is the `StoreResolver` trait: Wave 1 ships `DefaultResolver`, Wave 2 swaps in `ProjectRouter`. The trait, the route grammar, and the seam call site are all Wave-1 work.

## 3. The Six Contracts as Concrete Interfaces

### C1 — Connection bundle (wire form resolved in ADR-001)

Cloud-wide artifact; **slug is NOT in it**. Carries `{base_url, token, cert_fingerprint}`. Server emits via `client-bundle`; client ingests via `init --remote`.

Canonical JSON (the encoded payload):
```json
{"v":1,"base_url":"https://cloud.example:8443","token":"<64-hex>","fp":"sha256:<64-hex>"}
```
Wire form (ADR-001): `unimatrix-bundle:<base64url(canonical-json)>` — single line, scheme-prefixed, no padding. One parser each side. Field order fixed for the canonical encode.

### C2 — Cert-fingerprint format (ADR-002)

`sha256:<lowercase-hex>` — SHA-256 over the **served leaf certificate's DER** (not PEM, not the chain). Computed once on the Rust side from the same DER bytes rustls serves; the JS client recomputes over the DER it receives in `checkServerIdentity` and compares constant-form-equal. Cross-stack parity fixtures generated from the Rust oracle, never hand-written (SR-02, pattern #4766).

### C3 — Public-URL knob (ADR derived; one derivation function)

`UNIMATRIX_PUBLIC_URL` (default = container service-name/host; loud `https://<EDIT-ME>:8443` placeholder if unset). **One knob, three consumers**, all reading from a single `derive_public_url() -> PublicUrl { base_url, host, sans }`:

| Consumer | Derivation |
|----------|-----------|
| Bundle `base_url` (C1) | `base_url` verbatim |
| `allowed_hosts` default | `host` when set; permissive-with-warning when unset |
| Cert SAN (C2/SR-01) | `["localhost","127.0.0.1","0.0.0.0", host]` |

Test asserts `bundle.host ∈ cert.SANs` (SR-10). Auto-detect from socket is rejected.

### C4 — URL/route grammar + resolve_store seam (ADR-003; THE isolation seam)

Route grammar (locked by ADR-005 / OQ-C resolution):
```
/v1/tools/...            → ProjectKey::Default        (single-project alias; local + cloud Wave 1)
/v1/{slug}/tools/...     → ProjectKey::Slug(slug)     (cloud Wave 2; additive)
/health  (GET)           → auth-bypassed health
/observe (POST)          → existing vnc-022 path (unaffected)
```

Seam interface (Wave 1, caller-owned, router injected — SR-07):
```rust
/// Transport-derived project identity. Never constructed from a request payload.
pub enum ProjectKey {
    Default,            // slug-free: local path-hash store OR cloud single-project alias
    Slug(ProjectSlug),  // Wave 2 only; cloud multi-project
}

pub trait StoreResolver: Send + Sync + 'static {
    /// The single funnel. Every read/write in the process resolves here.
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}

// Wave 1
pub struct DefaultResolver { store: Arc<Store> } // returns `store` for ProjectKey::Default;
                                                 // RouteError::UnknownProject for any Slug
// Wave 2 (swapped in; same trait)
pub struct ProjectRouter { default: Option<Arc<Store>>, slugs: HashMap<ProjectSlug, ProjectEntry> }
```

`SlugRouter` (the new layer) parses the path into a `ProjectKey`, calls `resolve_store`, and threads the resulting `Arc<Store>` into the per-project `McpAdapter`. **Per-slug hot-path routing lives inside the seam method** (Principle #7 per-slug caches rebuilt per project by tick), not in a new edge (SR-07). The local install uses the identical seam: the daemon constructs `DefaultResolver { store: path_hash_store }` and every request resolves `ProjectKey::Default` (SR-08 / A2 — the path-hash logic is unchanged from ADR-004; the slug is a *different resolver behind the same trait*, never a leak of path-hash into cloud).

Invariants the seam enforces in **all** modes (local UDS, cloud alias, cloud slug):
1. Identity is transport-derived (URL slug or path-hash) — payload cannot name a project.
2. The resolved `Arc<Store>` is the sole write capability, threaded from the edge.
3. Single funnel, no bypass — proof-grade treatment.

### C5 — Slug + register/attach model (ADR-004 below; cardinality)

`ProjectSlug` = operator-declared identity, **decoupled from any client path-hash**. Allowlist (SR-09): `^[a-z0-9][a-z0-9-]{0,62}$`, lowercase, no `.`/`/`/`%`/path separators, ≤ 63 chars. Validated at the parse edge **before** any filesystem use, so `../`, encoded separators, and absolute paths are unrepresentable and **cannot escape** `/data/.unimatrix/{slug}/`.

Two operations:
- **register** (server-side CLI, Wave 2): creates the store + `/data/.unimatrix/{slug}/` tree. Never client-auto-created.
- **attach** (`init --remote …/v1/<slug>`): no store creation; errors if slug unregistered.

Cardinality: **N clients : 1 slug : 1 tenant**, AND **1 client : 1 project** (permanent OSS boundary). Multi-LLM = N distinct client instances attaching the same slug (N:1), no per-LLM code. The 1:1 bound is enforced at the transport (each client init bakes exactly one slug into its base-url; it has no mechanism to address a second), so cross-project fan-out is unrepresentable (SR-06). Enterprise relaxes via server-enforced `unimatrix_project` claim on the C6 seam — additive.

### C6 — Auth / scope / transport separation

Three concerns, never collapsed: **token authorizes** (`BearerValidator` seam, `auth.rs`), **slug scopes data** (NOT a security boundary in OSS single-tenant — it is an integrity boundary), **cert secures transport** (pinning). Enterprise binds slug → JWT `unimatrix_project` claim + RBAC on the same `BearerValidator` seam. The OSS code holds the seam (the trait already exists); enterprise is additive (SR-04).

## 4. Data Flow

### 4.1 Provisioning (first boot, in the Rust binary — distroless, no shell)
1. `ensure_data_directory(--project-dir /data)` → `ProjectPaths { data_dir = /data/.unimatrix/{hash}, ... }` (ADR-005).
2. `load_or_generate_token(data_dir)` → 32-byte token, hex at `{data_dir}/token` mode `0600` (exists).
3. `load_or_generate_cert(data_dir, sans)` [NEW] → `{data_dir}/tls/{cert,key}.pem`, key `0600`, SANs from `derive_public_url()` (SR-01, SR-11: fail loud-and-actionable if `/data` unwritable; no `.unwrap()`).
4. Listener binds `0.0.0.0:8443` with the TLS acceptor (existing `build_tls_acceptor`).

### 4.2 Bundle emission (`client-bundle`, sync pre-tokio subcommand — C-10)
1. Read token + leaf cert DER from data volume.
2. `fp = "sha256:" + hex(sha256(leaf_der))` (C2).
3. `base_url = derive_public_url().base_url` (C3).
4. Encode `unimatrix-bundle:<base64url(json)>` (C1) → stdout. Token never logged elsewhere, never in any DB.

**Output contract (hard contract — stdout/stderr split):**

| Stream | Content | Rationale |
|--------|---------|-----------|
| **stdout** | the opaque pasteable `unimatrix-bundle:<base64url(json)>` blob, and nothing else | Pipeable — `client-bundle \| pbcopy` / redirection yields exactly the blob with no contamination. |
| **stderr** | human-readable decoded echo of **`base_url` + `cert-fingerprint` ONLY**, with the **TOKEN REDACTED / OMITTED** | NFR-06: the token is never logged — this redaction is **non-negotiable**. The echo exists to catch the most likely onboarding failure: an unset `UNIMATRIX_PUBLIC_URL` producing an `https://<EDIT-ME>:8443` base-url (C3) that the operator would otherwise paste blind. Showing `base_url` + `fp` (both non-secret) lets the operator eyeball the URL before distributing; the token stays out of the terminal/scrollback entirely. |

The token appears **only** inside the base64url stdout blob, never in the stderr echo, never in any log line.

### 4.3 Client attach (`init --remote <bundle> [--slug <s>]`)
1. Parse `unimatrix-bundle:` → `{base_url, token, fp}` (schema-validate; reject on any missing/extra field — trust boundary, SR-09).
2. Append slug → effective endpoint `base_url + /v1/{slug}/tools/...` (or `/v1/tools/...` for Default).
3. Persist client config (token + pinned `fp` + endpoint). Copy skills. Do NOT append CLAUDE.md block (init prints the `/unimatrix-init` pointer only).
4. Size gate < 250 KB enforced as a Wave-1 acceptance test.

### 4.4 Request serving (per §1.2). Reads/writes resolve through `resolve_store(ProjectKey)`; per-slug tick rebuilds per-project hot caches.

### 4.5 Cert-rotation runbook (documented operator DELIVERABLE — not an afterthought)

Cert rotation invalidates the pinned fingerprint (C2/ADR-002): every client pins `sha256(leaf_der)`, so a new cert means every client rejects the new leaf until re-pinned. This is **by design** (pinning, not CA trust), but it MUST ship with a documented operator procedure so a rotation-without-re-bundle is a known 3-step fix, not an opaque TLS failure.

The runbook is a **deliverable** (a short operator doc shipped with the container), paired with the already-designed clean/diagnosable rejection of the old fingerprint:

1. **Rotate the cert** — replace `{data_dir}/tls/{cert,key}.pem` (or delete to trigger first-boot `load_or_generate_cert` regeneration) and restart the container.
2. **Re-run `client-bundle`** — emits a new bundle carrying the new `fp` (§4.2); base-url and token are unchanged.
3. **Re-init clients** — `init --remote <new-bundle>` re-pins the new fingerprint on each client.

The pairing that keeps this a 3-step fix rather than an opaque error: the client's fingerprint mismatch in `checkServerIdentity` (ADR-002) MUST reject with a clean, diagnosable message naming the expected vs. presented `sha256:` fingerprint and pointing at "re-run `client-bundle` and `init --remote`". A rotation-without-re-bundle then surfaces as a legible "pinned fingerprint mismatch — cert was rotated; re-bundle" error, not a bare TLS handshake failure. Minimal procedure, not a feature — no rotation tooling, no automation; the existing `client-bundle` + `init` flow is the rotation flow.

## 5. Technology Decisions

| Decision | Choice | Rationale / ADR |
|----------|--------|-----------------|
| Cert generation | Promote test-only `rcgen 0.13` helper to production `load_or_generate_cert` with explicit SANs/validity/`0600` | No new crate (constraint, A3). SR-01: production params, not inherited test defaults. Rotation procedure: §4.5 runbook (re-bundle + re-init) |
| TLS stack | Existing `tokio-rustls 0.26` + `rustls-pemfile 2` `build_tls_acceptor` | Already wired; `TlsConfig.enabled` seam preserved for enterprise proxy |
| Fingerprint | `sha256(DER)` via existing hashing; `rand 0.9` already present for token | No new crate (A3) |
| Bundle wire form | `unimatrix-bundle:<base64url(json)>` | ADR-001 — copy-paste safe, single parser |
| Store handle | `Arc<Store>` (`unimatrix_store::SqlxStore`) threaded from the seam | Existing Arc-clone pattern; in-process multi-store (C4 — no process-per-project) |
| Routing | New `SlugRouter` layer + `StoreResolver` trait wrapping the existing `ProjectRouter` | ADR-003; reuses the axum/tower stack |
| Client | Pure JS, zero native binary, zero deps; custom `checkServerIdentity` fingerprint compare | SR-03 size gate; no CA path |

No new server crates. Client and shipped JS stay dependency-free.

## 6. Integration Points with Existing Code

| Integration point | Existing surface | vnc-034 change |
|-------------------|------------------|----------------|
| Subcommand dispatch | `main.rs` C-10 sync block (Hook/Version/Health/Stop, ~L247–389) | Add `Command::ClientBundle` → `client_bundle::run` (sync, pre-tokio) |
| Listener wiring | `main.rs` L840–900, gated `config.http.enabled` | Insert `SlugRouter` between `PathRouter` and `ProjectRouter`; provision cert before acceptor |
| TLS | `http/tls.rs` `build_tls_acceptor`, test-only `generate_self_signed` (L118) | Promote to `load_or_generate_cert`; add `fingerprint_leaf_der` |
| Token | `http/token.rs` `load_or_generate_token` | Reused unchanged; read by `client-bundle` |
| Auth | `http/auth.rs` `StaticTokenAuth`, `BearerValidator`, `/health` bypass | Unchanged; enterprise claim is the additive seam |
| Router | `http/router.rs` `ProjectRouter` (pass-through), `McpAdapter`, `PathRouter` | `ProjectRouter` becomes the Wave-2 `StoreResolver`; new `SlugRouter` + `DefaultResolver` |
| Store / paths | `engine/project.rs` `ensure_data_directory`, `compute_project_hash`; `open_store_with_retry` | Local path-hash resolution feeds `DefaultResolver`; Wave-2 per-slug data dirs |
| Config | `config.rs` `HttpConfig`, `TlsConfig`, `load_config` | Add `[[projects]]` slug config (Wave 2); `UNIMATRIX_PUBLIC_URL` env read |
| Container | nan-014 Dockerfile/compose (ADR-005 `--project-dir /data`, `HOME=/data`) | `EXPOSE 8443`/`ports`, HTTP-enable env, bind-mount UID docs; hardening preserved |
| `init --remote` | client F3 base flow (#679), F4a TS HTTP client (#680) | Bundle ingestion + slug append + cert pinning + size gate |

## 7. Integration Surface (exact signatures — downstream MUST NOT invent these)

| Integration point | Type / Signature | Source |
|-------------------|------------------|--------|
| `ProjectRouter` (existing) | `struct ProjectRouter<ReqBody> { default_server: McpAdapter, .. }`; `route_mcp(&mut self, Request<ReqBody>) -> Result<Response<BoxBody<Bytes,Infallible>>, Infallible>` | router.rs:297–364 |
| `McpAdapter` (existing) | `struct McpAdapter { streamable: StreamableHttpService<UnimatrixServer, LocalSessionManager>, max_body_bytes: usize }`; `new(server, max_body_bytes, allowed_origins)` | router.rs:385–482 |
| `PathRouter` (existing) | `struct PathRouter<ReqBody> { project_router, observe_ctx }`; dispatch /health · /observe · MCP | router.rs:95–270 |
| `build_tls_acceptor` (existing) | `fn(config: &TlsConfig) -> Result<Option<TlsAcceptor>, ServerError>` | tls.rs:33–79 |
| `TlsConfig` (existing) | `{ enabled: Option<bool>, cert_path: Option<PathBuf>, key_path: Option<PathBuf> }`; `is_enabled()` | config.rs:2149–2180 |
| `generate_self_signed` (test-only) | `fn() -> (Vec<u8>, Vec<u8>)` (PEM cert, PEM key) — promote | tls.rs:118–125 |
| `load_or_generate_token` (existing) | `fn(data_dir: &Path) -> Result<Vec<u8>, ServerError>` (32 raw bytes) | token.rs:37–54 |
| `StaticTokenAuthLayer` / `BearerValidator` | `StaticTokenAuthLayer::new(token_bytes: [u8;32])`; `trait BearerValidator { fn validate(&self,&str) -> Pin<Box<dyn Future<Output=Result<ResolvedIdentity,AuthError>>+Send+'_>> }` | auth.rs:61–167 |
| `start_http_listener` (existing) | `async fn<S>(config: &HttpConfig, tls: Option<TlsAcceptor>, service: S, shutdown: CancellationToken) -> Result<(JoinHandle<()>, SocketAddr), ServerError>` | listener.rs:38–79 |
| `HttpConfig` (existing) | `{ enabled, content_port:8443, bind_address:"0.0.0.0", max_concurrent_sessions, max_request_body_bytes, connection_timeout_secs, allowed_origins }` | config.rs:2107–2126 |
| `ensure_data_directory` (existing) | `fn(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>` | project.rs:146–187 |
| `compute_project_hash` (existing) | `fn(project_root: &Path) -> String` (SHA-256[..16]) | project.rs:130–136 |
| `open_store_with_retry` (existing) | `async fn(db_path: &Path) -> Result<Arc<Store>, Box<dyn Error>>` | main.rs:1676–1707 |
| `UnimatrixServer` (existing) | fields `store: Arc<Store>`, `entry_store: Arc<Store>`, `vector_store`, ...; `tool_router: ToolRouter<Self>` | server.rs:191–241 |
| `build_context_with_external_identity` (existing) | `fn(.., external_identity: Option<&ResolvedIdentity>) -> Result<ToolContext, ErrorData>` | server.rs:414–519 |
| `load_config` (existing) | `fn(home_dir: &Path, data_dir: &Path) -> Result<ConfigLoadResult, ConfigError>` | config.rs:2806–2894 |
| **`StoreResolver`** [NEW] | `trait StoreResolver { fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>; }` | router.rs (new) |
| **`ProjectKey`** [NEW] | `enum ProjectKey { Default, Slug(ProjectSlug) }` | router.rs (new) |
| **`ProjectSlug`** [NEW] | newtype; `TryFrom<&str>` enforcing `^[a-z0-9][a-z0-9-]{0,62}$` | router.rs (new) |
| **`run_client_bundle`** [NEW] | `fn(project_dir: Option<PathBuf>) -> Result<(), ServerError>` (sync, pre-tokio C-10) | client_bundle.rs (new) |
| **`derive_public_url`** [NEW] | `fn(env: &Env) -> PublicUrl { base_url: String, host: String, sans: Vec<String> }` | new module |
| **`fingerprint_leaf_der`** [NEW] | `fn(der: &[u8]) -> String` → `"sha256:" + lowercase_hex(sha256(der))` | tls.rs (new) |
| **Bundle (C1)** [NEW] | `unimatrix-bundle:<base64url(json{v,base_url,token,fp})>` | client_bundle.rs + JS |

## 8. Wave 1 vs Wave 2 Boundary (interface level)

**Wave 1 (Groups A + B; #726 + #725) — ships the full transport/security/client stack against a single implicit project:**
- C1/C2/C3/C6 fully realized. C4 route grammar locked; `StoreResolver` trait + `SlugRouter` seam + `DefaultResolver` built (the one store routes *through* the seam, not around it — A4). C5 register/attach *modeled*; only the Default path exercised.
- Route shape: `/v1/tools/...` (the default alias — OQ-C). `/v1/{slug}/...` parses to `ProjectKey::Slug` and returns `RouteError::UnknownProject` until Wave 2 (additive; no Wave-1 client re-init — SR-05).
- Includes the **shared C1/C2 contract fixtures** (cross-stack parity, OQ-D) consumed by both server and client.

**Wave 2 (Group C; #727) — adds only the routing dimension to the validated Wave-1 base:**
- Swap `DefaultResolver` → `ProjectRouter` (same `StoreResolver` trait, same `SlugRouter` call site). No interface re-cut.
- `[[projects]]` config + `ProjectSlug` resolver + per-slug data dirs (`/data/.unimatrix/{slug}/`) + register/list/delete CLI + per-slug hot caches.
- Purely additive: existing `/v1/tools/...` clients keep working (the Default resolver remains the answer for slug-free requests).

**The boundary is the `StoreResolver` trait.** Everything above it (auth, TLS, bundle, client) is wave-1; everything that varies between single- and multi-project is one trait impl swap.

## 9. Open Questions Resolved (ADR cross-reference)

| OQ | Resolution | ADR |
|----|-----------|-----|
| OQ-A — bundle wire form | `unimatrix-bundle:<base64url(json)>` | ADR-001 |
| OQ-B — slug discovery for attach | No listing surface in OSS; operator tells the slug out-of-band; `init` appends it | ADR-004 |
| OQ-C — Wave-1 addressing | `/v1/tools/...` default alias (Wave 2 additive; no client re-init) | ADR-005 |
| OQ-D — wave-to-issue mapping | Wave 1 = #726 + #725 (with shared C1/C2 contract-fixtures sub-deliverable); Wave 2 = #727 | ADR-006 |
| C2 fingerprint format | `sha256:<lowercase-hex>` over leaf DER + parity fixtures | ADR-002 |
| C4 route + seam | `StoreResolver` trait, two resolvers, single funnel | ADR-003 |
| Container HTTP-enable mechanism (§10 Q1) | env var `UNIMATRIX_HTTP_ENABLED=true`, container-scoped, not a baked config (C3 surface consistency) | ADR-007 |

## 10. Open Questions for the Human

1. ~~**Container HTTP-enable mechanism (final form).**~~ **RESOLVED → ADR-007.** Container HTTP-enable is the env var `UNIMATRIX_HTTP_ENABLED=true` (container-scoped), NOT a baked config file — surface consistency with `UNIMATRIX_PUBLIC_URL` (C3), greppable/overridable in `compose.yaml` without an image rebuild (distroless, no shell), global binary default `http.enabled=false` stays clean, boolean is non-sensitive (token/cert stay as files per NFR-05/06).
2. **OQ-B confirmation.** No slug-listing endpoint is the safe default (smallest attack surface). Confirm the operator-out-of-band UX is acceptable for solo-developer onboarding, or whether an *authenticated* `client-bundle --list-slugs` (Wave 2, behind bearer) is wanted later. Recorded in ADR-004 as deferred/additive.
3. **base64url alphabet + cap.** ADR-001 fixes base64url no-pad; confirm a hard bundle length cap (e.g. ≤ 4 KB) as a client-parser DoS guard at the trust boundary (SR-09).
