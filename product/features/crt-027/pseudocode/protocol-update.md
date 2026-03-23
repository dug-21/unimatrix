# Protocol Update — Pseudocode
# File: .claude/protocols/uni/uni-delivery-protocol.md

## Purpose

Add `context_briefing(...)` calls at six phase-boundary points in the SM delivery protocol.
This is a text file edit — no code changes. The SM agent reads this protocol and follows it.

Per FR-19, AC-14, NFR-07: every context_briefing call must specify `max_tokens: 1000` to
bound context window consumption at phase boundaries.

---

## Six Insertion Points

### Point 1: After `context_cycle(type: "start", ...)`

**Location**: In the "Initialization" section, step 5, after the context_cycle call block.

**Before**:
```
5. **Declares feature cycle** — before any agent spawning:
   ```
   context_cycle(
     type: "start",
     topic: "{feature-id}",
     next_phase: "spec",
     agent_id: "{feature-id}-delivery-leader"
   )
   ```
6. Plans Stage 3b waves from the IMPLEMENTATION-BRIEF before spawning any implementation agents
```

**After**:
```
5. **Declares feature cycle** — before any agent spawning:
   ```
   context_cycle(
     type: "start",
     topic: "{feature-id}",
     next_phase: "spec",
     agent_id: "{feature-id}-delivery-leader"
   )
   context_briefing(
     topic: "{feature-id}",
     session_id: "{session-id}",
     max_tokens: 1000
   )
   ```
   Include the briefing result as a knowledge package in each spawned agent's context for Stage 3a.
6. Plans Stage 3b waves from the IMPLEMENTATION-BRIEF before spawning any implementation agents
```

### Point 2: After `context_cycle(type: "phase-end", phase: "spec", ...)`

**Location**: In Stage 3a, the "Gate results" block for spec phase-end.

**Before**:
```
**Gate results:**
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", agent_id: "{feature-id}-delivery-leader")`
  2. Commit pseudocode + test plans + updated brief ...
```

**After**:
```
**Gate results:**
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", agent_id: "{feature-id}-delivery-leader")`
  2. `context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)`
     Include briefing result in uni-validator agent context for Gate 3a.
  3. Commit pseudocode + test plans + updated brief ...
```

### Point 3: After `context_cycle(type: "phase-end", phase: "spec-review", ...)`

**Location**: Gate 3a results, after spec-review phase-end.

**Before**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "spec-review", next_phase: "develop", agent_id: "{feature-id}-delivery-leader")`
  2. Commit pseudocode + test plans + updated brief (`pseudocode: component design + test plans (#{issue})`), then proceed to Stage 3b
```

**After**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "spec-review", next_phase: "develop", agent_id: "{feature-id}-delivery-leader")`
  2. `context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)`
     Include briefing result in each uni-rust-dev agent context for Stage 3b.
  3. Commit pseudocode + test plans + updated brief (`pseudocode: component design + test plans (#{issue})`), then proceed to Stage 3b
```

### Point 4: After `context_cycle(type: "phase-end", phase: "develop", ...)`

**Location**: Gate 3b results, after develop phase-end.

**Before**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "develop", next_phase: "test", agent_id: "{feature-id}-delivery-leader")`
  2. Commit all implementation code (`impl: Stage 3b complete (#{issue})`), then proceed to Stage 3c
```

**After**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "develop", next_phase: "test", agent_id: "{feature-id}-delivery-leader")`
  2. `context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)`
     Include briefing result in uni-tester agent context for Stage 3c.
  3. Commit all implementation code (`impl: Stage 3b complete (#{issue})`), then proceed to Stage 3c
```

### Point 5: After `context_cycle(type: "phase-end", phase: "test", ...)`

**Location**: Gate 3c results, after test phase-end.

**Before**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "test", next_phase: "pr-review", agent_id: "{feature-id}-delivery-leader")`
  2. Proceed to Phase 4
```

**After**:
```
- **PASS** →
  1. `context_cycle(type: "phase-end", phase: "test", next_phase: "pr-review", agent_id: "{feature-id}-delivery-leader")`
  2. `context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)`
     Include briefing result in uni-review-pr and uni-docs context for Phase 4.
  3. Proceed to Phase 4
```

### Point 6: After `context_cycle(type: "phase-end", phase: "pr-review", ...)`

**Location**: Phase 4 closing, after pr-review phase-end.

**Before**:
```
context_cycle(type: "phase-end", phase: "pr-review", agent_id: "{feature-id}-delivery-leader")
context_cycle(type: "stop", topic: "{feature-id}", outcome: "Session 2 complete. All gates passed. PR: {url}", agent_id: "{feature-id}-delivery-leader")
```

**After**:
```
context_cycle(type: "phase-end", phase: "pr-review", agent_id: "{feature-id}-delivery-leader")
context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)
context_cycle(type: "stop", topic: "{feature-id}", outcome: "Session 2 complete. All gates passed. PR: {url}", agent_id: "{feature-id}-delivery-leader")
```

---

## Quick Reference Section Update

The "Quick Reference: Message Map" section (~line 534) must also be updated to reflect
the new context_briefing calls. Update the text map to show briefings after each phase-end.

**Before** (excerpt):
```
Init:       Read IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md
            context_cycle(type: "start", topic: "{feature-id}", next_phase: "spec", ...)
Stage 3a:   Task(uni-pseudocode) + Task(uni-tester) — parallel, ONE message
            ...wait for both to complete...
            context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", ...)
            ...PASS → context_cycle(phase-end, spec-review → develop) → commit → Stage 3b
```

**After** (excerpt):
```
Init:       Read IMPLEMENTATION-BRIEF.md + ACCEPTANCE-MAP.md
            context_cycle(type: "start", topic: "{feature-id}", next_phase: "spec", ...)
            context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)
Stage 3a:   Task(uni-pseudocode) + Task(uni-tester) — parallel, ONE message
            ...wait for both to complete...
            context_cycle(type: "phase-end", phase: "spec", next_phase: "spec-review", ...)
            context_briefing(topic: "{feature-id}", session_id: "{session-id}", max_tokens: 1000)
            ...PASS → context_cycle(phase-end, spec-review → develop)
                      context_briefing(topic: "{feature-id}", ..., max_tokens: 1000)
                      → commit → Stage 3b
```

---

## Verification Requirements (AC-14, R-11)

After the edit, these checks must pass:

1. `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` returns ≥ 6.
2. Every `context_briefing` call has `max_tokens: 1000`.
3. Briefing calls appear immediately after (not before) the corresponding `context_cycle` call.
4. No `context_cycle(type: "phase-end", ...)` call is followed immediately by a non-briefing line
   (the briefing must be the very next MCP call).

---

## Error Handling

Not applicable — this is a text file edit. The protocol is read by the SM agent, not executed.

---

## Key Test Scenarios

Static verification only (no automated tests for text files):

**T-PU-01** `six_briefing_calls_present` (AC-14, R-11, non-negotiable):
- `grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md` == 6 or more

**T-PU-02** `all_briefing_calls_have_max_tokens_1000` (NFR-07):
- All `context_briefing` calls have `max_tokens: 1000`
- No call omits the `max_tokens` parameter

**T-PU-03** `briefing_after_cycle_start` (AC-14):
- Visual diff confirms a `context_briefing` call immediately after `context_cycle(type: "start", ...)`

**T-PU-04** `briefing_after_each_phase_end` (AC-14):
- Visual diff confirms `context_briefing` immediately after each of the five `phase-end` calls:
  spec, spec-review, develop, test, pr-review
