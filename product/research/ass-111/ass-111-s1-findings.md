# FINDINGS: ass-111 S1 — Tenancy & identity model

**Spike**: ass-111 S1 (campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation (code + ecosystem)
**Confidence**: directional overall. Q4's feasibility claims rest on S6's validated PoC against Claude Code 2.1.332 and OpenCode 2.4.1. Its enforcement design is directional.
**Status**: FINAL. Q4, the H3 note, E3 and R3/R5 have been reconciled with `ass-111-s6-findings.md`.

Unimatrix entries consulted: #5705 (PL-4), #5704 (PL-3), #5734 (trusted-identity goal), #5836 (personal-cloud goal), #5792 (C19), #1839 (security model evolution), #4951 (vnc-034 ADR-004 slug/register/attach), #4745 (vnc-025 ADR-007 session_key seam), #5196 (global-handle config lock), #5348 (bidirectional isolation lesson), #2160 / #2155 (alc-003 identity source), #4362 (vnc-014 ADR-008).

---

## Ground truth: the identity and authorization path today (code-verified)

This is the baseline every answer below builds on. It has changed since ass-100/101 (vnc-050 landed), so those spikes' "15 chokepoints" counts are stale.

| Step | Where | What actually happens |
|---|---|---|
| Transport credential | `http/auth.rs` `BearerValidator`, `http/auth/scoped.rs` `TokenTableValidator` | A legacy 32-byte token or a `tokens.json` scoped token (SHA-256 at rest, constant-time full scan). The result is a `BearerPrincipal::{Legacy, Scoped{label, mode, allowed_slugs}}` plus a `ResolvedIdentity{"http-bearer" or "token:<label>", Restricted}` in request extensions. |
| Project | `http/router/seam.rs` `parse_project_key` → `ProjectKey::Slug` | Comes only from the URL path `/v1/{slug}/…`. It is never in the payload, and an unknown slug returns 404 with no default (vnc-038 ADR-004). |
| Slug access | `http/router/slug_guard.rs` `check` | Runs before `resolve_store`. A scoped token addressing a slug outside `allowed_slugs` gets a 403, and the denial row goes to the token's **home** slug log. **Legacy reaches every slug.** |
| Store | `MultiProjectRouter::resolve_store` / `adapter_for` / `registry_for` / `services_for` | One map. Each slug gets a fully built `UnimatrixServer`: its own store, vector index, session registry and service layer. |
| Capability | `server.rs:637` `require_cap` | Effective = `cap_ceiling` (from the token mode; Legacy and stdio have **no ceiling**) **∩** `registry.require_capability(ctx.agent_id)`. |
| Agent identity | `server.rs:526` `build_context_with_external_identity` | `ctx.agent_id` = the **self-asserted `agent_id` tool param**, resolved through `resolve_or_enroll`, which is fail-open and auto-enrolls with permissive defaults. `external_identity` is `None` at all 25 call sites in `mcp/tools.rs`. |
| Audit | `mcp/tools.rs` tool-call `AuditEvent`s | `agent_id` = self-asserted. `credential_type` is **hard-coded `"none"`** (e.g. `tools.rs:954,1316,1733`). `agent_attribution` = `clientInfo.name`, which the client reports itself. The authenticated principal (`token:<label>`) appears **only** on the per-request `http_access` row. |

**Live consequence (V1, not hypothetical).** Under the legacy token, or over stdio, a caller that passes `agent_id: "human"` resolves to the protected bootstrap agent `human`. That agent holds `Admin` (`unimatrix-store/src/registry.rs:46-60`), so the caller passes the `Admin` gate on `context_quarantine` (`tools.rs:2257`) and `context_enroll` (`tools.rs:2381`). Scoped tokens are safe: their ceiling never contains `Admin`. In a multi-user SaaS this means "any member token plus a typed string = project admin". V2 must remove it (gap G6).

---

## Findings

### Q: "What is the identity hierarchy (tenant → project → scope, plus shared scope) and where does each identifier come from (transport, token claim, attestation — never payload)?"

**Answer**: The hierarchy has five levels. Two of them (tenant and project) are **data boundaries**. One (principal) is the **authentication subject**. Two (client and agent) are **attribution or narrowing** layers. Tenant and project come from the URL. Principal comes from the credential. Role and grants come from server-side state. Agent type comes from attestation, or else stays audit-only. Nothing that confers access is read from the payload.

```
Operator (platform)  — out-of-band admin plane; NOT a tenant role, no MCP surface
 └─ Tenant           — isolation + billing boundary          ← URL segment, verified vs membership
     ├─ Principal    — who authenticated (user | service | workload)  ← credential (cryptographic)
     │    └─ Role / grants (owner|writer|reader; per-project mode)    ← control-plane DB (server)
     ├─ Project      — data/integrity boundary: own store, hash chain, audit, learning, vector
     │    │                                                   ← URL segment
     │    ├─ Client / session — one harness connection; clientInfo; session_id   ← client-reported (attribution only)
     │    │    └─ Agent type  — tiered (T1–T3), narrowing/tier-gate only  ← adapter-signed attestation, else self-asserted (audit-only)
     │    └─ Shared-scope read grants (0..n)                  ← control-plane DB, keyed on the HOME project
     └─ Shared space — a project-shaped store owned by one tenant, with its own chain/audit,
                       readable only by explicitly granted (tenant, project) consumers
                                                              ← addressed by its own URL for writes
```

| Identifier | Source | Trust class | Used for |
|---|---|---|---|
| `tenant` | URL path segment, resolved to an immutable `tenant_id` by the control plane. Access requires principal ∈ tenant. | transport + server state | isolation, routing, billing |
| `project` | URL path segment, resolved to an immutable `project_id` | transport | store selection (`resolve_store`), integrity boundary |
| `principal` | Bearer credential: OAuth access token (`iss`,`sub`,`aud`), scoped-token hash, or a workload token obtained by OIDC exchange | cryptographic | authentication, attribution |
| `role` / project grant | Control-plane membership and grant tables. **Not** a token claim (see rule R4). | server state | authorization ceiling |
| credential ceiling | OAuth `scope`, or token `mode` (`read` / `read-write`) | credential | narrows the role, gates learning (`records_learning` needs Write) |
| shared-scope grants | Control-plane grant table keyed on the home `(tenant, project)` | server state | read fan-out (S3) |
| `client_type` | `clientInfo.name` (legacy) or per-request `_meta` (2026-07-28) | **client-reported** | attribution only |
| `session_id` | Declared by the client. On T2 harnesses, prefer the harness-set `_meta` (`ai.opencode/sessionID`, `claudecode/toolUseId`; S6 P7). | client-reported (T0/T1); harness-set (T2) | attribution and session signals. Must be namespaced by principal (rule R6). |
| `agent_type` | Adapter-signed harness attestation bound to the principal's device key (Q4, tier T2/T3). Otherwise the `agent_id` param (T0). | tiered: attested (T2/T3) or audit-only (T0) | narrowing policy, tier-gated operations, audit |
| attested project claim | Adapter-derived from harness `cwd` / `location`, inside the attestation (T2+) | adapter-signed | integrity cross-check against the URL project (R3); never an addressing input |

**Rules**
- **R1 — Never from the payload.** Tenant, project, principal, role and grants must never come from tool arguments. This generalises vnc-034 ADR-004 (#4951) / FR-X2 "mis-targeting unrepresentable" to the tenant level.
- **R2 — Tenant is checked twice.** The URL tenant must equal one of the principal's memberships, or a service or workload principal's bound tenant. A mismatch returns 403 with a fixed body, whether or not the tenant exists. That keeps the vnc-050 R-03.3 no-enumeration rule.
- **R3 — Project is the integrity boundary, unchanged.** The rule "1 client binds 1 project URL" holds. A per-project grant check is the **server-enforced per-project claim** that #5836 names as the enterprise relaxation. A writer must hold a write grant on exactly the project in the URL.
  - On T2+ harnesses there is a second check. The adapter signs a per-call project claim derived from harness `cwd` (Claude Code) or `location` (OpenCode v2; S6 Q1). A **write** whose attested project differs from the URL project gets a 409 and an audit row.
  - This guards against *misconfiguration*, such as one OpenCode v2 background service hosting several directories, or a wiring file copied between repos. It is not an adversary control: the claim derives from files the LLM can write.
  - The URL stays the only addressing input (R1).
  - "Coordination across project scopes" beyond this claim belongs to the harness, not the knowledge engine (Hard constraint).
- **R4 — Authorization state lives server-side, not in IdP claims.** The token proves *who* the principal is, plus a coarse consent ceiling. Membership, role and project grants are looked up per request from the control plane. This keeps the IdP swappable, avoids stale claims after a revocation, and keeps BearerValidator a pure authentication seam (ass-060 invariant 5).
- **R5 — Agent identity never widens.** Effective capability = role grant ∩ credential ceiling ∩ tier gate ∩ agent-type policy.
  - A project may require tier T2+ (an attested, allowed `agent_type`) for designated operations. Example: writing `decision`.
  - A lower-tier call is denied *those* operations and keeps everything else role ∩ ceiling allows.
  - Agent identity never confers tenant or project access, and tenant isolation never depends on it.
- **R6 — Client-reported fields are attribution, never authorization.** `clientInfo.name` is documented as "Non-spoofable" in `schema.rs:385`. That is true only against the LLM: any token holder can set it. Session keys must be namespaced `(tenant, project, principal, session_id)` through the `session_key()` seam (#4745) so that two principals in one project cannot collide or poison each other's session signals.
- **R7 — Immutable IDs for storage, slugs for routing.** Storage directories, hash-chain attribution and audit rows key on immutable `tenant_id` / `project_id`. Slugs are routing aliases. A deleted slug is tombstoned and never reissued. Otherwise a later tenant that registers a freed slug receives the old tenant's mis-pointed clients' writes (slug-reuse takeover).
- **R8 — Shared space is opt-in and enumerated.** There is no "public to all tenants" shared space. Each grant names a consumer `(tenant, project)`, as the Hard constraint requires. Read/write/learning semantics are S3's.

**Evidence**: `seam.rs` (transport-only `ProjectKey`, single-variant enum kept "so future keys can be added additively"); `principal.rs` (principal → ceiling, `permits_slug`, `home_slug`); `slug_guard.rs`; `server.rs:526-637`; #4951; #4745; #5836 (1:1 integrity boundary, enterprise per-project claim); ass-060 invariants 1–7.

**Recommendation**: Adopt the five-level hierarchy and rules R1–R8 as the V2 identity contract. Treat the URL as the only addressing input and the control plane as the only authorization state. Keep agent identity as a narrowing layer.

---

### Q: "Does the existing `resolve_store` seam and (tenant, project) keying extend to multi-tenant without re-architecture? Where does it break?"

**Answer**: **The seam's contract extends. Its implementation does not.** The contract is a transport-derived key, one funnel, `store`/`adapter`/`registry`/`services` all from the same map, no default, and a guard before the funnel. Adding a tenant dimension is additive: `ProjectKey` was deliberately kept an enum for this. The breaks are in **what sits behind the trait**: a static, eager map built from config at startup; a flat global slug namespace; a single-file token table; and an identity path that authorizes on a self-asserted string. None of these is a re-architecture of the request path. Together they amount to a **new control plane plus a resolver rewrite behind an unchanged funnel shape**.

**Gap list against current seams** (size scale from ass-100: handler-only, schema+handler, transport, architectural)

| # | Seam | Break | Size |
|---|---|---|---|
| G1 | `ProjectKey` / `parse_project_key` | Has no tenant dimension. Fix: add `ProjectKey::Tenant{tenant, project}` and route grammar `/v1/{tenant}/{project}/…`. `/v1/{slug}` stays V1-only. | schema+handler (additive variant, as designed) |
| G2 | `MultiProjectRouter` (eager `HashMap`, built at startup from `[[projects]]`; `projects.rs:17` "restart-applied") | Self-serve signup cannot restart the server. Needs a **dynamic resolver** with control-plane lookup, lazy open and eviction. | architectural (resolver internals). The funnel is unchanged. |
| G3 | `StoreResolver` trait | `adapter_for` returns `Option<&McpAdapter>`, a borrow from a static map, and the contract says "O(1) … no I/O". Lazy open and eviction need an owned `Arc` return and an async or I/O-capable resolve. This is a trait signature re-cut at one call site (`SlugRouter::route_mcp`). | schema+handler |
| G4 | Per-slug `UnimatrixServer` (own vector index, caches, tick) | Memory and tick cost grow linearly with N projects. Global models are Arc-shared (#5196). Per-tenant hot-path policy belongs to **S2**. | → S2 |
| G5 | `tokens.json` (`token_table.rs`: one file, stat-stamp reload, O(N) constant-time scan, no tenant field, CLI-written) | Does not scale to thousands of tenant-self-managed tokens. Move token records (hash, label, tenant, project grants, mode, TTL) into the control-plane DB. Per-token constant-time comparison remains, using a hash-prefix index followed by a `ct_eq` confirm. | schema+handler |
| G6 | `require_cap` → `registry.require_capability(self-asserted agent_id)` | Authorization rests on a forgeable string. The `agent_id:"human"` → Admin path is live under legacy/stdio (see Ground truth). Admin must come from **principal role**, and the agent registry becomes an *agent-type policy* table that is consulted only when attested. | architectural (the ass-100 Q5 gate rewrite; PL-4 #5705) |
| G7 | `BearerPrincipal::{Legacy, Scoped}` | Needs `User{tenant_id, iss, sub}`, `Service{tenant_id, token_id}` and `Workload{tenant_id, trust_policy_id}` variants. `permits_slug(slug)` → `permits(tenant, project, mode)` via the control plane. `BearerValidator` was built for this extension (FR-14). | schema+handler |
| G8 | Legacy token (reaches every slug, no ceiling) | Has no place in multi-tenant. Remove it from the tenant data plane. Operator break-glass moves to a separate admin plane. | handler-only (`legacy_token_enabled=false` already exists, vnc-050 FR-11) |
| G9 | Tool-call audit attribution | `credential_type:"none"` and a self-asserted `agent_id` on every write row. `created_by` is self-asserted. Under many principals per project, Principle 2's "full attribution" needs a `principal_id` (and `credential_type`) on every audit row and entry, separate from `agent_id`. Additive columns (ass-060 invariant 4). | schema+handler |
| G10 | Denial audit sink (`home_slug`) | A cross-tenant probe by a principal with no project in the target tenant has no home log. Denials go to the **principal's own tenant** log plus a **platform audit log**, never the target's (extends ADR-002 #5806). | schema+handler |
| G11 | Control-plane state changes (members, grants, tokens, trust policies, shared grants) | Have no append-only audit today because the state does not exist yet. Principle 2 applies: they need a control-plane audit log enforced by triggers. | schema+handler (new store; engine → S2) |
| G12 | `session_key()` seam (#4745, degenerate today) | Must actually encode `(tenant, project, principal, session_id)` (rule R6). | handler-only (the single function was designed for this) |
| G13 | `ProjectConfigEntry` is slug-only; config overlay is per slug | Tenant-facing config is S5 Q4. Global-handle sections stay locked (#5196). | → S5 |
| G14 | `client_type_map` keyed on `Mcp-Session-Id` | Breaks under stateless 2026-07-28 (ass-107). Independent of tenancy but lands in the same release window. | schema+handler (ass-107 E) |
| G15 | `external_identity` dormant (25 × `None`); params structs drop unknown fields (no `deny_unknown_fields`, no `flatten`; ass-100 Q1, S6) | The ingress point for attested agent identity (Q4). An args-borne attestation is **silently discarded** at typed deserialization today. Extract it from the raw `arguments` (pre-deserialization capture) or from request `_meta`, strip it before any handler sees the arguments or anything is stored, then thread the verified identity as `Some`. | schema+handler. The gate change is G6. |
| G17 | No device-key or replay state | T2 needs three new pieces of state: a control-plane **device-key registry** (Ed25519 public key → principal, revocable), a per-project **single-use call-id cache** sized to the assertion TTL, and an Owner-declared **agent-type policy** table (reusing the unenforced `agent_registry.allowed_categories` / `allowed_topics` columns). | schema+handler (cache sizing → S2) |
| G16 | Local STDIO/UDS bypass path (ADR-006 #5087) | Goes away if H1 holds. Remove it, or keep it only as a dev mode. It must not stay a second identity path into SaaS. | handler-only (deletion) |

**Where it does *not* break**: per-project store, hash chain, audit and analytics isolation (ass-060 invariants 1–3); the guard-before-funnel ordering; the no-default rule; observe and MCP sharing one funnel (vnc-046); the token ceiling as the learning gate (`cap_ceiling::records_learning`); the slug allowlist parse edge (reused for tenant slugs).

**Evidence**: `seam.rs:52-60` (`ProjectKey` "kept as a single-variant enum … so future keys can be added additively"); `seam.rs:131-188` (trait, "O(1) … no I/O"); `project_resolver.rs` (eager map; "register/delete restart the server"); `projects.rs:17,458`; `token_table.rs`; `auth/scoped.rs`; `server.rs:526-655`; `registry.rs` (infra and store bootstrap); `tools.rs:954,1316,2257,2381`; #4745; #5196. Lesson #5348: any tenant-isolation gate must be **bidirectional** (A↛B *and* B↛A).

**Recommendation**: Keep the funnel and its no-default and guard-first discipline as-is. Add `ProjectKey::Tenant` (G1). Re-cut `StoreResolver` for owned, lazy resolution (G3). Rewrite the resolver as a control-plane-backed dynamic registry (G2). Close G6/G8/G9 **before** a second tenant exists. They are the only items where a multi-user tenant turns a V1 nuisance into a privilege-escalation or attribution-integrity defect. Ship a bidirectional cross-tenant isolation gate (#5347/#5348 shape) as the first V2 proof.

---

### Q: "What authentication mix is required: OAuth 2.1 (interactive + claude.ai connector), scoped bearer tokens, headless/CI (GitHub Actions) credentials, public-chat access? What is the minimum role model?"

**Answer**: Three credential families, all ending in the same principal → role → ceiling path:
1. **OAuth 2.1** for humans and interactive harnesses, including the claude.ai connector. Unimatrix is a **resource server**.
2. **OIDC workload federation** for CI. A GitHub Actions OIDC token is exchanged for a short-lived Unimatrix token, so no stored secret is needed.
3. **Scoped bearer tokens** (vnc-050 shape, moved into the control plane) for headless clients without OIDC and for application backends, Med Search's public chat included.

**Public chat gets no Unimatrix credential.** End users talk to Med Search's backend. The backend holds a `read`-mode service token, so "every port authenticated" holds and read mode does not feed learning (`records_learning` requires Write).

**Auth-method matrix per client class**

| Client class | Credential | Obtained by | Principal | Ceiling | Agent tier (S6) | Learning | Notes |
|---|---|---|---|---|---|---|---|
| Interactive harness: Claude Code, OpenCode | OAuth 2.1 access token with refresh rotation, **plus** a per-machine Ed25519 device key for attestation | MCP-spec flow: PRM (RFC 9728) → AS metadata → CIMD (or DCR) → auth code + PKCE S256 + `resource` (RFC 8707). Device public key enrolled over the authenticated session. | `User` | scope `read` / `read-write` ∩ role | **T2** (T3 target) | if Write | One login should serve MCP **and** the hook/observe channel. See Q5: the Node client holds the token and proxies MCP through the existing `mcp-bridge`. |
| Interactive harness: Codex | OAuth 2.1 | same | `User` | same | **T1** | if Write | `PreToolUse` carries no per-call agent identity (S6 Q2). Full retrieval and observation, but no T2-gated operations. |
| claude.ai connector / cloud routines | OAuth 2.1 | Same flow. Callback `https://claude.ai/api/mcp/auth_callback`. Claude picks CIMD only if the AS advertises `client_id_metadata_document_supported` plus `none` auth; otherwise it uses DCR. | `User` | same | **T1** | if Write | The AS **must offer DCR as well as CIMD**: the spec deprecates DCR (2026-07-28), but claude.ai still falls back to it. No hooks or adapter, so retrieval-plus-write only (S4). |
| CI (GitHub Actions) | Short-lived Unimatrix token (≤1h) | Job requests a GH OIDC token (`permissions: id-token: write`, `audience=<Unimatrix canonical URI>`) and exchanges it at a Unimatrix token-exchange endpoint (RFC 8693). The server verifies it against GitHub's JWKS (`iss=https://token.actions.githubusercontent.com`) and matches a per-project **trust policy** on `repository_id` / `repository_owner_id` plus `ref` or `environment`. | `Workload` | policy mode | **T1** (the workload *is* the identity) | if Write | Match on **immutable IDs**, not names (GitHub moved new repos to an ID-based `sub` on 2026-07-15). Removes the "write-everything secret in CI" problem C19 names (#5792). A harness running inside CI can add T2 with an ephemeral device key. |
| Other automation (cloud cron, Cloudflare site, non-OIDC CI) | Scoped bearer token | Created by the tenant owner in UI or CLI. Shown once, hashed at rest, optional TTL. | `Service` | token mode | T1 | if Write | vnc-050 semantics unchanged. Never `Admin`. |
| App backend for public chat (Med Search) | Scoped bearer token, `read` | as above | `Service` | `read` | T1 | **no** | The public web user is anonymous to Med Search and invisible to Unimatrix. Rate limits and abuse controls are Med Search's job. Unimatrix sees one service principal. Signal isolation details → S4. |
| Operator | Separate admin plane, not the MCP data plane | Out-of-band (SSO plus hardware key) | Operator | platform | n/a | n/a | Replaces the legacy god-token (G8). |

**Spec obligations the resource server must meet** (MCP 2025-11-25 and 2026-07-28 authorization):
- Serve PRM.
- Return 401 with `WWW-Authenticate: Bearer resource_metadata=…, scope=…`.
- Validate audience: the token must be issued for this server (RFC 8707).
- Return 403 `insufficient_scope` for step-up.
- Never pass tokens through.
- Treat stdio as out of scope: the spec says stdio SHOULD NOT use OAuth and should take credentials from the environment.

**Authorization server: buy, don't build.** A hosted IdP or AS that supports CIMD and DCR, with GitHub/Google social login, keeps client secrets and signing keys out of Unimatrix databases (Principle 8) and off the solo operator's security surface. CIMD support is GA in Clerk, experimental in Keycloak, and reported (secondhand) for WorkOS and Auth0. The only token Unimatrix itself mints is the CI exchange token. Unimatrix's validator accepts two issuers: the IdP for users, and the Unimatrix exchange endpoint for workloads. Vendor choice is escalated (E1).

**Minimum role model** (tenant-scoped; project grants narrow it)

| Role | Projects | Capabilities | Admin (quarantine, enroll/agent policy) | Control plane |
|---|---|---|---|---|
| **Owner** | all in tenant | Read, Search, Write, SessionWrite | yes | members, projects (register/delete), tokens, CI trust policies, shared-space grants, export/offboard |
| **Writer** | granted projects | Read, Search, Write, SessionWrite | no | none |
| **Reader** | granted projects | Read, Search | no | none |
| Service / Workload principal | bound project(s) | per mode (`read` / `read-write`) | **never** | none |

Effective = role (or principal mode) ∩ credential scope ∩ agent-type policy (when attested and enforced). `Admin` is granted only by a human Owner role, never by a token or workload. That keeps the vnc-050 rule "never grants Admin" and closes G6. Personal tenants (one user) are just Owner-only tenants.

**H2 challenge**:
- **Accept** "HTTPS is the only *server* transport". UDS and stdio servers retire under H1.
- **Reject** "only transport" for the client side. Keep the local stdio→HTTPS bridge (`mcp-bridge`) as a **client adapter**. It is the place to hold one OAuth credential for both MCP and observe, to support harnesses with weak remote-MCP OAuth, and optionally to carry attestation out of the args. S6 showed that in-harness args mutation already works on Claude Code and OpenCode, so the ass-106 proxy channel (ii) is not required. It is not a server transport and adds no server attack surface.
- **Reframe** "OAuth alongside bearer". In SaaS, OAuth is the *primary* human path and scoped bearer tokens are for non-human principals only. A long-lived bearer token for a human is a regression from short-lived, rotated OAuth tokens.

**Evidence**: MCP authorization spec 2025-11-25 and 2026-07-28 (fetched); Claude connector auth docs and support article (callback, CIMD/DCR selection); GitHub Actions OIDC docs (issuer, claims, immutable `sub`); IdP CIMD status (Clerk changelog, Keycloak docs); `principal.rs` (`TokenMode` table, never-Admin); `cap_ceiling.rs` (`records_learning`); ass-107 D (connector path = the OAuth SEPs; design against rmcp 3.x); #1839 (three-layer model: authn / authz / personalization kept separate).

**Recommendation**:
- Use OAuth 2.1 (hosted AS, CIMD + DCR) for humans and connectors.
- Use RFC 8693 exchange of GitHub OIDC tokens, matched on immutable repo IDs, for CI.
- Use control-plane scoped tokens for every other non-human client, with public chat behind the app backend on a `read` token.
- Adopt the Owner / Writer / Reader + service-principal role model, with Admin restricted to human Owners.

---

### Q: "Can agent identity become non-forgeable (harness-attested) for at least one harness, and what does capability enforcement look like when it is (PL-4 fail-closed)?"

**Answer**: **Harness-attested: yes, on two harnesses today. Non-forgeable: not yet, on any harness.**

- **Attested (S6, validated on real binaries).** Per-call harness attestation works on **Claude Code 2.1.332** (`PreToolUse` `agent_type` plus `updatedInput`) and on **OpenCode 2.4.1** (`execute.before.agent` plus mutable `input`, including Code Mode).
  - On both, it defeats in-band forgery: an LLM-typed `agent_id`, forged attestation fields, unknown, primary or denied subagent types, and replay of a signature the LLM has seen. The attestation is never echoed into the LLM's history (S6 P1–P7).
  - Codex is **T1**: `PreToolUse` carries no per-call agent identity.
- **Not non-forgeable.** S6 proved three limits that hold even against the LLM:
  - **L1 — the LLM can author attested agents.** An LLM with file-write access can create an agent definition, and the harness then attests it as genuine. OpenCode does this in the same session; Claude Code from the next session, under `bypassPermissions`.
  - **L2 — the LLM can read the signing secret.** An OpenCode shell can read it, because there is no sandbox and the shell runs as the same uid.
  - **L3 — the primary LLM can borrow privileges.** It can pick any subagent type its spawn permissions allow.

This **corrects my draft's definition** ("non-forgeable versus the LLM, not versus the user"). That property holds only at **T3**: confinement plus definition pinning. T3 is a design target and unvalidated. It has a documented path on Claude Code (Bash sandbox `denyRead` and an egress allowlist, with hooks running outside the sandbox) and none inside OpenCode. **No tier is non-forgeable versus the tenant's own user or machine.**

**Identity tiers.** I adopt S6's T0–T3 model with one amendment. In SaaS every call is authenticated, so **T1 is the floor for authorization**. T0 is an annotation (the self-asserted `agent_id`), never a tier any call runs at.

| Tier | Agent identity on the call | Defeats | Harnesses today |
|---|---|---|---|
| T0 (annotation) | self-asserted `agent_id`, audit only | nothing | all |
| **T1** (floor) | none beyond the authenticated principal + project grant + ceiling (Q1–Q3) | cross-tenant / cross-project access, wrong-scope tokens | all MCP clients, claude.ai, CI, services |
| **T2** | T1 + adapter-signed harness `agent_type` + harness session + per-call binding | in-band forgery: hallucinated or injected `agent_id`, forged fields, replay, unknown types | **Claude Code, OpenCode (validated)** |
| T3 | T2 + definition hash pinned server-side + LLM tool surface confined (no key read, no direct egress to Unimatrix) | L1, L2. L3 remains a spawn-permission policy matter. | Claude Code (sandbox + managed settings, unvalidated); OpenCode only inside an external container |

**Attestation contract (harness-neutral, Unimatrix-owned format)**
- **Key.** A **per-machine Ed25519 device key** generated by `init` and stored 0600 outside the workspace (OS keychain where available). Only the **public key** is registered to the principal, through the OAuth-authenticated session (device enrolment), so Principle 8 holds. Devices are listed and revocable per principal (G17).
  - This replaces my draft's generic "edge key" and ass-101's operator-pasted key.
  - Never a shared HMAC secret in the environment: that is exactly what L2 broke.
- **Assertion.** Signed by the adapter (the OpenCode plugin or a Claude Code `PreToolUse` hook) per call. Fields:
  - `agent_type` (harness-supplied);
  - harness session id (`_meta` `ai.opencode/sessionID` / `claudecode/toolUseId`);
  - single-use call id;
  - SHA-256 of the canonicalised tool arguments (with the attestation removed);
  - `aud` = canonical project URI;
  - attested project claim from `cwd` / `location` (R3);
  - `device_key_id`;
  - `exp` ≤ 5 min;
  - at T3, a definition hash (OpenCode can compute it in process, S6 P8; Claude Code's hook hashes the file).
- **Carrier.** Request `_meta` (`io.unimatrix/attest`) where the harness lets the adapter set it. Otherwise a reserved top-level args field, which works today on both harnesses.
  - The server captures it **before typed deserialization**, strips it from the arguments, and never stores it in content or audit detail (G15).
  - Params structs must stay free of `deny_unknown_fields` so non-T2 clients are unaffected.
- **Binding.** Verification passes only if all of these hold:
  - the device key belongs to the **same principal** as the request's bearer credential;
  - `aud` equals the URL project;
  - the args hash matches;
  - the call id is unseen.

  A stolen assertion without the bearer, an assertion moved to another call, or one presented by another principal all fail.

**Enforcement shape (PL-4).** I reconcile my draft (reject unattested calls) with S6 (downgrade them), and recommend **"downgrade absent, reject invalid, gate privilege on tier"**:

| Call state | Server action | Why |
|---|---|---|
| No attestation | Runs at **T1**. Tier-gated operations get 403 `insufficient_tier` with a clear message; everything else follows role ∩ ceiling. Never auto-enrolled, never given a registry identity from `agent_id`. | Keeps ubiquity: Codex, claude.ai, CI, and a Claude Code call whose `updatedInput` was dropped by auto mode. Fail-closed on **privilege**, not on access. |
| Attestation present but invalid (bad signature, expired, replayed, args-hash or `aud` mismatch, revoked device, key not bound to the principal) | **Reject** (401) before `build_context`, with an audit row in the principal's tenant log | A present-but-bad attestation signals tampering or a broken adapter. Silently downgrading it would hide attacks. Matches PL-4's "rejected before context build". |
| Valid T2 (or T3) | `agent_type` is matched against Owner-declared policy (G17). An unknown type is treated as T1, never enrolled. Policy narrows, and gated operations are allowed if the type is listed. | Meets R5: agent identity never widens beyond role ∩ ceiling. |
| Adapter failure (cannot sign) | The adapter **fails open**: the call is sent unattested, so it downgrades to T1. | Observation and retrieval never block the harness (S6). |

- **Policy surface.** Per project, the Owner declares:
  - which operations or categories require T2+ (default none, which is byte-identical to T1 behaviour; this preserves PL-4's "flag OFF ⇒ today");
  - a per-`agent_type` capability subset and category write-allowlist, in the existing unenforced `agent_registry.allowed_categories` / `allowed_topics` columns;
  - optionally, a "require T3" setting once confinement is validated.

  This replaces my draft's single `enforce_agent_identity` reject flag. Under the reject model, Codex and claude.ai would lose all access to any enforcing project, a cost with no security gain over tier-gating.
- **Value statement.** T2 policy stops *accidental, hallucinated and injection-driven* role misuse. It does **not** stop a determined primary LLM that uses L1, L2 or L3. Claims made by the `trusted-identity` goal must be worded per tier.
- **Audit.** Record `attestation_tier`, `attested_agent_type`, `device_key_id` and `principal_id` in columns separate from the self-asserted `agent_id` and from `agent_attribution` (ADR-007 non-conflation; ass-100 Q2; G9).
- **PL-4 `done_when` (#5705) needs amending.** "Absent identity rejected" becomes "absent ⇒ T1, never auto-enrolled; invalid ⇒ rejected before context build". This is for the capability owner or architect, not this spike.

**H3 note**: Reject H3 as framed (agrees with S6). Claude Code is at T2 parity with OpenCode, and is the only harness with a documented T3 path. Giving up ubiquity buys no identity. Tenant isolation, project integrity, connector and CI access need no harness depth at all. Keep one harness-neutral contract, with Claude Code and OpenCode as the first T2 adapters, OpenCode pinned and CI-verified (S6 Q3). On cross-project coordination, the only engine-side piece is the attested per-call project claim (R3). Anything beyond it is harness coordination and stays outside Unimatrix.

**Evidence**: S6 verdict P1–P8, L1–L3 and tier table (`ass-111-s6-findings.md`); ass-100 Q1 (unknown args silently dropped), Q5 (hollow gate, fail-open), Q8 (delegation smaller); ass-101 Q3 (pinned asymmetric key, expiry as revocation), Q5 (verify once at the auth layer), Q6 (fail-closed at the auth layer); #5705 PL-4; #5734 trusted-identity goal; code: `server.rs:526-560` (`external_identity` override), `tools.rs` (25 × `None`), store `registry.rs` (`allowed_categories` / `allowed_topics` columns).

**Recommendation**:
- Adopt the amended T1–T3 tiers (T0 as annotation only) and the harness-neutral Ed25519 device-key attestation contract.
- Enforce as "downgrade absent / reject invalid / tier-gate privileged operations", with the server fail-closed on privilege and the adapter fail-open.
- Ship T2 on Claude Code and OpenCode, with Codex and connectors at T1.
- Pursue T3 (managed sandbox plus definition pinning) Claude Code-first, before any goal claims "non-forgeable".

---

### Q: "What is the tenant onboarding flow (signup → project → client attach) at minimum friction?"

**Answer**: One browser login, one command per repo, and zero secrets in repo files. Register and attach stay distinct (#4951). The difference from V1 is that register becomes an explicit, Owner-authorised, self-serve action instead of operator CLI plus restart. Cert pinning, out-of-band slug communication and `client-bundle` hand-offs disappear for SaaS: a CA-issued certificate replaces pinning, and the authenticated API replaces out-of-band discovery.

**Flow sketch**
1. **Signup** (web, about 30 s): "Sign in with GitHub" (hosted IdP) auto-creates a **personal tenant** (handle = GitHub login, tombstoned on deletion) with the user as Owner. Org tenants come later, through invites.
2. **Project** (CLI, in the repo): `npx unimatrix init`. It opens a browser (loopback OAuth plus PKCE; device flow RFC 8628 for headless or SSH boxes), lists the user's tenants and projects, and offers "create project `<repo-name>`". That is an explicit confirmation, and Owner-only. The server registers the project in the control plane **without a restart** (G2). A V1 export can be imported at this step (S7).
3. **Client attach** (same command): writes portable harness wiring (URL `https://<host>/v1/{tenant}/{project}`, no secrets, no absolute paths; nan-024 rule from #5836) for each detected harness (Claude, Codex, OpenCode). It stores the refresh token in the OS keychain, or `~/.unimatrix/…` at 0600. The Node client serves both the MCP bridge and observe from that one credential. On Claude Code and OpenCode, the same command generates the per-machine Ed25519 device key, enrols its public half over the authenticated session (T2, Q4), and installs the attestation adapter (a `PreToolUse` hook or a plugin). A second machine runs `init`, attaches, and enrols its own device key.
4. **claude.ai**: paste the project URL into Custom Connector and approve consent. Nothing else.
5. **CI**: in project settings, "Trust GitHub repo" picks the repo (stored as `repository_id`), branch or environment, and mode. The workflow adds `permissions: id-token: write` and a single `npx unimatrix ci-auth` step. No secret is stored.
6. **App backend / public chat**: in project settings, "Create read token". It is shown once and goes into the app's secret manager.

**What gets removed from V1**: `unimatrix project register` + restart; `client-bundle` + fingerprint pinning; out-of-band slug communication; the operator-held legacy token; `tokens.json` CLI editing.

**Evidence**: #5836 (register vs attach, no machine paths in host config, client resolves credentials at runtime); #4951 (no listing surface in OSS, which an authenticated SaaS API reverses safely); `projects.rs` (restart-applied registration); ass-109 Q3/Q6 (per-harness local overlays for machine-specific values); GitHub OIDC docs; Claude connector docs.

**Recommendation**: Ship the six-step flow above. Its prerequisites are G2 (dynamic registration), G5/G7 (control-plane principals and tokens), the hosted AS, and a Node-client OAuth path that shares one credential between MCP and observe.

---

## Escalations for human

- **E1 — Authorization server: buy vs build, and which vendor.** I recommend a hosted IdP/AS with CIMD + DCR + GitHub login (Clerk, WorkOS, Auth0, or self-hosted Keycloak, where CIMD is still experimental). This is a cost and strategic-dependency decision for a solo operator, so it needs a human call.
- **E2 — Personal tenant auto-created at signup** (recommended) vs org-first tenants. This affects billing and the shape of Med Search's tenant.
- **E3 — Required agent-identity tier for V2, and what `trusted-identity` may claim.** I recommend that V2 launch per-agent-type policy at **T2**: it resists in-band forgery and is validated on Claude Code and OpenCode. T3 (confined plus definition-pinned) would follow, Claude Code-first. The goal text should say "harness-attested (tiered)", **not** "non-forgeable", until T3 is validated, because L1–L3 show T2 is forgeable by an LLM that has file-write or shell access.
  - Requiring T3 at launch means mandating a managed sandbox profile for Claude Code and an external container for OpenCode.
  - Who pins agent-definition hashes (Unimatrix-distributed definitions, H6/S5, or harness config) is part of the same decision.
  - This overlaps S6 escalations 2 and 3; synthesis should merge them.
- **E4 — Public chat never receives a Unimatrix credential.** Med Search's backend holds a `read` service token. This confirms the Med Search product boundary: Unimatrix is not exposed to anonymous web users.
- **E5 — Retire the legacy all-slug token in V2.** Operator access moves to a separate admin plane.

---

## Unanswered Questions

- **T3 confinement efficacy.** Not tested: whether Claude Code's sandbox (`denyRead` on the device-key path plus an egress allowlist that excludes the Unimatrix host) stops a shell-capable LLM while the hook keeps working. Reason: needs a sandbox PoC (S6 unanswered; bubblewrap may be missing from the devcontainer). It gates any "non-forgeable" claim.
- **Attestation over remote HTTPS MCP.** Validated only over stdio. Reason: S6 PoC scope. The carrier is transport-agnostic, but the `_meta` and args paths through remote streamable HTTP and the rmcp 3.x upgrade (ass-107) need an integration test.
- **Codex per-call identity.** Codex stays at T1 until a live run shows per-call agent identity and `updatedInput` on MCP tools. Reason: no Codex binary was available to S6.
- **Single-use call-id cache sizing and placement under multi-tenant load (G17).** Reason: S2 scope (hot-path state per tenant).
- **PL-4 `done_when` amendment** (absent ⇒ T1, invalid ⇒ reject). Reason: owned by the capability owner or architect, not this spike.
- **Per-tenant hot-path cost of G2/G4 (lazy open and eviction).** Not measured here. Reason: this is S2's scope.
- **Shared-space read, write and learning semantics** beyond the identity rules R3/R8. Reason: this is S3's scope.
- **The exact claude.ai behaviour against a CIMD-only AS.** The docs and user reports disagree. Reason: external behaviour that needs a live test; ass-108 (#1014) was never written and should be folded into V2 auth design.

---

## Out-of-Scope Discoveries

- **The live `agent_id:"human"` → Admin path in V1.** Any legacy-token or stdio caller passes the `Admin` gate (`context_quarantine`, `context_enroll`) by typing `agent_id: "human"`. Today there is one user, so the risk is low. It is still a Principle 3 violation and worth a V1 bugfix if V1 is maintained (S7 decides V1's disposition).
- **The `agent_attribution` doc comment** (`unimatrix-store/src/schema.rs:385`) calls `clientInfo.name` "Non-spoofable". It is client-reported. A future consumer could mistake it for an authorization input, so the comment should be corrected.
- **Tool-call audit rows hard-code `credential_type: "none"`** (`tools.rs:954` and others) even on bearer-authenticated HTTP calls. ass-060 invariant 4 ("Audit log carries credential_type") is not met in V1 today.
- **`agent_registry.allowed_topics` / `allowed_categories` columns are already in the schema but unenforced.** They are a ready home for per-agent-type policy. Pattern candidate after validation.
- **Slug-reuse takeover (R7)** applies to V1 as well. `project delete` followed by re-`register` of the same slug silently receives stale clients' writes. Minor while V1 has one operator.

---

## Recommendations Summary

- **Q1 (hierarchy)**: Operator ▸ Tenant ▸ {Principal → Role} ▸ Project ▸ Client/Session ▸ Agent type, plus enumerated shared spaces. Tenant and project come from the URL, principal from the credential, role and grants from the control-plane DB, and agent type from attestation or audit-only. Never the payload. Rules R1–R8.
- **Q2 (seam)**: The `resolve_store` contract extends: add `ProjectKey::Tenant`. Behind it, replace the static map with a control-plane-backed dynamic resolver and re-cut the trait for owned, lazy results. Close G6 (Admin from a self-asserted `agent_id`), G8 (legacy token) and G9 (principal on every audit row) before a second tenant exists. Prove it first with a bidirectional cross-tenant isolation gate.
- **Q3 (auth and roles)**: OAuth 2.1 for humans and connectors (Unimatrix as resource server; hosted AS with CIMD + DCR). GitHub OIDC → RFC 8693 short-lived token for CI, matched on immutable repo IDs. Control-plane scoped tokens for other automation. Public chat sits behind the app backend on a `read` token, so it neither writes nor learns. Roles: Owner / Writer / Reader + service principals, with Admin for human Owners only. H2: HTTPS-only server transport accepted; the client-side stdio bridge is kept; OAuth is primary for humans.
- **Q4 (agent identity)**:
  - Harness attestation (T2) is validated on Claude Code and OpenCode; Codex and connectors are T1. Nothing is non-forgeable yet: L1–L3 break T2 for an LLM with file-write or shell access, so that claim waits for T3 (confinement plus definition pinning).
  - Adopt T1–T3 (T0 as annotation only) and a harness-neutral attestation contract. It uses a per-machine Ed25519 device key with only the public half registered to the principal. The signed assertion covers agent type, session, single-use call id, args hash, `aud` = project and project claim. It is extracted from `_meta` or raw args and stripped before deserialization.
  - PL-4 enforcement: **downgrade absent, reject invalid, tier-gate privileged operations.** The server fails closed on privilege; the adapter fails open. Policy lives on the existing `allowed_categories` columns.
  - H3: reject as framed and keep ubiquity.
- **R3 addendum**: on T2 harnesses, writes must carry an attested project claim (from `cwd` / `location`) that equals the URL project. This is a misconfiguration guard, not an addressing input. Coordination beyond it stays in the harness.
- **Q5 (onboarding)**: GitHub login → personal tenant → `npx unimatrix init` (browser OAuth, explicit Owner-only project create with no restart, portable wiring, one credential for MCP and observe) → paste the URL into claude.ai → "Trust GitHub repo" for CI → "Create read token" for app backends.

---

## Sources

- [MCP Authorization spec 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization)
- [MCP Authorization spec 2026-07-28](https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization)
- [Claude connectors: authentication](https://www.claude.com/docs/connectors/building/authentication)
- [Claude support: custom connectors (callback, DCR, custom client ID)](https://support.claude.com/en/articles/11503834)
- [GitHub Actions: OpenID Connect](https://docs.github.com/en/actions/concepts/security/openid-connect)
- [Clerk: CIMD GA](https://clerk.com/changelog/2026-09-17-client-id-metadata-documents-ga)
- [Keycloak: MCP authorization server](https://www.keycloak.org/securing-apps/mcp-authz-server)
- [WorkOS: CIMD for MCP](https://workos.com/blog/client-id-metadata-documents-cimd-oauth-client-registration-mcp)
- [Auth0: CIMD vs DCR](https://auth0.com/blog/cimd-vs-dcr-mcp-registration/)
