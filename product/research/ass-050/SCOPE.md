# ASS-050: Security Model Review — OSS + Enterprise Foundation

**Date**: 2026-04-13
**Tier**: 0 — blocks all Wave 2 delivery scoping
**Feeds**: W2-2 (HTTPS transport), W2-3 (enterprise identity), OSS developer cloud model
**Related**: ASS-041 (transport + auth stack), ASS-048 (enterprise security requirements), ASS-042 (enterprise security model architecture — pending)

---

## Question

The current Unimatrix security model was designed around a now-invalid assumption: that spawned subagents receive independent `session_id`s, enabling identity to be pinned to a session at the platform level. Anthropic revoked subagent session isolation mid-build. The fallback — agents declare `agent_id` per-call — is what was implemented. It is spoofable, high-friction, and architecturally inconsistent with the two-tier security goal.

The two-tier security goal:

**Personal cloud**: Secure HTTPS connection point. Only authorized clients can access Unimatrix. Audit log shows all accesses. Zero enrollment friction for individual developers.

**Enterprise** (vision, not yet built): Higher security guarantees. Non-spoofable identity. Full auditability. Three-role RBAC (Admin / Operator / Auditor). SOC 2 Type I readiness. ISO 42001 AI governance foundation.

The question this spike answers: Is the current implementation tweaks away from supporting this goal, or does it require reconstruction? Produce the revised security model for OSS, the extension surface specification for enterprise, and an explicit map of architectural seams that must not be closed.

---

## Why It Matters

Wave 2 delivery cannot be scoped until this is resolved. Every feature that touches auth, identity, audit, or capability gating builds on this foundation. If the foundation is wrong, each Wave 2 feature inherits the wrong assumption and the correction cost compounds.

The current `agent_id`-per-call model also creates a poor first impression for the developer cloud. A solo developer should be able to connect their MCP client and have it work — not enroll an agent, configure an agent_id, and pass it on every call. Fixing this is the highest-priority OSS security change.

The audit log schema is a one-way decision. Schema fields added now are available for future governance analysis. Fields omitted now require a breaking migration to add later — and audit logs with inconsistent schemas are not usable as compliance evidence.

---

## What to Explore

### 1. Current Implementation Audit

Read and assess the existing security implementation:

- `crates/unimatrix-server/src/infra/registry.rs` — `AgentRegistry`, `TrustLevel`, `Capability`
- `crates/unimatrix-server/src/infra/audit.rs` — `AuditLog`, current schema and fields
- `crates/unimatrix-server/src/mcp/identity.rs` — `ResolvedIdentity`, current resolution logic
- `context_enroll` tool implementation — what enrollment actually does today
- `crates/unimatrix-server/src/main.rs` — how identity is wired into the server at startup

For each component, answer:
- What was this built to do?
- What does it assume about identity (session-pinned vs. per-call declared)?
- What breaks or becomes redundant if `agent_id` is no longer a per-call security mechanism?
- What is load-bearing and should be preserved?
- What was compensating for the lost session-pinned identity and can be simplified?

Categorize every required change as: **(a) additive** (new trait, optional field, new impl), **(b) non-breaking modification**, or **(c) breaking change with migration cost**. If anything is (c), state the blast radius explicitly.

---

### 2. OSS Developer Model — Personal Cloud

Design the zero-friction security model for an individual developer running Unimatrix on HTTPS.

**Identity model**: The bearer token IS the authorization credential. Any client presenting a valid token has full access. There is no per-agent identity at the OSS tier — the token represents "this is an authorized client of this deployment," not "this is agent X."

Answer the following:

- What is the token lifecycle? Generated once at first run, persisted at `{data_volume}/token` with mode 0600 (ASS-041 pattern). How is it surfaced to the user? Printed once on first run (Jupyter model).
- Where does token validation happen in the current request path? Is it purely additive as a tower middleware layer, or does it require changes to tool dispatch?
- How does `agent_id` attribution work in this model? Answer: `agent_id` for observation and audit purposes comes from MCP `clientInfo.name` (ASS-049 confirmed this is available). It is no longer a security mechanism — it is metadata. Assess whether the current codebase can treat `agent_id` as optional metadata without breaking existing tool implementations.
- What happens to `AgentRegistry` and `context_enroll` in the OSS tier? Hypothesis: they become no-ops or are bypassed entirely by a permissive default mode. Confirm or contradict.
- What does the audit log record at this tier? At minimum: token fingerprint (hash, not value), `clientInfo.name`, tool called, timestamp, session_id if present. Is the current `AuditLog` struct capable of recording this, or does the schema need new fields?

---

### 3. Enterprise Extension Surface Specification

The enterprise crate (`unimatrix-compliance` in private repo `unimatrix-collective`) must be able to inject OAuth 2.1 JWT validation and three-role RBAC without modifying any OSS crate. Produce the interface specifications.

**Required interfaces** — confirm whether each needs to be designed new or already exists in usable form:

**`BearerValidator` trait** (in `unimatrix-server`):
```
trait BearerValidator: Send + Sync {
    async fn validate(&self, token: &str) -> Result<ResolvedIdentity, AuthError>;
}
```
OSS impl: `StaticTokenAuth` — validates constant-time equality against stored token, returns a fixed `ResolvedIdentity` with full capability.
Enterprise impl: `JwtBearerAuth` — decodes bearer token as JWT, validates `exp`/`iss`/`aud`/`sub`, resolves role from control plane, returns `ResolvedIdentity` with role-scoped capabilities.

For each: confirm the return type (`ResolvedIdentity`) is sufficient to carry both the OSS case and the enterprise case, or propose changes.

**Capability gating** — today, capability checks are in tool handlers. Does the OSS tier need capability gating at all (if any valid token = full access)? Design this so enterprise can enforce per-role capability checks without the OSS code containing RBAC logic.

**Startup plugin registration** — how does the enterprise binary supply its `BearerValidator` impl to the server at startup? Assess whether `main.rs`'s current constructor pattern supports injection, or whether a `ServerBuilder` abstraction is needed.

**`AuditLogWriter` trait** (if needed) — the compliance audit log enterprise needs (structured, exportable, retention-policy enforced) differs from the current `AuditLog`. Assess whether enterprise can own a separate write path triggered from the identity resolution layer, or whether the OSS `AuditLog` needs an extension hook.

For each interface: produce the trait signature, the OSS default impl, and the contract the enterprise impl must satisfy.

---

### 4. Audit Log Schema

The audit log schema is effectively immutable once compliance evidence depends on it. Design it correctly now.

The future audit record must support: "AI agent X with credential type Y and capability Z called tool T with parameters P at time T, during session S, in the context of feature cycle F, and the action was authorized/denied."

Assess the current `AUDIT_LOG` table schema against this requirement. Produce a recommended schema that:

- Records `session_id` — the link to `cycle_events` (goal embedding, feature context)
- Records `credential_type` — `static_token` for OSS/personal cloud, `jwt` for enterprise. Weak now, strong later when enterprise plugs in
- Records `capability_used` — which capability gate was evaluated (even if OSS always passes)
- Records `agent_attribution` — `clientInfo.name` or JWT `sub` claim, whichever is available
- Records `tool`, `action`, `outcome` (authorized / denied / error)
- Includes an extensible `metadata` JSON field for AI system attributes (model, agent role, context version) — required for ISO 42001 schema extensibility without migration
- Is append-only — no UPDATE or DELETE on audit rows, ever

Flag any current fields that need to be renamed or repurposed. Flag any schema changes that are breaking vs. additive. The goal is one migration that gets this right, not incremental patching.

---

### 5. Session-Pinned Identity Seam

Anthropic revoked subagent session isolation. The capability may return — from Anthropic, from another platform, or from enterprise JWT providing a non-spoofable per-session identity anchor. ISO 42001 AI agent auditability requires session-level attribution that cannot be self-reported.

Identify every place the current design hardcodes "identity comes from agent-declared `agent_id` per call." For each:

- Can this be made injectable (accept identity from connection context instead)?
- What is the cost of making it injectable now vs. retrofitting later?
- Flag any pattern that would require breaking changes to enable session-pinned identity later

Produce a **seam map**: a list of the three to five most critical places where the identity resolution must remain injectable without hardcoded assumptions about the source of identity.

The session integrity record — `goal_embedding` (cycle_events) → tasks (future) → actions (audit_log) → outcome (cycle_events stop) — is linked by `session_id` today. Confirm that this linkage is queryable across tables and that no current schema or code change would break it.

---

### 6. Behavioral Provenance — Don't-Foreclose-It Constraints

A future capability exists (not to be designed now): semantic alignment between declared session goals and actual agent actions. This is the foundation for ISO 42001 AI governance at the session level — "did the AI do what it said it would?"

The data needed for this future capability:

- `goal_embedding` in `cycle_events` at session start — **already exists** (crt-043). Confirm it is indexed and joinable to `audit_log` via `session_id`.
- `audit_log` tool inputs — stored as JSON today. Constraint: **do not truncate or compress `audit_log` input payloads** in any future schema optimization. The raw inputs are the action record future alignment analysis reads.
- Task capture anchor — tasks are currently invisible to Unimatrix. When future work adds task tracking, it needs a `session_id` foreign key and a timestamp to be orderable with `audit_log` entries. Identify where in the current schema a `task_log` table would anchor.
- `observations.phase` — already captures workflow context per action. Confirm this is retained and indexed.

Produce a **don't-foreclose list**: schema constraints and code patterns that must be documented as invariants so future engineers don't inadvertently break the behavioral provenance record.

---

## Output

1. **Implementation audit** — component-by-component assessment: what's load-bearing, what's compensating for the lost session-pinned identity assumption, what can be simplified. Change categorization: additive / non-breaking / breaking with blast radius.

2. **OSS personal cloud security model** — complete specification: token lifecycle, tower middleware placement, `agent_id` as optional metadata, `AgentRegistry` disposition, audit log content at this tier.

3. **Enterprise extension surface** — trait signatures with OSS default impls and enterprise impl contracts: `BearerValidator`, capability gating pattern, startup plugin registration, `AuditLogWriter` if needed.

4. **Audit log schema recommendation** — full table schema, field-by-field rationale, migration classification (breaking vs. additive), append-only enforcement mechanism.

5. **Seam map** — three to five critical identity resolution seams that must remain injectable, with current-state assessment and the cost of making them injectable now vs. later.

6. **Don't-foreclose list** — explicit schema invariants and code patterns that preserve the behavioral provenance record for future goal-action alignment analysis.

---

## Constraints

- Do not design the enterprise RBAC system — that is ASS-042's scope. This spike designs the *interface* the enterprise crate plugs into, not the implementation behind it.
- Do not design the behavioral provenance / goal-action alignment capability. Identify only the data preservation constraints required to keep it possible.
- Every proposed change must be categorized as additive, non-breaking modification, or breaking with migration cost. No unclassified changes.
- The OSS security model must have zero enrollment friction. If a proposed design requires `context_enroll` before first use, it fails this constraint.
- Audit log schema changes must be designed to survive ISO 42001 certification review — consult the ASS-048 findings on AI-specific audit requirements.

---

## Breadth

`codebase + prior research`

Primary sources: current implementation (read-only), ASS-041 FINDINGS.md (transport + auth stack), ASS-048 FINDINGS.md (enterprise security requirements, SOC 2 / ISO 42001 constraints), ASS-049 FINDINGS.md (`clientInfo.name` as agent attribution source).

This spike reads the Unimatrix codebase. It does NOT modify any code.

---

## Approach

`audit + design`

Phase 1 — read and understand the current implementation before forming any recommendations. Do not reason from memory about how the code works; read the actual files.

Phase 2 — produce the revised model and interface specifications. Ground every recommendation in a specific observation from Phase 1 or a specific prior research finding.

Mark every recommendation as: confirmed by code read / derived from prior spike finding / reasoned inference (flag confidence).

---

## Confidence Required

`high` — this spike's output directly gates delivery scoping. Recommendations stated with false confidence will produce wrong delivery scope. Flag uncertainty explicitly rather than collapsing it into a confident recommendation.

---

## Inputs

- `crates/unimatrix-server/src/infra/registry.rs`
- `crates/unimatrix-server/src/infra/audit.rs`
- `crates/unimatrix-server/src/mcp/identity.rs`
- `crates/unimatrix-server/src/main.rs`
- `crates/unimatrix-server/src/server.rs`
- ASS-041 FINDINGS.md — transport stack, `StaticTokenAuth` design, `clientInfo` attribution
- ASS-048 FINDINGS.md — SOC 2 / ISO 42001 requirements, AI-specific risk mitigations
- ASS-049 FINDINGS.md — `clientInfo.name` as agent attribution, multi-LLM session handling
- WAVE2-ROADMAP.md — W2-2 and W2-3 goal statements
