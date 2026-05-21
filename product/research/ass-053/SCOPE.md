# ASS-053: REST API Connectivity + Admin Plane Decoupling Seams

**Date**: 2026-04-22
**Tier**: 1 — informs W2-3 delivery scope and enterprise extension surface
**Feeds**: W2-3 (security model), vnc-014 (HTTPS transport delivery), enterprise private repo (admin plane)
**Related**: ASS-050 (security model foundation — read first), ASS-041 (transport + auth stack)

---

## Question

ASS-050 specified the security model for Unimatrix's MCP-over-HTTPS transport: `BearerValidator` tower middleware, `StaticTokenAuth` OSS impl, `ResolvedIdentity` capability model. That model was designed around the MCP protocol's `initialize` handshake — which provides `clientInfo.name` for attribution and an rmcp-assigned session UUID for audit linkage.

Two use cases require the security model to extend beyond MCP:

**REST API data plane**: Clients that cannot or do not want to use an MCP client should be able to access the same 12 Unimatrix capabilities through a conventional REST API. Same bearer token auth, same capability model, same audit requirements — but no MCP `initialize` handshake. This changes the attribution story (`clientInfo.name` is not available) and the session linkage story (no rmcp session UUID). The question is whether the ASS-050 model extends cleanly, and what gaps must be filled.

**Admin plane decoupling**: A future admin application (enrollment management, cross-repo activity visibility, context administration) will operate on Unimatrix from a separate client — not through the data plane MCP or REST interface. Port 8444 is already reserved as the admin port. The question is: what seams must the OSS codebase expose now so the enterprise admin application can plug in later without requiring changes to OSS code? The admin application itself is not in scope — only the decoupling surface.

This spike answers both questions and produces the interface specifications that gate vnc-014 delivery scoping.

---

## Why It Matters

If the REST API data plane is not designed now, the first developer who tries to use Unimatrix from a non-MCP client (a Python script, a CI pipeline, a non-Claude LLM without an MCP runtime) hits a hard wall. The bearer token model from ASS-050 is conceptually the right answer, but the audit model is broken without `clientInfo.name` and the rmcp session UUID — two sources that only exist on the MCP path.

If the admin plane seam is not specified before W2-3 ships, the extension point gets closed by implementation choices. Port 8444 is reserved but has no defined interface. If W2-3 wires up the HTTPS listener without an admin plane hook, adding one later means modifying the OSS server startup — exactly the kind of OSS change that should not be required for enterprise features.

Both seams are cheapest to design before W2-3 is implemented. They are expensive to retrofit after.

---

## Constraints

**Hard** (technically fixed):
- The OSS REST API must use the same `BearerValidator` trait specified in ASS-050. No new auth abstraction.
- The `ResolvedIdentity` + `require_cap()` model is the capability gate for both paths. Tool handlers do not change.
- Port 8444 is the admin port. This is fixed by W2-2 / ASS-041.
- Admin application design is out of scope. This spike designs the seam, not the application.
- Every proposed OSS change must be classified: additive, non-breaking modification, or breaking with blast radius.

**Hypothesis** (design positions, subject to challenge):
- The REST API and MCP path share the same `BearerValidator` middleware — the auth layer does not need to know which transport it is protecting.
- The admin plane credential is separate from the data plane token — a separate secret or JWT audience, not the same bearer token.
- Cross-repo admin operations require a separate store handle (or store registry) that the admin plane receives at startup injection — not the single-repo store the data plane holds.

---

## What to Explore

### 1. REST API Data Plane — Security Model

The REST API exposes the same operations as the MCP tool surface via conventional HTTP endpoints. Security must be equivalent.

**Auth**: The same `BearerValidator` middleware from ASS-050 applies without modification — it validates `Authorization: Bearer <token>` on any HTTP request regardless of protocol. Confirm this by tracing the ASS-050 middleware placement in `main.rs` and verifying it sits above the transport layer, not inside the MCP handler.

**Attribution without `clientInfo.name`**: On the MCP path, `agent_attribution` is populated from `ctx.peer.peer_info()` (OQ-01, confirmed). On a REST API path there is no MCP handshake and no `peer_info`. Answer:
- What replaces `clientInfo.name` as the non-spoofable attribution source on the REST path?
- Can the API key ID (SHA-256 fingerprint of the bearer token, already computed by `StaticTokenAuth`) serve as the attribution anchor? Or should the REST path require clients to supply an `X-Agent-Id` header (optional, informational — same as `agent_id` tool param on the MCP path)?
- What does `agent_attribution` contain in the audit log for a REST API call? Define the value explicitly for each case: OSS static token, enterprise JWT.

**Session linkage without rmcp session UUID**: On the MCP path, `audit_log.session_id` is populated from the rmcp-assigned `Mcp-Session-Id` header (OQ-03, confirmed). On a REST path, no rmcp session UUID exists. Answer:
- Should REST API clients supply an explicit `X-Session-Id` header (optional)? If present, populate `audit_log.session_id`. If absent, use `""` (acceptable for stateless callers).
- Is this sufficient for the behavioral provenance chain (Invariant 2/6 from ASS-050), or does it break the `cycle_events` join in a way that needs a different fix?
- For enterprise JWT callers, can the JWT `jti` claim serve as a session anchor? Assess feasibility.

**`credential_type` value**: ASS-050 defined `'static_token'` and `'jwt'` as the credential type values for the MCP path. The REST path uses the same credentials — the `credential_type` values are unchanged. Confirm this: a `StaticTokenAuth`-validated REST call records `credential_type = 'static_token'`, same as a `StaticTokenAuth`-validated MCP call. No new values needed.

**Capability model**: `require_cap()` calls in tool handlers do not change. REST API callers authenticated by `StaticTokenAuth` receive a full-capability `ResolvedIdentity` — same as MCP callers. No per-endpoint capability differentiation in OSS tier. Confirm this is consistent with the enterprise tier: a JWT-authenticated REST caller receives role-scoped capabilities through the same `JwtBearerAuth` path. No new capability gating logic needed.

**What changes in `build_context()`**: The MCP path uses `build_context_with_external_identity()` (Seam 2 from ASS-050) to read `clientInfo.name` and the rmcp session UUID from `RequestContext`. The REST path cannot use the same inputs. Answer: does the REST path use the existing `build_context()` (tool-param `agent_id`, no session UUID), or does it need a separate `build_context_for_rest()` variant? What is the cost of each approach?

---

### 2. Admin Plane Decoupling Seams

The admin plane is a separate application that connects to Unimatrix's admin port (8444) to perform privileged operations: enrollment management, cross-repo data access, context administration. The OSS code must expose seams for this without containing the admin logic.

**Port 8444 interface**: Port 8444 is reserved by W2-2. Currently it has no defined handler — the reservation is a placeholder. What must be wired up on port 8444 in the OSS binary for the enterprise admin application to plug into? Options:
- **Nothing now** — port 8444 stays closed in OSS; enterprise binary opens it with its own listener. OSS only ensures the port is not used for anything else.
- **Health/identity endpoint only** — port 8444 serves a single `GET /admin/identity` that returns the server's identity (version, schema version, repo ID). Enterprise adds its own handlers.
- **Plugin hook** — port 8444's router is constructed via a trait that the enterprise binary implements. OSS constructs the router by calling the trait; enterprise supplies the implementation.

Assess which option preserves the cleanest seam without overengineering. The admin application is enterprise-only; OSS does not implement it.

**Admin credential separation**: The data plane uses a bearer token stored at `{data_volume}/token`. The admin plane requires a distinct credential for two reasons: (1) different threat model — the admin token grants cross-repo visibility; (2) key rotation — rotating the data plane token should not affect the admin plane and vice versa.

Answer:
- What is the OSS admin credential model? Options: second static token at `{data_volume}/admin-token`, or no admin auth in OSS (admin port closed until enterprise opens it).
- For enterprise: JWT with a separate `aud` claim (e.g. `unimatrix:admin` vs. `unimatrix:data`). Does the `BearerValidator` trait need to be parameterized by audience, or is audience validation the enterprise impl's responsibility?
- Does the admin `BearerValidator` share the same trait as the data plane `BearerValidator`, or is a separate `AdminBearerValidator` trait needed? Challenge: a single trait used for both planes is simpler; separate traits allow independent enterprise extension. Assess.

**Cross-repo store access**: The current data plane holds a single `Arc<SqlxStore>` scoped to one repo. Admin operations need to read across repos. What is the seam?
- The store registry pattern: a `StoreRegistry` that maps `repo_id → Arc<SqlxStore>`. The admin plane receives the registry at startup; the data plane continues to hold a single store. Is this additive to the current startup wiring, or does it require refactoring `main.rs`?
- What OSS types must be `pub` (or moved to `unimatrix-core`) for the enterprise admin crate to read store data without depending on internal `unimatrix-store` impl details?
- Are there schema-level concerns — e.g., do per-repo SQLite databases need a `repo_id` field in the schema for cross-repo join queries, or is repo identity always derived from file path?

**Admin audit log**: Admin operations (enrollment changes, cross-repo reads) must be audited. Does the admin plane write to the same `audit_log` table in each affected repo's database, or does it maintain a separate admin audit log? Assess the compliance implications of each approach. If separate, does the `EnterpriseAuditWriter` trait from ASS-050 cover the admin path, or is a separate trait needed?

---

### 3. Startup Wiring — Both Planes

`main.rs` currently starts one listener (or two ports in W2-2). The admin plane adds a third listener (port 8444). Assess the startup wiring impact:

- Can the admin plane listener be added as an additive `Option<Arc<dyn AdminPlaneHandler>>` field on the server, populated by the enterprise binary at startup and absent (listener not bound) in OSS?
- What is the minimum OSS change to `main.rs` to support this without the enterprise binary having to fork `main.rs`?
- Does the current `PidGuard` / `flock` locking strategy in vnc-004 need adjustment when two binaries (OSS data plane + enterprise admin plane) might legitimately run side-by-side against the same data volume?

---

### 4. Seam Map Addendum

ASS-050 produced a seam map for the MCP-path identity resolution (5 seams). This section produces an equivalent seam map for the two new paths — three to five seams for the REST API data plane and three to five seams for the admin plane decoupling. Same format as ASS-050 Section 5:
- Current state
- Injectable now? (with the specific overload or trait change)
- Cost now vs. cost of retrofitting later
- Pattern that would foreclose this seam

---

## Output

1. **REST API data plane security model** — complete specification: attribution without `clientInfo.name`, session linkage without rmcp UUID, `credential_type` value, capability model confirmation, `build_context()` variant decision.

2. **Admin plane decoupling specification** — port 8444 interface decision (nothing / identity endpoint / plugin hook), admin credential model (OSS + enterprise), `StoreRegistry` seam assessment, admin audit log approach.

3. **Startup wiring assessment** — additive admin listener pattern, `main.rs` impact, `PidGuard` multi-binary concern.

4. **Seam map addendum** — 3-5 seams for REST path, 3-5 seams for admin plane. Each with current state, injection cost now vs. later, foreclosure patterns.

5. **Change table** — every proposed change classified as additive / non-breaking / breaking with blast radius. No unclassified changes.

---

## Breadth

`codebase + prior research`

Primary sources:
- ASS-050 FINDINGS.md and FINDINGS-OQ01.md — security model foundation; read before examining code
- ASS-041 FINDINGS.md — transport stack, port reservation, `BearerValidator` design
- `crates/unimatrix-server/src/main.rs` — current startup wiring, port binding, listener construction
- `crates/unimatrix-server/src/server.rs` — `build_context()`, tool dispatch
- `crates/unimatrix-server/src/infra/audit.rs` — `AuditLog`, `AuditEvent` construction
- `crates/unimatrix-store/src/db.rs` — store construction, schema

This spike does NOT modify any code.

---

## Approach

`design + audit`

Phase 1 — read ASS-050 FINDINGS.md and FINDINGS-OQ01.md in full before touching any code. The REST API and admin plane questions are extensions of that model; starting from the code without that context produces redundant analysis.

Phase 2 — read the current `main.rs` and `server.rs` to understand the actual startup wiring and tool dispatch before proposing seam changes.

Phase 3 — produce the specifications. Ground every recommendation in a specific observation from Phase 1 or Phase 2. Mark each recommendation: confirmed by code read / derived from prior spike / reasoned inference (state confidence).

---

## Confidence Required

`high` — same requirement as ASS-050. This spike's outputs gate vnc-014 delivery scoping and enterprise extension surface decisions. Flag uncertainty explicitly rather than collapsing it into a confident recommendation.
