# vnc-034 — Personal-Cloud Multi-Project Serving (Server + Client + Routing)

> **Umbrella feature.** Designs the full personal-cloud serving arc as one coherent capability, then decomposes into delivery waves. Supersedes the standalone framing of vnc-032 (#726), nan-019 (#725), and vnc-033 (#727) — those become this feature's **delivery waves**, and their existing SCOPEs are input, not waste. Scoped in a uni-zero session 2026-06-11.

## Problem Statement

The personal-cloud destination (goal #4934) is **one operator-run container, one bearer token, one command, serving N projects to N clients over HTTPS** — single tenant, full per-project isolation, multi-LLM clients. The operator stands up the container (the server side) and defines N projects inside it; clients connect *in*, each bound to a single project. The container never attaches outward. The substrate is partially shipped: W2-2 added the HTTPS transport + bearer auth to the binary (#658), nan-014 built + publishes the multi-arch container to GHCR (#629), vnc-027 shipped the TS hook client (#680). But three gaps remain between "substrate" and "reachable cloud":

1. The container does not serve a reachable network endpoint (`serve --foreground` is UDS-only; no `EXPOSE`/`ports`).
2. There is no client-side remote init for non-Linux clients, and no bundle-driven connection flow.
3. There is no multi-project routing — one container serves at most one project's knowledge.

These were initially split into three issues. But they **share interfaces** — the connection bundle, the cert-fingerprint format, the URL/slug structure, the store-resolution seam — that will drift if designed in isolation (the fingerprint format alone is a wire contract between the server and the client). This umbrella designs the arc once so those contracts are locked coherently, and lets the design decompose delivery into waves.

## Objective

The Unimatrix container is the **server/operator side** — it does not attach to anything. The operator stands up and operates the container, defines N projects inside it, and gets one connection bundle; **clients attach in** to it. The operator attaches any number of clients, each bound to a single project (**N clients per project; each client → exactly one project**), over pinned TLS; each project's knowledge is fully isolated. **Easy + secure client onboarding for a solo developer is a first-class aim** — installing, configuring, and connecting a client must be fast and safe. **Single tenant, N projects, N clients per project, 1 client : 1 project** — the 1-client-per-project boundary is **permanent for OSS/cloud**, not a stepping stone; a solo developer runs a separate client instance / container per project. True N:N client↔project is enterprise-only and requires RBAC. Enterprise (multi-tenant + OAuth/RBAC) extends the same seams without re-architecture (ass-060 seven invariants).

## Goals (full requirement set)

**Group A — Server serving** (from vnc-032 #726):
1. Container serves a reachable **TLS HTTPS** endpoint (not UDS `--foreground`); listener bound `0.0.0.0` inside the container, driven by config/env.
2. **First boot auto-generates BOTH credentials** with zero operator input: 32-byte bearer token (exists) AND a self-signed cert+key (must build), persisted to the data volume; operator may override either.
3. A server-side **`client-bundle`** command emits `{base-url, token, cert-fingerprint}` as one blob.
4. **Cert-fingerprint pinning** trust model; the server computes/exposes the fingerprint.
5. Dockerfile/compose wired to the posture (`EXPOSE`/`ports` per Non-Goals, refreshed stale comments, documented bind-mount UID); nan-014 hardening preserved (non-root UID 65532, distroless, ORT pinning, `/shared` `:ro`).

**Group B — Client** (from nan-019 #725) — *primary aim: easy + secure onboarding for a solo developer (fast install / configure / connect)*:
6. `init --remote` **ingests the bundle**; pure-JS client, zero native binary. Each client instance is **bound to exactly one project** (the slug appended at init); it does not connect to or multiplex multiple projects — a different project means a separate client instance.
7. Targets **Linux, macOS (Apple Silicon), Windows** — all HTTPS-remote (Windows is HTTP-remote only; no Windows local mode).
8. **Attach-to-existing-slug is first-class** (multiple clients per project); the client never auto-creates a project — errors if the target slug is unregistered. **Multi-LLM (Claude Code, Codex CLI, Gemini CLI) is exactly this case, not a separate axis**: each LLM CLI is a distinct client instance attaching the *same* slug (N clients : 1 project), each bound 1:1 to that project. "Connect identically via HTTPS" is validated as the N-clients-one-project path, not new per-LLM machinery.
9. Skills copied in remote mode; **init↔uni-init boundary** — `init` prints the `/unimatrix-init` pointer, does NOT append the CLAUDE.md knowledge block.
10. Remote-only install **< 250 KB** (no 31 MB binary, no 87 MB model).

**Group C — Multi-project routing** (from vnc-033 #727; ass-060):
11. **Path-prefix slug routing** `/v1/{slug}/tools/…`; operator-declared slugs in `[[projects]]` config; `ProjectRouter` resolves slug → store.
12. **Per-slug isolation**: own DB, vector index, hash chain, analytics under `/data/.unimatrix/{slug}/`; no cross-project sharing in OSS.
13. **Project lifecycle**: register / list / delete slugs (CLI).
14. **Single-project backward-compat**: `[[projects]]` absent ⇒ `/v1/tools/…` with no slug (current behavior, zero change).

## Shared Contracts (FIRST-CLASS — the reason for the umbrella; design locks these ONCE)

These interfaces span features and consumers. They MUST be designed coherently here, not re-derived per wave — divergence on any of them is a delivery-time bug.

- **C1 — Connection bundle.** The artifact the server emits (`client-bundle`) and the client ingests (`init --remote`). Carries `{base-url, token, cert-fingerprint}`. **Cloud-wide** (one bundle per cloud); the **slug is appended per-project at init** (`/v1/{slug}`), not part of the bundle.
- **C2 — Cert-fingerprint format.** `sha256:<lowercase-hex>` — SHA-256 over the served leaf certificate's DER. The server computes it; the client pins the exact cert. **Locked identically on both sides** (this is the wire contract that motivated the umbrella).
- **C3 — Public-URL knob.** `UNIMATRIX_PUBLIC_URL` (default = container service-name/hostname). The single piece of operator knowledge the cloud cannot auto-derive (NAT / publish-remap / service-name vs host). **One knob, three consumers**: the bundle's base-url, the `allowed_hosts` default, and the cert SAN all derive from it. Auto-detect from the socket is explicitly rejected.
- **C4 — URL/route structure + store-resolution seam (THE isolation seam).** `/v1/{slug}/tools/…` with `/v1/tools/…` as the single-project **default-project alias**. The **`resolve_store(request) -> Arc<Store>` seam is DESIGNED here** (interface), built minimal in Wave 1 (returns the one store), populated by the `ProjectRouter` in Wave 2. **This seam carries the cross-project knowledge-integrity guarantee (C5) and is SHARED with the local-UDS install — it is not a cloud-only path.** The isolation invariant the design MUST satisfy identically in ALL modes (local UDS single-project, cloud single-project alias, cloud multi-slug):
  1. **Project identity comes from the transport, never the request payload** — the slug from the URL path (cloud) or the daemon's project-dir path-hash (local, ADR-004). The agent has no field in which to name another project, so mis-targeting is **unrepresentable**, not merely rejected.
  2. **The resolved `Arc<Store>` is the sole write capability**, threaded from the routing edge; no downstream path can obtain a different store handle.
  3. **Single funnel, no bypass** — every read/write resolves through this one seam, which therefore receives **proof-grade correctness treatment** (the integrity of every project rides on it).
  Given the no-`unsafe` / no-`.unwrap()` constraint, **in-process multi-store satisfies this without separate processes — process-per-project is NOT required** (it would tax single-binary #6 and the hot-path #7 at N× model memory for an OS boundary safe Rust already largely provides). The same seam reduces to local single-project (slug absent ⇒ the one path-hash store), so the common local install exercises the exact code the cloud depends on. This is the definitive answer to "how do we leave a seam for an undesigned router" — the *interface and its isolation invariant* are designed up front; only the Wave-2 `ProjectRouter` population is deferred. Principle #7 hot-path caches become **per-slug**, rebuilt per-project by tick.
- **C5 — Slug + register/attach model.** The slug is **operator-declared project identity, decoupled from any client's local path-hash** (the local 1:1 path-hash assumption must NOT leak into cloud mode). Two distinct operations: **register a project** (server-side, creates the store, never client-auto-created) vs **attach a client** (`init --remote …/v1/<slug>`, no store creation). Cardinality **N clients : 1 slug : 1 tenant**, **AND 1 client : 1 project** (a **permanent OSS/cloud boundary**, not a temporary limitation — a single client instance is bound to exactly one project and CANNOT connect to or multiplex multiple projects). **The basis is knowledge-integrity, not access control**: a client bound to project B that writes into project A's hash chain permanently corrupts A's knowledge with content that never followed A's conventions — catastrophic and unrollbackable, because every write is attributed and chained. 1:1 removes the agent's ability to mis-target at all; RBAC is precisely what lets enterprise relax it safely — a server-enforced per-project `unimatrix_project` claim *rejects* a wrong-project write rather than relying on client config to prevent it. A solo/personal developer's needs are met entirely by 1-client-per-project: a different project means a separate client instance / container. True N:N client↔project is an **enterprise-only capability requiring RBAC** — additive on the existing auth seam (C6 / `BearerValidator` / ass-060 enterprise extension), **never an OSS re-architecture**. Multiple independent clients/containers connecting to the **same** project is explicitly allowed (that is just N clients : 1 project); the only restriction is per-client fan-out across projects. Attribution within a project by `session_id`/`agent_id`.
- **C6 — Auth/scope/transport separation.** Token authorizes (the `BearerValidator` enterprise seam), slug scopes data (NOT a security boundary in OSS single-tenant), cert secures transport. Three concerns, deliberately not collapsed. Enterprise binds the slug to a JWT `unimatrix_project` claim + RBAC on the same seam.

## Wave Decomposition (guidance — the design refines/confirms)

- **Wave 1 — Single-project serving + client (mechanics validation).** Groups A + B against a single implicit project. Contracts C1/C2/C3/C6 fully realized; C4 built minimal (route shape + `resolve_store` seam returning the one store); C5 register/attach *modeled* but only the single-project path exercised. **End state: operator runs the container, gets a bundle, attaches a Linux/macOS/Windows client over pinned TLS, knowledge flows — the cloud is usable.** Maps to #726 (vnc-032) + #725 (nan-019).
- **Wave 2 — Multi-project routing.** Group C: populate the `resolve_store` seam with `ProjectRouter`, `[[projects]]` config + slug lifecycle, per-slug isolation, full register/attach. Designed and built **against the validated Wave-1 cloud**, so the router meets reality, not a guess. Maps to #727 (vnc-033).

Rationale for the cut: Wave 1 proves the entire transport/security/client stack end-to-end on the simplest topology; Wave 2 adds only the routing dimension to a known-good base. The shared contracts (C1–C6) are designed across both waves up front so Wave 2 slots in without re-cutting interfaces.

## Non-Goals (enterprise / out of scope — all additive on seams above, no re-architecture)

- **No proxy-terminated / K8s TLS termination in OSS** — enterprise-only, additive on the `TlsConfig` seam (leave the seam; do not pre-build proxy support).
- **No plaintext-to-client mode** — the published port is always TLS; no `tls.enabled=false` exposed in the OSS posture.
- **No CA-trust / SAN-based hostname validation** — fingerprint pinning is the OSS trust model; CA+SAN is the enterprise/proxy path.
- **No cross-project knowledge sharing / owner store** — enterprise (ass-060 Q5).
- **No OAuth / JWT / RBAC per-slug authz** — enterprise; the slug is the seam.
- **No multi-TENANT** — OSS is one tenant, many projects.
- **No one client connecting to / multiplexing multiple projects** — a single client instance is bound to exactly one project; a client that needs a different project uses a separate client instance. This is a **permanent OSS/cloud boundary**, not a deferred feature — true N:N client↔project is enterprise-only and requires RBAC (additive on the C6 / `BearerValidator` seam, never an OSS re-architecture). *Use case:* an IDE running in a container connects to its project-dedicated Unimatrix; that same IDE client does NOT reach a different project's repository — but the operator can spin up a separate container/client and connect it to the exact same project (same-project multi-connection is allowed; per-client fan-out is not).
- **No macOS / Windows SERVER** — the server is Linux-only (binary or docker, arm/intel); non-Linux targets are pure-JS clients. No darwin/windows server packages, no cross-compile.
- **No rate limiting, secret-rotation tooling, separate-auth-domain `/metrics`, or adversarial testing (#628)** — deferred / enterprise. **No new `/metrics` endpoint** (the W2-2 prose mentions it aspirationally; the code has none — do not add an unauthenticated one).
- **No CLAUDE.md knowledge-block append in `init`** — `uni-init` owns it (init prints the pointer only).
- **No local-UDS behavior change** — the binary's global `http.enabled` stays `false`; HTTP-enable is container-scoped only.
- **No `npm link`-based client install** — copy-install only (the nan-016 isolation rule).

## Background Research (EXISTS vs MUST-BUILD — grounded in code)

Full server-side EXISTS/MUST-BUILD analysis is in `product/features/vnc-032/SCOPE.md` (carried as input). Summary:

**EXISTS** — HTTP listener wiring gated on `config.http.enabled` (`main.rs:840-900`), bearer-token auto-gen (`http/token.rs load_or_generate_token`), TLS-from-PEM acceptor (`http/tls.rs build_tls_acceptor`, `TlsConfig` seam), configurable bind (`HttpConfig.bind_address` default `0.0.0.0`, port `8443`, `enabled` default `false`), constant-time bearer auth (`http/auth.rs`, `/health` GET bypass), `rcgen 0.13` (test-only today), `allowed_origins` CSRF layer (vnc-023), a pass-through `ProjectRouter`/`McpAdapter` (`http/router.rs`). Client: F3 `init --remote` base flow (#679), F4a TS HTTP client (#680). Container: nan-014 Dockerfile/compose + GHCR multi-arch publish (#629).

**MUST BUILD** — first-boot self-signed cert generation (promote the test-only rcgen helper to a production `load_or_generate_cert`), SHA-256-DER fingerprint computation, the `client-bundle` subcommand, container HTTP-enable mechanism, Dockerfile `EXPOSE`/compose `ports`/refreshed comments/bind-mount UID docs, the `resolve_store` funnel seam (Wave 1) and `ProjectRouter` slug map + `[[projects]]` config + lifecycle (Wave 2), client mode-selection + macOS/Windows validation + bundle ingestion + attach-to-slug, remote-install size gate.

**Grounding:** ass-060 (multi-project data architecture, seven OSS invariants, slug routing, volume layout, migration), ass-050 (bearer security model + enterprise extension surface), ass-068/ass-069 (unified TS client + client-streamed transcript), goal #4934 (personal-cloud destination).

## Acceptance Criteria (umbrella-level; per-wave detail produced in design)

Server (Wave 1): `docker compose up` with no operator config serves HTTPS on the TLS port, reachable from a sibling container by service name; no plaintext port published. First boot auto-generates token + cert (loaded, not regenerated, on restart); operator override works. `client-bundle` emits `{base-url, token, sha256:fingerprint}`; the fingerprint equals SHA-256 of the served leaf DER. Token never logged / imaged / committed / in any DB. No unauthenticated endpoint on the published port beyond `GET /health`. nan-014 hardening preserved. Host bind-mounted `/data` is writable by UID 65532 (documented; binary fails loud-and-actionable if not).

Client (Wave 1): `init --remote <bundle>` produces a working pure-JS client on Linux / macOS-arm / Windows with no native binary; attaches to an existing slug; the resulting client is bound to **exactly one project** (no multi-project connection / multiplexing); never auto-creates a project; remote install < 250 KB; skills copied; CLAUDE.md block NOT appended. Onboarding is fast and safe (install → ingest bundle → connect) for a solo developer. **Multi-LLM** is proven as the N-clients-one-project path — at least two distinct CLIs (e.g. Claude Code + Codex CLI) attach the same slug and connect identically over HTTPS; no per-LLM code path.

Routing (Wave 2): `/v1/{slug}/…` routes to the per-slug store; `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged; per-slug isolation (no cross-project read); register/list/delete lifecycle; N clients on one slug share the store, attributed by session_id (each such client still bound to that single slug — same-project multi-connection allowed, per-client cross-project fan-out is not).

Contracts (both waves): C2 fingerprint format identical server↔client; C3 `UNIMATRIX_PUBLIC_URL` feeds base-url + allowed_hosts + SAN; C4 store access funnels through `resolve_store` with the route shape admitting `/{slug}` additively; cert rotation = re-bundle + re-`init` (documented).

## Constraints

- Dependencies already present (`rcgen 0.13`, `tokio-rustls 0.26`, `rustls-pemfile 2`, `rand 0.9`) — no new server crates for cert-gen/fingerprint. Client + shipped JS stay dependency-free.
- Rust: no `unsafe`, no `.unwrap()` in non-test code, max 500 lines/file (the test-only rcgen helper must be refactored cleanly on promotion).
- No secrets in any DB — token/cert live as files on the data volume, key mode `0600`.
- Distroless runtime has no shell — first-boot provisioning is done by the Rust binary itself, not a shell entrypoint.
- `data_dir = /data/.unimatrix/{hash}` (not `/data`) — cert/token persistence and docs must use the resolved path; the project hash depends on `--project-dir /data`.
- TLS must stay internally terminable (`tls.enabled` seam) for the enterprise proxy path — do not hardcode TLS so the seam disappears.
- `client-bundle`, like `health`/`version`, is a pre-tokio sync subcommand (C-10).
- **Enterprise-extends seams carried from day one** (ass-060 invariants): slug = isolation boundary, `ProjectRouter` resolution seam, per-project DB/hash-chain, `BearerValidator` trait, extensible audit schema (credential_type, capability_used, agent_attribution). The OSS code must hold these so enterprise is additive.
- **Local-repo install parity is non-negotiable** — every personal-cloud change must function unchanged for the local single-project install (UDS, path-hash `data_dir` per ADR-004). The `resolve_store` isolation seam (C4) is ONE mechanism across local and cloud: local resolves the daemon's path-hash store (no slug); cloud resolves by slug. No cloud-only isolation path that the local install does not also exercise — the common local case is the seam's proving ground.
- Test infrastructure is cumulative — extend existing fixtures/helpers.

## Open Questions (resolved + genuinely open for the design)

**Resolved this session (uni-zero), carry as design defaults:**
- **OQ-1/5/7 collapse to C3** — `UNIMATRIX_PUBLIC_URL` (default container hostname/service-name; `--base-url` override; loud `https://<EDIT-ME>:8443` placeholder if unset) feeds the bundle base-url, the `allowed_hosts` default (derive when set; permissive-with-warning when unset), and the cert SAN (generous: `localhost`/`127.0.0.1`/`0.0.0.0` + the public-URL host).
- **HTTP-enable mechanism** — container-scoped (image ships `http.enabled=true` via env/container config + TLS auto-detect of the generated cert); the binary's global default stays `false`. Not a new serve flag if env/config is cleaner.
- **Cert persistence** — `{data_dir}/tls/cert.pem` + `key.pem`, key `0600`, operator certs mountable read-only.
- **Bind-mount UID** — document-only contract (named volume default = zero friction; host bind-mount = documented `chown 65532`); binary fails loud-and-actionable if `/data` is unwritable (no runtime chown — distroless non-root can't).
- **Fingerprint format** — `sha256:<lowercase-hex>` (C2), locked on both serve and client sides.

**Genuinely open — for the design session:**
- **OQ-A — bundle serialization.** JSON blob, base64-of-JSON, or a single URL-with-fragment? Affects copy-paste ergonomics and the client parser. Decide the wire form of C1.
- **OQ-B — slug discovery for attach.** When attaching a client, does the server expose a slug list (an endpoint or `client-bundle --list`), or does the operator simply tell the client the slug out-of-band? Affects the attach UX and whether any unauthenticated/authed listing surface is added.
- **OQ-C — Wave-1 single-project addressing.** Does Wave 1 serve at the `/v1/tools/…` default-project alias, or require a default slug from day one? Determines whether Wave 2 is purely additive or re-points Wave-1 clients.
- **OQ-D — wave-to-issue mapping confirmation.** Confirm Wave 1 = #726+#725 and Wave 2 = #727 (vs. a finer cut the architecture surfaces, e.g. carving the C1/C2 connection-contract into its own first deliverable consumed by both server and client).

## Tracking

GitHub: umbrella **vnc-034** = https://github.com/dug-21/unimatrix/issues/733. Delivery waves map to existing issues: **Wave 1** = #726 (vnc-032, server serving) + #725 (nan-019, client init); **Wave 2** = #727 (vnc-033, multi-project routing) — these become wave-tracking issues under vnc-034, their SCOPEs retained as input. Research basis: ass-060, ass-050, ass-068, ass-069. Advances goal #4934 (`personal-cloud`).
