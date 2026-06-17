# vnc-038 Implementation Brief — Mandatory Project Identity at the Deployment Entrypoint

> Revises the vnc-034 personal-cloud deployment contract. Closes bug #766 by construction via the **dumb-client invariant**: the server is the sole authority on route shape; the client posts to server-composed URLs verbatim. **Folds in #735** (token-to-stdout + Wave-1 cleanups, AC-11/12/13) — closes BOTH #766 AND #735. Compiled from Session 1 design artifacts (revision pass). Implementation-ready brief for Session 2.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-038/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-038/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-038/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-038/specification/SPECIFICATION.md |
| Risk-Test Strategy | product/features/vnc-038/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-038/ALIGNMENT-REPORT.md |

### ADR Files (architect output)

| ADR | Title | File | Unimatrix |
|-----|-------|------|-----------|
| ADR-001 | The Dumb-Client Invariant — server sole authority on route shape | architecture/ADR-001-dumb-client-invariant.md | #5080 |
| ADR-002 | `v:2` Bundle Carries Server-Composed MCP + Observe URLs | architecture/ADR-002-v2-bundle-server-composed-urls.md | #5081 |
| ADR-003 | Per-Slug Observe on the Per-Request Funnel — sole route, no boot fallback | architecture/ADR-003-per-slug-observe-funnel.md | #5082 |
| ADR-004 | Delete the Default — unified resolver (cloud/container HTTP only), single = N=1 | architecture/ADR-004-delete-default-unified-resolver.md | #5083 |
| ADR-005 | Reserved-Slug Re-Derivation under the new route grammar | architecture/ADR-005-reserved-slug-rederivation.md | #5084 |
| ADR-006 | Local STDIO/UDS Keeps Its Direct Path-Hash Binding — NOT a Resolver Key (REVISED/tightened) | architecture/ADR-006-local-uds-identity-under-unified-resolver.md | #5087 |
| ADR-007 | `register <slug>` writes `[[projects]]`; restart applies — atomic, re-attach-safe | architecture/ADR-007-register-writes-routing-intent.md | #5086 |
| ADR-008 | First-Boot Token Delivered Only via the `v:2` Bundle — Never to Stdout/Logs (NEW) | architecture/ADR-008-token-delivery-via-bundle-only.md | #5088 |

## Component Map

Pseudocode and test-plan files are produced in Session 2 Stage 3a. Components below are derived from the architecture; actual file paths are filled during delivery.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| Bundle codec (Rust) — `client_bundle.rs` | pseudocode/bundle-codec-rust.md | test-plan/bundle-codec-rust.md |
| Bundle decoder (JS) — `bundle.js` | pseudocode/bundle-decoder-js.md | test-plan/bundle-decoder-js.md |
| Client attach (JS) — `init.js` | pseudocode/client-attach-js.md | test-plan/client-attach-js.md |
| Hook transport (JS) — `transport-http.js` | pseudocode/hook-transport-js.md | test-plan/hook-transport-js.md |
| Route grammar + resolver — `seam.rs` / `project_resolver.rs` | pseudocode/route-grammar-resolver.md | test-plan/route-grammar-resolver.md |
| Observe route + handler — `router.rs` / `main.rs` | pseudocode/observe-route.md | test-plan/observe-route.md |
| Boot wiring — `main.rs` | pseudocode/boot-wiring.md | test-plan/boot-wiring.md |
| register CLI — `projects.rs` | pseudocode/register-cli.md | test-plan/register-cli.md |
| Reserved slugs — `config.rs` | pseudocode/reserved-slugs.md | test-plan/reserved-slugs.md |
| First-boot token (CI-1) — `token.rs` | pseudocode/token-redaction.md | test-plan/token-redaction.md |
| Local STDIO/UDS direct-binding guard (C-13) — `main.rs:859`/`:1158` | pseudocode/local-binding-guard.md | test-plan/local-binding-guard.md |
| Wave-1 cleanups (CI-2/CI-3) — `router.rs` / `public_url.rs` | pseudocode/wave1-cleanups.md | test-plan/wave1-cleanups.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Make project identity **mandatory and uniform** at the cloud/container deployment entrypoint, eliminating the no-slug / default-project route that lets a bundle silently land on the wrong store. The server becomes the sole authority on route shape: it composes fully-formed MCP and observe endpoint URLs into a versioned (`v:2`) client bundle, and the client posts to those URLs verbatim — closing bug #766 by construction and proving per-slug isolation at N=2. The local UDS/STDIO path-hash install (ADR-004/006) is left **unchanged** — it keeps its direct path-hash store binding and is NOT routed through the resolver. This pass also **folds in #735**: the first-boot bearer token is delivered only via the `v:2` bundle and never printed to stdout/logs (ADR-008), and two Wave-1 cleanups fall out of the router rewrite.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| RD-1 — store migration | Hard cut, **CLOUD/CONTAINER-HTTP only**; no existing served store to migrate; first boot serves nothing + loud "register a project to begin". GATE-2 confirmed local STDIO/UDS unaffected (no migration, no operator action). | SCOPE RD-1; SPEC FR-01/FR-14 | architecture/ADR-004-delete-default-unified-resolver.md (#5083); architecture/ADR-006-local-uds-identity-under-unified-resolver.md (#5087) |
| RD-2 — bundle payload | `v:2` bundle carries fully-formed **server-composed MCP + observe URLs** (not a bare slug); strict dual-side schema, parity corpus updated. | SCOPE RD-2; SPEC FR-06/FR-07 | architecture/ADR-002-v2-bundle-server-composed-urls.md (#5081) |
| RD-3 — observe routing | Observe is a **server-owned per-slug route on the per-request funnel**; client posts the bundle's observe URL verbatim. Closes #766 for init-Ping AND runtime hooks. | SCOPE RD-3; SPEC FR-09/FR-11/FR-12 | architecture/ADR-003-per-slug-observe-funnel.md (#5082) |
| RD-4 — apply mechanism | No live reload. `register <slug>` writes `[[projects]]` routing intent; **restart applies**. Same one command for project 1 and project N. | SCOPE RD-4; SPEC FR-02/FR-03/FR-04 | architecture/ADR-007-register-writes-routing-intent.md (#5086) |
| RD-5 — default deletion | Retire `DefaultResolver` + `/v1/tools→Default` arm + `_ => Default` fallback. Single cloud deployment = **N=1** through the unified resolver (no special-case arm). Local is the exception — direct binding, NOT a resolver key. | SCOPE RD-5; SPEC FR-01/FR-14 | architecture/ADR-004-delete-default-unified-resolver.md (#5083); architecture/ADR-006-local-uds-identity-under-unified-resolver.md (#5087) |
| RD-6 — #767 dependency | #767 (embedding-model RW-mount) is **NOT a dependency**; separate usability prerequisite. Provenance pinning deferred post-v1. | SCOPE RD-6 | — |
| Dumb-client invariant | Server is sole route-shape authority; client composes no paths, derives no identity — reads finished URLs from the validated bundle. The spine. | SCOPE C-01; SPEC NFR-01 | architecture/ADR-001-dumb-client-invariant.md (#5080) |
| Reserved-slug re-derivation | Reserved set re-derived from the new grammar; `tools` kept reserved (conservative — confirm preference, OQ-3). | SCOPE; SPEC FR-13 | architecture/ADR-005-reserved-slug-rederivation.md (#5084) |
| Local binding tightening (C-13) | Local UDS/STDIO keeps **direct path-hash store binding, NOT routed through the unified resolver and NOT a resolver key**. Delivery MUST NOT route local through the resolver (would regress AC-10). ADR-006 corrected #5085→#5087. | SPEC C-13/FR-14 | architecture/ADR-006-local-uds-identity-under-unified-resolver.md (#5087) |
| CI-1 — first-boot token (#735) | The first-boot bearer token (`http/token.rs:101`) is NOT emitted to stdout/logs; `v:2` bundle is the **sole token channel** (cloud surface). Redaction deployment-context-gated so local is unaffected. | SCOPE CI-1; SPEC FR-15/NFR-06 | architecture/ADR-008-token-delivery-via-bundle-only.md (#5088) |

## Files to Create/Modify

### Rust server (`crates/unimatrix-server/src/`)

- `client_bundle.rs` — `v:2` `Bundle { v, mcp_url, observe_url, token, fp }`; `BUNDLE_VERSION = 2`; `encode_bundle` composes both URLs from `{public_base}` + route grammar; strict 5-key `validate_schema`. (ADR-002)
- `http/router/seam.rs` — `parse_project_key`: single `/v1/{slug}/...` → `Slug` rule; remove `tools→Default` and `_ => Default` arms; `ProjectKey::Default` removed for served-project routing. (ADR-004)
- `http/router/project_resolver.rs` — unified resolver; drop `MultiProjectRouter.default` field, default `from_servers` params, `Default` arms in `adapter_for`/`resolve_store`. Slug-keyed only. (ADR-004)
- `http/router.rs` — remove top-level `/observe`; add per-slug `/v1/{slug}/observe` resolved per-request; **extract to ≤500 lines** (CI-2). (ADR-003, AC-12)
- `http/token.rs` — redact/gate the first-boot token print at `:101` so the token never reaches stdout/`tracing`. (ADR-008, CI-1)
- `http/public_url.rs` — remove stale module-level `#![allow(dead_code)]` + "until wiring lands" comment at `:19`. (CI-3, AC-13)
- `projects.rs` — `register` writes `[[projects]]` atomically (temp+fsync+rename), idempotent, re-attach-safe; remove State B/C print instructions (`:302-304`, `:335`). (ADR-007)
- `infra/config.rs` — `RESERVED_SLUGS` re-derivation/documentation under the new grammar (`:2483`). (ADR-005)
- `main.rs` — boot builds unified resolver from `project_slugs` only (empty ⇒ nothing servable, loud msg); delete boot-bound `resolve_store(&ProjectKey::Default)` for observe (`:1045-1052`); observe handler holds `Arc<dyn StoreResolver>` and resolves per-call. **Local STDIO (`:1158`) / UDS (`:859`) boot paths UNTOUCHED.** (ADR-003/004/006)

### JS client (`packages/unimatrix/lib/`)

- `hook-client/bundle.js` — `EXPECTED_KEYS = ["v","mcp_url","observe_url","token","fp"]`; `obj.v !== 2` reject; validate `https://` URLs; preserve guard ordering (length→scheme→base64url→JSON→schema). (ADR-002)
- `init.js` — store `mcp_url` verbatim; DELETE slug-append (`:305`) and default-append (`:307`) composition sites. (ADR-001)
- `hook-client/transport-http.js` — post to `observe_url` verbatim; DELETE `/observe` append (`:84`). (ADR-001)

### Tests

- Server `tests/` + JS test — extend the existing bundle codec **parity corpus** for `v:2` (hex vectors, re-exported `pub(crate)` oracle fns per #4956). (ADR-002, NFR-02/NFR-07)

## Data Structures

```rust
// v:2 bundle (client_bundle.rs) — Rust sole encoder
pub struct Bundle { v: u8, mcp_url: String, observe_url: String, token: String, fp: String }
pub const BUNDLE_VERSION: u8 = 2;
pub const BUNDLE_SCHEME: &str = "unimatrix-bundle:";

// Route key (seam.rs) — Default removed for served-project routing
enum ProjectKey { Slug(ProjectSlug) }   // unified resolver handles ONLY Slug (cloud)

// Resolver (project_resolver.rs) — loses `default` field + Default arms
struct MultiProjectRouter { slugs: HashMap<ProjectSlug, ProjectEntry> }
struct ProjectEntry { store: Arc<Store>, adapter: McpAdapter }

// Reserved slugs (config.rs) — re-derived from new grammar
pub const RESERVED_SLUGS: [&str; 4] = ["v1","health","observe","tools"];
```

```js
// v:2 bundle (bundle.js) — JS decode only
const EXPECTED_KEYS = ["v", "mcp_url", "observe_url", "token", "fp"];
const MAX_RAW_LEN = 4096;   // GUARD 1, runs FIRST on raw paste
// decodeBundle(raw) -> { v: 2, mcp_url, observe_url, token, fp }
class BundleError extends Error {}
```

## Function Signatures

```rust
// Bundle codec — composes BOTH URLs from {public_base} + route grammar
pub fn encode_bundle(v: u8, mcp_url: &str, observe_url: &str, token_hex: &str, fp: &str) -> Result<String, ServerError>
fn validate_schema(...) // exact keys {v, mcp_url, observe_url, token, fp}

// Route grammar — single rule, loud error on no-slug
pub(crate) fn parse_project_key(path: &str) -> Result<ProjectKey, RouteError>  // /v1/{slug}/... -> Slug only

// Resolver — only ProjectKey::Slug; no Default arm
fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>

// register — writes [[projects]] atomically, re-attach-safe (open, never genesis-clobber)
fn register(&self, raw_slug: &str) -> Result<(), ServerError>
```

```js
function decodeBundle(raw)  // -> { v, mcp_url, observe_url, token, fp }; strict v:2; verbatim URLs
```

## Route Grammar (server-owned, centralized)

- MCP: `/v1/{slug}/tools/...` → `ProjectKey::Slug` → per-slug store.
- Observe: `/v1/{slug}/observe` → `ProjectKey::Slug` → same per-slug store, on the per-request funnel.
- Retired: `/v1/tools/... → Default` alias, `_ => Default` fallback, `DefaultResolver`. No grammar arm resolves a servable project without a registered slug.
- **Local UDS/STDIO does NOT participate** — opens its path-hash store directly at boot and threads `Arc<Store>` to handlers; never enters the resolver, never calls `parse_project_key`, never a resolver key.

## Constraints

- **C-01 Dumb-client invariant** — server sole route-shape authority; client composes no paths; centralized server-side grammar, one test surface. The set of client-side path-composition sites after this feature is **empty** (invariant test).
- **C-02 Integrity is the basis** — 1-client:1-project no-pollination protects the unrollbackable hash chain (vnc-034 C5); framed as routing, not authz.
- **C-03 Strict bundle parity (ADR-001/002)** — Rust sole encoder, JS decodes; `v:2` is one atomic dual-side + corpus change; strict exact-key guard means partial rollout breaks decode.
- **C-04 Boot-time `[[projects]]` read** — satisfied by restart-to-apply; no live-reload built.
- **C-05 Distroless, no shell** — first-boot/self-registration provisioning is Rust-binary only.
- **C-06 Rust hygiene** — no `unsafe`, no `.unwrap()` in non-tooling, **≤500 lines/file** (drives `router.rs` extraction, AC-12), `tracing`, project error type with `.map_err` context.
- **C-07 No secrets in any DB** — token/cert stay as files on the data volume.
- **C-08 Reserved-slug set coupled to route grammar** — re-examine what `tools`/`observe` reserve.
- **C-09 Cumulative test infrastructure** — extend the bundle codec parity corpus, seam/funnel tests, project-lifecycle fixtures; no isolated scaffolding.
- **C-10 Hash chain is sacred** — `register` against an existing per-slug store re-attaches (open), never genesis-clobbers. No path-hash migration in scope.
- **C-11 N=2 proof is mandatory** — per-slug isolation (MCP and observe) proven at two registered projects; N=1 green is NOT proof (#4974 ceremonial-funnel precedent).
- **C-12 Scope the cutover to cloud/container-HTTP served-project only** — retiring `DefaultResolver` / `/v1/tools→Default` / `_ => Default` must not break local UDS/STDIO (AC-10) or the per-request MCP seam.
- **C-13 Local keeps direct path-hash binding; NOT a resolver key (ADR-006 tightening)** — local must NOT be routed through the unified resolver and must NOT be self-registered as a resolver key. The unified resolver handles only `ProjectKey::Slug`. Routing local through the resolver regresses AC-10 (the load-bearing R-13 / GATE-2 guard).
- **NFR-06 — Cloud HTTPS posture / no token to stdout** — bearer token MUST NOT be exposed on stdout or in logs; `http/token.rs:101` redacted/gated; `v:2` bundle is the sole token-delivery channel (HTTP/cloud surface only; local unaffected).

## Dependencies

- **Crates/components touched** — `unimatrix-server`: `client_bundle.rs`, `http/router.rs`, `http/router/seam.rs`, `http/router/project_resolver.rs`, `http/token.rs`, `http/public_url.rs`, `projects.rs`, `infra/config.rs`, `main.rs`. JS: `packages/unimatrix/lib/init.js`, `lib/hook-client/bundle.js`, `lib/hook-client/transport-http.js`.
- **vnc-034** — revises its deployment contract (bundle ADR-001/#4954, slug register/attach #4951, single-funnel seam #4963, default alias #4949, ceremonial-funnel lesson #4974). vnc-038 deprecates/corrects those via `context_correct`.
- **#766** — triggering bug; its repro is AC-07's concrete test.
- **#735 — folded in** (closes #735). No separate sequencing gate; carry-items CI-1/CI-2/CI-3 land on surfaces vnc-038 already reworks → AC-11/12/13.
- **Parity-corpus infra (#4956)** — reuse hex vectors, `pub use` re-export of oracle fns for `tests/`.
- **No external crates/services added.** Zero-dependency JS decoder preserved (NFR-08).

## NOT in Scope

- #767 (embedding-model RW-mount) — separate usability prerequisite, NOT a dependency.
- Embedding-model provenance / hash pinning — deferred post-v1.
- #768 (stale client-setup/README remote docs) — **pre-committed fast-follow** (GATE-3); not part of this diff.
- #769 (healthcheck ERROR noise) — separate.
- RBAC / cross-project access control / per-slug authz — enterprise-only.
- Cross-project knowledge sharing / owner store — unchanged from vnc-034; enterprise.
- Monetization / tenancy model — OSS stays single-tenant, N projects.
- Backward-compatible no-slug fallback — explicitly NOT kept (hard cutover).
- New unauthenticated endpoints / slug-listing surface — not added.
- **Local-UDS/STDIO deployment-model change** — local keeps its direct path-hash binding (NOT routed through the resolver, NOT a resolver key); NOT converted to multi-project cloud, NOT forced to a manual slug, no migration, no operator action.
- Store migration / path-hash adoption logic — no existing served store to migrate (hard cut).
- Live reload / dynamic registry / admin registration endpoint — restart-to-apply only.

## Delivery-Time Gates

| Gate | Concern | Status |
|------|---------|--------|
| **GATE-1** | #735 sequencing collision (same router/boot surface) | **RESOLVED** — #735 folded into vnc-038; no longer a separate gate. The three carry-items land inside this diff (AC-11/12/13). |
| **GATE-2** | RD-1 data-loss on the hard cut | **RESOLVED for local** — code-cited impact analysis confirmed local STDIO (`main.rs:1158`) / UDS (`main.rs:859`) open the path-hash store directly and never touch `parse_project_key`, the resolver, `ProjectKey::Default`, or a bundle. **Residual = a narrow human check: confirm zero existing CLOUD/CONTAINER-HTTP served deployments before the AC-09 hard cut.** R-13 is the load-bearing delivery guard. |
| **GATE-3** | #768 docs fast-follow | **UNCHANGED** — hard cutover staleness guarantees docs go stale; #768 committed to ship right after. Non-goal for this diff. |
| **GATE-4** | N=2 isolation proof (C-11) | **UNCHANGED, NON-NEGOTIABLE** — per-slug isolation (MCP and observe) proven at two registered projects; an N=1 green result is NOT accepted as proof (#4974). |

## Alignment Status

ALIGNMENT-REPORT.md (revision pass, 2026-06-17): **8 PASS, 1 WARN, 0 VARIANCE, 0 FAIL.** Vision Alignment, Milestone Fit, Scope Gaps, Scope Additions, #735 Fold-In, Token-Delivery Posture (ADR-008), Architecture Consistency, and Risk Completeness all PASS. The dumb-client invariant (principle #6) and no-secrets-in-DB (principle #8) are honored and reinforced.

**One WARN (advisory, human-owned, NOT a blocker for this feature):** The ADR-006 tightening (local bypasses the resolver; NOT a resolver key) diverges from one *literal phrase* of goal #4946 ("single funnel … identical in local-UDS and cloud"). The divergence is from the goal's **phrasing, not its intent** — the load-bearing invariant ("identity comes from the transport, the resolved store handle is the sole write capability") is preserved on both surfaces. GATE-2 shows the shipped code already binds local directly; routing it through the resolver would *create* the untested local-only path the clause guards against. Recommendation: **accept**, with a non-blocking, human-owned follow-up to refresh goal #4946's wording. Out of scope for vnc-038 to make that goal edit.
