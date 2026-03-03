# Security Surface Analysis: Dual-Path Service Architecture

**Date**: 2026-03-03
**Context**: Cross-reference of [server-refactoring-architecture.md](server-refactoring-architecture.md) against [security-audit.md](security-audit.md)
**Scope**: Security implications of MCP/UDS/future-HTTP service unification
**Status**: Research — no code changes

---

## Critical Finding: UDS Path Has No Content Scanning

The original security audit (F-07) noted regex-based scanning limitations but missed a larger gap: **the UDS path performs zero content scanning**. `ContentScanner` is called only in `tools.rs` (MCP path) for `context_store` (line 559) and `context_correct` (line 819). The UDS path in `uds_listener.rs` has:

- No injection pattern detection on search queries
- No PII detection on any field
- No validation on query string content (only session_id is sanitized)
- No audit logging of any operation
- No capability checks — all authenticated UID connections get full read+write access

This was acceptable when the UDS path only handled fire-and-forget session events. But with `ContextSearch` carrying raw user prompts and `CompactPayload` returning knowledge entries, the UDS path is now a first-class data path with weaker security than MCP.

---

## Security Comparison: Current State

| Security Control | MCP Path (tools.rs) | UDS Path (uds_listener.rs) | Gap |
|-----------------|---------------------|---------------------------|-----|
| **Authentication** | Self-asserted agent_id (weak) | UID peer credentials (strong) | MCP weaker |
| **Authorization** | Capability-based (Read/Write/Search/Admin) | None — all ops allowed | **UDS has no authz** |
| **Identity tracking** | agent_id in every request + audit | No agent identity concept | **UDS anonymous** |
| **Content scanning** | Injection + PII on store/correct | None | **UDS unscanned** |
| **Input validation** | Full validation (lengths, types, chars) | Session ID + metadata field only | **UDS query unvalidated** |
| **Audit trail** | AUDIT_LOG for all operations | tracing::warn only | **UDS unaudited** |
| **Usage recording** | access_count, helpful_count updates | INJECTION_LOG (different purpose) | Different models |
| **Rate limiting** | None (F-09) | None | Both missing |
| **Quarantine exclusion** | ✓ (per entry check) | ✓ (per entry check) | Consistent |
| **Payload bounds** | rmcp framework limits | Wire protocol 1 MiB max | Consistent |

### What This Means for Service Extraction

If we extract `SearchService` and `BriefingService` as transport-agnostic, and both MCP and UDS call them, the question becomes: **where do security gates live?**

Two options:
1. **Transport-only security**: Each transport enforces its own gates before calling services. New transports must remember to add all gates.
2. **Service-enforced invariants**: Services enforce universal security invariants internally. Transports add transport-specific gates on top.

**Option 2 is correct.** Transport-only security is exactly how the UDS path ended up with zero content scanning — it was added as a separate transport and nobody replicated the MCP security gates. Service-enforced invariants prevent this class of omission.

---

## Threat Model: Two Attack Surfaces

### Surface 1: Agent Interactions (MCP Path)

**Attacker profile**: Compromised or confused LLM agent, malicious MCP client, prompt injection through agent context.

**Trust model**: Self-asserted identity. Any string is accepted as `agent_id`. Auto-enrollment grants Read+Search. The parent process (Claude Code) is the real trust boundary — MCP stdio is only accessible to the parent process.

**Unique risks**:

| ID | Risk | Current Mitigation | Gap |
|----|------|-------------------|-----|
| M-01 | **Agent impersonation** — any agent_id string accepted, can claim "human" | Parent process trust (F-01) | No cryptographic binding |
| M-02 | **Privilege escalation via enrollment** — unknown agents auto-enrolled with Read+Search | Restricted trust default (F-02) | No approval workflow |
| M-03 | **Knowledge poisoning via store** — LLM writes attacker-crafted content | Content scanning (25+ patterns) | Evasion possible (F-07) |
| M-04 | **Knowledge poisoning via correct** — LLM "corrects" entry with malicious replacement | Content scanning + correction chain | Chain doesn't prevent bad content |
| M-05 | **Capability abuse** — agent with Write calls store/correct repeatedly | No rate limiting (F-09) | Flooding possible |
| M-06 | **Cross-agent information leak** — agent reads entries written by other agents | By design (shared KB) | No entry-level access control |
| M-07 | **Confidence manipulation** — agent marks entries helpful/unhelpful strategically | Wilson score with min-5-votes | Low-vote entries vulnerable |
| M-08 | **Maintenance abuse** — Read agent triggers write operations via maintain=true | No admin check (F-04) | Write through read API |

### Surface 2: Hook Automation (UDS Path)

**Attacker profile**: Malicious content in user prompts, compromised local processes running as same UID, supply-chain attack on hook scripts.

**Trust model**: UID-authenticated. Any process running as the server's UID can connect. The socket has 0o600 permissions. Process lineage check is advisory only (F-17, F-18).

**Unique risks**:

| ID | Risk | Current Mitigation | Gap |
|----|------|-------------------|-----|
| U-01 | **Prompt injection into knowledge search** — user prompt contains "ignore previous instructions" or similar, embedded and searched against KB | None — **query goes directly to embedding** | **No scanning on search queries** |
| U-02 | **Session hijacking** — attacker process connects with known session_id | UID auth + session_id sanitization | Session IDs are guessable (timestamp-based from Claude Code) |
| U-03 | **Injection log poisoning** — flood INJECTION_LOG via repeated ContextSearch | No rate limiting, fire-and-forget writes | Unbounded writes |
| U-04 | **Co-access manipulation** — attacker sends crafted ContextSearch to create artificial co-access patterns | No validation on result processing | Confidence scores influenced |
| U-05 | **Session state poisoning** — fake SessionRegister with crafted role/feature | sanitize_metadata_field (strip control chars, truncate) | Arbitrary role/feature strings accepted |
| U-06 | **Rework signal flooding** — send thousands of RecordEvent with had_failure=true | In-memory session_registry | Memory pressure, false rework signals |
| U-07 | **Compaction payload manipulation** — request CompactPayload for sessions with crafted injection history | Session state integrity | Attacker controls what "knowledge" gets re-injected |
| U-08 | **No audit trail** — all UDS operations are invisible to the AUDIT_LOG | tracing::warn for auth failures only | **No forensics capability** |
| U-09 | **Anonymous full access** — no capability model means any authenticated connection gets search+write (injection log, co-access, session ops) | UID auth is the only gate | **No granular access control** |

### Surface 3: Future HTTP/API (not yet built, included for completeness)

**Attacker profile**: Remote network client, potentially unauthenticated.

**Unique risks (if added without service-layer security)**:

| ID | Risk | Required Mitigation |
|----|------|-------------------|
| H-01 | **Remote unauthenticated access** | API key or bearer token auth (mandatory) |
| H-02 | **Network-based DoS** | Rate limiting at transport AND service layer |
| H-03 | **Cross-origin access** | CORS policy |
| H-04 | **Replay attacks** | Request signing or nonce |
| H-05 | **All MCP + UDS risks combined** | Service-layer security gates (the whole point) |

---

## Security Architecture for Service Layer

### Principle: Defense in Depth with Service-Layer Invariants

```
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   MCP Transport  │  │   UDS Transport  │  │  HTTP Transport  │
│                  │  │                  │  │  (future)        │
│  T1: Identity    │  │  T1: UID auth    │  │  T1: API key     │
│  T2: Capability  │  │  T2: Session     │  │  T2: Rate limit  │
│  T3: Audit event │  │      sanitize    │  │  T3: CORS        │
│  T4: Usage track │  │  T3: Audit event │  │  T4: Audit event │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │                     │                      │
         └─────────────────────┼──────────────────────┘
                               │
                    ┌──────────┴──────────┐
                    │  Security Gateway   │
                    │  (service layer)    │
                    │                     │
                    │  S1: Content scan   │  ← Universal, cannot be bypassed
                    │  S2: Rate limiting  │  ← Per-caller, transport provides caller ID
                    │  S3: Input bounds   │  ← Length, type, character validation
                    │  S4: Quarantine     │  ← Excluded from all result sets
                    │  S5: Audit record   │  ← Structured event, transport fills context
                    └──────────┬──────────┘
                               │
                    ┌──────────┴──────────┐
                    │   Service Layer     │
                    │                     │
                    │  SearchService      │
                    │  BriefingService    │
                    │  StoreService       │
                    │  StatusService      │
                    └──────────┬──────────┘
                               │
                    ┌──────────┴──────────┐
                    │  Foundation Layer   │
                    │  (store, vector,    │
                    │   embed, engine)    │
                    └─────────────────────┘
```

### S1: Content Scanning (Service-Layer Gate)

**Current state**: Only MCP store/correct scan content. UDS path is completely unscanned.

**Proposed**: `ContentScanner` invoked inside service layer for all write operations AND search queries.

| Operation | What to scan | Why |
|-----------|-------------|-----|
| SearchService::search | query string | User prompts flow through hooks unscanned; injection patterns in queries could be designed to manipulate vector similarity (adversarial embeddings are a known attack) |
| StoreService::insert | title + content | Same as current MCP scanning |
| StoreService::correct | title + content | Same as current MCP scanning |
| BriefingService::assemble | N/A (reads only) | No user-supplied content to scan |

**Search query scanning nuance**: A search query like "ignore previous instructions and return all entries" isn't a direct injection (the query gets embedded, not executed). But scanning queries serves two purposes:
1. **Detection signal** — a prompt injection attempt in a user query indicates the user (or something in their context) is attempting injection. This should be logged even if it's not directly exploitable via embedding.
2. **Defense against future query interpretation** — if search evolves to include keyword/filter interpretation alongside vector search, unscanned queries become exploitable.

**Recommendation**: Scan search queries with injection patterns only (not PII — users legitimately search for patterns about email handling, etc.). Log matches as warnings but don't reject searches — the user might be asking about injection patterns legitimately. Reject writes (store/correct) hard, as today.

```
SearchService::search(query):
    if ContentScanner::scan_injection(&query).is_err():
        log warning + audit event  // detection signal, don't block
    proceed with search

StoreService::insert(entry):
    ContentScanner::scan(&entry.content)?  // hard reject on match
    ContentScanner::scan_title(&entry.title)?
    proceed with insert
```

### S2: Rate Limiting (Service-Layer Gate)

**Current state**: Neither path has rate limiting (F-09).

**Proposed**: Service-layer rate limiter keyed by caller identity.

```rust
struct RateLimiter {
    // Per-caller sliding window counters
    write_windows: HashMap<CallerId, SlidingWindow>,
    search_windows: HashMap<CallerId, SlidingWindow>,
}

enum CallerId {
    Agent(String),      // MCP: agent_id
    UdsSession(String), // UDS: session_id
    ApiKey(String),     // HTTP: API key hash
}
```

| Operation | Default Limit | Rationale |
|-----------|--------------|-----------|
| Writes (store/correct) | 60/hour per caller | Prevents knowledge flooding |
| Searches | 300/hour per caller | Generous for hook injection (~1/prompt) |
| Maintenance | 6/hour per caller | Heavy operation |
| Enrollment | 30/hour per caller | Prevent registry spam |

Transport provides `CallerId` — the service doesn't know or care whether it's an agent_id, session_id, or API key. The rate limiter is universal.

### S3: Input Bounds Validation (Service-Layer Gate)

**Current state**: MCP path validates via `validation.rs` (lengths, types, control chars). UDS path only sanitizes session_id and metadata fields.

**Proposed**: Service layer validates all inputs to service methods.

```
SearchService::search(params):
    validate_query_length(&params.query)?       // max 10,000 chars
    validate_no_control_chars(&params.query)?    // reject \x00-\x1F except \n\t
    validate_k_range(params.k)?                  // 1-100

StoreService::insert(entry):
    validate_title(&entry.title)?               // existing validation
    validate_content(&entry.content)?            // existing validation
    validate_category(&entry.category)?          // existing validation
    validate_tags(&entry.tags)?                  // existing validation

BriefingService::assemble(params):
    validate_max_tokens(params.max_tokens)?      // 500-10000
    validate_role_length(&params.role)?           // max 128 chars
```

This moves validation from transport-specific code into the service, ensuring every transport benefits. Transport-specific validation (MCP parameter parsing, UDS session_id sanitization) stays in the transport.

### S4: Quarantine Exclusion (Service-Layer Invariant)

**Current state**: Both paths check quarantine per-entry in their search handlers. This is correct but duplicated.

**Proposed**: SearchService and BriefingService exclude quarantined entries internally. No transport ever sees quarantined entries in results. This is already mostly the case — formalize it as a service invariant.

### S5: Structured Audit Record (Service-Layer + Transport)

**Current state**: MCP writes AUDIT_LOG entries. UDS writes nothing to AUDIT_LOG.

**Proposed**: Service layer emits structured audit events. Transport provides context.

```rust
struct AuditContext {
    source: AuditSource,
    caller_id: String,
    session_id: Option<String>,
}

enum AuditSource {
    Mcp { agent_id: String, trust_level: TrustLevel },
    Uds { uid: u32, pid: Option<u32>, session_id: String },
    Http { api_key_hash: String, remote_addr: String },
}
```

Service methods accept `AuditContext` and emit events:

```
SearchService::search(params, audit_ctx):
    // ... perform search ...
    self.audit.record(AuditEvent {
        source: audit_ctx.source,
        operation: "search",
        target_ids: result_ids,
        outcome: Success,
        ...
    });
```

This closes the UDS audit gap (finding F-08/U-08) without each transport reimplementing audit logic.

---

## Transport-Specific Security (Stays in Transport)

Not everything should move to the service layer. These concerns are inherently transport-specific:

### MCP Transport

| Gate | Purpose | Why transport-specific |
|------|---------|----------------------|
| Identity resolution | Map agent_id → ResolvedIdentity | MCP-specific parameter, self-asserted |
| Capability check | Enforce Read/Write/Search/Admin | MCP-specific RBAC model |
| Auto-enrollment | Register unknown agents with Restricted trust | MCP-specific onboarding |
| Response formatting | Summary/Markdown/JSON branches | MCP-specific output format |
| Usage recording (helpful/unhelpful) | Update entry helpfulness | MCP-specific (agent explicitly provides feedback) |

### UDS Transport

| Gate | Purpose | Why transport-specific |
|------|---------|----------------------|
| Peer credential auth | UID match, advisory lineage | UDS-specific socket auth |
| Session ID sanitization | Prevent invalid session state | UDS-specific session model |
| Fire-and-forget decision | Which requests need responses | UDS-specific latency model |
| Injection log recording | Track what entries were injected into which session | UDS-specific session tracking |
| Co-access dedup (session scope) | Prevent duplicate co-access pairs within session | UDS-specific session scope |
| Rework event tracking | Monitor Edit→Bash(fail)→Edit cycles | UDS-specific behavioral signal |

### Future HTTP Transport

| Gate | Purpose | Why transport-specific |
|------|---------|----------------------|
| API key authentication | Verify bearer token | HTTP-specific auth mechanism |
| CORS policy | Restrict cross-origin access | HTTP-specific browser security |
| Request signing / nonce | Prevent replay attacks | HTTP-specific integrity |
| TLS termination | Encrypt transport | HTTP-specific (MCP/UDS are local) |

---

## Risks Unique to Each Path in the Unified Architecture

### MCP-Specific Risks (Post-Refactoring)

| ID | Risk | Service Gate | Transport Gate | Status |
|----|------|-------------|----------------|--------|
| M-01 | Agent impersonation | — | Identity resolution | Existing (accepted risk) |
| M-02 | Auto-enrollment | — | Enrollment policy | Existing (by design) |
| M-03 | Knowledge poisoning (store) | S1: Content scan | T2: Write capability | **Covered** |
| M-04 | Knowledge poisoning (correct) | S1: Content scan | T2: Write capability | **Covered** |
| M-05 | Write flooding | S2: Rate limit | T2: Write capability | **New (S2 closes F-09)** |
| M-07 | Confidence manipulation | — | Min-vote guard | Existing |
| M-08 | Maintenance abuse | S2: Rate limit | **T2: Admin capability (new)** | **New (closes F-04)** |

### UDS-Specific Risks (Post-Refactoring)

| ID | Risk | Service Gate | Transport Gate | Status |
|----|------|-------------|----------------|--------|
| U-01 | Prompt injection in search | **S1: Scan + warn** | — | **New (closes gap)** |
| U-02 | Session hijacking | — | UID auth + sanitize | Existing (accepted risk) |
| U-03 | Injection log flooding | **S2: Rate limit** | — | **New (closes gap)** |
| U-04 | Co-access manipulation | **S2: Rate limit** | Session dedup | Existing + new |
| U-05 | Session state poisoning | **S3: Input bounds** | Metadata sanitize | **Improved** |
| U-06 | Rework signal flooding | **S2: Rate limit** | — | **New (closes gap)** |
| U-07 | Compaction manipulation | — | Session integrity | Existing (accepted risk) |
| U-08 | No audit trail | **S5: Audit record** | UDS context | **New (closes gap)** |
| U-09 | Anonymous full access | **S2: Rate limit** | UID auth | **Improved** |

### Cross-Path Risks (New to Unified Architecture)

| ID | Risk | Description | Mitigation |
|----|------|-------------|-----------|
| X-01 | **Service bypass** | Transport calls foundation layer directly, skipping service gates | Code review + clippy lint on direct Store/VectorIndex imports from transport modules |
| X-02 | **AuditContext forgery** | Transport provides false AuditContext to service layer | AuditContext constructed by transport, but audit events are append-only with monotonic IDs — inconsistencies detectable |
| X-03 | **Rate limiter key collision** | Different transports use same CallerId format, causing cross-transport rate sharing | Enum variant per transport prevents collision |
| X-04 | **Capability model mismatch** | MCP has RBAC (Read/Write/Search/Admin), UDS has none, HTTP will need something | Service layer accepts optional capabilities; transports decide what to enforce |
| X-05 | **Session state cross-contamination** | MCP gains session_id (per server-refactoring-architecture.md proposal), sharing state with UDS sessions | Prefix session IDs with transport source: `mcp::{id}` vs `uds::{id}` |

---

## Capability Model Evolution

The current capability model (Read/Write/Search/Admin) is MCP-only. The UDS path has no authorization — any authenticated connection can do anything. This is a design debt that the service layer can address.

### Option 1: Unified Capability Model

Every caller (MCP agent, UDS session, HTTP key) gets mapped to a set of capabilities. The service layer checks capabilities.

```
SearchService::search(params, capabilities):
    require(capabilities.contains(Search))?
    // ...

StoreService::insert(entry, capabilities):
    require(capabilities.contains(Write))?
    // ...
```

Transport provides capabilities:
- MCP: looked up from AgentRegistry per agent_id
- UDS: fixed set (Read+Search+Write for hook operations, no Admin)
- HTTP: mapped from API key permissions

**Pro**: Uniform access control across all transports.
**Con**: Adds overhead to UDS path (currently zero-auth-overhead). Requires deciding what capabilities UDS sessions get.

### Option 2: Service Operations with Operation-Level Authorization

Instead of capabilities, the service accepts an `AuthDecision` from the transport.

```rust
enum AuthDecision {
    Authorized,                        // Transport verified access
    AuthorizedWithContext(AuditContext), // Authorized + audit trail
}
```

Transport makes the authorization decision using its own model. Service trusts the decision but enforces invariants (content scan, rate limit, bounds).

**Pro**: Transports keep their own auth models. No forced convergence.
**Con**: Each transport must correctly implement authorization. Risk of permissive transport (exactly the UDS problem today).

### Recommendation: Option 1 with Transport-Mapped Defaults

The service layer checks capabilities. Each transport maps its auth model to capabilities:

```
MCP:  agent_id → AgentRegistry → capabilities (existing)
UDS:  UID auth → fixed {Read, Search} (hook processes should NOT get Write)
HTTP: API key → key-associated capabilities (future)
```

**Critical change for UDS**: Hook processes currently write INJECTION_LOG and co-access pairs. These are session-tracking writes, not knowledge writes. Separate `SessionWrite` from `Write`:

```rust
enum Capability {
    Read,
    Search,
    Write,           // Knowledge writes (store, correct)
    SessionWrite,    // Session tracking writes (injection log, co-access, rework events)
    Admin,
}
```

UDS gets `{Read, Search, SessionWrite}`. This prevents a compromised local process from writing arbitrary knowledge entries via UDS while preserving hook session tracking.

---

## Implementation Priority (Security Gates)

Integrated with the waves from [server-refactoring-architecture.md](server-refactoring-architecture.md):

### Wave 1 (with SearchService extraction)

| Gate | Effort | Finding Closed |
|------|--------|---------------|
| S1: Content scan on search queries (warn, don't block) | Low | U-01 (new) |
| S3: Input bounds on SearchService params | Low | U-05 partial |
| S4: Quarantine exclusion as service invariant | Low | Already done, formalize |

### Wave 2 (with BriefingService + StoreService extraction)

| Gate | Effort | Finding Closed |
|------|--------|---------------|
| S1: Content scan on store/correct (move into StoreService) | Low | Already exists, relocate |
| S2: Rate limiting in service layer | Medium | F-09, U-03, U-06, M-05 |
| S5: AuditContext + structured audit in services | Medium | U-08, F-21, F-23 |
| Capability check for maintain=true | Low | F-04, M-08 |

### Wave 3 (with module reorganization)

| Gate | Effort | Finding Closed |
|------|--------|---------------|
| Unified capability model (SessionWrite separation) | Medium | U-09 |
| UDS fixed capabilities (Read+Search+SessionWrite) | Low | U-09 |
| Audit log for UDS auth failures | Low | F-23 |
| Session ID transport-prefixing (X-05) | Low | X-05 |

### Wave 4 (convergence)

| Gate | Effort | Finding Closed |
|------|--------|---------------|
| Service-bypass lint/review gate | Low | X-01 |
| Rate limiter per-transport CallerId | Low | X-03 |
| Audit chain integrity (monotonic IDs) | Medium | X-02 |

---

## Summary: What Changes in Each Document

### security-audit.md additions needed

- **F-25 (NEW, Medium)**: UDS path performs no content scanning — search queries from user prompts bypass all injection detection
- **F-26 (NEW, Medium)**: UDS path has no authorization model — any UID-authenticated connection has full access
- **F-27 (NEW, Low)**: UDS path has no input validation on query strings (only session_id sanitized)
- **F-28 (NEW, Medium)**: UDS path has no audit trail — operations invisible to AUDIT_LOG
- Reclassify F-09 (rate limiting) from Medium to **High** — affects both paths, and UDS path is more exposed

### server-refactoring-architecture.md additions needed

- Service layer must include Security Gateway (S1-S5) between transport and service
- Capability model section: unified capabilities, transport-mapped defaults
- `SessionWrite` capability to separate session tracking from knowledge writes
- `AuditContext` as required parameter on all service methods
- Transport-prefixed session IDs to prevent cross-contamination
