# ASS-056: Root Cause Analysis — agent_attribution Empty + session_id Linkage Broken in audit_log

**Date**: 2026-05-07
**Feeds**: GH #582 (bug investigation)
**Related**: ASS-050 (security model, session_id/attribution design), vnc-014 PR #577 (attribution write path implementation)

---

## Goal

All questions below are answerable by reading the codebase. Evidence must be pinned to file + line number.

**Q1**: Is the `initialize` override on `UnimatrixServer` (`server.rs:1035`) actually invoked in production stdio sessions? Specifically: is the rmcp 0.16.0 `ServerHandler::initialize` override guaranteed to fire before tool calls, or does rmcp 0.16.0 have a condition under which the default no-op runs instead?

**Q2**: Does `request_context.extensions` contain a `http::request::Parts` entry on the stdio transport path? Trace the rmcp 0.16.0 `RequestContext` construction for stdio (non-HTTP) to confirm whether `get::<http::request::Parts>()` returns `Some` or `None` — and whether the same holds for both the `initialize` context and the per-tool-call context.

**Q3**: Is there any code path (server restart, reconnect, UnimatrixServer clone or re-creation) that would produce a fresh `client_type_map` after `initialize` populates it? The `client_type_map` is an `Arc<Mutex<HashMap<String, String>>>` on `UnimatrixServer` — confirm no re-creation happens between the `initialize` call and subsequent tool calls.

**Q4**: The `session_id` column in `audit_log` is populated from the agent-declared `session_id` parameter (prefixed with `mcp::`), NOT the rmcp-assigned session UUID (see `server.rs:422–430`). The provenance chain for feature-cycle attribution is `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id` — where `sessions` is the Claude Code agent session table and `cycle_events` is populated by `context_cycle`. Confirm: (a) whether `audit_log.session_id` is being written as `mcp::`-prefixed or empty, and (b) whether `sessions` rows are being created for stdio Claude Code sessions and whether those rows have `feature_cycle` populated.

**Q5**: The issue reports "no audit_log entries match the codex-test session_id" — meaning there ARE entries in `audit_log`, but none with the expected session_id. What session_id value ARE the audit_log entries using — `mcp::` prefixed (correct), empty (broken), or the raw rmcp UUID (mis-implementation)? Trace which code path determines what value lands in `audit_log.session_id`.

---

## Breadth

`code-only` — all questions are answerable from the Unimatrix codebase and rmcp 0.16.0 source. No external research needed.

---

## Approach

`investigation` — two tracks, run in sequence:

1. **Unimatrix search first**: Before reading code, search Unimatrix for ADRs, lessons, and conventions tagged with `audit`, `attribution`, `session`, `vnc-014`, or `agent-identity`. Findings must align with or explicitly challenge stored architectural decisions. If a stored ADR contradicts what the code does, that contradiction is a finding.

2. **Code trace**: Trace the production stdio path from startup through `initialize` → `client_type_map` insert → tool call → `build_context_with_external_identity` → `client_type` lookup → `AuditEvent.agent_attribution`. Pin every assertion to a file:line.

---

## Confidence Required

`empirical` — every finding must cite specific code evidence (file:line). Do not produce directional answers without code confirmation. Where a path is non-deterministic (e.g., rmcp 0.16.0 behavior), find the rmcp source in the Cargo registry or vendor directory and cite it.

---

## Target Outputs

FINDINGS.md must contain:

1. **Unimatrix alignment**: Which stored ADRs/lessons are relevant; whether the implementation matches them or diverges; any divergence is itself a finding.
2. **Root cause for Q1**: Confirmed yes/no on whether `initialize` fires in production, with evidence. If it does not fire, identify the exact condition.
3. **Root cause for Q2**: Whether `http::request::Parts` is present in stdio `RequestContext.extensions` for both `initialize` and tool calls — confirmed by tracing rmcp source.
4. **Root cause for Q3**: Confirmation or refutation that `client_type_map` persists across the session. If there is a re-creation path, identify it.
5. **Diagnosis for Q4/Q5**: What `session_id` values ARE being written to `audit_log`. Confirm whether the two-hop provenance chain (`audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`) is structurally intact or broken.
6. **Fix targets**: For each confirmed root cause, identify the exact file:line that needs to change. Do not implement fixes — scope only.
7. **Blast radius**: Which other features/paths are affected by the same defects (e.g., HTTPS transport, REST path from ASS-053).

---

## Constraints

**Hard**:
- Researcher reads code only — no code changes, no Unimatrix writes.
- rmcp 0.16.0 is the pinned version. Behavior differences in other rmcp versions are out of scope.
- `agent_attribution` must come from transport-attested `clientInfo.name` only — not from the `agent_id` parameter. This is a security boundary (ASS-050 ADR-002); do not challenge it.

**Hypothesis** (challengeable):
- The root cause is a key mismatch in `client_type_map` between the `initialize` path and the tool call path.
- The `session_id` linkage issue is a separate defect from the `agent_attribution` defect — they share a root cause in context construction but are independent failure modes.

---

## Dependencies

None — this spike is standalone.

---

## Prior Art

- **ASS-050 FINDINGS.md**: Security model review. Section 5 contains the corrected provenance chain design: `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`. The session_id in audit_log is agent-declared (prefixed `mcp::`), NOT the rmcp UUID. This was confirmed by code read during vnc-014 design review.
- **vnc-014 PR #577**: Implemented `agent_attribution` write path. Added `agent_attribution TEXT NOT NULL DEFAULT ''` to `audit_log`. Added `initialize` override on `UnimatrixServer`. Added `client_type_map: Arc<Mutex<HashMap<String, String>>>`.
- **GH #582**: The bug report. Evidence: 3,759 audit_log entries all with `agent_attribution = ''`. Codex session (2026-05-07) confirmed to send `clientInfo.name`. Sessions table has correct `session_id` linkage but no `audit_log` entries match the codex-test `session_id`.
- **Code pre-read (scrum-master, 2026-05-07)**:
  - `initialize` is implemented at `server.rs:1035`, inside `impl rmcp::ServerHandler for UnimatrixServer`
  - Map insert at `server.rs:1083`: key = `mcp-session-id` header or `""` for stdio
  - Map lookup at `server.rs:452`: same key extraction logic
  - Both insert and lookup paths use `context.extensions.get::<http::request::Parts>()` with `unwrap_or("")` — the key should be `""` for stdio in both cases
  - Unit tests `SRV-U-02` (server.rs:3296) confirm map is populated correctly in test context
  - BUT: production sessions show `agent_attribution = ''` for ALL entries, including stdio sessions where tests pass
  - This discrepancy suggests the unit tests mock away the failure condition
