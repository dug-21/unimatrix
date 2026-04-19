# ASS-042: Enterprise Security Model Architecture

**Date**: 2026-04-09
**Tier**: 1 (integrator — absorbs ASS-041, ASS-045, ASS-047 findings)
**Feeds**: W2-3 (identity model, control plane), W2-0 (codebase boundary confirmation)

---

## Question

What is the right enterprise identity and access control model for Unimatrix — and how should it be implemented? What does the control plane schema look like, how does bootstrap work, and what multi-agent content integrity augmentations are needed?

The "right model" is not assumed. Admin XOR Operator is a working hypothesis, not a starting point. This spike must derive the model from enterprise security requirements (ASS-048), transport/auth primitives (ASS-041), codebase structure (ASS-045), and scalability constraints (ASS-047) — then design implementation accordingly.

This is the integrating architectural document for Wave 2. It cannot be completed until ASS-048 (enterprise security requirements), ASS-041 (transport + auth library selections), ASS-045 (codebase boundary), and ASS-047 (scalability ceiling + control plane DB technology decision) have reported findings.

---

## Why It Matters

Nothing in W2-3 can be delivery-scoped without this document. The identity model, RBAC binding location, control plane schema, and bootstrap flow are all interdependent. Getting any one of them wrong cascades into the others. This spike produces the architecture document that makes Wave 2 delivery estimation possible.

---

## What to Explore

### 0. Role Model and Scope Binding Architecture

**ASS-048 resolved the role count**: three roles confirmed — Admin (org-scoped), Operator (project-scoped), Auditor (org- or project-scoped). The remaining design questions are about *how scope binding works* within this model, and how it interacts with the `company → org → project` tenancy hierarchy.

**Admin is org-scoped, not server-wide.** An Admin credential grants full control within one `org` — agent enrollment, project registration, role binding management. It does not grant access across orgs. This distinction is required to preserve the company-level admin role for SaaS (post-Wave 2): a company admin will sit above org admins, and that hierarchy only works if org admin scope is bounded. In Wave 2 (one company, one org), org-scoped Admin behaves identically to server-wide Admin — but the scope constraint must be enforced structurally so that SaaS does not require a role model migration.

**Role × scope binding model**: every agent identity = role + scope binding. The binding target can be either a `project_id` or an `org_id`:

| Role | Scope binding | Meaning |
|------|--------------|---------|
| Admin | org_id | Full control within the org |
| Operator | project_id | Read/write on one project |
| Auditor | project_id | Read-only on one project |
| Auditor | org_id | Read-only across all projects in the org (compliance officer use case) |

The `role_bindings` table must support both binding targets (project-level and org-level) with a constraint that exactly one is set. OAuth JWT carries role class + `sub` only — scope binding is always resolved from the control plane at request time, never from JWT claims.

**Design questions remaining**:
- Can a single agent hold multiple bindings of the same role? (e.g., Operator on repo-A and Operator on repo-B — two rows in `role_bindings`) — almost certainly yes, confirm and design accordingly.
- Can an agent hold the Auditor role org-wide AND the Operator role on a specific project? (different role classes, different scope levels) — evaluate whether this is a valid combination or a violation of separation of duties.
- What is the enforcement order: JWT validation → role extraction → scope binding lookup → request proceeds? Specify exactly.

### 1. Identity Enforcement Mechanism
- **At issuance**: the server enforces mutual exclusivity when a credential is created — an Admin credential cannot carry Operator scopes, and vice versa. Simpler, but relies on issuance-time logic being correct and audited.
- **At validation**: every JWT validation checks that the token does not carry both `unimatrix:admin` and `unimatrix:operator` scopes. Defense-in-depth, but adds latency on the hot path.
- Evaluate both; recommend one. Document: how is mutual exclusivity maintained if tokens are long-lived and a credential is re-issued?
- What happens at the token level if a credential attempts to carry both scopes? Hard error at issuance? Validation rejection?

### 2. RBAC Binding Enforcement Model

**The hybrid model is confirmed**: JWT carries role class + `sub` (identity) only. Control plane `role_bindings` table carries scope bindings (project_id or org_id). Binding lookup happens at the TenantRouter layer on every request. This enables immediate revocation without token re-issue and keeps the IdP decoupled from Unimatrix's repo topology.

Design questions to resolve:

**Binding lookup performance**: At N=20 concurrent agents × 5 requests/sec = 100 binding lookups/sec against the control plane DB. ASS-047 confirms SQLite WAL is adequate for this load. Document the expected lookup latency and confirm whether a binding cache (in-memory, TTL-based) is needed in Wave 2 or can be deferred.

**Binding lookup for Auditor at org scope**: An org-scoped Auditor binding (`org_id` set, `project_id` null) must grant access to all projects within that org. The lookup logic: if `role_bindings` has an org-level row for the agent + the requested project's org_id matches → grant access. Specify the exact SQL and whether this is a join or a two-step lookup.

**TenantRouter enforcement contract**: For every MCP tool call, TenantRouter must:
1. Extract target `project_id` from the request context
2. Look up `role_bindings` for `(agent_id, project_id)` OR `(agent_id, org_id)` where org_id = project's org
3. If Admin role: skip binding lookup (org-scoped access is implicit for all projects in the org)
4. If no binding found: 403, not 401 (authenticated but not authorized for this resource)
5. Write `ResolvedBinding{agent_id, org_id, project_id, role}` to request context for downstream capability checks

Specify the exact enforcement order and where each step executes in the tower middleware stack.

### 3. Control Plane Schema

Design the complete control plane DB. The hierarchy is `company → org → project`. Wave 2 writes one row in `companies` and one row in `orgs`. SaaS adds rows. No multi-company or multi-org logic in Wave 2 — the schema supports it; the implementation does not.

**Required tables:**

```
companies
  company_id        TEXT PRIMARY KEY
  name              TEXT NOT NULL
  billing_email     TEXT                 -- null in private deployments
  billing_plan      TEXT                 -- null in private deployments
  billing_status    TEXT                 -- null in private deployments
  created_at        INTEGER NOT NULL     -- unix epoch

orgs
  org_id            TEXT PRIMARY KEY
  company_id        TEXT NOT NULL REFERENCES companies(company_id)
  name              TEXT NOT NULL
  created_at        INTEGER NOT NULL

projects
  project_id        TEXT PRIMARY KEY
  org_id            TEXT NOT NULL REFERENCES orgs(org_id)
  repo_hash         TEXT NOT NULL UNIQUE
  display_name      TEXT NOT NULL
  created_at        INTEGER NOT NULL
  created_by        TEXT NOT NULL        -- agent_id

agents
  agent_id          TEXT PRIMARY KEY
  org_id            TEXT NOT NULL REFERENCES orgs(org_id)
  role_class        TEXT NOT NULL        -- 'admin' | 'operator' | 'auditor'
  trust_level       TEXT NOT NULL        -- from existing alc-002 model
  enrolled_at       INTEGER NOT NULL
  enrolled_by       TEXT NOT NULL        -- agent_id

role_bindings
  binding_id        TEXT PRIMARY KEY
  agent_id          TEXT NOT NULL REFERENCES agents(agent_id)
  project_id        TEXT REFERENCES projects(project_id)   -- null for org-scope
  org_id            TEXT REFERENCES orgs(org_id)           -- null for project-scope
  role_class        TEXT NOT NULL        -- 'operator' | 'auditor' (admin binding is implicit from agents.role_class)
  granted_at        INTEGER NOT NULL
  granted_by        TEXT NOT NULL        -- agent_id
  -- CONSTRAINT: exactly one of project_id or org_id must be non-null
  -- CHECK (project_id IS NOT NULL) != (org_id IS NOT NULL)

audit_log
  event_id          TEXT PRIMARY KEY
  company_id        TEXT NOT NULL REFERENCES companies(company_id)
  org_id            TEXT NOT NULL REFERENCES orgs(org_id)
  agent_id          TEXT NOT NULL
  session_id        TEXT
  operation         TEXT NOT NULL
  target_ids        TEXT                 -- JSON array of affected resource IDs
  outcome           TEXT NOT NULL        -- 'success' | 'denied' | 'error'
  timestamp         INTEGER NOT NULL
  metadata          TEXT                 -- extensible JSON for ISO 42001 AI system attributes

credentials
  credential_id     TEXT PRIMARY KEY
  agent_id          TEXT NOT NULL REFERENCES agents(agent_id)
  client_id         TEXT NOT NULL UNIQUE
  credential_hash   TEXT NOT NULL        -- NEVER the secret; hash only
  issued_at         INTEGER NOT NULL
  revoked_at        INTEGER              -- null = active
```

**Design decisions to resolve:**
- `role_bindings.org_id` for Auditor org-scope: the check constraint syntax for "exactly one non-null" differs between SQLite (`CHECK (...)`) and PostgreSQL (`CHECK (...)`). Confirm the portable formulation with sqlx.
- `audit_log.metadata`: extensible JSON field for ISO/IEC 42001 AI system attributes (model, agent, context version). Schema must not be fixed — a JSONB/TEXT column that can be extended without migration satisfies this.
- Admin bindings: Admin agents have org-scoped access implicit in their `role_class`. Should `role_bindings` rows be created for Admins (explicit) or should Admin access be inferred from `agents.role_class` (implicit)? Explicit rows are more auditable. Implicit is simpler. Evaluate and recommend.

**Relationship to existing `AGENT_REGISTRY` in `knowledge.db`:**
The existing `AGENT_REGISTRY` (alc-002) in the data plane `knowledge.db` holds agent identity for the current OSS model. The control plane `agents` table is the authoritative source for enterprise identity. Evaluate:
- Does `AGENT_REGISTRY` in `knowledge.db` survive as a data-plane cache (denormalized, synced from control plane)?
- Or does it move entirely to the control plane, with the data plane querying the control plane for identity?
- The control plane must be authoritative — two registries that can diverge is a security risk.

Schema must use sqlx with no SQLite-specific SQL constructs. PostgreSQL migration must require only connection string + schema DDL changes, no application code changes.

### 4. Bootstrap Flow
A fresh enterprise deployment has: no agents enrolled, no admin credential, no org record. How does the first admin credential get created?

- **Option A — First-run CLI command**: `unimatrix bootstrap --admin-id "..." --output admin-credential.json`. Writes the first org record, first agent record, and first credential. Refuses to run if an admin already exists (idempotent guard). Appears in audit log.
- **Option B — Environment variable injection**: `UNIMATRIX_BOOTSTRAP_ADMIN_ID` + `UNIMATRIX_BOOTSTRAP_CREDENTIAL` read at first startup if no org exists. Consumed once, cleared from env after write. Container-friendly.
- **Option C — Config file bootstrap**: initial admin declared in `config.toml` bootstrap section. Consumed and deleted from config after first write.

Evaluate each against: security posture, operational convenience in container deployment, audit trail completeness. Can two of these options be supported simultaneously (e.g., CLI for dev, env var for container)?

Specify: what happens if bootstrap is attempted when an admin already exists? Hard error. What if the daemon starts with no admin and no bootstrap configured?

### 5. Multi-Agent Content Integrity Augmentation
Today's integrity model was designed for a single-agent environment. In a multi-agent enterprise deployment, the attack surface shifts. Explore:
- **Concurrent write attribution**: when multiple operators write simultaneously to the same repo, how is each write attributed? `agent_id` + `session_id` + `org_id` on every AUDIT_LOG entry? Is `session_id` reliably populated in the enterprise model?
- **Cross-agent contradiction detection cadence**: the contradiction scan currently runs on a tick. Under multi-agent load, newly stored entries may not be scanned for contradictions for up to one tick interval. Is this acceptable, or does the enterprise model need a shorter cadence or a per-store immediate check?
- **Privilege escalation via knowledge poisoning**: an operator could store knowledge entries crafted to manipulate what admin-capability tools surface. Content scanner mitigates this, but evaluate: are there trust-level-sensitive operations in `context_cycle_review` or `context_briefing` that should check the requestor's role?
- **Write rate limiting per agent**: currently no per-agent write rate limit beyond the write queue. Under enterprise deployment, should operators have a configurable write rate limit to prevent bulk poisoning?

### 6. Session Identity for Concurrent Same-Credential Sessions
The scenario: multiple concurrent MCP tool calls arrive from the same credential — either from multiple processes on the same machine, or from the same agent in concurrent task threads. Unimatrix receives calls that appear to originate from the same agent simultaneously.
- How does `session_id` work in this model? Is each connection/call a separate session, or must session context be carried explicitly?
- How does the attribution model for observations and the audit log handle concurrent calls from the same credential?
- Does the enterprise model need explicit design here, or is the existing session management sufficient?
- Note: this question should NOT encode assumptions about specific client tooling (e.g., IDE behavior). The solution must work for any conforming MCP client.

---

## Output

1. **Role model and scope binding design** — three-role confirmation, org-scoped Admin rationale, Auditor dual-binding model (project vs. org), multi-binding rules
2. **TenantRouter enforcement contract** — binding lookup flow, SQL specification, 403 vs. 401 distinction, request context propagation
3. **Control plane schema** — complete table definitions (`companies`, `orgs`, `projects`, `agents`, `role_bindings`, `audit_log`, `credentials`) with field types, constraints, and Wave 2 initial state (one company row, one org row)
4. **`AGENT_REGISTRY` reconciliation** — control plane authoritative, data-plane cache or migration plan
5. **Bootstrap flow specification** — chosen mechanism(s) with security analysis; creates one company + one org + first admin credential atomically
6. **Multi-agent integrity augmentations** — attribution model, contradiction detection cadence, write rate limiting
7. **Session identity model** — concurrent same-credential session attribution

---

## Constraints

- Hash chain integrity and audit log are non-negotiable (PRODUCT-VISION.md non-negotiables apply)
- Control plane schema: sqlx only, no SQLite-specific query constructs
- Per-repo DB isolation (separate files, one per repo) is fixed — control plane is a separate DB
- Existing `AGENT_REGISTRY` (alc-002) and trust hierarchy must be reconciled — not duplicated or abandoned
- Capability checks remain in the service layer regardless of transport (PRODUCT-VISION.md security non-negotiable #3)
- OAuth client secrets never stored in any database (PRODUCT-VISION.md security non-negotiable #5)

## Inputs Required from Prior Spikes

- **ASS-048**: enterprise security requirements — determines the role model (§0) and drives identity enforcement design (§1). ASS-042 cannot responsibly finalize §0 without this input.
- **ASS-041**: transport library and auth model selection (affects how identity is extracted — JWT `sub` vs. mTLS cert subject)
- **ASS-045**: codebase split decision (affects which crate owns control plane code and schema)
- **ASS-047**: scalability ceiling (determines control plane DB technology — SQLite ceiling + PostgreSQL migration trigger)
- **ASS-044**: admin console API surface decision (determines whether a router layer is introduced in W2 — see note below)

**Note — Router layer architectural requirement (from ASS-041)**: The security architecture must explicitly specify whether an HTTP router (e.g. axum `Router`) is introduced as the top-level dispatch layer in W2, or whether `StreamableHttpService` remains the direct handler. This has security implications:

1. **Middleware ordering**: auth middleware must resolve identity *before* routing — the architecture must be explicit that no route is reachable before the auth layer runs.
2. **Route-specific enforcement**: if the admin port (8444) and content port (8443) share a router, the architecture must specify where admin-only route restrictions are enforced — at the router layer (route guard) or the service layer (capability check). The non-negotiable is that capability checks remain in the service layer, but route-level guards (e.g. blocking non-Admin tokens from `/api/v1/admin/` entirely) may be appropriate as defense-in-depth.
3. **W3 extensibility contract**: if a router is introduced in W2, the security architecture should specify what can be added to it in W3 (REST endpoints, WebSocket upgrade handlers) without revisiting the auth stack design.

ASS-044 findings must be incorporated before this section can be finalized.
