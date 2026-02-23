# Security Roadmap Integration Recommendations

**Date**: 2026-02-23
**Status**: DRAFT -- Recommendations only. No changes to PRODUCT-VISION.md yet.
**Input**: MCP-SECURITY-ANALYSIS.md, RESEARCH-*.md findings
**Scope**: How to layer security into the existing nxs/vnc milestone structure

---

## Guiding Principles

1. **Schema changes are cheap now, expensive later.** Every `EntryRecord` field added after entries exist triggers scan-and-rewrite migration. Adding 7 fields in one migration (nxs-004) vs. 7 separate migrations across vnc-001 through crt-003 is the difference between one rewrite and seven.

2. **Security infrastructure that touches the write path must exist before the first MCP-written entry.** Once vnc-002 ships `context_store`, entries start flowing. Any provenance, attribution, or integrity field not present at that moment creates a permanent gap in the audit chain.

3. **Don't create a security milestone.** Security is not a phase -- it's a cross-cutting concern woven into every feature. Isolating it into its own milestone signals "optional" and creates integration pain. Instead, extend existing features with security responsibilities.

4. **Match defense sophistication to deployment context.** Unimatrix is local-first, stdio transport, single-machine. Many enterprise security patterns (OAuth 2.1, DIDs, mTLS) are premature. Build the hooks for them now; implement when the deployment context demands it.

5. **Foundational fields enable layered defenses.** A content hash field costs nothing to populate but enables Merkle trees, tamper detection, and rollback later. An agent_id field enables audit trails, anomaly detection, and access control later. The fields are cheap; the features they enable are where the complexity lives.

---

## Current State

| Feature | Status | Security Surface |
|---------|--------|-----------------|
| nxs-001 (Storage Engine) | **Shipped** | EntryRecord schema, 8 tables, no security fields |
| nxs-002 (Vector Index) | **Shipped** | hnsw_rs, no access control on search |
| nxs-003 (Embedding Pipeline) | **In progress** | ONNX model download, no signature verification |
| nxs-004 (Core Traits) | **Not started** | Trait definitions, async adapters |
| vnc-001 (MCP Server Core) | **Not started** | Server skeleton, transport, lifecycle |
| vnc-002 (v0.1 Tools) | **Not started** | context_search, context_lookup, context_store, context_get |
| vnc-003 (v0.2 Tools) | **Not started** | context_correct, context_deprecate, context_status, context_briefing |

---

## Recommendation 1: Extend nxs-004 with Security Schema Fields

**Current scope** (from PRODUCT-VISION.md): Storage traits (EntryStore, VectorStore, IndexStore) in core crate. Domain adapter pattern. `spawn_blocking` with `Arc<Database>` for async.

**Proposed addition**: Add 7 security-relevant fields to `EntryRecord` as part of the trait/schema work:

```
created_by: String        // Agent ID that created this entry
modified_by: String       // Agent ID that last modified this entry
content_hash: String      // SHA-256 of (title + ": " + content)
previous_hash: String     // Content hash before last update
version: u32              // Incremented on each update, starts at 1
feature_cycle: String     // Feature ID that generated this entry (e.g., "nxs-003")
trust_source: String      // "agent" | "human" | "system"
```

**Why nxs-004**: This feature is already about defining the canonical trait surface. The traits should expose these fields from day one so every consumer (vnc-001, vnc-002) sees them as part of the contract, not as bolted-on additions.

**Migration**: nxs-004 triggers one scan-and-rewrite of existing entries (from nxs-001/002 era). All 7 fields get default values: `created_by = ""`, `content_hash = computed`, `version = 1`, `trust_source = "system"`, etc. This is the last migration before MCP entries start flowing.

**Impact on nxs-004 scope**: Low. The fields are simple types. The real work is the trait definitions and async adapter patterns which are already in scope. Computing SHA-256 on insert/update is ~5 lines. Incrementing a version counter is trivial.

---

## Recommendation 2: Add Security Infrastructure to vnc-001

**Current scope**: rmcp 0.16 SDK, stdio transport, server instructions, auto-init, project isolation, graceful shutdown (compact + dump).

**Proposed additions**:

### 2A. Agent Registry Table

New redb table created during auto-init alongside existing tables:

```
AGENT_REGISTRY: Table<&str, &[u8]>
Key: agent_id (string)
Value: bincode-serialized AgentRecord {
    name: String,
    trust_level: TrustLevel,          // System | Privileged | Internal | Restricted
    capabilities: Vec<Capability>,    // Read, Write, Search, Admin
    allowed_topics: Option<Vec<String>>,
    allowed_categories: Option<Vec<String>>,
    enrolled_at: u64,
    last_seen_at: u64,
    active: bool,
}
```

**Bootstrap**: On first run, create a default `"human"` agent with `Privileged` trust and a `"system"` agent with `System` trust. Unknown agent_ids auto-enroll as `Restricted` (read-only).

**Integration point**: The MCP `initialize` handshake includes `clientInfo`. Extract or require `agent_id` from this. If not present, assign `"anonymous"` with `Restricted` trust.

### 2B. Audit Log Table

New redb table, append-only:

```
AUDIT_LOG: Table<u128, &[u8]>
Key: monotonic nanosecond timestamp
Value: bincode-serialized AuditEvent {
    request_id: String,
    session_id: String,
    agent_id: String,
    operation: String,
    target_ids: Vec<u64>,
    outcome: String,        // "success" | "denied" | "error"
    detail: String,
}
```

No delete or update operations exposed. Write-once.

### 2C. Agent Identification Flow

Every MCP tool call:
1. Extract `agent_id` from request metadata (or connection state)
2. Look up in AGENT_REGISTRY
3. Verify trust level permits the operation
4. Populate `created_by`/`modified_by` on entry mutations
5. Write AuditEvent to AUDIT_LOG

**Impact on vnc-001 scope**: Medium. Adds one table, one lookup per request, one write per request. The MCP server skeleton needs to thread `agent_id` through the request handling pipeline. This is simpler to build in from the start than to retrofit.

### 2D. Graceful Shutdown Update

The existing persistence note says vnc-001 must call `Store::compact()` and `VectorIndex::dump()`. Add: audit log does not require explicit flush (redb transactions are durable). Agent registry is persisted via normal redb transactions.

---

## Recommendation 3: Add Input Validation to vnc-002

**Current scope**: context_search, context_lookup, context_store, context_get. Dual response format. Near-duplicate detection at 0.92 threshold.

**Proposed additions**:

### 3A. Input Validation on context_store

Before writing to the store:

| Field | Validation | Limit |
|-------|-----------|-------|
| title | Required, non-empty, max length | 512 chars |
| content | Required, non-empty, max length | 65,536 chars (64KB) |
| topic | Required, pattern match `[a-zA-Z0-9_-]+` | 128 chars |
| category | Required, enum allowlist | See below |
| tags | Optional, max count, max per-tag length | 20 tags, 128 chars each |
| source | Optional, structured format | 256 chars |
| All strings | No null bytes, no control chars | -- |

Category allowlist (from PRODUCT-VISION.md alc-001): `outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`. Extensible via configuration.

### 3B. Content Scanning on context_store

Regex-based quick scan before write (ported from aidefence concept, native Rust):

- ~50 injection patterns (instruction override, jailbreak, role switching, delimiter abuse)
- PII patterns (emails, API keys, credentials)
- On match: log to AUDIT_LOG with threat details, reject the write with structured error

Estimated: ~150 lines of Rust using the `regex` crate. No external dependencies.

### 3C. Output Framing on context_search / context_lookup / context_get

Wrap returned entries in structured format that marks content as data:

```json
{
  "entries": [
    {
      "_meta": "KNOWLEDGE_ENTRY_DATA",
      "id": 42,
      "title": "...",
      "content": "...",
      "confidence": 0.85,
      "created_by": "uni-architect",
      "trust_source": "agent"
    }
  ]
}
```

The `structuredContent` response format (already planned) naturally provides this framing. Ensure the compact markdown `content` response also includes a framing header.

### 3D. Capability Check Before Execution

Every tool call checks the agent's capabilities from AGENT_REGISTRY:

| Tool | Required Capability |
|------|-------------------|
| context_search | `Read` |
| context_lookup | `Read` |
| context_get | `Read` |
| context_store | `Write` |

If capability check fails: log denial to AUDIT_LOG, return structured error.

**Impact on vnc-002 scope**: Medium. Input validation is standard practice for any API. Content scanning is a new module (~150 lines). Output framing is a formatting concern. Capability checks are one lookup per request. All of this is simpler to build into the tools from the start than to retrofit.

---

## Recommendation 4: Extend vnc-003 with Security Tools

**Current scope**: context_correct, context_deprecate, context_status, context_briefing.

**Proposed additions**:

### 4A. Security Metrics in context_status

The existing `context_status` tool returns health metrics. Add security-relevant metrics:

- Entries by trust_source (agent vs. human vs. system)
- Entries without attribution (created_by = "")
- Write frequency by agent (top 5 agents by write count)
- Entries with content_hash mismatches (tamper indicator)
- Last audit log entry timestamp

### 4B. Capability Check for Mutation Tools

| Tool | Required Capability |
|------|-------------------|
| context_correct | `Write` |
| context_deprecate | `Write` |
| context_status | `Admin` |
| context_briefing | `Read` |

### 4C. Content Scanning on context_correct

Same injection/PII scan as context_store (the correction creates a new entry).

**Impact on vnc-003 scope**: Low. Status metrics are queries over existing data. Capability checks are the same pattern as vnc-002.

---

## Recommendation 5: Defer These to Cortical Phase (crt)

These require operational data and can't be meaningfully implemented until knowledge is accumulating:

| Defense | Natural Home | Why Defer |
|---------|-------------|-----------|
| Contradiction detection | crt-003 (existing roadmap) | Needs a populated embedding space to compare against |
| Write rate limiting | crt-001 (Usage Tracking) | Needs baseline access patterns to set thresholds |
| Corroboration scoring | crt-002 (Confidence Evolution) | Needs multi-feature-cycle data |
| Behavioral anomaly detection | crt-001 (Usage Tracking) | Needs historical agent behavior profiles |
| Embedding consistency checks | crt-003 | Needs re-embedding infrastructure |
| Entry quarantine status | crt-003 or col-001 | Needs detection triggers to be meaningful |

These align with the existing Cortical phase goals. The crt-003 (Contradiction Detection) feature already plans to "flag entries with high embedding similarity but conflicting content" -- this is exactly the semantic poisoning defense. The security research validates and strengthens the case for crt-003's existing scope.

---

## Recommendation 6: Defer These to Collective/Matrix/Later Phases

| Defense | Natural Home | Trigger |
|---------|-------------|---------|
| Merkle root computation | col-004 (Feature Lifecycle) | When feature-scoped integrity verification is needed |
| Trusted snapshots / rollback | col-004 | When feature lifecycle gates need rollback capability |
| Full version history table | col-004 | When entry audit depth beyond hash-chain is needed |
| OAuth 2.1 for HTTP transport | vnc-001 variant | When Unimatrix supports remote connections |
| Cryptographic warrants | dsn-002 (Project Isolation) | When multi-machine delegation is needed |
| Dashboard security views | mtx-002 (Knowledge Explorer) | When visual security monitoring is needed |
| Model signature verification | nan-004 (Release Automation) | When distributing pre-built binaries |

---

## Summary: Security Work by Feature

| Feature | Existing Scope | Security Additions | Net Effort |
|---------|---------------|-------------------|------------|
| **nxs-004** | Traits, adapters, async | +7 EntryRecord fields, +1 migration | Low |
| **vnc-001** | Server, transport, lifecycle | +Agent Registry table, +Audit Log table, +agent identification flow | Medium |
| **vnc-002** | 4 tools, dual format, dedup | +Input validation, +content scanning (~150 LOC), +output framing, +capability checks | Medium |
| **vnc-003** | 4 tools | +Security metrics in status, +capability checks on mutations | Low |
| **crt-001** | Usage tracking | +Write rate limiting, +behavioral baselines | Already scoped |
| **crt-003** | Contradiction detection | +Semantic poisoning defense (embedding comparison) | Already scoped |
| **col-004** | Feature lifecycle | +Merkle roots, +snapshots, +rollback | Medium (future) |

---

## What This Does NOT Recommend

1. **No new milestone.** Security is woven into existing features, not isolated.
2. **No new feature prefix.** Security additions are scoped to existing features (nxs-004, vnc-001, vnc-002, vnc-003).
3. **No external dependencies.** Content scanning is native Rust (~150 LOC). No @claude-flow/aidefence, no Node.js sidecar, no new crates beyond `regex` and `sha2`.
4. **No premature enterprise security.** OAuth 2.1, DIDs, mTLS are deferred until deployment context demands them. The hooks exist (agent_id field, trust_level enum) but the implementations wait.
5. **No blocking dependencies.** Security additions extend features; they don't gate them. A vnc-002 that ships without content scanning is still useful -- the scanning can be added in a follow-up without breaking changes.

---

## Decision Points for Product Owner

Before integrating into PRODUCT-VISION.md, decisions needed:

1. **nxs-004 schema fields**: **APPROVED.** 7-field addition to EntryRecord. No live database exists yet, so no runtime migration needed -- but this is the first schema evolution event, so nxs-004 should also implement the scan-and-rewrite migration capability itself (the mechanism described in PRODUCT-VISION.md for future schema changes). This establishes the migration pattern for all future field additions.

2. **Agent identity model**: **APPROVED.** Implement Option A (self-reported `agent_id` tool parameter) for stdio transport. Design internal plumbing to accept identity from any source (parameter, `_meta`, OAuth token). **Roadmap note**: Evolve to Option B (`_meta` field) when MCP client support matures; evolve to OAuth 2.1 bearer tokens for HTTPS transport. See detailed analysis below.

3. **Category allowlist**: **APPROVED with note.** Initial set: `outcome`, `lesson-learned`, `decision`, `convention`, `pattern`, `procedure`. Must be extensible at runtime (not compile-time). **Scoping note**: The complete category taxonomy should be researched more thoroughly during vnc-002 SCOPE phase -- these are a starting point, not final.

4. **Trust level defaults**: **APPROVED.** Unknown agents auto-enroll as `Restricted` (read-only).

5. **Content scanning strictness**: **Scoping note**: Whether injection matches hard-reject or soft-flag needs deeper analysis during vnc-002 SCOPE phase. Both approaches have trade-offs (hard-reject is safer but risks false positives blocking legitimate knowledge; soft-flag is permissive but creates a review backlog). Recommend researching false positive rates on real Unimatrix content during scoping.

6. **Scope of vnc-001 expansion**: **APPROVED.** Agent Registry + Audit Log tables integrated into vnc-001.

---

## Decision Point #2: Agent Identity Model (Open)

### The Problem

When an MCP tool call arrives at the Unimatrix server, we need to know *which agent* is calling. This determines:
- What trust level to apply (Privileged vs. Internal vs. Restricted)
- What capabilities to grant (Read, Write, Admin)
- What to write into the `created_by` / `modified_by` fields
- What to log in the audit trail

### The Constraint

MCP stdio transport has **no built-in authentication**. The protocol provides:
- `initialize` handshake with `clientInfo: { name, version }` -- but this identifies the *MCP client* (e.g., "Claude Code"), not the *agent* (e.g., "uni-rust-dev working on nxs-004")
- No session tokens, no credentials, no agent identity in the protocol spec for stdio

In practice, Unimatrix runs as a single MCP server process spawned by Claude Code. All agents within that Claude Code session share the same stdio pipe. The MCP protocol sees one client, even when the human spawns 5 different agent subprocesses.

### Options

**Option A: Agent self-declaration via tool parameter**

Every tool call includes an optional `agent_id` parameter:
```json
{ "tool": "context_store", "params": { "title": "...", "content": "...", "agent_id": "uni-rust-dev" } }
```

- Pro: Simple, no protocol changes, works today
- Pro: Agents can be instructed via their agent definitions to always pass their identity
- Con: Self-reported -- a prompt-injected agent can lie about its identity
- Con: Pollutes every tool call with a boilerplate parameter

**Option B: Agent declaration via MCP metadata/headers**

Use MCP's `_meta` field (supported in the protocol for request metadata):
```json
{ "method": "tools/call", "params": { "name": "context_store", "arguments": { ... }, "_meta": { "agent_id": "uni-rust-dev" } } }
```

- Pro: Cleaner separation -- identity is metadata, not a tool argument
- Pro: MCP spec supports `_meta` on requests
- Con: Still self-reported
- Con: Not all MCP clients may forward `_meta` reliably

**Option C: Session-based identity via dedicated registration tool**

Expose a `context_register` tool that agents call once at start of their task:
```json
{ "tool": "context_register", "params": { "agent_id": "uni-rust-dev", "role": "developer", "feature": "nxs-004" } }
```
Server returns a session token. Subsequent calls include the token. Server maps token to agent.

- Pro: Explicit enrollment step -- agent must register before operating
- Pro: Server controls the session token (not purely self-reported)
- Con: Adds a mandatory setup step to every agent invocation
- Con: Session token is still passed by the agent (can be replayed or shared)
- Con: Breaks the "just call context_store" simplicity

**Option D: Derive identity from MCP client context**

Don't add agent_id to the protocol at all. Instead, derive it from available signals:
- `clientInfo.name` from initialize (identifies "Claude Code" but not the specific agent)
- Process-level signals (PID, environment variables)
- The human user is always the same in local stdio

All entries attributed to "human-via-claude-code". No per-agent granularity.

- Pro: Zero protocol changes, zero agent burden
- Con: All agents look identical in the audit trail
- Con: Cannot implement per-agent access control
- Con: Useless for the security model we're building

### Decision: Implement A, Design for B

**Implement Option A now**: `agent_id` as an optional parameter on all tools. If omitted, default to `"anonymous"`. The agent definition files (`.claude/agents/uni/*.md`) instruct agents to always pass their identity -- behavioral compliance is 70-85% effective (ASS-006 research), sufficient for attribution in a local trust model.

**Design internal plumbing for Option B**: The server's identity extraction should be a single function:

```rust
fn extract_agent_identity(request: &McpRequest) -> AgentIdentity {
    // 1. Check OAuth bearer token (HTTPS transport — future)
    // 2. Check _meta.agent_id (Option B — future)
    // 3. Check tool parameter agent_id (Option A — current)
    // 4. Fall back to "anonymous"
}
```

This priority chain means Option B and OAuth are non-breaking upgrades -- add a new extraction path, existing Option A calls continue to work.

**Roadmap evolution**:
- **vnc-001/vnc-002 (now)**: Option A — `agent_id` tool parameter, self-reported
- **Future vnc revision**: Option B — `_meta.agent_id` on MCP requests, when MCP client support for `_meta` forwarding is confirmed reliable
- **HTTPS transport (Tier 4)**: OAuth 2.1 bearer tokens — identity comes from verified token claims, no self-reporting

**Not pursuing Option C** (registration tool). The ceremony adds friction for marginal security gain on stdio. OAuth achieves the same goal properly when HTTPS arrives.

**Not pursuing Option D** (no agent identity). Per-agent attribution is the foundation of the security model.

---

## Decision Point #2 Addendum: What Changes with HTTPS Transport?

### The Shift

HTTPS changes the trust model from **"one user, one machine, one pipe"** to **"anyone on the network can connect."** Every assumption that makes stdio safe evaporates:

| Property | stdio | HTTPS |
|----------|-------|-------|
| Who can connect | Only the process that spawned the server | Anyone who can reach the port |
| Authentication | Inherited from OS (process isolation) | Must be explicit (OAuth 2.1 per MCP spec) |
| Number of concurrent clients | 1 | Unbounded |
| Agent identity source | Self-reported (acceptable for local) | Must come from authenticated token |
| Transport encryption | N/A (in-process pipe) | TLS required |
| Session management | Implicit (one pipe = one session) | Explicit (tokens, expiry, revocation) |
| Rate limiting urgency | Nice-to-have | Mandatory |
| CORS / origin control | N/A | Required |
| Multi-tenancy | N/A (single user) | Possible (multiple users, multiple projects) |

### What Changes for Agent Identity (Decision #2)

**The self-reporting problem disappears.** With HTTPS + OAuth 2.1:

1. Agent authenticates via OAuth 2.1 Authorization Code flow with PKCE
2. Token contains verified claims: `agent_id`, `trust_level`, `capabilities`, `scope`
3. Server validates token cryptographically -- no self-reporting, no behavioral compliance dependency
4. Token is scoped per MCP spec: "Minimal initial scope containing only low-risk discovery/read operations. Incremental elevation via targeted scope challenges."

Option A (self-reported parameter) becomes **unnecessary** -- identity comes from the bearer token. Option C (registration tool returning session token) becomes **how OAuth works** -- but with real cryptographic enforcement instead of a honor-system session token.

### What Changes for the Security Infrastructure We're Building

**Things that DON'T change** (already designed correctly):

| Component | Why It Survives HTTPS |
|-----------|-----------------------|
| EntryRecord security fields | Same 7 fields, same purposes. `created_by` is populated from token instead of parameter |
| Agent Registry table | Same schema. Populated from OAuth client registration instead of auto-enrollment |
| Audit Log table | Same schema. `agent_id` comes from verified token instead of self-report |
| Input validation | Same rules. Network access makes DoS more likely, not less |
| Content scanning | Same patterns. Network doesn't change what's dangerous to store |
| Capability model (Read/Write/Admin) | Maps to OAuth scopes. `Read` = `unimatrix:read`, `Write` = `unimatrix:write` |
| Trust levels | Map to token claims. `trust_level` in token payload, verified by server |
| Output framing | Same need -- tool responses are still data, not instructions |

**Things that MUST be added for HTTPS**:

| Component | Purpose | Complexity |
|-----------|---------|-----------|
| **OAuth 2.1 Authorization Server** | Issue and validate tokens | High (or use external: Auth0, Keycloak, etc.) |
| **TLS termination** | Encrypt transport | Medium (rustls or external reverse proxy) |
| **Token validation middleware** | Verify JWT/opaque tokens on every request | Medium |
| **CORS configuration** | Control which origins can call the API | Low |
| **Session management** | Token expiry, refresh, revocation | Medium |
| **Rate limiting (hard)** | Per-token request caps, mandatory for network | Medium |
| **SSRF prevention** | Block private IP ranges in any outbound requests | Low |
| **DNS rebinding protection** | Validate Host headers | Low |

**Things that change in character**:

| Component | stdio Behavior | HTTPS Behavior |
|-----------|---------------|----------------|
| Agent enrollment | Auto-enroll unknown as Restricted | Reject unknown -- must have valid OAuth client credentials |
| Trust level assignment | Configured locally in Agent Registry | Encoded in token claims by Authorization Server |
| Capability checks | Lookup in local redb table | Validate OAuth scopes on token |
| Audit log | Local forensics | Compliance requirement (potentially HIPAA/SOX/GDPR) |
| Rate limiting | Soft (anomaly detection) | Hard (deny above threshold) |

### The Key Architectural Insight

**The internal plumbing is the same.** What changes is *where identity comes from*:

```
stdio:  tool_call → extract agent_id from parameter → lookup registry → check capabilities → execute
HTTPS:  tool_call → extract agent_id from OAuth token → lookup registry → check capabilities → execute
```

The Agent Registry, Audit Log, capability model, and trust levels are **transport-agnostic**. The only transport-specific piece is the identity extraction layer -- a function with signature:

```
fn extract_agent_identity(request: &McpRequest, transport: Transport) -> AgentIdentity
```

For stdio: reads `agent_id` from tool parameter or `_meta`
For HTTPS: reads claims from validated OAuth bearer token

Everything downstream is identical.

### Design Recommendation

**Build the internal security infrastructure transport-agnostic from day one.** The Agent Registry, Audit Log, capability checks, and trust levels should not know or care whether the request arrived via stdio or HTTPS. They receive an `AgentIdentity` struct and enforce policy against it.

This means:
1. **Option A (self-reported param) works for stdio v1** -- identity quality is low but the enforcement infrastructure is real
2. **HTTPS upgrades identity quality** without changing enforcement -- swap the extraction function, everything else stays
3. **No security infrastructure is wasted** -- everything built for stdio serves HTTPS
4. **No foundation change required** -- the schema, tables, and enforcement patterns are identical

The only HTTPS-specific work is the OAuth integration layer and TLS -- which are explicitly Tier 4 ("build when needed") in our analysis. They don't touch the store, the registry, the audit log, or the capability model.

### Impact on Current Recommendations

**None.** Every recommendation in this document remains valid whether we ship stdio-only or add HTTPS later:

- nxs-004 schema fields: same
- vnc-001 Agent Registry + Audit Log: same structure, different population method
- vnc-002 input validation + content scanning: same
- vnc-003 security metrics: same
- Capability model: maps 1:1 to OAuth scopes when HTTPS arrives

The only decision that changes: **Option A becomes Option "OAuth token"** for HTTPS, with Option A remaining for stdio backward compatibility. Both feed the same `AgentIdentity` struct.
