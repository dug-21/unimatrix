# ASS-028: Session-Aware Category Affinity in Search

**Status**: Research complete. Feeds future feature scoping.
**Date**: 2026-03-21

---

## Problem Statement

`context_search` is session-blind. Every query is ranked purely on similarity,
confidence, and co-access history — none of which reflect what the current session
has been doing. An agent deep in a design session (storing decisions and patterns)
gets the same result distribution as an agent in a retro session (storing lessons
and outcomes), even on identical queries.

The session registry already tracks per-session state. The category histogram of
what has been stored in the current session is a real-time signal that indicates
which SDLC phase the session is in — without requiring agents to explicitly declare
it.

This research documents the design for using that signal to apply a
**category affinity boost** in the `context_search` ranking pipeline.

---

## Goals

1. Define the category histogram data model and where it lives in `SessionState`.
2. Describe how the histogram is accumulated on each `context_store` call.
3. Define the category affinity boost formula and its bounds within the ranking pipeline.
4. Map the full plumbing from `session_id` in `SearchParams` → `ServiceSearchParams`
   → `SearchService` → ranking.
5. Describe how the same signal surfaces in the UDS injection path.
6. Identify what is NOT in scope (Markov model training, GNN learning, W1-5 dependency).

---

## Non-Goals

- This spike does NOT design the Markov chain or GNN-based category prediction (W3-1).
- This spike does NOT modify the observation pipeline or edge schema (W1-5).
- This spike does NOT add phase metadata to CoAccess edges.
- This spike does NOT change `context_briefing` behavior.
- This spike does NOT require schema migrations.

---

## Background

### What Already Exists

`session_id` is already present on `SearchParams` (line 64, `mcp/tools.rs`):

```rust
/// Optional session ID (provided by hooks, not agent-reported).
#[serde(default)]
pub session_id: Option<String>,
```

It flows through `build_context()` into `ctx.audit_ctx.session_id`, where it is
used for usage recording and query log entries. It is **not** passed to
`ServiceSearchParams` — the search ranking pipeline never sees it.

`SessionRegistry` (`infra/session.rs`) already maintains per-session state across
calls: co-access windows, topic signals, rework events, agent actions. Category
tracking is the natural next field.

### The Signal

When an agent calls `context_store(category: "decision", ...)` in a session, that
is evidence of where the session is in the SDLC. The sequence:

```
decision × 3 → pattern × 2 → procedure × 1
```

strongly implies a design phase. The sequence:

```
lesson-learned × 4 → outcome × 1
```

implies a retro or post-delivery phase. No phase declaration is needed from the
agent — the histogram is the phase.

---

## Design

### 1. Category Histogram in SessionState

Add a `category_counts: HashMap<String, u32>` field to `SessionState` in
`infra/session.rs`. This is an in-memory, per-session counter — no persistence,
no schema change.

```rust
pub category_counts: HashMap<String, u32>,
```

Add a method to `SessionRegistry`:

```rust
pub fn record_category_store(&self, session_id: &str, category: &str) {
    if let Some(state) = sessions.get_mut(session_id) {
        *state.category_counts.entry(category.to_string()).or_insert(0) += 1;
    }
}
```

Called from the `context_store` tool handler, after the store succeeds, alongside
existing usage recording.

### 2. Session Anchor

The session anchor is `session_id`, injected by the pre-store hook (UDS path) and
available in `StoreParams.session_id`. The store handler already resolves
`session_id` into the audit context. The `record_category_store` call uses the
same resolved session_id.

No changes needed to how `session_id` is injected — hooks already supply it on
both `context_search` and `context_store` calls.

### 3. Plumbing session_id into ServiceSearchParams

`ServiceSearchParams` (`services/mod.rs` or `services/search.rs`) needs one new
optional field:

```rust
pub session_id: Option<String>,
```

In the `context_search` tool handler, pass it through:

```rust
let service_params = ServiceSearchParams {
    // ... existing fields ...
    session_id: ctx.audit_ctx.session_id.clone(),
};
```

`SearchService` receives `session_id`, looks up the session's category histogram
from `SessionRegistry`, and uses it during ranking.

### 4. Category Affinity Boost

The current ranking formula:

```
score = 0.85 · similarity + 0.15 · confidence + co_access_boost
```

The affinity boost is a small additive term applied after NLI re-ranking (or
after co-access boosting in the non-NLI path):

```
score += category_affinity_boost(result.category, session_histogram)
```

**Computing the boost:**

1. Normalize the session histogram to a probability distribution:
   `p(category) = count(category) / total_stores`
2. The boost for a result entry is `p(result.category) * AFFINITY_WEIGHT`
3. `AFFINITY_WEIGHT = 0.02` (max per-entry boost, keeping similarity dominant)

**Example:**

Session histogram: `{decision: 3, pattern: 2}` → total = 5
- A `decision` entry gets boost: `(3/5) * 0.02 = 0.012`
- A `pattern` entry gets boost: `(2/5) * 0.02 = 0.008`
- A `lesson-learned` entry gets boost: `0`

This is small enough that a highly similar but off-phase entry still wins. It
nudges ranking without overriding retrieval quality.

**Cold start (empty histogram):** boost is 0 for all entries — no effect. Existing
behavior is preserved.

### 5. Interaction with NLI Re-ranking

NLI re-ranking (crt-023) applies after HNSW retrieval. The category affinity boost
applies **after** NLI re-ranking, as a final fusion step — so it cannot override
an NLI-validated entailment score. The ordering:

```
HNSW retrieve (k=20) → NLI re-rank (top k) → co-access boost → category affinity boost → return top k
```

This mirrors the existing pipeline ordering for co-access. Both are small additive
adjustments on a ranked list, not re-retrieval steps.

### 6. UDS Injection Path

The UDS pre-compact hook (`uds/hook.rs`) already receives session context. After
this feature, it can include the current category histogram in the injected
synthesis:

```
Recent session activity: decision × 3, pattern × 2 (design phase signal)
```

This surfaces the signal in the model's context without requiring the agent to
query for it. The injection is informational — agents can use it or ignore it.

Implementation: the hook reads the session's category histogram from
`SessionRegistry` at injection time and formats it into the synthesis text. This
is an additive change to the existing injection template.

---

## Gap Analysis Against Current State

| Layer | session_id present? | Notes |
|---|---|---|
| `SearchParams` (MCP input) | ✓ | Line 64, tools.rs |
| `ctx.audit_ctx.session_id` | ✓ | Resolved by `build_context()` |
| `ServiceSearchParams` | ✗ | Field missing — needs adding |
| `SearchService` ranking | ✗ | Never receives session context |
| `SessionState.category_counts` | ✗ | Field missing — needs adding |
| `context_store` → histogram update | ✗ | Call missing — needs adding |
| UDS injection: category summary | ✗ | Template addition needed |

---

## Implementation Scope Estimate

This is a compact, bounded feature. All implementation builds on existing
infrastructure — no new crates, no schema changes, no migrations.

| Component | Work |
|---|---|
| `SessionState.category_counts` + `record_category_store()` | ~1 hour |
| `context_store` handler: call `record_category_store` | ~30 min |
| `ServiceSearchParams.session_id` field | ~30 min |
| `context_search` handler: pass `session_id` through | ~30 min |
| `SearchService`: look up histogram, apply affinity boost | ~2 hours |
| Tests: cold start, empty histogram, affinity ordering | ~2 hours |
| UDS injection template: category summary | ~1 hour |

Total: ~1 day implementation.

---

## What This Is Not (Scope Boundaries)

**Not a Markov model.** The affinity boost is computed directly from the session
histogram. There is no learned transition model — `P(next_category | current_sequence)`
is a W3-1 concern.

**Not phase-annotated CoAccess edges.** This does not write phase context onto
graph edges. That requires W1-5's observation pipeline generalization and is a
separate design decision.

**Not GNN input.** The session histogram feeds into the ranking formula at query
time. It does not produce training data or modify confidence weights.

**Not `context_briefing`.** Briefing is currently underused. The higher-value
surfaces for this signal are search (hot path, every query) and UDS injection
(pre-compact synthesis).

---

## Relationship to Planned Features

| Feature | Relationship |
|---|---|
| W1-5 (Observation Pipeline Generalization) | Independent — no dependency. W1-5 may later add phase-annotated edges that strengthen the signal, but this feature does not require it. |
| W3-1 (GNN Confidence Learning) | This feature produces per-session histograms. W3-1 could use accumulated histograms from `FEATURE_ENTRIES` (per-cycle category sequences) as training signal for category transition modeling. Complements, does not block. |
| GH #329 (pipeline fusion fix) | Independent. #329 fixes co-access overriding NLI in the fusion step. This feature adds a separate, bounded affinity term after that fix is applied. |

---

## Recommendation 2: PreCompact Transcript Restoration

### Problem

When Claude Code compacts the context window, recent conversation history is lost.
The current `PreCompact` hook fires a `CompactPayload` request to the server, which
returns a briefing synthesis — structured knowledge from Unimatrix. What it does
NOT restore is the actual recent conversation: the last few user prompts and
assistant responses the model was actively working through.

### What's Available

Claude Code sends `transcript_path` in every hook's stdin JSON. This field is
parsed by `HookInput` (wire.rs line 60) but **never read or used** anywhere in the
hook processing code. It points to the live session transcript file:

```
~/.claude/projects/{project-slug}/{session-uuid}.jsonl
```

The transcript is a JSONL file — one JSON object per line — with these relevant
record types:

- `type: "user"` — `message.content` array containing `type: "text"` blocks
  (human prompt) or `type: "tool_result"` blocks (tool responses)
- `type: "assistant"` — `message.content` array containing `type: "text"` blocks
  (assistant response), `type: "tool_use"` blocks (tool calls), `type: "thinking"`

**Critical timing:** after compaction, earlier message content is cleared from the
file. The `PreCompact` hook fires BEFORE compaction — the transcript still has
recent content intact at hook execution time.

### Design

The hook reads `transcript_path` locally (no server round-trip) before sending the
`CompactPayload` request:

1. Open `input.transcript_path`
2. Scan backward from end of file
3. Collect last k `{user_text, assistant_text}` pairs — specifically records where
   `type: "user"` has a content block of `type: "text"` with non-empty text
4. Format as a "Recent conversation" block
5. Prepend to the server's `BriefingContent` response before writing stdout

**Extraction logic (reverse scan):**
```
for line in reverse(transcript_lines):
    obj = parse(line)
    if obj.type == "assistant":
        collect assistant text blocks
    if obj.type == "user":
        collect user text blocks (type: "text" only, skip tool_result)
        if both collected → push pair, decrement k
    if k == 0 → stop
```

**Output format injected before briefing:**
```
=== Recent conversation (last 3 exchanges) ===
[User] {prompt text}
[Assistant] {response text}

[User] {prompt text}
[Assistant] {response text}
...
=== End recent conversation ===
```

### Budget

`MAX_INJECTION_BYTES` is 1400 bytes total. Transcript restoration competes with
the briefing synthesis for this budget. Options:

- **Separate budget**: increase injection limit for PreCompact specifically (e.g.,
  3000 bytes) since PreCompact is the highest-value injection point.
- **Fixed k with truncation**: k=2 pairs, hard truncate at 600 bytes, leave 800
  for briefing.
- **Fill-then-truncate**: fill briefing first (highest Unimatrix value), append
  transcript pairs with remaining budget.

Recommended: separate budget for PreCompact. The current 1400-byte limit was set
for general injection, not for the compaction-defense use case where more context
is directly valuable.

### Implementation Scope

All logic lives in the hook process (`uds/hook.rs`). No server changes required.

| Component | Work |
|---|---|
| Read `input.transcript_path` in PreCompact branch | ~30 min |
| Reverse-scan JSONL, extract last k text pairs | ~2 hours |
| Format and prepend to server response | ~30 min |
| Tests: empty transcript, post-compaction transcript, k pairs extraction | ~2 hours |
| Budget decision (separate PreCompact limit) | ~1 hour |

Total: ~1 day.

### Relationship to Recommendation 1

These are independent features. The category histogram (Rec 1) improves search
quality on every query. Transcript restoration (Rec 2) improves context continuity
at compaction time. They share no code and can ship in either order.

---

## Open Questions

**OQ-01: Should affinity boost apply to lookup (`context_lookup`) as well?**
Lookup is deterministic (filter-based), not similarity-ranked. A category filter
already scopes results. The affinity signal is less useful here — probably not.

**OQ-02: Should the histogram decay over time?**
Long-running sessions may shift phase mid-session (design → implementation → retro
in a single agent run). A simple exponential decay on older counts would make the
histogram reflect the *recent* phase rather than the whole session. Start without
decay (simpler), add decay if validation shows stale histograms degrading results.

**OQ-03: Should `AFFINITY_WEIGHT` be configurable?**
Start hardcoded at `0.02`. If eval shows the boost is too strong or too weak,
expose it in `config.toml` under `[inference]`. Don't configure prematurely.

**OQ-04: What happens with multi-category sessions?**
If a session uniformly distributes across all categories (e.g., an orchestrator
storing decisions, patterns, lessons, and outcomes), the per-category probability
is low and the boost is near-zero for all entries. This is correct behavior —
a general session gets general results.
