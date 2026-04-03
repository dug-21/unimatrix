# crt-043 Researcher Report

## Summary

SCOPE.md written to `product/features/crt-043/SCOPE.md`.

The three Group 5 behavioral signal infrastructure items are coherent as a single feature.
Key findings below, with several surprises.

## Key Findings

### Item A — audit_log feature_cycle_id

- `audit_log` table has 8 columns; no `feature_cycle_id`. Schema is at v19. Migration to v20 needed.
- `AuditEvent` struct is in `crates/unimatrix-store/src/schema.rs`. All 8 fields match the table.
- **Surprise**: MCP-path `AuditEvent` construction sites (in `tools.rs`) currently pass
  `session_id: String::new()` — despite `identity.session_id` being available in every handler.
  Both `session_id` and `feature_cycle_id` need to be plumbed simultaneously. There are ~20
  literal construction sites across `tools.rs`, `store_ops.rs`, and `store_correct.rs`.
- Session registry (`SessionRegistry`) is accessible from the MCP server handler context.
  Lookup by `identity.session_id` is O(1). The active feature cycle per session is in
  `SessionState.feature: Option<String>`.
- Two audit write paths: `audit_fire_and_forget` (spawn_blocking, MCP path) and
  `log_event_async` (UDS async path). Both need updating.

### Item B — Goal embedding

- `cycle_events` table: exists, has `goal TEXT` (added v16/col-025). No `goal_embedding` or
  `goal_text` column.
- **Surprise**: Zero HTTP client crates in the workspace. `reqwest`, `ureq`, `hyper` — none
  present. The only GitHub interaction in the codebase is a `github_issue: Option<i64>` column
  in an analytics table (stored number, never fetched from API).
- Two options for GH fetch: (a) `std::process::Command` invoking `gh issue view {N} --json
  title,body`, (b) add `ureq` as a minimal HTTP client dependency. Both require an ADR.
- Embedding pipeline: `embed_service.get_adapter().await` then `adapter.embed_entry("", text)` via
  `rayon_pool.spawn_with_timeout`. This exact pattern is used in search and store_correct — safe
  to reuse for goal embedding.
- **Key constraint**: The goal embedding CANNOT happen in `handle_cycle_event` (UDS listener path,
  40ms budget). It must happen in the `context_cycle` MCP tool handler via `tokio::spawn`
  fire-and-forget. The `insert_cycle_event` call in the UDS path would need to be followed by
  an UPDATE from the MCP path — two-phase write to the same row.
- The `context_cycle` MCP tool (`tools.rs:2127`) receives `CycleParams` including `feature_cycle`
  (the `topic` field). The embed service and rayon pool are on `self`.

### Item C — agent_role mandatory population

- `SessionRecord.agent_role: Option<String>` — nullable. 4/182 sessions populated in prod.
- `build_request("SessionStart", input)` in `hook.rs:345` reads `input.extra["agent_role"]` —
  Claude Code does NOT populate this field automatically.
- **Key finding**: Agent IDs follow a `{feature}-{role}` convention (e.g., `crt-043-researcher`,
  `col-041-architect`). Parsing the suffix at session register time provides a reliable derivation
  path when `agent_role` is absent. This is zero-config and works for all swarm agents.
- SubagentStart hook does NOT create a session — it fires a ContextSearch for injection.
  The subagent's session is registered at its own `SessionStart`. So derivation must happen
  in `handle_session_register`, not the SubagentStart path.
- No protocol currently injects `agent_role` into the `SessionStart` hook payload.

### Schema state

- Current schema version: 19. crt-043 advances to v20 (or v20+v21 if split).
- Migration idempotency pattern: `pragma_table_info` pre-check before `ALTER TABLE ADD COLUMN`
  (established v15, v16 — entry #1264).
- `cycle_events` DDL in both `db.rs` (CREATE TABLE IF NOT EXISTS) and `migration.rs` (ALTER
  TABLE). Both must stay in sync when new columns are added.

## Proposed Scope Boundaries

**In-scope as a single feature**: All three items. Rationale: shared migration event, small
individual footprint (1-3 files each), single PR keeps Group 6 unblocked cleanly.

**Out-of-scope (explicit)**: Retrieval changes, behavioral edges, goal_cluster table,
backfill of existing rows, hard failure on GH fetch errors, agent_role allowlist validation.

## Open Questions for Human

1. **GH fetch mechanism**: `gh` CLI subprocess vs. adding `ureq` crate. Which is preferred?
   `gh` = no new deps, honors existing auth; `ureq` = in-process, no CLI dependency.

2. **GitHub repo detection**: How should owner/repo be determined at runtime?
   Options: (a) parse `git remote`, (b) `GITHUB_REPO=owner/repo` env var, (c) other.

3. **Combined migration (v20) vs. split (v20 + v21)**: Preference for combining audit_log and
   cycle_events changes in one migration step?

4. **`AuditEvent` Default impl**: Add `Default` to reduce boilerplate at ~20 construction sites?

5. **Goal text source**: Issue title only vs. title + body vs. title + first paragraph?
   Affects H1 clustering quality.

6. **agent_role derivation scope**: Server-side only (session registration) or also wire-side
   (hook's `build_request`)? Server-side is cleaner.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #3397, #3396 (col-025 ADRs on goal
  storage), #1264 (idempotent migration pattern). No direct audit_log extension pattern existed.
- Stored: entry #4047 "AuditEvent schema extension requires updating 5 surfaces simultaneously"
  via /uni-store-pattern. Covers the multi-surface update requirement and the latent
  session_id: String::new() debt on the MCP path.
