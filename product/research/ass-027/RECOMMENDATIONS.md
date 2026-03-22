# ASS-027: Privileged Access Separation — Security Assessment

**Status**: Research complete. Feeds W2-2 and W2-3 design.
**Date**: 2026-03-20
**Branch**: feature/crt-023 (research only — do not commit until feature branch is ready)

---

## Question

> Once HTTP and OAuth connections are enabled, different LLM sessions will connect to
> Unimatrix. We may also support multiple projects from a single instance. I want to
> require new connections to be scoped: one scope for content interaction (search, read,
> write), a different scope (and different connection) for administration (enroll, and
> other Admin-only operations). Constraint: there is no read-only LLM — everyday agents
> need read, write, and search. How difficult is this relative to the OAuth work? What
> alternatives or different approaches should I consider from a security perspective?

---

## Current Security Model

### What Exists Today

The registry has four capabilities and four trust levels:

```
Capabilities: Read | Write | Search | Admin
Trust:        Restricted | Internal | Privileged | System
```

`agent_id` is a **per-call parameter** supplied by the caller. The server resolves it
against the registry, auto-enrolling unknown agents. Capability enforcement happens at
the tool dispatch layer — `require_capability(agent_id, Admin)` before any admin tool
executes.

**Default agent on auto-enroll (permissive mode)**: `[Read, Write, Search]` — no Admin.

### Current Weaknesses (from product vision)

| Issue | Severity |
|-------|----------|
| `agent_id` is caller-supplied and spoofable | High |
| Auto-enroll gives read+write to any unknown process | High |
| No token-based identity for STDIO | High |
| No path to OAuth for centralized deployment | Medium |

The product vision notes (W0-2 deferral): "The LLM controls `agent_id` and retries
until a name passes." This is the core problem: today's access control is attribute-based
only at the application layer, with no transport-layer identity anchor.

---

## The Proposed Pattern

Connection-scoped access separation mirrors **Privileged Access Management (PAM)**:

- **Content connection**: Token/credential scoped to `{read, write, search}`. Issued to
  LLM sessions. The agent is trusted to operate on knowledge but cannot modify the
  registry, quarantine entries, or change trust levels.
- **Admin connection**: Token/credential scoped to `{admin}`. Issued to operators
  (human, automation with explicit approval). Required for `context_enroll`,
  `context_quarantine` (admin actions), `context_deprecate`, and any future
  administrative tooling.

The critical property: **an LLM cannot escalate from a content token to an admin token**.
The separation lives at the credential issuance level — not at the tool dispatch layer
alone. A content token cannot request admin scope, even if the LLM asks for it.

---

## How This Maps to W2-3

W2-3 already has the right primitives. The planned OAuth scope → capability mapping is:

```
unimatrix:search → Search
unimatrix:read   → Read
unimatrix:write  → Write
unimatrix:admin  → Admin
```

### What W2-3 Already Delivers

1. JWT validation with `aud`, `iss`, `exp`, `sub` enforcement
2. `sub` claim → `agent_id` (non-spoofable audit attribution)
3. Scope strings extracted from JWT claims → capability flags
4. `unimatrix_project` claim → project database routing (multi-project support)
5. Tool dispatch already enforces `require_capability(agent_id, Admin)` — this gate
   exists now and does not need to change

### What the Proposed Pattern Adds

The separation is primarily a **policy + configuration layer** on top of W2-3, not
new implementation. The question is how strong a separation you want:

---

## Separation Options (Weakest → Strongest)

### Option A: Scope Enforcement Only (already in W2-3 plan)

The LLM gets a token with `unimatrix:read unimatrix:write unimatrix:search`.
An operator gets a token with those plus `unimatrix:admin`.

**What it prevents**: Admin tool calls from tokens without admin scope.
**What it does not prevent**: An LLM requesting a token with admin scope from the
authorization server (if the client registration allows it, or if the authz server
is misconfigured).
**Additional implementation work**: Zero. Already in W2-3.

---

### Option B: Separate OAuth Client Registrations (recommended)

Register **two OAuth client types** in the authorization server:

```
content-client:  allowed_scopes = [unimatrix:read, unimatrix:write, unimatrix:search]
admin-client:    allowed_scopes = [unimatrix:admin, unimatrix:read, ...]
```

The authz server enforces the scope ceiling at issuance — a `content-client` cannot
receive `unimatrix:admin` in its token regardless of what it requests.

**What this adds over Option A**: Credential blast radius is bounded. A compromised
content credential cannot be used for admin operations even if an attacker
understands the scope naming scheme.

**Additional implementation work**: Zero Rust code changes. This is an authorization
server configuration decision. Document in deployment guide.

**For single-server deployments**: A per-project "content credential" and a
single "operator admin credential" are the two client types. The admin credential
lives in a vault (1Password, AWS Secrets Manager, etc.) — never in an LLM's
context window.

---

### Option C: Separate Audience Values (defense in depth)

Issue content tokens with `aud: "unimatrix-content"` and admin tokens with
`aud: "unimatrix-admin"`. The server validates `aud` matches the expected value
for the operation class.

**What this adds**: A content token cannot be replayed against an admin endpoint or
an admin-protected route even if the scope string is present (e.g., due to a
misconfigured authz server).

**Additional implementation work**: ~1 day on top of W2-3 (audience-aware JWT
validation middleware, routing by audience). This is the standard defense against
JWT audience confusion attacks.

---

### Option D: Separate Admin Port — Network-Layer Separation

**The enterprise-friendly equivalent of UDS=admin.**

Instead of routing by transport type, route by port. Two HTTPS listeners on the same
process, same TLS stack:

```
Port 8443 (content):  exposes context_search, context_lookup, context_get,
                      context_store, context_correct, context_briefing,
                      context_status (read), context_cycle.
                      Requires content-scoped Bearer token. Exposed via load
                      balancer / ingress — reachable by LLM sessions.

Port 8444 (admin):    exposes all content tools + context_enroll,
                      context_quarantine, context_deprecate (admin actions),
                      context_status (with maintain=true).
                      Requires admin-scoped Bearer token. NOT exposed via load
                      balancer. Internal network / VPN / operator subnet only.
```

**How enterprise deployments enforce this**: The admin port is never added to the
ingress/load balancer config. It is reachable only via:
- Internal Kubernetes Service (ClusterIP, not LoadBalancer type)
- VPN-accessible subnet
- Bastion/jump host on the internal network
- Direct container-to-container within the cluster

No SSH required. An operator connects via VPN, then makes HTTPS requests to the
admin port. Standard enterprise remote admin — just HTTPS like any other API call,
but to an internally-routed address.

**What this prevents**: A remote LLM session cannot reach admin tools regardless of
its token contents — the port is unreachable from the internet. Network-layer
enforcement, not application-layer enforcement alone.

**Additional implementation work**: ~1 day on top of W2-3. Two separate rmcp HTTP
handler registrations with different tool sets, two port bindings, two middleware
chains (one validates content token audience, one validates admin token audience).
The server logic and store layer are shared — only the routing/registration surface
is doubled.

**Kubernetes deployment sketch:**
```yaml
# Content service — internet-facing
apiVersion: v1
kind: Service
spec:
  type: LoadBalancer
  ports:
    - port: 443
      targetPort: 8443   # content port only

---
# Admin service — internal only
apiVersion: v1
kind: Service
spec:
  type: ClusterIP          # never gets a public IP
  ports:
    - port: 8444
      targetPort: 8444
```

**Tradeoff**: Slightly more complex server startup config (two port bindings).
Operators must know to use the admin port, not the content port. Document clearly.

---

### Option D': mTLS for Admin Connection

An alternative to port separation that also works in containers. The admin
endpoint requires a **client certificate** in addition to (or instead of) a
Bearer token.

```
Content endpoint:  Bearer token only (LLM session token, issued by OAuth flow)
Admin endpoint:    Client certificate required + admin-scoped Bearer token
```

Client certificates are not something an LLM session can possess — they require
an out-of-band key pair and a certificate signed by a trusted CA managed by the
operator. Even if an attacker obtains an admin-scoped Bearer token, they also need
the client cert private key.

**What this adds**: Two-factor authentication for admin connections (something you
have = cert + something you know/are = token). Standard enterprise PKI pattern.

**Tradeoff**: Certificate management overhead. The operator needs to provision and
rotate client certs. Suitable for high-security enterprise deployments; likely
over-engineered for mid-market. Can be added later as a hardening option.

**Additional implementation work**: ~2 days on top of W2-3. Requires configuring
axum/hyper with `ClientAuth::Optional` on the content endpoint and
`ClientAuth::Required` on the admin endpoint, plus cert validation logic.

---

### Option E: Separate Server Instances (not recommended)

Run a content server (HTTP, content tools only) and an admin server (separate
network, all tools). Knowledge stores are shared volumes.

**Why not recommended**: Operational complexity doubles. Shared SQLite introduces
cross-process write contention unless W0-1's dual-pool is specifically designed for
multi-process access (it is not). The other options achieve the same security
property with far less overhead.

---

## Security Analysis: What Threats Does This Mitigate?

| Threat | Option A | Option B | Option C | Option D (ports) | Option D' (mTLS) |
|--------|----------|----------|----------|------------------|------------------|
| LLM with content token calls admin tools | Blocked | Blocked | Blocked | Blocked (no route) | Blocked |
| Compromised content credential used for admin | Partial* | Blocked | Blocked | Blocked (wrong port, no cert) | Blocked |
| Content credential scope escalation at authz server | Not blocked | Blocked | Blocked | Blocked (wrong port) | Blocked |
| JWT audience confusion attack | Not blocked | Not blocked | Blocked | Blocked (separate aud per port) | Blocked |
| Remote attacker with stolen admin token | Not blocked | Blocked | Blocked | Blocked (port not internet-facing) | Blocked (no cert) |
| Rogue agent enrolling other agents | Blocked | Blocked | Blocked | Blocked | Blocked |

*Partial: If attacker has token AND admin scope is issuable by that client type.

**Key insight for "no read-only LLM" constraint**: Content scope = `{read, write,
search}` is the floor for all LLM sessions. This is already the auto-enroll default.
The content/admin separation does not conflict with this — it simply ensures admin
operations are not reachable by any LLM session, read-write or otherwise.

---

## Multi-Project Interaction

W2-3's `unimatrix_project` custom claim handles project routing. This is **orthogonal**
to the content/admin separation:

```
Content token: { sub: "claude-session-X", scopes: [read, write, search],
                 unimatrix_project: "my-project" }
Admin token:   { sub: "operator-alice", scopes: [admin],
                 unimatrix_project: "*" }  // or no project restriction
```

A content token is scoped to a specific project. An admin token may span projects
(for enrollment/management) or be project-scoped (for delegated admin). This is a
policy decision, not an implementation constraint.

**Multi-project admin isolation**: For enterprise deployments, per-project admin
tokens are achievable with the same OAuth client registration approach (Option B) —
just include `unimatrix_project` in the token's claims and scope validation.

---

## Difficulty Summary

| Option | Additional Work vs W2-3 | Blocks W2-2/W2-3 delivery? |
|--------|-------------------------|---------------------------|
| A: Scope enforcement only | 0 | No (already planned) |
| B: Separate client registrations | 0 Rust, docs only | No |
| C: Audience separation | ~1 day Rust | No (additive) |
| D: Separate admin port | ~1 day Rust | No (additive) |
| D': mTLS for admin | ~2 days Rust | No (additive) |
| A+B (minimum viable) | 0 Rust | No |
| A+B+C+D (recommended) | ~2 days Rust | No |

---

## Recommendation

### Baseline: Option A + B (minimum viable, zero extra work)

Implement scope enforcement (already in W2-3) and document the two-client-type
policy (content clients vs admin clients) in the deployment guide.

**Content client registration policy:**
```
client_type: content
allowed_scopes: [unimatrix:read, unimatrix:write, unimatrix:search]
token_ttl: ≤ 1 hour
rotation: automatic (per session or daily)
issued to: LLM sessions, automated pipelines
```

**Admin client registration policy:**
```
client_type: admin
allowed_scopes: [unimatrix:admin, unimatrix:read, unimatrix:write, unimatrix:search]
token_ttl: ≤ 1 hour
rotation: manual, stored in vault
issued to: operators, ops automation — never to LLM session context
```

### Recommended Full: Option A + B + C + D

Layer all four for a coherent, enterprise-grade separation:

1. **A** — Tool-level capability enforcement (already in W2-3 plan, no new work)
2. **B** — Separate OAuth client registrations: authz server enforces scope ceiling
   at issuance (zero code, deployment policy)
3. **C** — Audience-separated tokens: `aud: "unimatrix-content"` vs
   `aud: "unimatrix-admin"`. Server validates audience per endpoint. Prevents token
   reuse across endpoint types. (~1 day, natural to add during W2-3 JWT middleware)
4. **D** — Separate content port (8443) and admin port (8444). Admin port is not
   exposed via load balancer — internal network / VPN / cluster-internal only.
   No SSH required: operators connect remotely via VPN or cluster-internal routing.
   (~1 day, done during W2-2 HTTP transport work)

**Total additional work**: ~2 days spread across W2-2 and W2-3.

**What this delivers**: Four independent defense layers. An attacker must defeat
network routing (D), audience validation (C), scope ceiling at the authz server (B),
and application-level capability checks (A) — in sequence, all of them. Any single
layer failing does not compromise admin access.

### Optional Hardening: Add D' (mTLS) — Defer

Client certificate requirement on the admin port. Suitable for regulated industries.
Adds meaningful PKI operational overhead. Defer until there is explicit customer demand.

### Note: UDS = admin was considered and rejected

Restricting admin to UDS (local/SSH access only) was the original Option D. Rejected
because it does not work for enterprise containerized deployments. The separate admin
port (new Option D) achieves the same network-layer separation while being fully
remote-admin-friendly.

---

## What to Resolve Before W2-2/W2-3 Design

1. **Admin port designation**: Agree on port numbers for content (8443) and admin
   (8444) as part of W2-2 design. The rmcp handler registration must split at this
   point. Once HTTP transport exposes admin tools on a content port, removing them
   is a breaking change.

2. **`context_enroll` dual-use clarification** (from product vision W2-3):
   The vision already flags that enrolling a workflow agent and registering an
   OAuth client are different operations. Resolve this before W2-3 implementation:
   - Option: `context_enroll` stays as-is (workflow agents); OAuth clients are
     registered via deployment config (not a runtime MCP tool call).
   - Option: Add a `context_register_client` admin tool for OAuth client lifecycle.
   - **Recommended**: Config-file-driven OAuth client registration (consistent with
     W1-5's "config-file-driven, not runtime MCP calls" principle). `context_enroll`
     remains for workflow agent trust level management. Separate concerns cleanly.

3. **Multi-project admin scope**: For enterprise, decide whether admin tokens are
   project-scoped or instance-wide. Affects `TenantRouter` design in W2-3.

---

## Summary

The instinct is correct and architecturally sound. The W2-3 primitives already
support this — it is primarily a design decision about how many defense layers to
apply on top of planned scope enforcement.

**Recommended**: A + B + C + D. Four layers, ~2 extra days of implementation work
spread across W2-2 and W2-3. Key properties:
- LLM sessions cannot reach admin tools via network routing (separate port, D)
- Content tokens cannot carry admin scope even if requested (authz server policy, B)
- Admin tokens cannot be replayed on the content endpoint (audience validation, C)
- Admin tool calls are blocked at dispatch even if the above somehow fails (A)
- No SSH required — operators connect remotely via VPN or cluster-internal network
- Works in containers — just configure ingress to expose only the content port

The "no read-only LLM" constraint does not complicate this — content scope is
`{read, write, search}` which is already the auto-enroll default. Admin separation
is purely about the fourth capability.

For enterprise: "LLM sessions are physically unreachable from the admin endpoint,
and cannot carry admin-capable credentials, by design" is a security story that
compliance reviewers will recognize immediately.
