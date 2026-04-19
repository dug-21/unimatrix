# FINDINGS: Canonical Hook Event Name Strategy — Blast Radius Analysis

**Spike**: ass-051
**Date**: 2026-04-14
**Approach**: audit-first (codebase-deep inventory → assessment → recommendation)
**Confidence**: empirical — all findings grounded in enumerated code locations

---

## Findings

### Q: Is the Claude Code canonical name decision correct, or does the blast radius of switching to neutral names turn out to be manageable?

**Answer**: The decision to keep Claude Code names as canonical is correct. The blast radius of switching to neutral names is not prohibitive in raw line count, but it triggers a mandatory DB migration on production data, breaks test isolation, and provides no behavioral benefit — the only new provider (Gemini CLI) maps trivially to existing names. Switching to neutral names now costs a schema version bump and a full row-rewrite migration against the `observations` table with no downstream functional improvement.

**Evidence**: Complete string reference audit below.

---

## Complete String Reference Inventory

### Tier 1 — Definition Site (changing here changes all uses)

**File**: `crates/unimatrix-core/src/observation.rs` — `pub mod hook_type`

| Constant | Value | Change type |
|----------|-------|------------|
| `PRETOOLUSE` | `"PreToolUse"` | Code change only — update constant value, all constant uses pick it up |
| `POSTTOOLUSE` | `"PostToolUse"` | Code change only |
| `POSTTOOLUSEFAILURE` | `"PostToolUseFailure"` | Code change only |
| `SUBAGENTSTART` | `"SubagentStart"` | Code change only |
| `SUBAGENTSTOPPED` | `"SubagentStop"` | Code change only |

Note: many hot-path comparisons use string literals directly (`"PreToolUse"`) rather than the constants. Both the constant definitions and all literal usages must be updated.

### Tier 2 — Storage Write (values go into the DB)

**File**: `crates/unimatrix-server/src/uds/listener.rs` — `extract_observation_fields()`

| Location | What is stored | Change type |
|----------|---------------|------------|
| `hook = event.event_type.clone()` (line ~2684) | Writes the raw `event_type` string to `observations.hook` | Code change AND DB migration |
| Normalization: `post_tool_use_rework_candidate` → `"PostToolUse"` (line ~2753) | Stores `"PostToolUse"` explicitly | Code change (rename target string) |

This is the critical persistence boundary. After any ingest, event names are stored verbatim in `observations.hook`. Existing rows contain `"PreToolUse"`, `"PostToolUse"`, `"SubagentStart"`, `"SubagentStop"`, `"PostToolUseFailure"`. Changing canonical names requires either (a) a migration rewriting all existing rows, or (b) a dual-read path handling both old and new names — which multiplies every downstream match arm.

### Tier 3 — DB Read / SQL Filter (operate on stored values)

**File**: `crates/unimatrix-store/src/query_log.rs`

| Lines | SQL | Change type |
|-------|-----|------------|
| ~253 | `AND o.hook = 'PreToolUse'` in `query_phase_freq_observations` | Code change only — SQL string; not compiler-checked |
| ~300 | `AND hook = 'PreToolUse'` in `count_phase_session_pairs` | Code change only |

Both functions filter to `PreToolUse` rows for the phase-frequency intelligence signal. If stored rows retain old names after a migration, these queries break. If no migration, they must match whichever names exist in the DB.

**File**: `crates/unimatrix-server/src/background.rs` — `fetch_observation_batch()` (lines ~1292–1348)

Reads `hook` column and calls `parse_event_type()` (identity pass-through). Also hardcodes `source_domain = "claude-code"` (~line 1330) — a separate concern targeted in vnc-013 regardless of event name choice. This path is safely insulated: if DB rows are migrated it reads neutral names; if not, it reads old names. Consistent with whichever canonical form exists in the DB.

### Tier 4 — In-Memory Filters (code-change-only; downstream of DB read)

**`crates/unimatrix-server/src/uds/hook.rs`** — `build_request()` match arms:
- `"SessionStart"` → `HookRequest::SessionRegister`
- `"Stop" | "TaskCompleted"` → `HookRequest::SessionClose`
- `"Ping"` → `HookRequest::Ping`
- `"UserPromptSubmit"` — match arm with word-count guard
- `"PreCompact"` — match arm
- `"PostToolUse"` — match arm with rework dispatch
- `hook_type::POSTTOOLUSEFAILURE` — match arm (uses constant)
- `"PreToolUse"` — match arm to `build_cycle_event_or_fallthrough()`
- `"SubagentStart"` — match arm with ContextSearch routing
- `event == "SubagentStart"` — filter (~line 89)

**`crates/unimatrix-server/src/uds/hook.rs`** — `extract_event_topic_signal()`:
- 5 match arms: `"PreToolUse"`, `"PostToolUse"`, `hook_type::POSTTOOLUSEFAILURE`, `"SubagentStart"`, `"UserPromptSubmit"`

**`crates/unimatrix-server/src/uds/listener.rs`** — `extract_observation_fields()`:
- 5 match arms: `"PreToolUse"`, `"PostToolUse" | "post_tool_use_rework_candidate"`, `"SubagentStart"`, `x if x == hook_type::POSTTOOLUSEFAILURE`, `"SubagentStop" | _`

**`crates/unimatrix-server/src/mcp/tools.rs`** — `context_cycle_review`:
- `o.event_type == "SubagentStart"` — filter
- `o.event_type == "PreToolUse"` — filter (×3)

**`crates/unimatrix-server/src/mcp/knowledge_reuse.rs`** (~line 87):
- `record.event_type != "PreToolUse"` — filter

**`crates/unimatrix-observe/src/detection/friction.rs`**:
- `record.event_type == "PreToolUse"` (×3), `== "PostToolUse"`, `== hook_type::POSTTOOLUSEFAILURE` (×2)

**`crates/unimatrix-observe/src/detection/agent.rs`**:
- `== "PostToolUse"`, `== "SubagentStart"`, `== "SubagentStop"`, `== "PreToolUse"` (×2), `== "PostToolUse"` (additional)

**`crates/unimatrix-observe/src/detection/scope.rs`**:
- `== "PostToolUse"` (×2), `== "PreToolUse"` (×2)

**`crates/unimatrix-observe/src/detection/session.rs`**:
- `== "SubagentStart"` — match arm

**`crates/unimatrix-observe/src/metrics.rs`**:
- 20 production comparisons using `hook_type::*` constants (PRETOOLUSE ×13, POSTTOOLUSE ×7, POSTTOOLUSEFAILURE ×3, SUBAGENTSTART ×2) across 12 metric computation paths

**`crates/unimatrix-observe/src/session_metrics.rs`**:
- `!= "PreToolUse"` (×3), `== "PreToolUse"` (×4), `== "SubagentStart"` — 7 comparisons

**`crates/unimatrix-observe/src/extraction/knowledge_gap.rs`**:
- `obs.event_type != "PostToolUse"` — filter

**Total production comparison sites**: 25 named locations across 4 crates. 5 constant definitions. SQL strings in 2 functions.

### Tier 5 — Domain Pack `event_types` (user-facing API)

**`crates/unimatrix-observe/src/domain/mod.rs`** — `builtin_claude_code_pack()`:
```
event_types: vec!["PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"]
```
Code change only — built-in pack. No user-authored domain packs exist today that reference Claude Code event names.

### Tier 6 — Test Fixtures (code-change-only, never persisted)

Approximately 120+ test fixture usages of event name strings across ~30 test modules (`event_type: "PreToolUse".to_string()`, assertions, test observation construction). All code-change-only: no persistence, no migration. Mechanical — a scripted rename handles them.

---

## DB Migration Assessment

### Feasibility

Changing canonical names requires rewriting existing `observations.hook` column values. The SQL is straightforward:

```sql
UPDATE observations SET hook = 'tool.before'       WHERE hook = 'PreToolUse';
UPDATE observations SET hook = 'tool.after'        WHERE hook = 'PostToolUse';
UPDATE observations SET hook = 'tool.after.failure' WHERE hook = 'PostToolUseFailure';
UPDATE observations SET hook = 'agent.start'       WHERE hook = 'SubagentStart';
UPDATE observations SET hook = 'agent.end'         WHERE hook = 'SubagentStop';
```

Reversible: each UPDATE inverts by swapping source and target. No schema column changes required (`hook TEXT NOT NULL` is already generic).

### Schema Version

Requires a schema version bump (v24 → v25) and a new migration test module, following the established pattern in `run_main_migrations`. The `schema_version` counter in the `counters` table must be updated atomically with the row rewrites.

### Performance Profile

The `observations` table has no index on `hook`. Full-table scan required for each UPDATE. At 100K rows: well under 1 second. At 1M rows: estimated 5–30 seconds depending on WAL mode and page size. An index before migration and dropped after would accelerate it but adds DDL complexity.

**Online safety**: SQLite WAL allows reads during UPDATE. Within a migration transaction (consistent with the project pattern), the window is a single atomic commit. The server must not accept traffic during migration — consistent with all existing Unimatrix schema migrations (server start runs migration before accepting connections).

### Risk of Partial Failure

If the migration transaction rolls back (disk full, power loss), `schema_version` stays at v24 and all rows retain pre-migration values. SQLite transaction atomicity guarantees no inconsistent intermediate state. Risk is low — the UPDATEs have no complex joins or subqueries.

---

## Intelligence Layer Coupling Assessment

### Deepest Couplings

`metrics.rs` is the most deeply coupled file: 20 production comparisons across 12 metric computation paths (tool call count, orphan detection, search miss rate, context load, edit bloat, parallel call rate, post-completion work, follow-up issue count, coordinator respawn, phase tracking). Uses `hook_type::*` constants — a constant value change propagates automatically, but test fixtures use raw strings and must be updated separately.

The 21 detection rules across `friction.rs`, `agent.rs`, `scope.rs`, `session.rs` each name event types explicitly. No abstract dispatch — each rule is a string comparison.

### Test Coverage Gaps

The SQL filters in `query_log.rs` are the highest-risk sites. They are:
- Not compiler-checked (SQL string literals)
- Exercised by integration tests that verify result *counts* not column values
- Silent on failure: a wrong hook name returns an empty result set, not an error

If the SQL is updated but existing DB rows are not migrated (or vice versa), the phase-frequency signal silently drops to zero. This is the primary correctness risk, and it is not caught by the type system.

### Behavioral Correctness Risks

In-memory filter sites have no routing-correctness risk: each `event_type == "X"` comparison updated to `event_type == "tool.before"` etc. Match arms are non-overlapping and exhaustive-by-wildcard.

Primary risk: the DB transition window. If any observation is written before migration and read by code expecting new names, it routes to the wildcard `_` arm — unclassified. The migration must be atomic with the code deployment, not asynchronous.

---

## Domain Pack Author Impact

### Current Inventory

1 domain pack exists today: the built-in `"claude-code"` pack (code-defined). Its `event_types` list references Claude Code event names. No user-authored domain packs are known to exist in any deployment.

The `[[observation.domain_packs]]` TOML extension API exists but has zero known users. All existing TOML-domain-pack tests use fictional domain-specific event names (`"incident_opened"`, `"build_started"`) — not Claude Code event names.

### Upgrade Path

If a user had configured a custom domain pack with `event_types = ["PreToolUse"]`, a rename would break `resolve_source_domain()` silently — their events would route to `"unknown"`. Since zero such deployments exist today, this is theoretical risk only.

An alias mechanism (accepting both old and new names for one release in `resolve_source_domain()`) is feasible and low-cost. The right trigger for implementing it is when the API has real users — not today.

---

## Neutral Name Vocabulary Evaluation

Proposed vocabulary from SCOPE.md §5 evaluated:

| Current (Claude Code) | Proposed neutral | Verdict |
|-----------------------|-----------------|---------|
| `PreToolUse` | `tool.before` | Viable. Dot-namespaced, MCP-aligned. |
| `PostToolUse` | `tool.after` | Viable. |
| `PostToolUseFailure` | `tool.after.failure` | Viable, but 3-segment breaks the 2-segment pattern. Alternative: `tool.failure`. |
| `SessionStart` | `session.start` | Viable. |
| `Stop` | `session.end` | Viable. `Stop` is opaque; `session.end` is clear. |
| `TaskCompleted` | `session.end` | Collapses two Claude Code events to one name. Already collapsed in code (`"Stop" | "TaskCompleted"` → SessionClose). Acceptable. |
| `UserPromptSubmit` | `prompt.submit` | Viable. |
| `PreCompact` | `context.compact` | Viable. |
| `SubagentStart` | `agent.start` | Viable. |
| `SubagentStop` | `agent.end` | Viable. |
| `Ping` | `system.ping` | Viable. |

**No collision with existing neutral events**: `cycle_start`, `cycle_phase_end`, `cycle_stop` use underscore-separated naming; proposed neutral names use dot-separated namespacing. No collision.

**Internal event `post_tool_use_rework_candidate`**: normalized to `"PostToolUse"` before DB write. In a neutral-names world, normalized to `"tool.after"` instead. Code-change-only. Not a new problem.

**Overall**: the vocabulary is sound and non-colliding. It is not the obstacle. The DB migration is.

---

## Recommendation

**Option A — Keep Claude Code names as canonical. vnc-013 proceeds as scoped.**

### Evidence

1. **DB migration cost is real.** `observations.hook` stores event name strings verbatim. Changing canonical names requires a schema v24→v25 migration rewriting all existing rows. The SQL is simple, but the migration must be atomic with deployment, requires a new schema version and migration test module, and adds operational risk. This is not a free refactor.

2. **Blast radius is bounded but spans 4 crates.** Complete inventory: 25 production comparison sites across `unimatrix-observe` (detection rules, metrics, session_metrics, extraction), `unimatrix-server` (hook.rs, listener.rs, tools.rs, knowledge_reuse.rs), and `unimatrix-store` (query_log.rs SQL). Plus 5 constant definitions, 4 domain pack event_type entries, and ~120 test fixture strings. This matches the col-023 HookType blast radius — which required a dedicated wave-based refactor PR. Including that inside vnc-013 would more than double its scope.

3. **Neutral names provide zero behavioral benefit today.** All three confirmed OOB providers map cleanly: Codex uses identical event names natively (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop` — per ASS-049 FINDINGS-HOOKS.md §Q1); Gemini maps via 3 new match arms (`BeforeTool` → `PreToolUse`, `AfterTool` → `PostToolUse`, `SessionEnd` → `Stop`). Normalization to neutral names vs. Claude Code names costs identical work at the ingest boundary. Downstream code is identical either way. The difference is only what string the canonical target holds.

4. **The architecture is already provider-agnostic.** The col-023 ADR-001 decision replaced the `HookType` enum with string-based `event_type` fields. The `DomainPackRegistry.resolve_source_domain()` is already a string-matching dispatch. The pipeline does not assume Claude Code semantics — it assumes specific string values. Renaming those strings is mechanical with no semantic improvement.

5. **SQL filter sites are the highest-risk locations.** `query_log.rs` lines ~253 and ~300 contain `AND o.hook = 'PreToolUse'` as literal SQL strings not caught by the compiler. If canonical names change and SQL is updated but existing DB rows are not (or vice versa), the phase-frequency intelligence signal silently drops to zero. This is the category of regression that the col-023 wave-based pattern was designed to prevent.

6. **No user-facing breaking change exists today.** Zero users configure domain packs with Claude Code event names. If neutral names were adopted pre-GA, the breaking change would hit only internal code and tests — not user deployments. This is the strongest argument for Option C. But the behavioral justification for paying that cost is absent: nothing works differently with neutral names, and no existing or planned provider requires them.

### Why Not Option B

The combination of DB migration + 4-crate wave refactor + SQL string risk + 120-fixture test sweep inside the same vnc-013 PR expands scope beyond what can be safely gated. vnc-013's goal — Gemini CLI support — is achievable without touching canonical names.

### Why Not Option C

Option C is technically viable and architecturally correct. But the three confirmed OOB providers (Claude Code, Codex, Gemini) all map cleanly to existing canonical names:

- **Claude Code**: already canonical — no mapping required
- **Codex**: uses identical event names natively (`PreToolUse`, `PostToolUse`, `SessionStart`, `Stop` per ASS-049 FINDINGS-HOOKS.md §Q1) — zero mapping required; the current hook vocabulary *is* Codex's vocabulary
- **Gemini**: different names (`BeforeTool`, `AfterTool`, `SessionEnd`), but maps cleanly with 3 new match arms in `hook.rs` — no canonical name change required

The correct trigger for Option C is: a provider arrives whose events cannot be sanely mapped to existing canonical names, OR a domain-pack author configures `event_types = ["PreToolUse"]` in their config. Neither condition exists today, and the three planned OOB providers do not create it.

---

## Unanswered Questions

None — all six areas from SCOPE.md were addressed.

---

## Out-of-Scope Discoveries

1. **`observations.hook` has no index.** The phase-freq SQL uses `WHERE o.hook = 'PreToolUse'` on an unindexed column. At scale (1M rows), this is a full-table scan on every PhaseFreqTable rebuild tick. An index `CREATE INDEX idx_obs_hook ON observations(hook)` would accelerate this with no behavioral change. Worth a follow-up issue.

2. **`post_tool_use_rework_candidate` is an undocumented internal event type.** `extract_observation_fields()` normalizes it to `"PostToolUse"` before DB write, but the raw string appears in test code. If this event leaked to the DB unhidden (e.g., a code bug), it would be invisible to all downstream queries. A guard assertion at the ingest boundary would prevent this.

3. **The col-023 wave-based refactor process (ADR-004, Unimatrix entry #2906) is the correct template for Option C** if it is ever triggered. Any future neutral-name rename spike should reference it as prior art.

---

## Recommendations Summary

- **Primary (Option A/B/C)**: **Option A** — keep Claude Code names canonical; DB migration + 4-crate blast radius does not justify the switch when all current providers map cleanly and no user is impacted.
- **DB migration**: Feasible and safe if ever needed (atomic SQLite transaction, reversible SQL, no column schema changes) — but unnecessary for vnc-013.
- **Intelligence layer coupling**: `metrics.rs` and detection rules are deepest consumers; `query_log.rs` SQL filters are highest-risk (SQL strings, not compiler-checked).
- **Domain pack author impact**: Zero today; upgrade cliff is theoretical; alias mechanism appropriate only when API has real users.
- **Neutral vocabulary**: Sound and non-colliding; adopt when a provider arrives whose events cannot be cleanly mapped to existing names.
