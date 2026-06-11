# SPECIFICATION — vnc-034 Personal-Cloud Multi-Project Serving

> Umbrella feature. Server serving + pure-JS client + multi-project routing, decomposed into two delivery waves over six shared contracts (C1–C6). Source: `product/features/vnc-034/SCOPE.md` (uni-zero, 2026-06-11). Risk source: `product/features/vnc-034/SCOPE-RISK-ASSESSMENT.md`.

## Objective

Turn the partially-shipped personal-cloud substrate into a reachable, operator-run cloud: one container serves N fully-isolated projects to N clients over pinned TLS HTTPS, single tenant, one bearer token, one connection bundle. The operator stands up the container and registers projects; clients attach *in*, each bound to exactly one project, with knowledge flowing at full fidelity. The design locks six cross-cutting wire/isolation contracts (C1–C6) once so the two delivery waves (Wave 1: serving + client; Wave 2: routing) slot together without interface drift, and every cloud mechanism reduces identically to the local-UDS single-project install.

---

## Domain Models — Ubiquitous Language

Downstream agents (architect, pseudocode, tester, risk strategist) MUST use these terms exactly.

| Term | Definition |
|------|------------|
| **Operator** | The human who stands up and runs the **server container**. Defines (registers) projects. Holds the bearer token and emits bundles. The container is the operator/server side and **never attaches outward**. |
| **Client** | A pure-JS edge instance (e.g. an LLM CLI: Claude Code, Codex CLI, Gemini CLI) that **attaches in** to the server, bound to exactly one project. |
| **Register (a project)** | Server-side operation that creates a project's store (DB, vector index, hash chain, analytics). Operator-only. A project is **never** auto-created by a client. |
| **Attach (a client)** | Client-side operation (`init --remote …/v1/<slug>`) that connects a client to an already-registered slug. Creates **no** store. Errors if the slug is unregistered. |
| **Project** | A unit of isolated knowledge: own DB, vector index, hash chain, analytics under `/data/.unimatrix/{slug}/`. The OSS isolation boundary. |
| **Tenant** | The single trust/billing boundary. OSS is **one tenant, many projects**. Multi-tenant is enterprise-only. |
| **Slug** | **Operator-declared** project identity carried in the URL path (`/v1/{slug}/…`). Decoupled from any client's local path. The cloud-mode resolver key. |
| **Path-hash** | The **local-UDS** project identity: a hash of the daemon's `--project-dir` (ADR-004). The local-mode resolver key. Distinct from slug; the local path-hash assumption MUST NOT leak into cloud mode. |
| **Bundle** | The single artifact the server emits (`client-bundle`) and the client ingests (`init --remote`). Carries `{base-url, token, cert-fingerprint}`. **Cloud-wide** (one per cloud); the slug is appended per-project at client init, not part of the bundle. |
| **Cert-fingerprint** | `sha256:<lowercase-hex>` — SHA-256 over the served leaf certificate's DER bytes. The pinning identity. Computed by the server; pinned by the client. A **wire contract** identical on both sides. |
| **resolve_store seam** | `resolve_store(request) -> Arc<Store>` — the single funnel through which all reads/writes resolve a store handle. One interface, two resolvers (slug for cloud, path-hash for local). The isolation seam (C4). |
| **`UNIMATRIX_PUBLIC_URL`** | The single piece of operator knowledge the cloud cannot auto-derive. One knob, three derived consumers: bundle base-url, `allowed_hosts` default, cert SAN. |

### Cardinality (load-bearing)

- **N clients : 1 project** — multiple clients may attach the same slug (this is exactly the multi-LLM case). Allowed.
- **1 client : 1 project** — a single client instance is bound to exactly one project and **cannot** connect to or multiplex multiple projects. A different project means a separate client instance / container. **Permanent OSS/cloud boundary** (AFFIRMED by human 2026-06-11), not a deferred limitation. Rationale: knowledge-base integrity + per-project self-learning consistency; cross-project access is RBAC-gated enterprise. See "Affirmed decisions" below.
- **1 tenant** — OSS single-tenant. Multi-tenant is enterprise.
- True N:N client↔project is **enterprise-only and requires RBAC** (additive on the C6 `BearerValidator` seam — never an OSS re-architecture).

---

## Shared Contracts (C1–C6) — referenced by requirements below

These are specified as **WHAT must hold**; the architect owns the HOW (serialization form, seam injection mechanism, etc.).

- **C1 — Connection bundle.** Server-emitted, client-ingested; carries `{base-url, token, cert-fingerprint}`; cloud-wide; slug appended at init. *Wire form (JSON / base64 / URL-fragment) is OQ-A — architecture-dependent.*
- **C2 — Cert-fingerprint format.** `sha256:<lowercase-hex>`, SHA-256 over served leaf DER, computed server-side, pinned client-side, **byte-identical on both stacks**.
- **C3 — Public-URL knob.** `UNIMATRIX_PUBLIC_URL` → single derivation feeding three consumers (base-url, allowed_hosts, cert SAN). Socket auto-detect rejected.
- **C4 — URL/route structure + `resolve_store` seam.** `/v1/{slug}/tools/…` with `/v1/tools/…` as the single-project default alias; one funnel, transport-derived identity, sole-write-capability handle, single-no-bypass, identical local/cloud.
- **C5 — Slug + register/attach model.** Operator-declared slug decoupled from path-hash; register ≠ attach; N:1 clients:project AND 1:1 client:project; integrity (not access-control) basis.
- **C6 — Auth/scope/transport separation.** Token authorizes (`BearerValidator` seam), slug scopes data (not a security boundary in OSS), cert secures transport — three concerns, not collapsed.

---

## Functional Requirements

Organized by Group (A/B = Wave 1, C = Wave 2). Each is testable.

### Wave 1 — Group A: Server serving

- **FR-A1** The container SHALL serve a reachable TLS HTTPS endpoint (not UDS `--foreground`), with the listener bound to `0.0.0.0` inside the container, driven by config/env. *(C4 transport)*
- **FR-A2** On first boot, with zero operator input, the server SHALL auto-generate **both** credentials: a 32-byte bearer token AND a self-signed certificate + private key, persisting them to the data volume. On subsequent boots it SHALL load (not regenerate) existing credentials. *(SR-01, SR-11)*
- **FR-A3** The operator SHALL be able to override either credential (supply own token and/or own cert+key, mountable read-only).
- **FR-A4** The generated certificate's SAN set SHALL be derived from `UNIMATRIX_PUBLIC_URL` plus the generous local set (`localhost`, `127.0.0.1`, `0.0.0.0`), with a defined validity period and the private key written mode `0600`. SAN/validity/key-mode are **production requirements**, not inherited from the test-only rcgen helper. *(SR-01, C3)*
- **FR-A5** The server SHALL provide a `client-bundle` command that emits the C1 bundle `{base-url, token, cert-fingerprint}` as a single artifact. It SHALL be a pre-tokio synchronous subcommand (like `health`/`version`, per constraint C-10).
- **FR-A5b** The `client-bundle` command SHALL echo the decoded **base-url** and **cert-fingerprint** to **stderr** in human-readable form, with the **token REDACTED/OMITTED** (never printed to stderr). **stdout** SHALL carry only the opaque pasteable bundle blob. This is an operator usability + safety affordance serving first-class onboarding (NFR-13, goal #4946): the operator can eyeball where the bundle points and which cert it pins without exposing the token. *(NFR-06 token-never-logged, NFR-13, ADR-001 #4947)*
- **FR-A6** The server SHALL compute the cert-fingerprint as `sha256:<lowercase-hex>` of the served leaf certificate's DER (C2), and the value in the bundle SHALL equal that of the cert actually served on the TLS port.
- **FR-A7** `UNIMATRIX_PUBLIC_URL` SHALL be the single source from which three consumers derive: the bundle base-url, the `allowed_hosts` default, and the cert SAN. When unset, the server SHALL use a loud `https://<EDIT-ME>:8443`-style placeholder and a permissive-with-warning `allowed_hosts` posture. Socket auto-detection SHALL NOT be used. *(C3, SR-10)*
- **FR-A8** The Dockerfile/compose SHALL be wired to the serving posture: `EXPOSE` / `ports` publish **only** the TLS port (no plaintext port), HTTP-enable is container-scoped (the global binary default `http.enabled` stays `false`), with refreshed comments and documented bind-mount UID. nan-014 hardening (non-root UID 65532, distroless, ORT pinning, `/shared` `:ro`) SHALL be preserved.
- **FR-A9** First-boot provisioning (cert+token generation, `/data` writability check) SHALL be performed by the Rust binary itself (distroless has no shell). If `/data` is unwritable by UID 65532, the binary SHALL fail **loud and actionable** — no silent failure, no panic, no `.unwrap()`. *(SR-11, constraint: distroless no-shell)*
- **FR-A10** No unauthenticated endpoint SHALL be exposed on the published port beyond `GET /health`. No `/metrics` endpoint SHALL be added.
- **FR-A11** A **cert-rotation runbook** SHALL be a **required deliverable**: a short, documented operator procedure covering **rotate cert → re-run `client-bundle` → re-`init` clients**. Paired with this, attempting to reconnect with a stale (pre-rotation) cert/fingerprint SHALL surface a **clear, diagnosable** error (recognizably a fingerprint mismatch pointing the operator to re-bundle), not an opaque failure. This is a short procedure, not a feature — no rotation tooling is added. *(NFR-13; W5)*

### Wave 1 — Group B: Client (primary aim: fast + safe solo-developer onboarding)

- **FR-B1** `init --remote <bundle>` SHALL ingest the C1 bundle and produce a working **pure-JS** client with **zero native binary** and zero added runtime dependencies in the shipped JS.
- **FR-B2** The client SHALL pin the server's exact certificate by the C2 fingerprint (custom `checkServerIdentity` / fingerprint compare, no CA-trust path), and the fingerprint it pins SHALL be computed identically to the server's. *(C2, SR-02, SR-03)*
- **FR-B3** Each client instance SHALL be bound to **exactly one project** — the slug appended at init (`…/v1/<slug>`). The client SHALL NOT connect to or multiplex multiple projects. *(C5, SR-06)*
- **FR-B4** Attach-to-existing-slug SHALL be first-class: the client SHALL attach to an already-registered slug and SHALL NOT auto-create a project; it SHALL error if the target slug is unregistered. *(C5)*
- **FR-B5** The client SHALL target Linux, macOS (Apple Silicon), and Windows, all HTTPS-remote (Windows is HTTPS-remote only — no Windows local mode).
- **FR-B6** Multi-LLM SHALL be realized as the N-clients-one-project path: distinct LLM CLIs are distinct client instances attaching the **same** slug, connecting identically over HTTPS, with **no per-LLM code path**.
- **FR-B7** In remote mode the client SHALL copy skills, and SHALL respect the init↔uni-init boundary: `init` prints the `/unimatrix-init` pointer and SHALL NOT append the CLAUDE.md knowledge block.
- **FR-B8** The remote-only install SHALL be `< 250 KB` (no 31 MB binary, no 87 MB model). Install SHALL be copy-install only (no `npm link`).
- **FR-B9** The bundle parser SHALL validate the bundle against a defined schema and reject malformed input at the trust boundary. The strict schema reject (missing / extra / wrong-type field) is the **load-bearing** guard. A **4 KB byte-length cap on the RAW pasted string** SHALL be enforced **BEFORE** base64url-decode and **BEFORE** JSON-parse as a belt-and-suspenders DoS guard. *(SR-09, ADR-001 #4947)*

### Wave 2 — Group C: Multi-project routing

- **FR-C1** The server SHALL route path-prefix slug requests `/v1/{slug}/tools/…` to the per-slug store via the `ProjectRouter` populating the `resolve_store` seam. *(C4)*
- **FR-C2** Slugs SHALL be operator-declared in `[[projects]]` config. The `ProjectRouter` SHALL resolve slug → store.
- **FR-C3** Each slug SHALL have its own DB, vector index, hash chain, and analytics under `/data/.unimatrix/{slug}/`, with **no cross-project sharing** in OSS (no cross-project read or write). *(C4 invariants)*
- **FR-C4** The server SHALL provide project lifecycle CLI: **register**, **list**, **delete** slugs.
- **FR-C5** Slug values SHALL be validated at the routing edge against an allowlist (defined charset + length bound); inputs containing path-traversal or encoded separators (`../`, encoded `/`, etc.) SHALL be rejected and SHALL NOT escape `/data/.unimatrix/{slug}/`. *(SR-09 — security, fix-before-merge)*
- **FR-C6** Single-project backward-compat SHALL hold: when `[[projects]]` is absent, `/v1/tools/…` (no slug) SHALL behave exactly as current (zero change). *(C4 default alias)*
- **FR-C7** N clients on one slug SHALL share that slug's store, attributed by `session_id`/`agent_id`; each such client remains bound to that single slug (same-project multi-connection allowed; per-client cross-project fan-out is not).

### Cross-wave: the isolation seam (C4) — required from Wave 1

- **FR-X1** `resolve_store(request) -> Arc<Store>` SHALL be the **single funnel** through which every read and write resolves a store handle. No downstream path SHALL obtain a store handle by any other route (single funnel, no bypass). *(SR-07, SR-08, A4)*
- **FR-X2** Project identity SHALL come from the **transport** — the URL-path slug (cloud) or the daemon project-dir path-hash (local, ADR-004) — and NEVER from the request payload. The agent SHALL have no request field in which to name another project, making cross-project mis-targeting **unrepresentable, not merely rejected**. *(C4 invariant 1, C5, SR-06)*
- **FR-X3** The resolved `Arc<Store>` SHALL be the **sole write capability**, threaded from the routing edge; no code path SHALL obtain a different store handle. *(C4 invariant 2)*
- **FR-X4** The seam SHALL reduce **identically** to the local-UDS path-hash store: one seam, two resolvers (slug | path-hash); local single-project resolves the one path-hash store when the slug is absent. There SHALL be **no cloud-only isolation path** that the local install does not also exercise. *(constraint: local-repo parity non-negotiable; SR-08, A2)*
- **FR-X5** In Wave 1 the single store SHALL be served **through** the seam (the Wave-1 default resolver returns the one store), not around it — so Wave 2's `ProjectRouter` populates a proven seam rather than replacing a bypass. *(SR-07, A4)*

---

## Non-Functional Requirements

| ID | Requirement | Measure / Target |
|----|-------------|------------------|
| **NFR-01** | Remote client install size | `< 250 KB`; enforced as a hard acceptance test (SR-03). |
| **NFR-02** | No new server crates for cert-gen/fingerprint | Use only present deps: `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`. Shipped client JS stays dependency-free. (A3) |
| **NFR-03** | Rust safety | No `unsafe`, no `.unwrap()` in non-test code. Provisioning paths use explicit fail-loud errors. (SR-11) |
| **NFR-04** | File size | Max 500 lines/file. The test-only rcgen helper SHALL be refactored cleanly on promotion. |
| **NFR-05** | Secret handling | Token and cert/key live as **files** on the data volume; key mode `0600`. No secret in **any** DB. |
| **NFR-06** | Token confidentiality | Token SHALL never be logged, baked into any image layer, committed, or persisted to any DB. |
| **NFR-07** | Transport security posture | The published port is always TLS; no `tls.enabled=false` exposed in OSS posture; no plaintext-to-client mode. |
| **NFR-08** | TLS seam preservation | TLS SHALL stay internally terminable (`tls.enabled` / `TlsConfig` seam intact) for the enterprise proxy path — TLS SHALL NOT be hardcoded such that the seam disappears. (SR-04) |
| **NFR-09** | Enterprise-extends seams | Carried from day one (ass-060 invariants): slug = isolation boundary, `ProjectRouter` resolution seam, per-project DB/hash-chain, `BearerValidator` trait, extensible audit schema (`credential_type`, `capability_used`, `agent_attribution`). OSS code SHALL hold these so enterprise is **additive**, not a re-architecture. Pattern: documented-but-degenerate seam (cf. ADR-007 vnc-025 `session_key`). (SR-04) |
| **NFR-10** | Local-repo install parity | Every personal-cloud change SHALL function unchanged for the local single-project install (UDS, path-hash `data_dir` per ADR-004). **Non-negotiable.** Verified by a Wave-1 local-install regression test, not deferred to Wave 2. (SR-08) |
| **NFR-11** | Data path correctness | `data_dir = /data/.unimatrix/{hash}` (not `/data`); cert/token persistence and docs use the resolved path; project hash depends on `--project-dir /data`. |
| **NFR-12** | Container hardening | Non-root UID 65532, distroless (no shell), ORT pinning, `/shared` `:ro` — preserved from nan-014. |
| **NFR-13** | Onboarding speed/safety | Install → ingest bundle → connect is a fast, safe flow for a solo developer (first-class aim); no manual cert wrangling beyond the bundle. |
| **NFR-14** | Test infrastructure | Cumulative — extend existing fixtures/helpers; add cross-stack fingerprint parity fixtures rather than hand-writing JS-side goldens (SR-02). |

---

## User / Operator Workflows

### W1 — Operator stands up the cloud (Wave 1)
1. Operator runs `docker compose up` with no config.
2. First boot: the Rust binary checks `/data` writability (fails loud-and-actionable if not), then auto-generates the 32-byte token and self-signed cert+key, persisting to the data volume (key `0600`).
3. The server serves HTTPS on the TLS port bound `0.0.0.0`; SAN/base-url/allowed_hosts derived from `UNIMATRIX_PUBLIC_URL` (or loud placeholder if unset).
4. Operator runs `client-bundle` → receives `{base-url, token, sha256:fingerprint}`.

### W2 — Operator registers a project (Wave 2; modeled in Wave 1)
1. Operator declares a slug in `[[projects]]` config (or via `register` CLI).
2. Server creates the per-slug store. **Register is server-side and creates the store** — never client-driven.

### W3 — Client attaches (Wave 1 single-project / default alias; Wave 2 by slug)
1. Operator installs the pure-JS client (copy-install, `< 250 KB`).
2. Operator hands the bundle (and, in Wave 2, the slug) to the client. *(Slug discovery mechanism is OQ-B — architecture-dependent.)*
3. `init --remote <bundle>` (slug appended → `…/v1/<slug>`): client ingests bundle, pins the cert by fingerprint, copies skills, prints the `/unimatrix-init` pointer (no CLAUDE.md append).
4. Client errors if the slug is unregistered (**attach ≠ register**). Otherwise it is now bound to **exactly one project**.
5. Knowledge flows over pinned TLS — same fidelity as local UDS.

### W4 — Multi-LLM (the N-clients-one-project path)
- Operator repeats W3 for each LLM CLI (Claude Code, Codex CLI, Gemini CLI) against the **same** slug. Each is a separate client instance attaching one project; all connect identically over HTTPS. No per-LLM machinery. Each remains 1:1 with that project.

### W5 — Cert rotation (runbook deliverable, FR-A11)
- Operator regenerates/replaces the cert, re-runs `client-bundle`, and each client re-runs `init --remote` with the new bundle. Captured as a short **required runbook deliverable** (rotate → re-bundle → re-init). If a client tries to reconnect on a stale fingerprint without re-bundling, it gets a clear, diagnosable fingerprint-mismatch error pointing to re-bundle.

---

## Acceptance Criteria

Per-wave, derived from the SCOPE Acceptance Criteria section. Verification method noted for each. AC-IDs trace downstream.

### Wave 1 — Server (Group A)

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| **AC-W1-S1** | `docker compose up` with no operator config serves HTTPS on the TLS port, reachable from a sibling container by service name. | Integration: sibling-container HTTPS request succeeds. |
| **AC-W1-S2** | No plaintext port is published. | Inspect published ports; assert only the TLS port; attempt plaintext connect fails. |
| **AC-W1-S3** | First boot auto-generates token + cert; on restart they are loaded, not regenerated; operator override works. | Boot twice, assert identity of token+cert across restarts; supply override, assert it is used. |
| **AC-W1-S4** | `client-bundle` emits `{base-url, token, sha256:fingerprint}`, and the fingerprint **equals** SHA-256 of the served leaf DER. *(C2 wire-contract equality)* | Compute SHA-256 of the served leaf DER independently; assert byte-equality with bundle value. Cross-stack parity fixture. |
| **AC-W1-S5** | Token is never logged, imaged, committed, or stored in any DB. | Grep logs/image layers; assert token absent. Schema audit: no token column in any DB. |
| **AC-W1-S5b** | `client-bundle` echoes decoded **base-url** + **cert-fingerprint** to **stderr** (human-readable); the **token** appears nowhere in stdout, stderr, or logs; stdout carries only the opaque bundle blob. *(FR-A5b, NFR-06, NFR-13)* | Run `client-bundle`; assert (a) base-url and fingerprint present on stderr, (b) the token string is absent from stdout AND stderr AND any log output, (c) stdout is the opaque `unimatrix-bundle:…` blob only. |
| **AC-W1-S6** | No unauthenticated endpoint on the published port beyond `GET /health`; no `/metrics` endpoint. | Probe endpoints unauthenticated; assert only `/health` responds; assert no `/metrics`. |
| **AC-W1-S7** | nan-014 hardening preserved (UID 65532, distroless, ORT pinning, `/shared` `:ro`). | Image inspection / container runtime assertions. |
| **AC-W1-S8** | Host bind-mounted `/data` writable by UID 65532 (documented); binary fails loud-and-actionable if not. | Mount unwritable `/data`; assert actionable error, no panic, no `.unwrap()`. |
| **AC-W1-S9** | `UNIMATRIX_PUBLIC_URL` feeds base-url + allowed_hosts + SAN from one derivation; bundle host ∈ cert SAN. *(C3 three-consumer derivation, SR-10)* | Set the knob; assert all three consumers reflect it; assert bundle host ∈ cert SAN. |

### Wave 1 — Client (Group B)

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| **AC-W1-C1** | `init --remote <bundle>` produces a working pure-JS client on Linux, macOS-arm, Windows, with no native binary. | Per-OS integration: init then a live knowledge call over HTTPS. |
| **AC-W1-C2** | The client pins the exact server cert by the C2 fingerprint; a wrong/changed cert is rejected. *(C2 equality, SR-02)* | Connect with matching cert (succeeds); connect with mismatched cert (rejected). |
| **AC-W1-C3** | Remote install `< 250 KB`. | Measure install footprint; assert `< 250 KB`. Hard gate. |
| **AC-W1-C4** | Client attaches to an existing slug; never auto-creates a project; errors if the slug is unregistered. *(C5)* | Attach unregistered slug → error; attach registered slug → success; assert no store created by the client. |
| **AC-W1-C5** | The resulting client is bound to exactly one project — no multi-project connection / multiplexing is representable. *(C5, SR-06)* | Assert the client has no API/field to target a second project; mis-target is unrepresentable. |
| **AC-W1-C6** | Skills copied; CLAUDE.md knowledge block NOT appended; `/unimatrix-init` pointer printed. | Inspect post-init filesystem and output. |
| **AC-W1-C7** | **Multi-LLM** proven as the N-clients-one-project path: ≥2 distinct CLIs (e.g. Claude Code + Codex CLI) attach the **same** slug and connect identically over HTTPS; no per-LLM code path. | Two distinct CLIs attach one slug; both connect; assert single shared client code path. |
| **AC-W1-C8** | Onboarding (install → ingest bundle → connect) is fast and safe for a solo developer. | End-to-end timed walkthrough; no manual cert handling beyond the bundle. |
| **AC-W1-C9** | The bundle parser validates against a schema and rejects malformed bundles at the trust boundary; the strict schema reject is the load-bearing guard. *(SR-09)* | Feed malformed/truncated/oversized/missing-field/extra-field/wrong-type bundles; assert rejection, no crash. |
| **AC-W1-C10** | The 4 KB byte-length cap is enforced on the RAW pasted string **BEFORE** base64url-decode and **BEFORE** JSON-parse (length-check-first ordering, belt-and-suspenders). *(SR-09, ADR-001 #4947)* | Feed an over-cap raw string; assert it is rejected by the length check **prior to** any decode/parse (e.g. an over-cap string that is not valid base64url still rejects on length, not on decode error); assert the cap rejects before schema validation runs. |

### Wave 1 — Isolation seam (C4, required Wave 1)

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| **AC-W1-X1** | All store access funnels through `resolve_store`; the Wave-1 single store is served **through** the seam, not around it. *(SR-07)* | Source assertion: single resolution funnel; Wave-1 resolver returns the one store; no bypass call sites. |
| **AC-W1-X2** | The seam reduces identically to the local-UDS path-hash store; the local single-project install exercises the **same** code path. *(NFR-10, SR-08)* | **Local-install regression test in the Wave-1 set** (not deferred): local UDS resolves the path-hash store through the seam. |
| **AC-W1-X3** | Mis-targeting is unrepresentable: project identity is transport-derived; no request payload field names a project. *(C5, FR-X2, SR-06)* | Inspect request types; assert no project-naming field; transport is the sole identity source. |

### Wave 2 — Routing (Group C)

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| **AC-W2-R1** | `/v1/{slug}/…` routes to the per-slug store. | Request two slugs; assert each lands in its own store. |
| **AC-W2-R2** | `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged (single-project backward-compat). *(C4 default alias, OQ-C dependent)* | Run with no `[[projects]]`; assert current behavior, zero change. |
| **AC-W2-R3** | Per-slug isolation: no cross-project read or write. | Write into slug A; assert unreadable/unwritable from slug B's path. |
| **AC-W2-R4** | Register / list / delete lifecycle works. | CLI exercises each; assert store created/listed/removed. |
| **AC-W2-R5** | N clients on one slug share the store, attributed by `session_id`; each client stays bound to that one slug. *(C5)* | Multiple clients on one slug; assert shared store + correct per-session attribution. |
| **AC-W2-R6** | Slug allowlist rejects path-traversal / encoded separators; no escape from `/data/.unimatrix/{slug}/`. *(SR-09 — security, fix-before-merge)* | Feed `../`, encoded `/`, over-length slugs; assert rejection and no filesystem escape. |

### Contracts (both waves)

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| **AC-CT-C2** | Fingerprint format `sha256:<lowercase-hex>` identical server↔client. *(C2)* | Cross-stack parity fixture: server-computed == client-pinned for the same DER. |
| **AC-CT-C3** | `UNIMATRIX_PUBLIC_URL` feeds base-url + allowed_hosts + SAN from a single derivation function. *(C3)* | Unit test the derivation; assert all three consumers read from it; bundle host ∈ SAN. |
| **AC-CT-C4** | Store access funnels through `resolve_store`; the route shape admits `/{slug}` **additively** (Wave 2 adds no bypass, re-points no Wave-1 client). *(C4, OQ-C/OQ-D dependent)* | Assert Wave-2 router injects into the existing seam; Wave-1 clients unchanged. |
| **AC-CT-C6** | Token authorizes, slug scopes, cert secures — three concerns not collapsed; `BearerValidator` / `TlsConfig` / slug seams intact for enterprise. *(C6, SR-04, NFR-09)* | Source assertion: seam interfaces present and degenerate-but-documented (cf. session_key pattern). |
| **AC-CT-ROT** | A cert-rotation runbook is a **required deliverable**: a documented operator procedure (rotate cert → re-`client-bundle` → re-`init` clients). Rotating **without** re-bundling SHALL surface a **clear, diagnosable** error (recognizable fingerprint mismatch directing the operator to re-bundle). *(FR-A11)* | Assert the runbook doc exists and ships as a deliverable. Rotate cert; re-bundle; re-init client; assert reconnect succeeds. Separately, rotate cert WITHOUT re-bundling; attempt reconnect; assert a clear/diagnosable fingerprint-mismatch error (not opaque). |

---

## Constraints (carried from SCOPE)

1. **No new server crates** for cert-gen/fingerprint — only `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`. Client + shipped JS dependency-free. *(A3)*
2. Rust: **no `unsafe`**, **no `.unwrap()`** in non-test code, **max 500 lines/file**; test-only rcgen helper refactored cleanly on promotion.
3. **No secrets in any DB** — token/cert as files on the data volume, key `0600`.
4. **Distroless runtime has no shell** — first-boot provisioning is done by the Rust binary, not a shell entrypoint.
5. `data_dir = /data/.unimatrix/{hash}` (not `/data`); cert/token persistence + docs use the resolved path; hash depends on `--project-dir /data`.
6. **TLS internally terminable** (`tls.enabled` / `TlsConfig` seam) — do not hardcode TLS so the seam disappears (enterprise proxy path).
7. `client-bundle` is a pre-tokio sync subcommand (C-10), like `health`/`version`.
8. **Enterprise-extends seams from day one** (ass-060 seven invariants): slug = isolation boundary, `ProjectRouter` resolution seam, per-project DB/hash-chain, `BearerValidator` trait, extensible audit schema. OSS holds these so enterprise is additive.
9. **Local-repo install parity is non-negotiable** — `resolve_store` (C4) is ONE mechanism across local (path-hash, no slug) and cloud (slug); no cloud-only isolation path the local install does not exercise.
10. Test infrastructure is cumulative — extend existing fixtures/helpers.

---

## Dependencies

- **Present crates (reuse):** `rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`.
- **Existing server surface:** HTTP listener gated on `config.http.enabled` (`main.rs`), bearer auto-gen (`http/token.rs`), TLS-from-PEM acceptor + `TlsConfig` seam (`http/tls.rs`), configurable bind (`HttpConfig`), constant-time bearer auth + `/health` bypass (`http/auth.rs`), `allowed_origins` CSRF layer (vnc-023), pass-through `ProjectRouter`/`McpAdapter` (`http/router.rs`).
- **Existing client surface:** F3 `init --remote` base flow (#679), F4a TS HTTP client (#680).
- **Existing container surface:** nan-014 Dockerfile/compose + GHCR multi-arch publish (#629).
- **Research grounding:** ass-060 (multi-project architecture, seven OSS invariants, slug routing, volume layout), ass-050 (bearer security + enterprise surface), ass-068/ass-069 (unified TS client + client-streamed transcript). Goal #4934 / vision goal #4946 (personal-cloud destination).
- **Pattern reference:** ADR-007 vnc-025 (#4745) — documented-but-degenerate enterprise seam (`session_key`) is the precedent for NFR-09 seam treatment.

---

## NOT in Scope (explicit exclusions)

All are enterprise / additive on the seams above — **no OSS re-architecture**:

- No proxy-terminated / K8s TLS termination in OSS (additive on `TlsConfig` seam — leave the seam, do not pre-build proxy support).
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

---

## Open Questions (architecture-dependent — not pre-decided here)

These four are genuinely open and affect acceptance; the architect owns the resolution. Acceptance criteria above are written to be satisfiable under any resolution and are flagged where the OQ bites.

- **OQ-A — Bundle serialization (wire form of C1).** JSON / base64-of-JSON / single URL-with-fragment. Affects copy-paste ergonomics and the client parser (FR-B9 / AC-W1-C9). *Spec impact: AC-W1-S4 asserts the three fields and fingerprint equality regardless of encoding.*
- **OQ-B — Slug discovery for attach.** Server-exposed slug list (endpoint or `client-bundle --list`) vs out-of-band operator hand-off. Affects attach UX (W3 step 2) and whether any listing surface is added (which would interact with AC-W1-S6). *Spec impact: if a listing surface is added, it must respect the "no unauthenticated endpoint beyond /health" criterion.*
- **OQ-C — Wave-1 single-project addressing.** Serve at the `/v1/tools/…` default alias vs require a default slug from day one. Determines whether Wave 2 is purely additive or re-points Wave-1 clients (SR-05). *Spec impact: AC-W2-R2 and AC-CT-C4 assume the additive alias (per design recommendation R-b) — confirm.*
- **OQ-D — Wave-to-issue mapping.** Confirm Wave 1 = #726 + #725, Wave 2 = #727, vs a finer cut (e.g. carving the C1/C2 connection-contract into its own first deliverable consumed by both server and client). *Spec impact: AC grouping follows the two-wave cut; a finer cut would re-group, not re-content.*

### Affirmed decisions (no longer open)

- **A1 (1-client:1-project) — AFFIRMED (human, 2026-06-11) as a PERMANENT OSS/cloud boundary.** Rationale (documented basis for the boundary, NOT elevated to product goals of their own): (a) **knowledge-base integrity** — grounded in vision goal #4946 — each project's store stays a closed isolation boundary; and (b) **per-project self-learning consistency** — each project configures its own LLM clients on how to write/use Unimatrix, so consistency depends on that per-project setup; one client spanning two projects would break that per-project configuration model. Cross-project learning would require a read-only cross-project access capability gated by RBAC — i.e. the paid/enterprise version (additive on the C6 `BearerValidator` seam, never an OSS re-architecture) — kept out of OSS for simplicity and marketability. *(Integrity and consistency are RATIONALE for this boundary only; the sole stated goal remains #4946. No other feature is obligated to treat integrity/consistency as a requirement.)*

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vision goal #4946 (personal-cloud destination: confirms all six contracts C1–C6, register/attach model, 1:1 integrity boundary, one resolve_store seam across local+cloud) and ADR-007 vnc-025 #4745 (documented-but-degenerate enterprise seam pattern, applied as NFR-09 seam-treatment precedent). No conflicting prior spec conventions found; proceeded with SCOPE + risk-assessment as primary sources.
