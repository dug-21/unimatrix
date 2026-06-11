# vnc-034 Wave 1 — Pseudocode Overview

> Per-component pseudocode for **Wave 1 only** (single-project HTTPS serving + pure-JS remote client + the C1/C2 connection-contract build-first sub-deliverable). Wave 2 (ProjectRouter, ProjectRegistry, slug-resolver logic) is OUT OF SCOPE — modeled here only as documented seam points. Grounded in ARCHITECTURE §2/§3/§7, ADR-001..007, and the existing `crates/unimatrix-server` + `packages/unimatrix` code surface.

## Components in this wave

| Component | File | Crate / location | Realizes |
|-----------|------|------------------|----------|
| CertProvisioner | cert-provisioner.md | `unimatrix-server/src/http/tls.rs` | FR-A2/A3/A4/A9, SR-01, R-07, R-08, R-11 |
| FingerprintComputer | fingerprint-computer.md | `unimatrix-server/src/http/tls.rs` | C2, FR-A6, R-02 |
| PublicUrl | public-url.md | `unimatrix-server/src/http/public_url.rs` (new) | C3, FR-A7, R-09 |
| BundleCodec | bundle-codec.md | `unimatrix-server/src/client_bundle.rs` (new) + JS decoder | C1, FR-A5/A5b/B9, R-05 |
| SlugRouter + StoreResolver seam | slug-router.md | `unimatrix-server/src/http/router.rs` | C4 (seam), FR-X1..X5, R-01, R-03 (parse-edge) |
| DefaultResolver | default-resolver.md | `unimatrix-server/src/http/router.rs` | C4 Wave-1 resolver, FR-X4/X5, R-04 |
| RemoteClient (`init --remote`) | remote-client.md | `packages/unimatrix/lib/init.js` + `lib/hook-client/` | FR-B1..B9, R-05, R-06, R-02 (pin) |
| Container posture | container-posture.md | `Dockerfile`, `compose.yaml`, `config.rs` | ADR-007, FR-A1/A8, NFR-12 |

**Doc deliverable (no pseudocode):** the Cert-rotation runbook (FR-A11 / AC-CT-ROT) is a Stage-3b documentation deliverable — a short operator doc (rotate cert → re-run `client-bundle` → re-`init` clients) shipped with the container. It has NO pseudocode file; its only code-side pairing is the diagnosable fingerprint-mismatch error designed in remote-client.md (`checkServerIdentity`).

## Locked contracts and signatures (downstream MUST NOT invent)

```rust
// router.rs (new)
pub enum ProjectKey { Default, Slug(ProjectSlug) }
pub struct ProjectSlug(String);                  // TryFrom<&str>: ^[a-z0-9][a-z0-9-]{0,62}$
pub trait StoreResolver: Send + Sync + 'static {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}
pub struct DefaultResolver { store: Arc<Store> }

// tls.rs (new/promoted)
fn load_or_generate_cert(data_dir: &Path, sans: &[String]) -> Result<(CertPem, KeyPem), ServerError>;
fn fingerprint_leaf_der(der: &[u8]) -> String;   // "sha256:" + lowercase_hex(sha256(der))

// public_url.rs (new)
fn derive_public_url(env: &Env) -> PublicUrl;    // { base_url, host, sans }

// client_bundle.rs (new)
fn run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError>;  // sync, pre-tokio (C-10)
```

C1 wire form (LOCKED, ADR-001): `unimatrix-bundle:<base64url(canonical-json)>`, single line, no padding,
canonical field order `v, base_url, token, fp`; decoded JSON = exactly `{"v":1,"base_url":..,"token":..,"fp":..}`.
C2 fingerprint (LOCKED, ADR-002): `sha256:<64 lowercase hex>` over the **served leaf DER** (not PEM, not chain).

## Shared types

| Type | Defined in | Used by |
|------|-----------|---------|
| `ProjectKey`, `ProjectSlug`, `StoreResolver`, `RouteError` | slug-router.md | default-resolver.md, slug-router.md; Wave-2 ProjectRouter (seam) |
| `PublicUrl { base_url: String, host: String, sans: Vec<String> }` | public-url.md | cert-provisioner.md (sans), bundle-codec.md (base_url), container/config (allowed_hosts) |
| `CertPem = Vec<u8>`, `KeyPem = Vec<u8>` (PEM byte buffers) | cert-provisioner.md | bundle-codec.md (reads leaf DER), tls acceptor |
| Bundle JSON `{ v:u8, base_url:String, token:String, fp:String }` | bundle-codec.md | remote-client.md (JS decoder mirror) |
| `Env` (env-var accessor: a `&dyn Fn(&str)->Option<String>` or struct over `std::env`) | public-url.md | derive_public_url; testable without `set_var` (Rust 2024 forbids unsafe set_var) |
| `ServerError`, `RouteError` | existing `crate::error` + new `RouteError` variant | all server components |

## Data flow (Wave 1, end to end)

```
FIRST BOOT (Rust binary, distroless no-shell — ARCHITECTURE §4.1)
  ensure_data_directory(--project-dir /data) -> ProjectPaths{ data_dir = /data/.unimatrix/{hash} }
  load_or_generate_token(data_dir)                      [existing, reused]
  pu = derive_public_url(env)                           [public-url.md]
  load_or_generate_cert(data_dir, &pu.sans)             [cert-provisioner.md] -> tls/{cert,key}.pem (key 0600)
  build_tls_acceptor(TlsConfig{cert,key})               [existing] -> TlsAcceptor
  resolver = DefaultResolver{ store }                   [default-resolver.md]
  SlugRouter{ resolver } inserted between PathRouter and McpAdapter  [slug-router.md]
  start_http_listener(0.0.0.0:8443, acceptor, service)  [existing]

BUNDLE EMISSION (client-bundle, sync pre-tokio subcommand — ARCHITECTURE §4.2)
  run_client_bundle(project_dir):                       [bundle-codec.md]
     read token (hex) + leaf DER from data volume
     fp = fingerprint_leaf_der(der)                     [fingerprint-computer.md]
     base_url = derive_public_url(env).base_url         [public-url.md]
     stdout <- "unimatrix-bundle:" + base64url(json{v,base_url,token,fp})   (blob ONLY)
     stderr <- human echo of base_url + fp ONLY         (token REDACTED — FR-A5b)

CLIENT ATTACH (init --remote <bundle> [--slug s] — ARCHITECTURE §4.3)
  decode bundle:                                        [remote-client.md, mirrors bundle-codec.md]
     (1) 4 KB RAW-string byte-length cap  BEFORE decode/parse   (belt-and-suspenders)
     (2) strip scheme prefix -> base64url-decode -> JSON.parse
     (3) STRICT schema reject (load-bearing): exactly {v,base_url,token,fp}
  endpoint = base_url + ("/v1/" + slug + "/tools" | "/v1/tools")
  pin: custom checkServerIdentity compares sha256(cert.raw) == fp.hexpart
  persist client config (token + pinned fp + endpoint); copy skills; size gate < 250 KB

REQUEST SERVING (per ARCHITECTURE §1.2)
  ... PathRouter -> SlugRouter.parse(path)->ProjectKey -> resolver.resolve_store(key) -> Arc<Store> -> McpAdapter
```

## C1/C2 build-first ordering rule (ADR-006 — load-bearing)

The shared connection contract is authored ONCE inside Wave 1 and consumed by both server (#726) and client (#725):

1. **First:** `fingerprint_leaf_der` (C2, fingerprint-computer.md) + the Rust **oracle test** that emits the golden parity corpus (`GOLDEN\t<der-hex>\t<fingerprint>`), and the BundleCodec Rust encoder (C1, bundle-codec.md).
2. **Then:** the server `run_client_bundle` (consumes the encoder + fingerprint) AND the JS decoder mirror + JS `checkServerIdentity` (consume the committed fixtures, never hand-write the JS golden — SR-02).
3. The committed fixture corpus is the single source of truth: divergence fails CI, not a user's connect.

Implication for Stage 3b: fingerprint-computer.md and the Rust half of bundle-codec.md land before remote-client.md's decoder/pin and before run_client_bundle's stderr/stdout split are validated against the corpus.

## Wave-2 seam points (modeled here, NOT implemented this wave)

| Seam point | Wave-1 state | Wave-2 fills |
|-----------|-------------|--------------|
| `StoreResolver` trait | `DefaultResolver` only; `ProjectKey::Slug(_)` -> `RouteError::UnknownProject` | swap in `ProjectRouter` impl at the same `SlugRouter` call site (no interface re-cut) |
| `ProjectSlug` newtype + allowlist parse | Built + validated at the parse edge (route grammar exists; slug path inert) | resolver maps slug -> per-slug `Arc<Store>` |
| Route grammar `/v1/{slug}/tools/...` | Parses to `ProjectKey::Slug`; inert resolver | lights up additively; `/v1/tools/...` alias unchanged (ADR-005) |
| Per-slug hot caches | N/A (single store) | live INSIDE `resolve_store`, not a new edge (ADR-003, SR-07) |
| `[[projects]]` config, register/list/delete CLI, per-slug data dirs | NOT present | Wave 2 (#727) — project-router.md / project-registry.md (NOT authored this wave) |

The Wave 1 ↔ Wave 2 boundary IS the `StoreResolver` trait. Everything above it is Wave 1.

## Cross-cutting constraints (apply to every component file)

- Rust: no `unsafe`, no `.unwrap()`/`.expect()` in non-test code, max 500 lines/file, errors via `.map_err()` into `ServerError`/`RouteError`.
- No new crates: only `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`, plus already-present `hex`, `base64`, `sha2`/ring, `serde_json`. Client + shipped JS: zero runtime deps.
- Secrets are files, never DB: token + cert/key on the data volume, key mode `0600`. Token redacted from all stdout/stderr/logs except the opaque bundle blob.
- Distroless no-shell: all first-boot provisioning is in the Rust binary; fail loud-and-actionable (no panic) if `/data` unwritable.
```
