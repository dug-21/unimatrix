# Roadmap Recommendation: Observation Pipeline & Token Reduction

**Context:** col-001 (Outcome Tracking) is starting now. This recommends how to layer in the observation pipeline from ANALYSIS.md and a token reduction benefit, without disrupting the M5 sequence.

---

## The Layering Problem

col-001 builds **explicit** outcome tracking — agents call `context_store(category: "outcome", ...)`. The observation pipeline produces **implicit** signals — Unimatrix watches what agents do. These are complementary signal sources, not competing designs. The question is sequencing.

The M5 dependency chain today:

```
col-001 (Outcome Tracking)      ← starting now
  └→ col-002 (Retrospective Pipeline)
       └→ col-003 (Process Proposals)
            └→ col-004 (Feature Lifecycle)
```

---

## Recommendation: Three Interventions

### 1. Design Influence on col-001 (NOW — zero scope change)

col-001's structured tag schema should accommodate observed outcomes without requiring observation infrastructure. This means one addition to the recognized tag keys:

| Key | Values | Purpose |
|-----|--------|---------|
| `source` | `agent`, `observed`, `inferred` | Distinguishes how the outcome was produced |

This is NOT a col-001 scope change. col-001 only validates tags — it doesn't care what produces the entries. Adding `source` to the recognized key set is a single enum variant and 3 lines of validation code. But it means the observation pipeline (whenever it ships) can store observed outcomes into the same OUTCOME_INDEX, queryable alongside explicit outcomes, without retrofitting the tag schema.

**Action:** Add `source` to `OutcomeTagKey` enum and `OutcomeTag` enum during col-001 implementation. Optional tag (not required like `type`). Default assumption when absent: `source:agent`.

### 2. New Feature: ass-010 "Observation Pipeline Research" (parallel to col-001)

A research spike — NOT implementation. Runs alongside col-001 without blocking it.

**Scope:**
- Set up a minimal PostToolUse hook on `Write`, `Edit`, and `Bash` tool calls
- Capture tool I/O to a JSON-lines spool file for 2-3 feature cycles
- Analyze the spool manually to answer:
  - What is the signal-to-noise ratio? (How many tool calls produce actionable knowledge signals?)
  - Can we reliably detect "agent followed convention X" from code diffs?
  - Can we reliably infer gate pass/fail from test output?
  - What does "context utilization" look like? (Did the agent reference briefing content?)
- Produce a findings document with concrete extraction rules

**Why research first:** The ANALYSIS.md identifies 6 potential benefits but they're theoretical. Before building extraction infrastructure, we need to validate that the signal exists and is extractable. A 2-day spike with manual analysis answers this. If the signal quality is poor, we save ourselves from building a pipeline that produces noise.

**Output:** `product/research/ass-010/` with findings, signal quality assessment, and extraction rule candidates.

### 3. New Feature: col-001b "Observation Infrastructure" (between col-001 and col-002)

If ass-010 validates the signal quality, build the minimal observation pipeline before col-002 starts. This gives col-002 both explicit AND observed signals from day one.

```
col-001 (Outcome Tracking)      ← explicit outcomes
  └→ col-001b (Observation Infra) ← observed signals flowing into same OUTCOME_INDEX
       └→ col-002 (Retrospective Pipeline)  ← aggregates BOTH signal types
            └→ col-003 (Process Proposals)
                 └→ col-004 (Feature Lifecycle)
```

**col-001b scope:**
- PostToolUse hook infrastructure (shell script + spool consumer)
- Signal extraction rules (validated by ass-010)
- Auto-generation of `source:observed` outcome entries from extracted signals
- Context utilization tracking (which briefing entries were referenced in subsequent work)
- A new confidence factor: `observed_utilization` (7th stored factor, weight redistributed)

**col-001b is optional.** If ass-010 shows poor signal quality, skip it. col-002 works fine with explicit-only outcomes. The observation pipeline becomes a later enhancement rather than a foundation piece.

---

## Token Reduction: How It Layers In

Token reduction is not a separate feature — it's a **consequence** of observation that materializes across col-001b, col-002, and col-004.

### The Token Problem Today

```
context_briefing → ~2000 tokens of orientation (role duties, conventions, ADRs, patterns)
context_search   → N results × ~200 tokens each
                 → injected as tool_result (user-role messages)
                 → compete with code, test output, other tools for ~200K context window
```

Unimatrix has zero visibility into whether those tokens were useful. The briefing returns the same content structure every time, regardless of what the agent actually needs. We're optimizing for coverage ("include everything relevant") not precision ("include only what helps").

### How Observation Enables Token Reduction

**Stage 1 — Measurement (col-001b):**

A PostToolUse hook on Unimatrix's own tools (`mcp__unimatrix__context_briefing`, `mcp__unimatrix__context_search`) captures what was returned. A PostToolUse hook on `Write` and `Edit` captures what the agent produced. Comparing the two reveals utilization:

- Entry #42 (convention: "use Result<T, AppError>") was in the briefing → agent's code uses `Result<T, AppError>` → utilization = HIGH
- Entry #67 (ADR: "use redb for storage") was in the briefing → agent never references storage → utilization = LOW for this task

Over multiple sessions, each entry accumulates a utilization profile: "useful for architect in design, useless for rust-dev in implementation."

**Stage 2 — Profiling (col-002 input):**

The retrospective pipeline aggregates utilization data into role-phase profiles:

```
architect + design:    ADRs 92%, conventions 41%, patterns 78%
rust-dev + implement:  ADRs 23%, conventions 89%, patterns 95%
tester + testing:      ADRs 8%, conventions 12%, patterns 67%
```

**Stage 3 — Optimization (col-004 or context_briefing enhancement):**

`context_briefing` uses utilization profiles to budget tokens:

- For architect in design: allocate 60% of token budget to ADRs + patterns, 15% to conventions
- For rust-dev in implementation: allocate 60% to conventions + patterns, 15% to ADRs
- For tester in testing: allocate 80% to patterns (test patterns, risk strategies), minimal ADRs

**Projected impact:**

Current: ~2000 tokens per briefing, ~50% relevance (estimated)
Optimized: ~1200-1500 tokens per briefing, ~80% relevance (projected)

At Opus pricing ($15/M input tokens), across a feature cycle with ~50 briefing calls:
- Current: 50 × 2000 = 100K tokens = $1.50
- Optimized: 50 × 1300 = 65K tokens = $0.98

The dollar savings are modest. The real value is **attention efficiency** — less noise in context means better agent performance. The "lost in the middle" effect means that irrelevant entries in a briefing actively degrade utilization of relevant entries. Cutting low-utilization content doesn't just save tokens; it makes the remaining content more effective.

### Where Token Reduction Lives in the Roadmap

| Capability | Feature | Dependency |
|-----------|---------|------------|
| Measure utilization | col-001b | ass-010 validates signal quality |
| Aggregate into profiles | col-002 | col-001b provides utilization data |
| Apply to briefing budget | col-004 or briefing enhancement | col-002 provides profiles |

Token reduction is not a single feature — it's a feedback loop that materializes incrementally. The key enabler is col-001b's utilization measurement. Without it, the other stages have no data to work with.

---

## Revised M5 Sequence

```
col-001   Outcome Tracking              ← starting now (explicit outcomes)
  |
  +-- ass-010  Observation Research      ← parallel spike (2 days, manual analysis)
  |
  └→ col-001b  Observation Infra        ← IF ass-010 validates signal (1 week)
       |                                   (observed outcomes + utilization measurement)
       └→ col-002  Retrospective Pipeline ← aggregates both signal types
            |                               + utilization profiles
            └→ col-003  Process Proposals ← evidence from explicit + observed
                 └→ col-004  Feature Lifecycle ← token-optimized briefings
```

### What Changes vs. Current Roadmap

| Item | Change | Risk |
|------|--------|------|
| col-001 scope | Add `source` tag key (~3 lines of code) | Negligible |
| ass-010 (new) | Research spike, 2 days, parallel | Zero — doesn't block anything |
| col-001b (new) | Optional feature, 1 week, between col-001 and col-002 | Low — skippable if signal quality is poor |
| col-002 scope | Designed to accept both signal types (richer input) | Low — observation data is additive, not required |
| col-004 scope | Token-optimized briefings (if utilization data exists) | Low — enhancement, not core scope |

### What Does NOT Change

- PRODUCT-VISION.md milestone structure (M5 remains M5)
- col-001 through col-004 core scopes
- M5 dependency on M4 (complete)
- The M5→M6→M7 chain

---

## Decision Points

1. **Now:** Accept/reject the `source` tag addition to col-001's tag schema.
2. **During col-001 implementation:** Kick off ass-010 in parallel (or defer).
3. **After ass-010 findings:** Decide whether col-001b is worth building or skip to col-002.
4. **During col-002 design:** Incorporate utilization profiles if col-001b shipped, or design for explicit-only if it didn't.

Each decision point has a clear "skip" path. Nothing is irreversible. The worst case is: ass-010 shows poor signal quality, we skip col-001b, and M5 proceeds exactly as originally planned with explicit-only outcomes.
