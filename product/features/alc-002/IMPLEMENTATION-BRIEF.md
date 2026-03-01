# Implementation Brief: alc-002 Agent Enrollment Tool

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/alc-002/SCOPE.md |
| Scope Risk Assessment | product/features/alc-002/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/alc-002/architecture/ARCHITECTURE.md |
| Specification | product/features/alc-002/specification/SPECIFICATION.md |
| Risk Strategy | product/features/alc-002/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/alc-002/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| registry | pseudocode/registry.md | test-plan/registry.md |
| tool | pseudocode/tool.md | test-plan/tool.md |
| validation | pseudocode/validation.md | test-plan/validation.md |
| response | pseudocode/response.md | test-plan/response.md |
| error | pseudocode/error.md | test-plan/error.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Add a `context_enroll` MCP tool (10th tool) that enables Admin-level agents to enroll new agents or update existing agents with specific trust levels and capabilities. This unblocks swarm agents from writing to Unimatrix with proper per-agent identity and audit trails.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Trust level parsing approach | Strict exhaustive matching, no fallback default | SR-04, ARCHITECTURE.md | architecture/ADR-001-strict-trust-level-parsing.md |
| Bootstrap agent protection scope | Both "system" and "human" protected by identity, not trust level | SR-03, ARCHITECTURE.md | architecture/ADR-002-bootstrap-agent-protection.md |

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/registry.rs` | Modify | Add `EnrollResult` struct, `enroll_agent()` method, `PROTECTED_AGENTS` constant |
| `crates/unimatrix-server/src/tools.rs` | Modify | Add `EnrollParams` struct, `context_enroll` tool function |
| `crates/unimatrix-server/src/validation.rs` | Modify | Add `validate_enroll_params()`, `parse_trust_level()`, `parse_capabilities()` |
| `crates/unimatrix-server/src/response.rs` | Modify | Add `format_enroll_success()` function |
| `crates/unimatrix-server/src/error.rs` | Modify | Add `ProtectedAgent` and `SelfLockout` variants, error codes 32004/32005 |

## Data Structures

### EnrollResult (new, registry.rs)

```rust
pub struct EnrollResult {
    /// Whether this was a create (true) or update (false).
    pub created: bool,
    /// The final agent record after enrollment.
    pub agent: AgentRecord,
}
```

### EnrollParams (new, tools.rs)

```rust
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnrollParams {
    /// Agent ID to enroll or update.
    pub target_agent_id: String,
    /// Trust level: "system", "privileged", "internal", "restricted".
    pub trust_level: String,
    /// Capabilities: ["read", "write", "search", "admin"].
    pub capabilities: Vec<String>,
    /// Calling agent (must have Admin).
    pub agent_id: Option<String>,
    /// Response format: "summary", "markdown", "json".
    pub format: Option<String>,
}
```

### New ServerError variants (error.rs)

```rust
/// Attempt to modify a protected bootstrap agent.
ProtectedAgent { agent_id: String },
/// Caller attempted to remove own Admin capability.
SelfLockout,
```

## Function Signatures

### registry.rs

```rust
impl AgentRegistry {
    pub fn enroll_agent(
        &self,
        caller_id: &str,
        target_id: &str,
        trust_level: TrustLevel,
        capabilities: Vec<Capability>,
    ) -> Result<EnrollResult, ServerError>;
}
```

### validation.rs

```rust
pub fn validate_enroll_params(params: &EnrollParams) -> Result<(), ServerError>;
pub fn parse_trust_level(s: &str) -> Result<TrustLevel, ServerError>;
pub fn parse_capabilities(caps: &[String]) -> Result<Vec<Capability>, ServerError>;
```

### response.rs

```rust
pub fn format_enroll_success(
    result: &EnrollResult,
    format: ResponseFormat,
) -> CallToolResult;
```

### tools.rs (tool handler)

```rust
#[tool(
    name = "context_enroll",
    description = "Enroll a new agent or update an existing agent's trust level and capabilities. Requires Admin capability."
)]
async fn context_enroll(
    &self,
    Parameters(params): Parameters<EnrollParams>,
) -> Result<CallToolResult, rmcp::ErrorData>;
```

## Constraints

- No new crate dependencies
- No schema changes (AGENT_REGISTRY table format unchanged)
- Follow existing tool execution pipeline: identity -> capability -> validation -> business logic -> format -> audit
- Error codes 32004 (ProtectedAgent) and 32005 (SelfLockout) must not collide with existing 32001-32003
- `context_enroll` is NOT counted as a write operation in `is_write_operation()` (it is administrative, not a knowledge write)
- All response formats (summary, markdown, json) must be supported

## Dependencies

- No new crates
- Existing infrastructure: `AgentRegistry`, `AuditLog`, `validation.rs`, `response.rs`, `error.rs`
- No cross-crate changes (all changes within `unimatrix-server`)

## NOT in Scope

- Topic/category restrictions on enrolled agents (`allowed_topics`, `allowed_categories` fields not settable)
- Bulk enrollment
- Agent deactivation (`active: false`)
- Agent listing/querying tool
- Automated enrollment rules (prefix matching, pattern-based)

## Alignment Status

All checks PASS. No variances requiring approval. See product/features/alc-002/ALIGNMENT-REPORT.md.
