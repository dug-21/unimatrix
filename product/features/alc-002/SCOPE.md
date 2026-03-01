# alc-002: Agent Enrollment Tool

**Issue:** #46 — Agent registry lacks enrollment tool; spawned agents blocked from writes
**Phase:** Alcove (Agent management & profiles)
**Type:** Feature (bug-driven)

## Problem Statement

Spawned swarm agents (`uni-architect`, `uni-specification`, `uni-rust-dev`, etc.) that pass their `agent_id` to Unimatrix write tools (`context_store`, `context_correct`, `context_deprecate`) are blocked by the capability system.

Auto-enrolled agents receive `TrustLevel::Restricted` with only `[Read, Search]` capabilities (`registry.rs:167-172`). Only `"system"` and `"human"` — bootstrapped at startup — have `Write` capability. There is no mechanism to promote agents or grant capabilities after server initialization.

**Error:** `Agent '{agent_id}' lacks Write capability. Contact project admin.`

**Impact:** Any agent that tries to store ADRs, outcomes, patterns, or corrections is blocked unless it omits `agent_id` (defaulting to `"human"`), which destroys the per-agent audit trail.

**Current workaround:** PreToolUse hook overrides `agent_id` to `"human"` on all Unimatrix MCP calls.

## Proposed Solution

Add a `context_enroll` MCP tool (10th tool) that allows Admin-level agents to enroll new agents or update existing agents with specific trust levels and capabilities.

### Why an enrollment tool (vs. alternatives)

| Option | Pros | Cons |
|--------|------|------|
| **Enrollment tool** (chosen) | Explicit, auditable, flexible, no config files | Requires Admin caller to enroll each agent |
| Prefix-based auto-enrollment (`uni-*` → Write) | Zero-friction | Implicit rules, hard to audit, no fine-grained control |
| Registry seed file | Declarative, version-controlled | Another config format to maintain, no runtime promotion |

The enrollment tool aligns with the existing security model (Admin-gated operations like `context_quarantine`) and preserves full audit trail.

### Tool Contract

```
context_enroll(
  target_agent_id: string,     # required — agent to enroll/promote
  trust_level: string,         # required — "system" | "privileged" | "internal" | "restricted"
  capabilities: string[],      # required — ["read", "write", "search", "admin"]
  agent_id: string,            # optional — caller (must have Admin)
  format: string               # optional — "summary" | "markdown" | "json"
)
```

**Behavior:**
- If `target_agent_id` does not exist: create new AgentRecord with specified trust level and capabilities
- If `target_agent_id` already exists: update trust level and capabilities (promotion or demotion)
- Caller must have `Admin` capability (same gate as `context_quarantine`)
- Audited via AuditLog

**Security constraints:**
- Cannot remove own Admin capability (prevent lockout)
- Cannot modify `"system"` agent (bootstrap-only)

## Affected Files

| File | Changes |
|------|---------|
| `crates/unimatrix-server/src/registry.rs` | Add `enroll_agent()` method |
| `crates/unimatrix-server/src/tools.rs` | Add `context_enroll` tool, `EnrollParams` struct |
| `crates/unimatrix-server/src/validation.rs` | Add `validate_enroll_params`, `parse_trust_level`, `parse_capabilities` |
| `crates/unimatrix-server/src/response.rs` | Add `format_enroll_success` |

## Acceptance Criteria

1. **Admin agent can enroll a new agent with Write capability** — the enrolled agent can then call `context_store` successfully
2. **Admin agent can promote an existing Restricted agent** — upgrading capabilities without re-enrollment
3. **Non-Admin agents are rejected** — `context_enroll` returns CapabilityDenied for Restricted/Internal callers
4. **System agent is protected** — attempts to modify `"system"` return an error
5. **Self-lockout prevented** — caller cannot remove their own Admin capability
6. **Enrollment is audited** — AuditLog records the operation with caller and target
7. **Existing auto-enrollment unchanged** — unknown agents still get Restricted on first contact with other tools

## Testing Strategy

- Unit tests: `enroll_agent()` create, update, protection rules
- Tool-level tests: `context_enroll` param validation, capability checks, format output
- Integration: enroll agent → verify that agent can call `context_store`
- Full suite: `cargo test --workspace` + clippy

## Dependencies

- No new crates
- No schema changes (uses existing AGENT_REGISTRY table)
- No MCP protocol changes (additive tool)

## Tracking

- GitHub Issue: #46
