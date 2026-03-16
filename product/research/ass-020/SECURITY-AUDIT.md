# ASS-020 — Security Audit: Implementation Drift from Original Security Principles

**Date:** 2026-03-14 (restructured 2026-03-15)
**Scope:** Unimatrix MCP server — all security-relevant layers
**Method:** Static audit + architectural reasoning session with human
**Issue:** #270

---

## Access Control — Architectural Position

### Why the Capability System Is a Future Capability

The original security model was designed correctly for where the ecosystem is *going*, not where it is today. The capability hierarchy (Restricted → Read → Write → Admin) is meaningful only when `agent_id` is cryptographically bound to the caller.

**Current reality (stdio transport):** `agent_id` is a self-reported string parameter in the MCP tool call. Any caller can pass any value. A malicious caller passes `agent_id: "human"` and every capability gate is bypassed regardless of what the gate checks. The identity layer is unverifiable at the transport level.

**Additional practical constraint:** In a single-repo, local stdio deployment, every caller is the developer's own Claude session. There is no adversarial multi-tenant threat. Requiring agents to self-report correct identity creates friction with no real security benefit — and in practice leads to everyone using `agent_id: "human"` to avoid tool call failures, which makes the capability system vestigial.

**Decision:** Access control findings are classified as **Tier 3 — Future Capability**, blocked on verifiable agent identity in the MCP protocol (OAuth 2.1 bearer tokens, `_meta.agent_id` from a trusted runtime, or HTTPS transport).

### Staged Identity Model — Bridge Strategy

Rather than discarding the architecture, the intended progression is:

| Stage | Identity Mechanism | Access Control State |
|-------|--------------------|---------------------|
| **Current** (stdio, single-repo) | Self-reported `agent_id` per tool call | `PERMISSIVE_AUTO_ENROLL = true`, Write default. Intentional. Documented. |
| **Near-term** (still stdio) | `UNIMATRIX_SESSION_AGENT` env var in `settings.json` | One configured identity per MCP session. Zero per-call friction. Hooks auto-differentiate (`"hook"`, `"background"`). Meaningful project identity without agent burden. |
| **Future** (HTTPS / multi-tenant) | OAuth bearer token claims → same capability pipeline | Flip `PERMISSIVE_AUTO_ENROLL = false`. All gates activate. No code changes to capability logic. |

The env-var bridge eliminates the `"human"` everywhere problem. The MCP server reads `UNIMATRIX_SESSION_AGENT` at startup as the session default; tool calls can override per-call if needed. When moving to HTTPS, the env-var slot is replaced by the token claim — same architectural binding point, different trust mechanism.

**`PERMISSIVE_AUTO_ENROLL = true` is intentional for the current phase** — documented here rather than treated as debt. The code comment at `registry.rs:30` ("In production this should be false") refers to future multi-tenant production, not current single-repo stdio deployment.

---

## Original Principles Summary

The 7-layer defence from `product/PRODUCT-VISION.md` and `product/research/mcp-security/MCP-SECURITY-ANALYSIS.md`:

| Layer | Original Principle | Current Phase Status |
|-------|--------------------|---------------------|
| **Identity** | All agents identified; unknown → Restricted | Intentionally permissive for stdio phase. Future: env-var bridge → token binding. |
| **Access Control** | S1–S5 SecurityGateway; capability gate per tool | Future capability. Gates are in place; identity binding is the missing piece. |
| **Input Validation** | ~50 injection + PII patterns; all write fields scanned | Partially implemented. Tags/topic/source unscanned. Background tick bypasses scanner. **Actionable now.** |
| **Output Framing** | `[KNOWLEDGE DATA]` delimiters on read tools | Never implemented for any read path. **Actionable now.** |
| **Audit Integrity** | Append-only log; all mutations tracked; request_id + hashes | Partially implemented. Coverage gaps. request_id/hash fields absent. |
| **Hash Chain Integrity** | Verified on read | Written but never verified on read. Medium priority. |
| **Feedback / Trust** | Background writes through same gateway as agents | Background tick bypasses gateway entirely. **Actionable now.** |

---

## Restructured Findings

### Tier 1 — Act Now (identity-independent, GH issues filed)

These findings have nothing to do with who is calling. They are structural gaps that would matter in any deployment model, including a fully verified multi-tenant future.

| Priority | Finding | Severity | File:Line | GH Issue |
|----------|---------|----------|-----------|----------|
| **P1** | `context_briefing` output has no `[KNOWLEDGE DATA]` framing — briefing is auto-injected at session start; stored content can overwrite agent instructions (OWASP ASI06) | Critical | `mcp/response/briefing.rs:62–75` | #271 |
| **P2** | `context_search`, `context_lookup`, `context_get` responses have no output framing — original vnc-002 spec required it; never implemented for read paths | High | `mcp/response/entries.rs:39–60` | #272 |
| **P3** | Background tick calls `store.insert()` directly, bypassing `SecurityGateway::validate_write()` — auto-extracted entries receive no S1 content scan | High | `background.rs:1023–1029` | #273 |
| **P4** | `tags`, `topic`, `source` fields not passed through `scan_content()` — metadata injection evades S1 regardless of caller | Medium | `services/gateway.rs:270–295` | #274 |
| **P5** | Hash chain never verified on read — `content_hash` / `previous_hash` written but never checked on retrieval; tampered entries pass undetected | Medium | `mcp/tools.rs` (get/lookup paths) | — |

**P1 + P2 fix** (SD-3 + SD-4): Add `[KNOWLEDGE DATA]` / `[/KNOWLEDGE DATA]` wrapper in `format_briefing()` and the three read formatters in `entries.rs`. Small effort, closes the primary OWASP ASI06 surface.

**P3 fix** (SD-7): Route background tick writes through `SecurityGateway::validate_write()` with `AuditSource::Internal`. The Internal source already exempts rate limiting and capability checks — only S1 content scan applies.

**P4 fix** (SD-5): Pass `tags` (joined), `topic`, and `source` through `scan_content()` in the existing `validate_write()` block.

---

### Tier 2 — Operational Hygiene (low friction, sensible defaults)

Not security boundaries for the current deployment, but worth doing for signal clarity and forward compatibility.

| Item | Finding | Recommendation | Effort |
|------|---------|----------------|--------|
| **OG-1** | `PERMISSIVE_AUTO_ENROLL = true` grants Write to all unknown callers | Intentional for stdio phase. When implementing env-var bridge: set `false` as the production default, `true` for dev via env var. | Near-term |
| **OG-2** | `context_retrospective` has no `require_cap()` call | Add `require_cap(Read)` — not as a security boundary but to prevent inadvertent session telemetry exposure to passive retrieval agents. One line. | Low |
| **OG-3** | `write_count_since()` undercounts mutations (excludes deprecate, quarantine, enroll) | Expand coverage. Matters for forensics when identity becomes verifiable. | Small |
| **OG-4** | `context_status` Admin→Read downgrade (PR #252) has no ADR | Document as intentional. Not a problem; just undocumented. | Trivial |

---

### Tier 3 — Future Capability (blocked on verifiable identity)

These are not debt. They are staged implementation waiting for the MCP ecosystem to provide cryptographically verifiable agent identity.

| Finding | Blocked On |
|---------|-----------|
| Full capability hierarchy enforcement (Write vs Admin gates) | Verifiable `agent_id` — OAuth bearer tokens, `_meta.agent_id` from trusted runtime, or HTTPS transport |
| Persistent rate limit windows | Stable trusted identity to bind limits to across restarts |
| `request_id` in audit records | Verifiable caller identity to make forensic tracing meaningful |
| Per-agent behavioral baselines (anomaly detection) | Stable identity to accumulate baselines against |
| `context_status` Admin-gate re-raise | Restore after identity is verifiable; current Read gate is correct for stdio phase |
| OAuth 2.1 / TLS transport | Future MCP protocol evolution |

---

## Intentional Relaxations (documented)

| Relaxation | Rationale | Documentation Status |
|------------|-----------|---------------------|
| `PERMISSIVE_AUTO_ENROLL = true` | stdio phase; single-repo; self-reported identity makes Write-default no less secure than Restricted-default | Documented here (ADR needed) |
| `context_status` Admin→Read (PR #252) | Usability: status is a read operation; Admin gate caused errors for all non-Admin callers | Filed as fix; no ADR |
| UDS callers exempt from rate limiting (`gateway.rs:58`) | UDS callers are local hook processes; rate limiting a local IPC caller provides no value | Implicit in architecture |
| In-memory rate limit windows | No persistent identity to bind to; windows reset on restart with no security loss at current scale | Implicit in architecture |

---

## Appendix: ADR — Staged Identity Model

**Decision:** Agent identity progresses through three stages (self-reported → env-var configured → token-bound). The capability system is built once and activated when the identity mechanism reaches the appropriate stage.

**Context:** MCP stdio transport does not provide cryptographically verifiable agent identity. Single-repo local deployments have no adversarial multi-tenant threat. Building friction into per-call identity reporting causes developers to default to privileged identities, defeating the purpose.

**Consequences:** Access control findings are deferred to Future Capability. Output framing and content scanning findings (identity-independent) are prioritized for immediate action. The env-var bridge provides a friction-free path to meaningful project-level identity before full token binding is available.

**Files audited:**
- `product/PRODUCT-VISION.md`
- `product/research/mcp-security/` (5 documents)
- `crates/unimatrix-server/src/infra/registry.rs`
- `crates/unimatrix-server/src/infra/scanning.rs`
- `crates/unimatrix-server/src/infra/audit.rs`
- `crates/unimatrix-server/src/services/gateway.rs`
- `crates/unimatrix-server/src/mcp/tools.rs`
- `crates/unimatrix-server/src/mcp/response/entries.rs`
- `crates/unimatrix-server/src/mcp/response/briefing.rs`
- `crates/unimatrix-server/src/background.rs`
