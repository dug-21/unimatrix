# ASS-051: Canonical Hook Event Name Strategy — Blast Radius Analysis

**Date**: 2026-04-14
**Tier**: 1 — blocks vnc-013 canonical naming decision
**Feeds**: vnc-013 (canonical event normalization), downstream intelligence layer design
**Related**: ASS-049 (multi-LLM compatibility findings), vnc-013 SCOPE.md

---

## Question

vnc-013 made a deliberate design decision: use Claude Code event names (`PreToolUse`,
`PostToolUse`, `Stop`, etc.) as Unimatrix's canonical event vocabulary, mapping all
other providers' names to them at the ingest boundary. The stated rationale was
minimizing blast radius — all downstream code already operates on these strings.

The alternative: define a provider-neutral canonical vocabulary (e.g., `tool.before`,
`tool.after`, `session.end`) and map all providers — including Claude Code — to it.
This is the architecturally correct position for a multi-LLM product, but the cost of
the change is unknown.

**This spike answers: is the Claude Code canonical name decision correct, or does the
blast radius of switching to neutral names turn out to be manageable?** The output is
a concrete recommendation with full evidence, not a restatement of the tradeoff.

---

## Why It Matters

The vnc-013 canonical name decision is load-bearing. Once the normalization layer ships
with Claude Code names as canonical:

- Event name strings are stored in the `observations.hook` column in production
  databases. Changing canonical names post-ship requires a DB migration touching
  existing rows — a breaking change with no clean rollback.
- All downstream intelligence (SQL queries, detection rules, `context_cycle_review`,
  `knowledge_reuse`, phase frequency tables, behavioral signal emission) is built on
  these string constants. Changing them later cascades through every consumer.
- The `hook_type` string constants in `unimatrix-core` become part of the public API
  surface for domain pack authors configuring `event_types` lists.

Getting this wrong now means the "fix" is a schema migration plus a full audit of every
downstream consumer — after the intelligence layer has grown more complex. The cost of
the correct decision is much lower before vnc-013 ships than after.

---

## What to Explore

### 1. Complete String Reference Audit

Find every location in the codebase that references hook event name strings directly
— either as literals, as `hook_type` constants, or in SQL. For each:

- File and line number
- Whether the string is a **match arm** (branching logic), a **filter** (SQL WHERE or
  Rust conditional), a **storage write** (written to DB), or a **constant definition**
- Whether changing to a neutral name requires a **code change only**, a **DB migration**,
  or a **domain pack API change** (affects `event_types` lists in `config.toml`)

Known locations to start from (not exhaustive — the spike must verify completeness):

**hook.rs `build_request()`**: `"SessionStart"`, `"Stop"`, `"TaskCompleted"`, `"Ping"`,
`"UserPromptSubmit"`, `"PreCompact"`, `"PostToolUse"`, `"PostToolUseFailure"`,
`"PreToolUse"`, `"SubagentStart"` — match arms, normalization site.

**hook.rs `extract_event_topic_signal()`**: `"PreToolUse"`, `"PostToolUse"`,
`"SubagentStart"`, `"UserPromptSubmit"`, `"PostToolUseFailure"` — match arms.

**listener.rs `extract_observation_fields()`**: `"PreToolUse"`, `"PostToolUse"`,
`"post_tool_use_rework_candidate"`, `"SubagentStart"`, `"PostToolUseFailure"`,
`"SubagentStop"` — match arms feeding DB writes to `observations.hook`.

**listener.rs source_domain hardcode**: Not a canonical name, but derives from provider
— include for completeness.

**background.rs `parse_observation_rows()`**: Reads `observations.hook` from DB.
If names change and DB is migrated, this is automatically correct. If DB is NOT
migrated, this breaks.

**background.rs test fixtures**: Literal strings in test data — code change only.

**knowledge_reuse.rs**: `if record.event_type != "PreToolUse"` — code change only.

**tools.rs `context_cycle_review`**: `o.event_type == "SubagentStart"`,
`o.event_type == "PreToolUse"` — code change only, but these are in the intelligence
layer's core query path.

**query_log.rs SQL**: `AND o.hook = 'PreToolUse'` — hardcoded SQL string in at least
two functions. Code change only, but SQL string changes are error-prone and require
careful testing.

**`unimatrix-core/src/observation.rs` `hook_type` constants**: `PRETOOLUSE`,
`POSTTOOLUSE`, `POSTTOOLUSEFAILURE`, etc. — these are the definition site. Changing
these changes the constants everywhere they are used (find all uses).

**`domain/mod.rs` `builtin_claude_code_pack()` `event_types` list**: The domain pack
registry filters by event type. If canonical names change, this list changes — and so
does the `event_types` field in user-authored domain packs in `config.toml`. This is
a **user-facing breaking change**.

**`infra/validation.rs` cycle event constants**: `cycle_start`, `cycle_phase_end`,
`cycle_stop` — these are already neutral. Confirm they are independent of the hook
event naming decision.

**`cycle_events` table**: Stores `cycle_start`, `cycle_stop`, `cycle_phase_end`.
Confirm these are synthetic events, not stored provider event names, and therefore
unaffected by canonical naming changes.

**`observations.hook` column (DB)**: This is the critical persistence boundary. Are
provider event names (e.g., `"PreToolUse"`) stored directly, or are they transformed
before storage? What is the volume of existing rows in a typical production DB? What
does a migration to neutral names look like — is it a simple UPDATE, or does it require
backfilling derived fields?

---

### 2. DB Migration Assessment

If canonical names change, existing `observations` rows contain Claude Code names. New
rows would contain neutral names. Mixed-name DBs break every downstream query unless
migrated.

Assess:
- Is a migration feasible? What SQL does it require? Is it reversible?
- What is the performance profile of the migration on a large `observations` table
  (e.g., 100K rows, 1M rows)? Is it safe to run online or does it require downtime?
- Does the migration require a schema version bump and a new migration test?
- What is the risk of the migration failing partway and leaving the DB in an
  inconsistent state?

---

### 3. Domain Pack Author Impact

The `event_types` field in domain pack configuration (`config.toml` `[domain_packs]`
sections) is a list of event name strings that controls which events are attributed to
a given `source_domain`. If canonical names change, domain pack authors must update
their `event_types` lists.

Assess:
- How many domain packs exist today (built-in and user-configured)?
- What is the upgrade path for existing deployments with custom domain packs?
- Is there a deprecation / alias mechanism that could smooth the transition
  (e.g., accept both old and new names for one release)?

---

### 4. Intelligence Layer Coupling Assessment

The downstream intelligence layer (phase frequency table, co-access graph,
`context_cycle_review`, behavioral signal emission, knowledge reuse metric) was built
assuming specific event name strings. Assess the coupling:

- Which intelligence components have the deepest coupling to specific event name strings?
- Are any of the couplings in a form that would be difficult to test after a rename
  (e.g., SQL WHERE clauses that are not covered by integration tests)?
- Are there any behavioral correctness risks — places where a neutral name would be
  routed incorrectly by existing match logic even after code changes?

---

### 5. Neutral Name Vocabulary Proposal

If the recommendation is to switch to neutral names, produce a concrete proposed
vocabulary that:

- Covers all current Claude Code hook event types
- Is provider-agnostic (does not assume any single client's naming convention)
- Aligns with MCP spec lifecycle concepts where possible
- Does not collide with the existing `cycle_start`, `cycle_phase_end`, `cycle_stop`
  synthetic event names (which must remain unchanged)

Proposed starting point to evaluate (not prescriptive):

| Current (Claude Code) | Proposed neutral | Notes |
|-----------------------|-----------------|-------|
| `PreToolUse` | `tool.before` | MCP lifecycle: before tool invocation |
| `PostToolUse` | `tool.after` | MCP lifecycle: after tool invocation |
| `PostToolUseFailure` | `tool.after.failure` | Error variant |
| `SessionStart` | `session.start` | Session open |
| `Stop` | `session.end` | Session close |
| `TaskCompleted` | `session.end` | Alias — confirm semantics |
| `UserPromptSubmit` | `prompt.submit` | User turn |
| `PreCompact` | `context.compact` | Context window management |
| `SubagentStart` | `agent.start` | Subagent lifecycle |
| `SubagentStop` | `agent.end` | Subagent lifecycle |
| `Ping` | `system.ping` | Internal liveness |

Evaluate this vocabulary against the blast radius findings. If neutral names require
the same code change count but eliminate the DB migration risk (because the migration
can be done in the same feature), the neutral name decision may be correct.

---

### 6. Recommendation

Produce a concrete recommendation — one of:

**A. Keep Claude Code names as canonical.** Evidence: the blast radius is large enough
that the migration and downstream risk outweigh the architectural benefit. Document the
specific risks that make switching inadvisable. vnc-013 proceeds as scoped.

**B. Switch to neutral names, do it in vnc-013.** Evidence: the blast radius is
manageable within vnc-013 scope. The DB migration is straightforward and the code
changes are bounded. Include the migration SQL and updated change list for vnc-013.

**C. Switch to neutral names, but as a separate pre-vnc-013 feature.** Evidence:
the blast radius is manageable but too large to include safely in vnc-013. A dedicated
renaming feature ships first; vnc-013 builds on the neutral foundation.

State which option the evidence supports and why. Do not hedge — the spike's job is
to resolve the decision, not restate the tradeoff.

---

## Output

1. **Complete string reference inventory** — every location, categorized by change
   type (code-only / DB migration / user-facing API), with file and line reference.

2. **DB migration assessment** — feasibility, SQL, performance profile, risk of
   partial failure, schema version implications.

3. **Intelligence layer coupling map** — which components are most deeply coupled,
   test coverage gaps, behavioral correctness risks.

4. **Domain pack author impact** — current pack inventory, upgrade path, alias options.

5. **Neutral vocabulary proposal** — evaluated against the blast radius findings.

6. **Concrete recommendation** — Option A, B, or C with full evidence. This is the
   primary deliverable.

---

## Constraints

- Read the codebase thoroughly. Do not rely on the known locations listed in §1 —
  treat them as starting points and verify that the audit is complete via grep/search.
- Do not propose changes to `cycle_start`, `cycle_phase_end`, or `cycle_stop` — these
  are already neutral synthetic events and are out of scope.
- The recommendation must be grounded in the actual blast radius evidence, not
  architectural preference. If the blast radius is small, say so; if large, say so.
- Do not design the vnc-013 implementation. Scope recommendations only.

---

## Confidence Required

`empirical` — the recommendation must be grounded in data collected from the codebase (enumerated locations, categorized change types, measured migration scope). Reasoning from memory or architectural preference is insufficient.

---

## Breadth

`codebase-deep`

This spike is primarily a codebase audit. It must read and enumerate actual code
locations — not reason from memory or prior spike findings. The inventory in §1 is
a known-locations starting point, not the complete list.

---

## Approach

`audit-first`

Phase 1: Build the complete string reference inventory by reading and searching the
codebase. Do not form a recommendation until the inventory is complete.

Phase 2: Assess DB migration, domain pack impact, and intelligence layer coupling
from the inventory.

Phase 3: Evaluate the neutral name vocabulary against the findings.

Phase 4: Produce the recommendation.

---

## Inputs

- `crates/unimatrix-core/src/observation.rs` — `hook_type` constants (definition site)
- `crates/unimatrix-server/src/uds/hook.rs` — normalization site, `build_request()`
- `crates/unimatrix-server/src/uds/listener.rs` — `extract_observation_fields()`
- `crates/unimatrix-server/src/services/knowledge_reuse.rs`
- `crates/unimatrix-server/src/mcp/tools.rs` — `context_cycle_review` handler
- `crates/unimatrix-server/src/services/query_log.rs` — SQL string references
- `crates/unimatrix-server/src/background.rs` — test fixtures, `parse_observation_rows()`
- `crates/unimatrix-observe/src/domain/mod.rs` — `builtin_claude_code_pack()`
- `product/features/vnc-013/SCOPE.md` — current vnc-013 design and rationale
- ASS-049 FINDINGS-HOOKS.md — provider event name mapping research
