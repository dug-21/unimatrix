# vnc-034 Implementation Brief — Personal-Cloud Multi-Project Serving

> **Umbrella feature.** Server serving (Group A) + pure-JS remote client (Group B) + multi-project routing (Group C), decomposed into two delivery waves over six locked shared contracts (C1–C6). This brief compiles the Session 1 design (SCOPE, SPECIFICATION, ARCHITECTURE, ADR-001..007, RISK-TEST-STRATEGY, ALIGNMENT-REPORT) into the implementation-ready surface Session 2 consumes. Regenerated 2026-06-11 after the human locked four design decisions (A1 permanence affirmed; ADR-007 HTTP-enable env var; 4 KB raw-string bundle cap before decode; two onboarding items elevated to hard requirements) — see *Resolved Decisions* and *Delivery Guidance*.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-034/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-034/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-034/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-034/architecture/ARCHITECTURE.md |
| Risk Strategy | product/features/vnc-034/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-034/ALIGNMENT-REPORT.md |
| ADR-001 — Bundle wire form | product/features/vnc-034/architecture/ADR-001-bundle-wire-form.md |
| ADR-002 — Cert fingerprint format | product/features/vnc-034/architecture/ADR-002-cert-fingerprint-format.md |
| ADR-003 — resolve_store seam | product/features/vnc-034/architecture/ADR-003-resolve-store-seam.md |
| ADR-004 — Slug register/attach | product/features/vnc-034/architecture/ADR-004-slug-register-attach.md |
| ADR-005 — Wave-1 default alias | product/features/vnc-034/architecture/ADR-005-wave1-default-alias.md |
| ADR-006 — Wave-to-issue mapping | product/features/vnc-034/architecture/ADR-006-wave-to-issue-mapping.md |
| ADR-007 — Container HTTP-enable env var | product/features/vnc-034/architecture/ADR-007-http-enable-env-var.md |

## Goal

Turn the partially-shipped personal-cloud substrate into a reachable, operator-run cloud: one Linux container serves N fully-isolated projects to N clients over pinned TLS HTTPS — single tenant, one bearer token, one connection bundle. The operator stands up the container and registers projects; clients (pure-JS, any LLM CLI) attach *in*, each bound to exactly one project, knowledge flowing at full fidelity. The design locks six cross-cutting wire/isolation contracts (C1–C6) once so the two delivery waves slot together without interface drift, and every cloud mechanism reduces identically to the local-UDS single-project install.

## Delivery Waves (this umbrella decomposes into existing wave issues — ADR-006)

| Wave | Scope | Issue(s) |
|------|-------|----------|
| **Wave 1 — Single-project serving + client** | Groups A + B against a single implicit project. C1/C2/C3/C6 fully realized; C4 built minimal (route grammar + `StoreResolver` trait + `SlugRouter` seam + `DefaultResolver` returning the one store); C5 register/attach *modeled*, only the Default path exercised. End state: operator runs the container, gets a bundle, attaches a Linux/macOS-arm/Windows client over pinned TLS, knowledge flows. | #726 (vnc-032, server serving) + #725 (nan-019, client init) |
| **Wave 1 sub-deliverable — C1/C2 contract (build-first)** | Shared connection-contract authored ONCE inside Wave 1, consumed by both #726 and #725: C1 bundle codec (Rust encoder + JS decoder, ADR-001) and C2 fingerprint + cross-stack parity fixtures from the Rust oracle (ADR-002). No own issue — a build-ordering rule: lands before either `client-bundle` or client bundle-ingestion depends on it. | (sub-deliverable within Wave 1) |
| **Wave 2 — Multi-project routing** | Group C against the validated Wave-1 base. Swap `DefaultResolver` → `ProjectRouter` behind the same `StoreResolver` trait + same `SlugRouter` call site; `[[projects]]` config + `ProjectSlug` resolver + per-slug data dirs + register/list/delete CLI + per-slug hot caches. Purely additive — no Wave-1 client re-init. | #727 (vnc-033, multi-project routing) |

The Wave 1 ↔ Wave 2 boundary **is** the `StoreResolver` trait. Everything above it (auth, TLS, bundle, client) is Wave-1; everything that varies between single- and multi-project is one trait-impl swap.

## Delivery Guidance (coordination — locked 2026-06-11)

Deliver this as **ONE feature in TWO passes** — NOT as separate feature designs, and NOT as a big-bang single PR.

| Pass | Scope | PR closes / references |
|------|-------|------------------------|
| **Wave 1** | Server (#726) + client (#725) + the C1/C2 connection-contract build-first sub-deliverable | Its PR **closes #725 + #726** and **references #733** |
| **Wave 2** | Multi-project routing (#727) — populate the `resolve_store` seam with `ProjectRouter` | Its PR **closes #727** |

**PR-size caveat (delivery decides, not pre-cut here):** if a delivery session finds Wave 1 too heavy for a comfortably reviewable PR, the natural sub-cut is **[C1/C2 contract + cert/serving] as PR-1** and **[client init] as PR-2**. This is a fallback, judged against the *real diff* — do NOT pre-cut Wave 1 into two PRs in advance; the single Wave-1 PR is the default, and a split is taken only if the diff genuinely warrants it.

## Component Map

Maps architecture §2 components to pseudocode + test-plan files (file paths populated in Session 2 Stage 3a).

| Component | Wave | Pseudocode | Test Plan |
|-----------|------|-----------|-----------|
| CertProvisioner (`load_or_generate_cert`) | 1 | pseudocode/cert-provisioner.md | test-plan/cert-provisioner.md |
| FingerprintComputer (`fingerprint_leaf_der`) | 1 | pseudocode/fingerprint-computer.md | test-plan/fingerprint-computer.md |
| PublicUrl (`derive_public_url`) | 1 | pseudocode/public-url.md | test-plan/public-url.md |
| BundleCodec (`client_bundle` + JS mirror) | 1 | pseudocode/bundle-codec.md | test-plan/bundle-codec.md |
| SlugRouter + StoreResolver/ProjectKey/ProjectSlug (resolve_store seam) | 1 (seam) / 2 (slug resolver) | pseudocode/slug-router.md | test-plan/slug-router.md |
| DefaultResolver | 1 | pseudocode/default-resolver.md | test-plan/default-resolver.md |
| ProjectRouter (StoreResolver impl) | 2 | pseudocode/project-router.md | test-plan/project-router.md |
| ProjectRegistry / lifecycle CLI | 2 | pseudocode/project-registry.md | test-plan/project-registry.md |
| RemoteClient (`init --remote`) | 1 | pseudocode/remote-client.md | test-plan/remote-client.md |
| Container posture (Dockerfile/compose) | 1 | pseudocode/container-posture.md | test-plan/container-posture.md |
| Cert-rotation runbook (required operator doc — FR-A11) | 1 | (doc deliverable, no pseudocode) | test-plan/cert-rotation-runbook.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |
| C1/C2 cross-stack parity fixtures (Rust oracle → committed corpus) | test fixtures (location per Stage 3a) | #726 server, #725 client, Gate 3c (ADR-002, ADR-006, SR-02) |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| OQ-A — Bundle wire form (C1) | `unimatrix-bundle:<base64url(canonical-json)>` single line, scheme-prefixed, no padding; canonical field order `v,base_url,token,fp`. **Guard ordering (LOCKED):** (1) **4 KB cap enforced on the RAW pasted string's byte length BEFORE base64url-decode and BEFORE JSON-parse** — belt-and-suspenders DoS pre-filter; (2) **strict schema reject is the load-bearing guard** (exactly four keys, `base_url` https, token 64-hex, `fp ^sha256:[0-9a-f]{64}$`). The cap is no longer "confirm" — it is fixed at 4 KB, length-check-first | ARCHITECTURE §3 C1, §4.3, §9; ADR-001 | architecture/ADR-001-bundle-wire-form.md |
| C2 — Cert-fingerprint format | `sha256:<lowercase-hex>` over served **leaf DER** (not PEM, not chain); single Rust oracle + cross-stack parity fixtures, JS golden never hand-written | ARCHITECTURE §3 C2, §9 | architecture/ADR-002-cert-fingerprint-format.md |
| C4 — Route grammar + resolve_store seam | One `StoreResolver` trait, two resolvers (slug \| path-hash); single funnel, transport-derived identity, in-process multi-store (no process-per-project); per-slug hot caches inside the seam method | ARCHITECTURE §3 C4, §9 | architecture/ADR-003-resolve-store-seam.md |
| OQ-B — Slug discovery for attach (C5) | No slug-listing surface in OSS; operator hands slug out-of-band; `init` appends it. Slug allowlist `^[a-z0-9][a-z0-9-]{0,62}$` at the parse edge. Authenticated `--list-slugs` deferred (additive) | ARCHITECTURE §3 C5, §9 | architecture/ADR-004-slug-register-attach.md |
| OQ-C — Wave-1 addressing | `/v1/tools/...` default alias → `ProjectKey::Default`; NOT a mandatory default slug; Wave 2 `/v1/{slug}` is purely additive (no Wave-1 client re-init) | ARCHITECTURE §3 C4, §9 | architecture/ADR-005-wave1-default-alias.md |
| OQ-D — Wave-to-issue mapping | Wave 1 = #726 + #725 (with C1/C2 contract fixtures as a build-first sub-deliverable); Wave 2 = #727. No re-cut of existing issues | ARCHITECTURE §9 | architecture/ADR-006-wave-to-issue-mapping.md |
| C3 — Public-URL knob | `UNIMATRIX_PUBLIC_URL` → single `derive_public_url()` feeding bundle base-url, `allowed_hosts` default, cert SAN; loud `<EDIT-ME>` placeholder when unset; socket auto-detect rejected | ARCHITECTURE §3 C3 | (in ARCHITECTURE §3/§5; ADR-001/002 cross-ref) |
| C6 — Auth/scope/transport separation | Token authorizes (`BearerValidator` seam), slug scopes data (integrity boundary, not security in OSS), cert secures transport — three concerns, never collapsed; enterprise binds slug→JWT claim additively | ARCHITECTURE §3 C6 | (held across ADR-003/004; NFR-09) |
| Container HTTP-enable mechanism | **`UNIMATRIX_HTTP_ENABLED=true` env var** (container-scoped, set in image/`compose.yaml`), NOT a baked config file. Surface-consistent with `UNIMATRIX_PUBLIC_URL` (C3) — one mechanism, greppable/overridable without image rebuild (distroless, no shell). Global binary default `http.enabled=false` stays clean; env carries only the non-sensitive boolean (token/cert stay `0600` files per NFR-05/06) | ARCHITECTURE §9, §10 Q1 | architecture/ADR-007-http-enable-env-var.md |
| A1 — 1-client:1-project boundary | **AFFIRMED (human, 2026-06-11) as a PERMANENT OSS/cloud boundary** — no longer an open product bet. Rationale (documented basis ONLY, NOT elevated to product goals): (a) knowledge-base integrity (grounded in goal #4946); (b) per-project self-learning consistency (each project configures its own clients on how to write/use Unimatrix). Cross-project learning = read-only RBAC = enterprise/paid (additive on the C6 `BearerValidator` seam, never an OSS re-architecture). Integrity/consistency are rationale for this boundary only; the sole stated goal remains #4946 | SPECIFICATION §Affirmed decisions, ALIGNMENT-REPORT | architecture/ADR-004-slug-register-attach.md (cardinality) |

## Files to Create / Modify

**Wave 1 — Server (#726)**
- `crates/unimatrix-server/src/http/tls.rs` — promote test-only `generate_self_signed` to production `load_or_generate_cert(data_dir, sans)` (key `0600`, SANs from C3, defined validity); add `fingerprint_leaf_der(der) -> "sha256:"+hex` (C2).
- `crates/unimatrix-server/src/client_bundle.rs` *(new)* — `run_client_bundle(project_dir) -> Result<(), ServerError>`; sync pre-tokio subcommand (C-10); reads token + leaf DER. **stdout = the opaque `unimatrix-bundle:` blob ONLY** (pipeable, no contamination). **stderr = human-readable echo of `base_url` + `cert-fingerprint` ONLY, with the TOKEN REDACTED/OMITTED** (FR-A5b — hard requirement; the token appears nowhere in stdout, stderr, or any log line per NFR-06). The stderr echo is the onboarding affordance that catches an unset `UNIMATRIX_PUBLIC_URL` placeholder before the operator distributes the bundle.
- `crates/unimatrix-server/src/http/` *(new module or config addition)* — `derive_public_url(env) -> PublicUrl { base_url, host, sans }` (C3 single derivation).
- `crates/unimatrix-server/src/http/router.rs` — new `SlugRouter` layer between `PathRouter` and `McpAdapter`; `StoreResolver` trait, `ProjectKey` enum, `ProjectSlug` newtype, `DefaultResolver`; existing `ProjectRouter` retained for Wave-2 extension.
- `crates/unimatrix-server/src/main.rs` — add `Command::ClientBundle` to the C-10 sync subcommand block (~L247–389); insert `SlugRouter` + cert provisioning into the listener wiring (~L840–900).
- `Dockerfile`, `compose.yaml` — `EXPOSE 8443` / `ports` publish TLS port only; container-scoped HTTP-enable via **`UNIMATRIX_HTTP_ENABLED=true` env var** (ADR-007), set alongside `UNIMATRIX_PUBLIC_URL` in `compose.yaml`; refreshed comments; documented bind-mount UID 65532; nan-014 hardening preserved.
- `crates/unimatrix-server/src/config.rs` (Wave 1) — read `UNIMATRIX_HTTP_ENABLED` as an env override of `HttpConfig.enabled` (parallels the existing `UNIMATRIX_PUBLIC_URL` read in `load_config`); global binary default stays `false` (ADR-007).
- **Cert-rotation runbook (required deliverable, FR-A11)** — a short operator doc shipped with the container covering **rotate cert → re-run `client-bundle` → re-`init` clients**. Paired with a clean/diagnosable stale-fingerprint rejection (the client's `checkServerIdentity` mismatch names expected-vs-presented `sha256:` and points to re-bundle). Not rotation tooling — the existing `client-bundle` + `init` flow IS the rotation flow.

**Wave 1 — Client (#725)**
- `lib/hook-client/` (pure JS) — `init --remote <bundle> [--slug <s>]`: bundle parse with **4 KB raw-string byte-length cap enforced BEFORE base64url-decode and JSON-parse**, then **strict schema reject (load-bearing guard)** of the decoded JSON (C1, FR-B9); cert pin via custom `checkServerIdentity` fingerprint compare (C2) whose **mismatch is a clean, diagnosable error** naming expected-vs-presented `sha256:` and pointing to re-bundle (FR-A11 pairing); slug append, skills copy, size gate. No native binary, zero added runtime deps, copy-install only.
- C1/C2 parity fixtures (shared corpus, generated from the Rust oracle) — consumed by both #726 and #725.

**Wave 2 — Routing (#727)**
- `crates/unimatrix-server/src/http/router.rs` — `ProjectRouter` implements `StoreResolver` (slug → per-slug `McpAdapter`/store; per-slug hot caches); drop-in swap at the `SlugRouter` call site.
- `crates/unimatrix-server/src/projects.rs` *(new)* — `ProjectRegistry` + register/list/delete lifecycle CLI; creates `/data/.unimatrix/{slug}/` (own DB, vector index, hash chain, analytics).
- `crates/unimatrix-server/src/config.rs` — `[[projects]]` slug config.

## Data Structures

```rust
// Transport-derived project identity — never constructed from a request payload (C4 inv.1)
pub enum ProjectKey {
    Default,            // slug-free: local path-hash store OR cloud single-project alias
    Slug(ProjectSlug),  // Wave 2 only; cloud multi-project
}

// Slug allowlist newtype — TryFrom<&str> enforces ^[a-z0-9][a-z0-9-]{0,62}$ (C5/ADR-004)
pub struct ProjectSlug(/* validated lowercase 1–63 chars */);

pub struct DefaultResolver { store: Arc<Store> }                 // Wave 1
pub struct ProjectRouter   { default: Option<Arc<Store>>,        // Wave 2 (same trait)
                             slugs: HashMap<ProjectSlug, ProjectEntry> }

pub struct PublicUrl { base_url: String, host: String, sans: Vec<String> }  // C3

// C1 bundle (canonical JSON, encoded as unimatrix-bundle:<base64url>)
// {"v":1,"base_url":"https://...:8443","token":"<64-hex>","fp":"sha256:<64-hex>"}
```

## Function Signatures (key interfaces — downstream MUST NOT invent these)

```rust
trait StoreResolver: Send + Sync + 'static {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;  // the single funnel
}
fn run_client_bundle(project_dir: Option<PathBuf>) -> Result<(), ServerError>;     // sync, pre-tokio (C-10)
fn derive_public_url(env: &Env) -> PublicUrl;                                      // C3 single derivation
fn fingerprint_leaf_der(der: &[u8]) -> String;                                    // "sha256:" + lowercase_hex (C2)
fn load_or_generate_cert(data_dir: &Path, sans: &[String]) -> Result<(/*cert*/, /*key*/), ServerError>; // SR-01
```

Reused unchanged (architecture §7): `build_tls_acceptor`, `load_or_generate_token`, `StaticTokenAuthLayer`/`BearerValidator`, `start_http_listener`, `ensure_data_directory`, `compute_project_hash`, `open_store_with_retry`, `load_config`, `ProjectRouter`/`McpAdapter`/`PathRouter`.

## Constraints

1. **No new server crates** for cert-gen/fingerprint — only `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`. Client + shipped JS dependency-free.
2. Rust: **no `unsafe`**, **no `.unwrap()`** in non-test code, **max 500 lines/file**; the test-only rcgen helper refactored cleanly on promotion.
3. **No secrets in any DB** — token/cert/key as files on the data volume, key mode `0600`.
4. **Distroless runtime has no shell** — first-boot provisioning runs in the Rust binary, not a shell entrypoint.
5. `data_dir = /data/.unimatrix/{hash}` (not `/data`); cert/token persistence + docs use the resolved path; hash depends on `--project-dir /data`.
6. **TLS internally terminable** (`tls.enabled` / `TlsConfig` seam) — do not hardcode TLS so the seam disappears (enterprise proxy path).
7. `client-bundle` is a pre-tokio sync subcommand (C-10), like `health`/`version`.
8. **Enterprise-extends seams from day one** (ass-060 seven invariants): slug = isolation boundary, `ProjectRouter` resolution seam, per-project DB/hash-chain, `BearerValidator` trait, extensible audit schema. OSS holds these so enterprise is additive. Pattern: documented-but-degenerate seam (ADR-007 vnc-025 `session_key`).
9. **Local-repo install parity is non-negotiable** — `resolve_store` (C4) is ONE mechanism across local (path-hash, no slug) and cloud (slug); no cloud-only isolation path the local install does not also exercise. Verified by a Wave-1 local-install regression test (NFR-10).
10. Test infrastructure is cumulative — extend existing fixtures/helpers; add cross-stack fingerprint parity fixtures from the Rust oracle (never hand-write the JS golden).

## Dependencies

- **Present crates (reuse, no new):** `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`.
- **Existing server surface:** HTTP listener gated on `config.http.enabled` (`main.rs`), bearer auto-gen (`http/token.rs`), TLS-from-PEM acceptor + `TlsConfig` seam (`http/tls.rs`), configurable bind (`HttpConfig`), constant-time bearer auth + `/health` bypass (`http/auth.rs`), `allowed_origins` CSRF layer (vnc-023), pass-through `ProjectRouter`/`McpAdapter` (`http/router.rs`).
- **Existing client surface:** F3 `init --remote` base flow (#679), F4a TS HTTP client (#680).
- **Existing container surface:** nan-014 Dockerfile/compose + GHCR multi-arch publish (#629).
- **Research grounding:** ass-060 (multi-project architecture, seven OSS invariants, slug routing, volume layout), ass-050 (bearer security + enterprise surface), ass-068/ass-069 (unified TS client + client-streamed transcript). Goal #4934 / vision goal #4946 (personal-cloud destination).
- **Pattern reference:** ADR-007 vnc-025 (#4745) — documented-but-degenerate enterprise seam (`session_key`), the precedent for NFR-09 seam treatment.

## NOT in Scope (enterprise / additive on seams above — no OSS re-architecture)

- No proxy-terminated / K8s TLS termination in OSS (additive on `TlsConfig` seam).
- No plaintext-to-client mode; no `tls.enabled=false` in OSS posture.
- No CA-trust / SAN-based hostname validation (fingerprint pinning is the OSS trust model).
- No cross-project knowledge sharing / owner store.
- No OAuth / JWT / RBAC per-slug authz (slug is the seam).
- No multi-tenant (OSS is one tenant, many projects).
- **No one client connecting to / multiplexing multiple projects** — permanent OSS/cloud boundary; a different project = a separate client instance. Same-project multi-connection IS allowed; per-client cross-project fan-out is NOT.
- No macOS / Windows **server** (server is Linux-only; non-Linux = pure-JS clients). No darwin/windows server packages, no cross-compile.
- No rate limiting, secret-rotation tooling, separate-auth-domain `/metrics`, or adversarial testing (#628). **No new `/metrics` endpoint** (deferred to #732).
- No CLAUDE.md knowledge-block append in `init` (`uni-init` owns it; init prints the pointer only).
- No local-UDS behavior change (global `http.enabled` stays `false`; HTTP-enable is container-scoped only).
- No `npm link`-based client install (copy-install only — nan-016 isolation rule).

## Alignment Status

**ALIGNMENT-REPORT.md: PASS 6 · WARN 0 · VARIANCE 0 · FAIL 0.** No variances requiring approval. The design directly advances goal #4946 (personal-cloud), honors all 8 architectural principles, and holds all enterprise capabilities as documented-but-degenerate seams (never pre-built). All 14 SCOPE goals + 6 contracts trace into spec FRs and AC-IDs; all 11 scope risks trace to R-01..R-13.

**All previously-escalated items are now CLOSED — nothing remains pending for human review.** The four open items the prior brief surfaced were resolved by the human on 2026-06-11:

1. **A1 — permanent 1-client:1-project boundary: AFFIRMED.** No longer a load-bearing bet awaiting confirmation. The boundary is permanent for OSS/cloud; its documented rationale is knowledge-base integrity (goal #4946) + per-project self-learning consistency. Cross-project learning is read-only RBAC = enterprise/paid (additive on the C6 `BearerValidator` seam, never an OSS re-architecture). Integrity and consistency are rationale for the boundary ONLY — they are NOT elevated to product goals; the sole stated goal remains #4946.
2. **Container HTTP-enable: RESOLVED → `UNIMATRIX_HTTP_ENABLED=true` env var** (ADR-007), surface-consistent with `UNIMATRIX_PUBLIC_URL` (C3). No longer a deferred delivery detail.
3. **Bundle length cap: FIXED at 4 KB**, enforced as a raw-string byte-length check BEFORE base64url-decode and JSON-parse; the strict schema reject is the load-bearing guard (ADR-001, FR-B9, AC-W1-C9/C10). No longer "confirm."
4. **Two onboarding items ELEVATED to hard requirements:** (a) `client-bundle` stderr echo of base-url + fingerprint with the token REDACTED (FR-A5b / AC-W1-S5b); (b) the cert-rotation runbook as a required deliverable paired with a diagnosable stale-fingerprint rejection (FR-A11 / AC-CT-ROT).
