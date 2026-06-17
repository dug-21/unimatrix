# vnc-038 — Specification: Mandatory Project Identity at the Deployment Entrypoint

> Revises the vnc-034 personal-cloud deployment contract. Source: `product/features/vnc-038/SCOPE.md` (Goals 1-6 + folded-in #735 carry-items, RD-1..6, AC-01..AC-13) and `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-07). Design artifact only — no code changes in this phase. Closes #766 AND #735 (#735 folded in: token-to-stdout + Wave-1 cleanups, AC-11/12/13).

## Objective

Make project identity mandatory and uniform at the cloud/container deployment entrypoint, eliminating the no-slug / default-project route that lets a bundle silently land on the wrong store. The server becomes the sole authority on route shape: it composes fully-formed MCP and observe endpoint URLs into a versioned (`v:2`) client bundle, and the client posts to those URLs verbatim, composing no paths. This closes bug #766 (init-time and runtime observe 404s) by construction and proves per-slug isolation at N=2, while leaving the local UDS/STDIO path-hash install (ADR-004) unchanged. It also folds in the #735 carry-items that land on the surfaces this feature already reworks: the first-boot token is delivered only via the `v:2` bundle and never printed to stdout/logs (cloud HTTPS posture, NFR-06/AC-11), and two Wave-1 cleanups fall out of the router rewrite (`router.rs` under the 500-line guideline, AC-12; stale `dead_code` allow removed from `public_url.rs`, AC-13).

## Ubiquitous Language

These terms carry exact meaning throughout the pipeline. Downstream agents must use them as defined.

- **Served-project model (cloud/container):** the deployment shape this feature changes. A container serves one or more registered projects over HTTPS. This is where the hard cutover applies.
- **Local UDS/STDIO install:** the single-project install addressed over a Unix domain socket (or STDIO), whose identity is the ADR-004 path-hash. It opens `~/.unimatrix/{hash}/unimatrix.db` directly at boot and threads the `Arc<Store>` straight to its handlers (STDIO `main.rs:1158`, UDS `main.rs:859`). It keeps its **direct path-hash store binding, untouched by the HTTP resolver** — it is NOT routed through the unified resolver, never calls `parse_project_key`, never references `ProjectKey::Default`, and never uses a bundle. NOT a "no-slug default"; explicitly out of scope for the cutover (AC-10). Per the ADR-006 tightening, local is NOT self-registered as a resolver key.
- **Slug:** operator-declared project identity, `^[a-z0-9][a-z0-9-]{0,62}$` (ADR-004 `ProjectSlug` newtype), decoupled from any path-hash. Validated at the parse edge before any filesystem use.
- **Reserved slug:** a name that names a route segment, not a project, and therefore must be rejected at registration. Today `["v1","health","observe","tools"]` (`config.rs:2483`); re-derived by this feature against the new route grammar (SR-05).
- **register `<slug>`:** the single server-side command that (a) creates the per-slug data dir + genesis store and (b) writes `[[projects]]` routing intent. Identical operation for project 1 and project N.
- **Routing intent (`[[projects]]`):** the TOML array in `config.toml` that is the source of truth for which slugs are routable. Read once at boot (`main.rs:1004`). Restart applies newly written intent (restart-to-apply).
- **Restart-to-apply:** the accepted mechanism by which newly registered routing intent becomes live. NOT the thing being eliminated. The eliminated thing is the hand-edit-`config.toml` asymmetry.
- **Client bundle (`v:2`):** `unimatrix-bundle:<base64url(canonical-json)>` carrying the server-composed MCP endpoint URL AND observe endpoint URL (not a bare slug), plus `token` and `fp`. Versioned, strict-schema validated, parity-corpus pinned.
- **Server-composed URL:** a fully-formed endpoint URL the server authored end-to-end (scheme, host, `/v1/<slug>/...` path). The client treats it as opaque — it appends, substitutes, and derives nothing.
- **Dumb-client invariant:** the cross-cutting spine. The server is the sole authority on route shape; the client composes NO paths and derives NO identity. It reads finished URLs from the validated bundle and posts to them byte-for-byte.
- **Per-request funnel (seam):** the single point (`http/router/seam.rs`, `SlugRouter`) where `resolve_store(parse_project_key(path))` runs once per request. The sole Wave1↔Wave2 swap point. This feature extends it to cover observe.
- **Resolver:** the `StoreResolver` trait object that maps a `ProjectKey` to a store handle. This feature retires `DefaultResolver` for the served-project model; a single deployment is N=1 through the unified resolver. The unified resolver handles ONLY `ProjectKey::Slug` (cloud/container HTTP). Local UDS/STDIO bypasses it entirely (direct path-hash binding) — it is not a resolver key (ADR-006 tightening).
- **Cross-pollination:** a request bound to project B reaching project A's store. The integrity hazard (vnc-034 C5) this feature makes structurally impossible. Catastrophic and unrollbackable — corrupts A's hash chain.
- **N=2 proof:** isolation demonstrated with two registered projects, not one. The #4974 ceremonial-funnel precedent makes N=1 green insufficient as proof.

## Domain Models

### Project
- A registered served identity. Attributes: `slug` (validated `ProjectSlug`), data dir `/data/.unimatrix/{slug}/` (own DB, vector, hash chain, analytics), routing-intent presence in `[[projects]]`.
- Lifecycle states (extending `projects.rs` precedent): **Unregistered** (no dir, no routing intent) → **Registered** (`register` ran: dir + genesis store + `[[projects]]` written) → **Routable** (restart applied; resolver maps its slug to its store).
- Invariant: `register` against an existing per-slug store re-attaches (opens), never genesis-clobbers (`projects.rs` State B precedent; SR-07).

### Client Bundle (`v:2`)
- Canonical JSON, fixed field order, strict exact-key schema. Carries: `v:2`, the **server-composed MCP endpoint URL**, the **server-composed observe endpoint URL**, `token`, `fp`. (Exact key names are an architect decision; the constraint is finished URLs, not a bare slug.)
- Rust is the sole encoder; JS decodes (ADR-001). Both sides validate strictly; the parity corpus is the shared oracle.
- A decoded `v:2` bundle yields URLs the client posts to verbatim; the client appends no slug and composes no path after validation.

### Route Grammar (server-owned, centralized)
- MCP: `/v1/{slug}/tools/...` → `ProjectKey::Slug` → per-slug store.
- Observe: `/v1/{slug}/observe` → `ProjectKey::Slug` → same per-slug store, on the per-request funnel.
- Retired: `/v1/tools/... → Default` alias, the `_ => Default` fallback arm, `DefaultResolver` (served-project model). No grammar arm resolves a servable project without a registered slug. The unified resolver handles only `ProjectKey::Slug`.
- Local UDS/STDIO does NOT participate in this route grammar at all. It opens its single path-hash store directly at boot (STDIO `main.rs:1158`, UDS `main.rs:859`) and threads the `Arc<Store>` straight to its handlers — it is NOT routed through the unified resolver and is NOT self-registered as a resolver key (ADR-006 tightening). AC-10 is the guardrail.

### Reserved Slug Set
- Derived from the route grammar above. Any segment that names a route (`v1`, `health`, `observe`, and any others the new grammar introduces) is rejected at registration. `tools` reservation is re-examined now that `/v1/tools/...` is no longer the default alias (SR-05).

## Functional Requirements

Each is testable; verification methods are listed under Acceptance Criteria.

- **FR-01 — Loud first boot, no silent default.** A fresh served-project deployment with no registered project serves NO request under any default/no-slug identity. Any request (MCP or observe) to a no-slug target fails loud with an actionable message containing the substance "register a project to begin." No request resolves to a default store. (Goal 1, RD-1, AC-01/AC-09)
- **FR-02 — Uniform one-command registration.** `register <slug>` is a single operation that creates the per-slug data dir + genesis store AND writes `[[projects]]` routing intent, with zero hand-edit of `config.toml`. (Goal 2, RD-4, AC-02)
- **FR-03 — `register` writes routing intent, not instructions.** `register <slug>` no longer prints config.toml instructions (`projects.rs:334-335`); it writes the `[[projects]]` entry. After a restart the slug is routable. The replaced sequence `register → hand-edit → restart` becomes `register → restart`. (Goal 2, RD-4, AC-03)
- **FR-04 — Nth project identical to first.** Onboarding a 2nd/Nth project is the IDENTICAL single `register <slug>` command as the first — same operation, no config-file edit, restart-to-apply. (Goal 4, AC-04)
- **FR-05 — Atomic, idempotent registration.** The `[[projects]]` write is atomic (no partial/malformed config on interruption); `register` against an existing slug re-attaches (opens the store), never genesis-clobbers. Provisioning runs in the Rust binary (distroless has no shell). (SR-07, Constraints)
- **FR-06 — `v:2` bundle carries server-composed URLs.** The server emits a `v:2` bundle carrying the fully-formed MCP and observe endpoint URLs it composed (not a bare slug), plus token and fingerprint. (Goal 3, RD-2, AC-05)
- **FR-07 — Strict dual-side `v:2` validation, parity-pinned.** Both the Rust encoder and the JS decoder validate the `v:2` schema strictly (exact keys, well-formed URLs, version reject of unknown major). The existing bundle codec parity corpus is updated to cover `v:2`. Partial (single-side) rollout fails decode by design. (Goal 3, RD-2, SR-02, AC-05)
- **FR-08 — Client posts URLs verbatim; composes nothing.** After bundle validation the client posts to the bundle's MCP and observe URLs byte-for-byte. It appends no slug, substitutes no host/path, and derives no identity. Every former client-side path-composition site (`init.js:303-308`, `transport-http.js:84`) is eliminated as a closed set. (Dumb-client invariant, SR-01, AC-05)
- **FR-09 — Observe is a server-owned per-slug route on the per-request funnel.** Observe resolves its store per request through the same funnel as MCP (`resolve_store(parse_project_key(path))`), not via a boot-bound single handle. The resolved per-request handle is the SOLE observe route — no boot-bound fallback, no parallel adapter path (#4974 precedent). (Goal 5, RD-3, SR-03, AC-06)
- **FR-10 — Single-store resolution for every request.** Given any registered slug, an MCP request and an observe request each resolve to exactly that slug's store. No code path lets a request bound to project B reach project A's store. (Goal 5, AC-06)
- **FR-11 — Init-time observe over the real per-slug route.** `init --bundle <bundle>` performs its validation Ping to the observe URL from the bundle, over the real per-slug route, and receives 200. (Goal 6, AC-07)
- **FR-12 — Runtime hook telemetry over the per-slug route.** Every runtime hook event posts to the per-slug observe route from the bundle and is accepted (200). (Goal 6, AC-08)
- **FR-13 — Reserved-slug re-derivation.** The reserved-slug set is re-derived from the new route grammar. Registration is rejected against every reserved name so no registerable slug can shadow a route segment. (SR-05, Constraints)
- **FR-14 — Local UDS/STDIO unchanged, direct path-hash binding.** The local single-project UDS/STDIO install keeps its ADR-004 path-hash as its identity. It opens its store directly at boot (STDIO `main.rs:1158`, UDS `main.rs:859`) and threads the `Arc<Store>` straight to its handlers — it is NOT routed through the unified resolver, never calls `parse_project_key`, never references `ProjectKey::Default`, and uses no bundle. It is NOT required to supply a manual slug. Delivery must NOT route local through the unified resolver (doing so would regress AC-10 by re-introducing a cross-store path). The cutover does not apply to it; local users require NO migration and NO operator action. (Goal 1 carve-out, RD-1, RD-5, ADR-006 tightening, AC-10)
- **FR-15 — First-boot token not emitted to stdout/logs (#735 CI-1).** On the HTTP/cloud first-boot surface, the bearer token is NOT printed to stdout or written to logs; the `http/token.rs:101` print is redacted/gated. The token reaches the remote client solely via the validated `v:2` client bundle. (Scoped to the HTTP/cloud first-boot surface; local STDIO/UDS has no bundle and is unaffected.) (NFR-06, AC-11)
- **FR-16 — `router.rs` under the 500-line guideline (#735 CI-2).** After the route-grammar rewrite (default-alias removal, per-slug observe, centralized grammar), `crates/unimatrix-server/src/http/router.rs` is at or under the 500-line guideline; the extraction falls out of the rewrite, not a separate effort. (NFR-09, AC-12)
- **FR-17 — Stale `dead_code` allow removed (#735 CI-3).** The stale module-level `#![allow(dead_code)]` and the "until wiring lands" comment at `crates/unimatrix-server/src/http/public_url.rs:19` are removed. (AC-13)

## Non-Functional Requirements

- **NFR-01 — Dumb-client invariant (cross-cutting).** Route grammar is centralized server-side in one place with one test surface. The client cannot emit a path the server did not author. Measurable: the set of client-side path-composition sites after this feature is empty, asserted by an invariant test (FR-08).
- **NFR-02 — Strict bundle parity (ADR-001).** Rust is the sole encoder; JS decodes. `v:2` is one atomic dual-side + corpus change. The existing parity corpus is reused (hex-encoded vectors, re-exported `pub(crate)` oracle fns per #4956), not re-scaffolded. (SR-02)
- **NFR-03 — Integrity, not access control.** The no-pollination guarantee is framed as a routing property protecting the hash chain (vnc-034 C5), not an authz feature. No per-slug authz, no new authenticated/unauthenticated surface is added.
- **NFR-04 — No new endpoints, no secrets in any DB.** No new `/metrics`-style open surface; any slug surface stays behind existing bearer auth. Token and cert remain files on the data volume.
- **NFR-05 — Restart-to-apply only; no live reload.** No dynamic registry, admin endpoint, or in-process reload is built. `[[projects]]` is read once at boot; restart applies. Acceptable for this single-dev, not-always-on deployment.
- **NFR-06 — Cloud HTTPS posture / no token to stdout (#735 CI-1).** Under the cloud HTTPS posture, the bearer token MUST NOT be exposed on stdout or in logs. The first-boot token print (`http/token.rs:101`) is redacted/gated, and the `v:2` client bundle is the SOLE token-delivery channel to the remote client. Scoped to the HTTP/cloud first-boot surface; local STDIO/UDS has no bundle and is unaffected. The architect may record this as ADR-008; reference it if added. (FR-15, AC-11)
- **NFR-09 — Rust constraints.** No `unsafe`; no `.unwrap()` in non-tooling code; max 500 lines/file (drives `router.rs` extraction, FR-16/AC-12); `tracing` logging; project error type with `.map_err` context.
- **NFR-07 — Cumulative test infrastructure.** Verification extends the existing bundle codec parity corpus, the seam/funnel tests, and the project-lifecycle fixtures. No isolated scaffolding.
- **NFR-08 — Bundle size budget.** The JS decoder stays zero-dependency under its existing size budget; the length cap guard ordering (raw-length cap before decode/parse) from ADR-001 is preserved for `v:2`.

## Acceptance Criteria

AC-IDs are carried verbatim from SCOPE.md. Each lists a verification method pinned to an existing test surface.

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| **AC-01** | No request resolves under a default/no-slug identity; the `/v1/tools/...` alias and no-slug `/v1` target no longer resolve a servable project; attempt fails loud. | Seam/funnel test (`http/router/seam.rs` surface): assert `parse_project_key` no longer maps `/v1/tools/...` or the `_` arm to a servable `Default`; assert the unified resolver returns an error (no servable store) for a no-slug path. |
| **AC-02** | First project comes up via the single `register <slug>` — creates per-slug data dir AND writes `[[projects]]`, no hand-edit; restart applies. | Project-lifecycle fixture: run `register <slug>` from clean state; assert data dir + genesis store created AND `[[projects]]` entry written; assert no instruction string printed; assert routable after a boot-time `[[projects]]` re-read. |
| **AC-03** | `register <slug>` writes routing intent rather than printing instructions; routable after restart. | Same fixture as AC-02: assert the `projects.rs:334-335` instruction print is gone and replaced by an atomic `[[projects]]` write; assert boot reads it and the slug resolves. |
| **AC-04** | Onboarding the Nth project is the IDENTICAL `register <slug>` command, no config edit, restart-to-apply. | Project-lifecycle fixture at N=2: register a second slug via the same command path; assert second `[[projects]]` entry written and second store created with no manual edit; both slugs routable after restart. |
| **AC-05** | Bundle carries server-composed MCP and observe URLs; decoded bundle yields finished URLs posted verbatim; client appends no slug / composes no path; `v:2` strict-schema rejects on both Rust and JS sides; parity corpus updated. | Bundle codec parity corpus (extended for `v:2`): round-trip Rust-encode → JS-decode equality; strict-reject cases (missing/extra/wrong-type key, malformed URL, unknown major version) on both sides; invariant test asserting the client posts the bundle's URLs byte-for-byte (no client-side composition). |
| **AC-06** | With two registered projects, an MCP request and an observe request each resolve to exactly one project's store; no path lets B reach A. **N=2 proof required.** | Seam/funnel test at N=2 (not N=1, per #4974): a counting/recording resolver asserts each MCP and each observe request consults the resolver once with the transport-derived `ProjectKey` and resolves the matching store; assert no boot-bound or parallel observe path exists (observe goes through the funnel only). |
| **AC-07** | End-to-end: bundle → `init --bundle` → init-time Ping/observe returns 200 over the real per-slug route (the #766 repro passes). | The #766 repro is the concrete test: drive `init --bundle <v:2 bundle>`; assert the init-time Ping posts to the bundle's observe URL and the real per-slug `/v1/{slug}/observe` route returns 200 (was 404). |
| **AC-08** | Runtime hook telemetry posts to a reachable per-slug observe route and is accepted (the #766 wider blast radius). | Hook-client transport test: a runtime hook event posts to the bundle's observe URL; assert the per-slug observe route accepts it (200) and resolves to the bundle's project store. |
| **AC-09** | Hard cut (CLOUD/CONTAINER-HTTP ONLY): a no-slug cloud/container HTTP deployment is no longer valid; first boot serves nothing until a project is registered, failing loud with an actionable "register a project to begin" message; no silent default; no path-hash adoption logic. This statement applies ONLY to cloud/container HTTP — of which there are currently zero deployments; local STDIO/UDS users are explicitly unaffected (see AC-10). | First-boot test on the served-project model: from empty state assert no servable store exists, every request fails loud with the actionable message, and no adopt/derive code path runs. Assert no local-transport code path (STDIO `main.rs:1158` / UDS `main.rs:859`) is touched by the cutover. |
| **AC-10** | Local single-project UDS/STDIO / path-hash install (ADR-004) continues to function unchanged — keeps its **direct path-hash store binding, untouched by the HTTP resolver**, not required to take a manual slug; cutover does not apply. **GATE-2 confirmed:** local STDIO (`main.rs:1158`) and UDS (`main.rs:859`) open `~/.unimatrix/{hash}/unimatrix.db` directly at boot and never touch `parse_project_key`, the HTTP resolver, `ProjectKey::Default`, or any bundle — existing local stores keep working with NO migration and NO operator action; delivery must NOT route local through the unified resolver. | Local-UDS/STDIO fixture (existing): assert the path-hash store still resolves directly over UDS/STDIO with no slug supplied and NOT through the unified resolver, behavior unchanged from ADR-004; assert delivery did not add a resolver-key path for local (would regress this AC). |
| **AC-11** | First-boot bearer token is NOT emitted to stdout or logs (`http/token.rs:101` redacted/gated); the remote client obtains the token via the validated `v:2` client bundle — cloud HTTPS posture (#735 CI-1, NFR-06). | First-boot token-surface test on the HTTP/cloud first-boot path: assert stdout and captured logs contain no token substring on first boot; assert the emitted `v:2` bundle carries the token. (Local STDIO/UDS has no bundle and is out of this AC's surface.) |
| **AC-12** | After the route-grammar rewrite, `crates/unimatrix-server/src/http/router.rs` is at or under the 500-line guideline (#735 CI-2, NFR-09). | Line-count / structure check on `router.rs` post-rewrite: assert file length ≤ 500 lines. |
| **AC-13** | The stale module-level `#![allow(dead_code)]` and "until wiring lands" comment at `crates/unimatrix-server/src/http/public_url.rs:19` are removed (#735 CI-3). | Absence check (grep) on `public_url.rs`: assert no `#![allow(dead_code)]` and no "until wiring lands" comment remain. |

## User Workflows

### W1 — First project on a fresh deployment (operator)
1. Operator starts the container; no project registered.
2. Any client attach or request fails loud: "register a project to begin." (AC-01/AC-09)
3. Operator runs `register <slug>` once → per-slug store created + `[[projects]]` written. (AC-02/AC-03)
4. Operator restarts the daemon; the slug is now routable. (Restart-to-apply, NFR-05)
5. Operator runs `client-bundle` → receives a `v:2` bundle with server-composed MCP + observe URLs.

### W2 — Adding the Nth project (operator)
1. Operator runs the SAME `register <slug2>` command — no config edit. (AC-04)
2. Operator restarts; both slugs routable.
3. Operator generates a separate `v:2` bundle per project.

### W3 — Client attach and run (agent/client)
1. Client receives a `v:2` bundle out-of-band; runs `init --bundle <bundle>`.
2. Client decodes and strictly validates the bundle (length cap → strict schema). (AC-05)
3. Init-time validation Ping posts to the bundle's observe URL verbatim → 200. (AC-07)
4. Runtime hook events post to the bundle's observe URL verbatim → 200. (AC-08)
5. MCP requests post to the bundle's MCP URL verbatim; every request resolves to exactly the bundle's project store. (AC-06)

### W4 — Local UDS/STDIO install (unchanged)
1. Local install opens its single path-hash store directly at boot (STDIO `main.rs:1158`, UDS `main.rs:859`) and threads the `Arc<Store>` to its handlers; no slug supplied, no resolver, no bundle. (AC-10)
2. Behavior is identical to ADR-004; the cutover does not touch this path. No migration, no operator action. (RD-1, RD-5, GATE-2)

## Constraints

Carried from SCOPE.md and the risk assessment; first-class requirements.

- **C-01 — Dumb-client invariant.** Server is sole route-shape authority; client composes no paths, derives no identity. Centralized server-side route grammar, one test surface. (SR-01)
- **C-02 — Integrity is the basis.** 1-client:1-project no-pollination protects the unrollbackable hash chain (vnc-034 C5); framed as routing, not authz. (NFR-03)
- **C-03 — Strict bundle parity (ADR-001).** Rust sole encoder, JS decodes; `v:2` is atomic dual-side + corpus; strict exact-key guard means partial rollout breaks decode. (SR-02)
- **C-04 — Boot-time `[[projects]]` read.** Satisfied by restart-to-apply; no live-reload built. (NFR-05, SR-07)
- **C-05 — Distroless, no shell.** First-boot/self-registration provisioning is Rust-binary only. (FR-05)
- **C-06 — Rust hygiene.** No `unsafe`, no `.unwrap()` in non-tooling, ≤500 lines/file, `tracing`, project error type with context. (NFR-09)
- **C-07 — No secrets in any DB.** Token/cert stay as files on the data volume. (NFR-04)
- **C-08 — Reserved-slug set coupled to route grammar.** Changing the default alias forces re-examination of what `tools`/`observe` reserve. (FR-13, SR-05)
- **C-09 — Cumulative test infrastructure.** Extend the bundle codec parity corpus, seam/funnel tests, project-lifecycle fixtures; no isolated scaffolding. (NFR-07)
- **C-10 — Hash chain is sacred.** `register` against an existing per-slug store re-attaches (open), never genesis-clobbers. No path-hash migration in scope. (FR-05, SR-07)
- **C-11 — N=2 proof is mandatory.** Per-slug isolation (MCP and observe) is proven at two registered projects, not one; an N=1 green result is not accepted as proof (#4974 ceremonial-funnel precedent). (SR-03, AC-06)
- **C-12 — Scope the cutover to cloud/container-HTTP served-project only.** Retiring `DefaultResolver` / `/v1/tools→Default` / `_ => Default` must not break local UDS/STDIO (AC-10) or the per-request MCP seam. GATE-2 confirmed the deletions touch only the HTTP path; local opens its store directly and is unaffected. (SR-04, RD-1)
- **C-13 — Local keeps its direct path-hash binding; NOT a resolver key (ADR-006 tightening).** Local UDS/STDIO must NOT be routed through the unified resolver and must NOT be self-registered as a resolver key. The unified resolver handles only `ProjectKey::Slug` (cloud). Routing local through the resolver would regress AC-10. (RD-5, AC-10)

## Dependencies

- **vnc-034** — this feature revises its deployment contract. Direct inputs: bundle wire form (ADR-001 / #4954), slug identity + register/attach (ADR-004 / #4951), the single-funnel seam (#4963), the default alias being retired (ADR-005 / #4949), and the ceremonial-funnel lesson (#4974).
- **#766** — triggering bug; its repro is AC-07's concrete test.
- **#735 — folded into vnc-038** (no separate sequencing gate; the prior GATE-1 collision is resolved by folding in). Its three carry-items land on surfaces this feature already reworks (first boot, `router.rs`): CI-1 token-to-stdout (AC-11/NFR-06), CI-2 `router.rs` extraction (AC-12/NFR-09), CI-3 `public_url.rs` cleanup (AC-13). vnc-038 closes #735.
- **Existing test surfaces** — bundle codec parity corpus (#4956: hex vectors, re-exported `pub(crate)` oracle fns), `http/router/seam.rs` seam/funnel tests, `projects.rs` project-lifecycle fixtures, hook-client transport tests, first-boot/token-surface tests.
- **Crates/components touched (for architect)** — `crates/unimatrix-server/src/http/router.rs`, `http/router/seam.rs`, `http/token.rs` (#735 CI-1), `http/public_url.rs` (#735 CI-3), `projects.rs`, `config.rs` (reserved slugs), `client_bundle.rs`, `main.rs` boot path (STDIO `:1158`, UDS `:859` — both untouched by the cutover); JS `packages/unimatrix/lib/init.js`, `lib/hook-client/transport-http.js`.

## NOT in Scope

Explicit exclusions to prevent scope creep (variances will be flagged by the vision guardian).

- **#767 (embedding-model RW-mount on first boot)** — separate usability prerequisite, NOT a dependency (RD-6); not absorbed into this diff.
- **Embedding-model provenance / hash pinning** — deferred post-v1.
- **#768 (stale client-setup/README remote docs)** — pre-committed fast-follow; not part of this diff.
- **#769 (healthcheck ERROR noise)** — separate.
- **RBAC / cross-project access control / per-slug authz** — enterprise-only, additive on the C6 seam.
- **Cross-project knowledge sharing / owner store** — unchanged from vnc-034; enterprise.
- **Monetization / tenancy model** — OSS stays single-tenant, N projects.
- **Backward-compatible no-slug fallback** — explicitly NOT kept; this is a hard cutover (RD-1/RD-5).
- **New unauthenticated endpoints / slug-listing surface** — not added.
- **Local-UDS/STDIO deployment-model change** — local keeps its direct path-hash store binding (NOT routed through the unified resolver, NOT a resolver key per ADR-006); NOT converted to multi-project cloud, NOT forced to a manual slug, no migration, no operator action.
- **Store migration / path-hash adoption logic** — no existing served store to migrate (hard cut, no existing users).
- **Live reload / dynamic registry / admin registration endpoint** — restart-to-apply only.

## Open Questions (for architect / human)

- **OQ-1 (architect)** — Exact `v:2` bundle key names/shape for the MCP and observe URLs (two distinct URL fields vs one base + server-composed sub-paths). SCOPE requires finished URLs the client posts verbatim; the field layout is an architecture decision.
- **OQ-2 (architect) — RESOLVED by the ADR-006 tightening.** Local UDS/STDIO is NOT routed through the unified resolver and is NOT a resolver key — it keeps its direct path-hash store binding, opening its store at boot (STDIO `main.rs:1158`, UDS `main.rs:859`) and threading the `Arc<Store>` to its handlers. The unified resolver handles only `ProjectKey::Slug` (cloud). The SR-04 tension with RD-5 is resolved by separation, not a special-case resolver arm: local bypasses the resolver rather than registering inside it. (See C-13, AC-10.)
- **OQ-3 (architect)** — Final reserved-slug set under the new grammar: does `tools` remain reserved once `/v1/tools/... → Default` is gone, and what new segments (e.g. per-slug `observe`) must be reserved? (SR-05)
- **OQ-4 (human / leader) — RESOLVED by folding #735 in.** #735 is no longer a separate effort on the same surface; its three carry-items (CI-1/CI-2/CI-3) are folded into vnc-038 as AC-11/12/13. There is no separate sequencing gate or same-surface adjacency risk; vnc-038 closes #735.
- **OQ-6 (architect)** — Whether the first-boot token-redaction posture (NFR-06/AC-11) warrants a dedicated ADR-008. The constraint is fixed (token never to stdout/logs; `v:2` bundle is the sole token channel; HTTP/cloud surface only); recording it as an ADR is the architect's call.
- **OQ-5 (human) — Local data-loss concern RESOLVED by GATE-2; residual is cloud-only.** GATE-2's code-cited impact analysis (2026-06-17) confirmed local STDIO/UDS opens its path-hash store directly and never touches the deleted HTTP paths, so local users lose no data and need no migration (RD-1, AC-10). The only residual is the RD-1 assumption that zero existing *cloud/container HTTP* deployments hold a default served store; if any do, the hard cut (AC-09) loses data and a one-time migration is needed despite "no users."

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-034 ADRs #4954 (bundle wire form), #4951 (slug register/attach), #4949 (default alias being retired), #4963 (single-funnel seam) and the ceremonial-funnel lesson #4974; all applied to ubiquitous language, domain models, and the N=2 proof constraint.
- Queried (revision pass, vnc-038-agent-2-spec): mcp__unimatrix__context_briefing — surfaced vnc-038 ADR-004 (#5083 delete-the-default / N=1) and ADR-006 (#5085 local UDS under the unified resolver, since tightened to a direct-binding / NOT-a-resolver-key statement); applied to the strengthened AC-09/AC-10, C-13, FR-14, and OQ-2 resolution. No new knowledge stored (read-only tier; spec decisions are feature-specific).
