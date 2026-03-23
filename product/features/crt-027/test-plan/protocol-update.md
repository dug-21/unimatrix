# Test Plan: protocol-update (uni-delivery-protocol.md)

## Component

`.claude/protocols/uni/uni-delivery-protocol.md`

Changes: Add `context_briefing(topic="{feature-id}", session_id: "{session-id}", max_tokens: 1000)`
after each of six call sites:
1. After `context_cycle(type: "start", ...)`
2. After `context_cycle(type: "phase-end", phase: "spec", ...)`
3. After `context_cycle(type: "phase-end", phase: "spec-review", ...)`
4. After `context_cycle(type: "phase-end", phase: "develop", ...)`
5. After `context_cycle(type: "phase-end", phase: "test", ...)`
6. After `context_cycle(type: "phase-end", phase: "pr-review", ...)`

## Risks Covered

R-11 (completeness — all six points present, max_tokens cap on every call)

## ACs Covered

AC-14

---

## Test Expectations

Protocol updates are text file changes — no Rust unit tests apply. All verification is
via static checks.

### Static Verification Tests (R-11)

#### `protocol_context_briefing_count_at_least_six` (R-11 scenario 1, AC-14)
```bash
grep -c "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md
```
**Assert**: Returns `>= 6`

#### `protocol_all_six_insertion_points_present` (R-11 scenario 2, AC-14)
Visual diff inspection must confirm `context_briefing` appears immediately after each of:

| Call Site | Expected Position |
|-----------|------------------|
| `context_cycle(type: "start", ...)` | Briefing call on next logical line |
| `context_cycle(type: "phase-end", phase: "spec", ...)` | Briefing call on next logical line |
| `context_cycle(type: "phase-end", phase: "spec-review", ...)` | Briefing call on next logical line |
| `context_cycle(type: "phase-end", phase: "develop", ...)` | Briefing call on next logical line |
| `context_cycle(type: "phase-end", phase: "test", ...)` | Briefing call on next logical line |
| `context_cycle(type: "phase-end", phase: "pr-review", ...)` | Briefing call on next logical line |

#### `protocol_max_tokens_present_on_every_briefing_call` (R-11 scenario 3, AC-14)
```bash
grep "context_briefing" .claude/protocols/uni/uni-delivery-protocol.md | grep -v "max_tokens: 1000"
```
**Assert**: Returns zero lines — every `context_briefing` call in the protocol specifies
`max_tokens: 1000`.

---

## Behavioral Expectations (Non-Automated)

These are expectations for how the SM will behave after the protocol update. They are
verifiable through process observation, not automated tests.

### Briefing at session start
After `context_cycle(type: "start", ...)`, the SM calls
`context_briefing(topic="{feature-id}", session_id: "{session-id}", max_tokens: 1000)`.
The returned flat indexed table is included in each spawned agent's prompt for the first phase.

### Briefing at every phase-end
After each `context_cycle(type: "phase-end", ...)`, the SM calls briefing before spawning
agents for the next phase. The briefing content is current knowledge for the feature at
the time of the phase transition.

### Budget enforcement
Each briefing call caps at `max_tokens: 1000`. This bounds SM context window consumption
to approximately 4000 bytes per phase boundary (6 boundaries * 1000 tokens max = 6000
tokens added to SM context at most).

---

## Edge Cases

| Scenario | Verification | Expected |
|----------|-------------|----------|
| Protocol has more than 6 `context_briefing` calls | `grep -c` count check | Acceptable — extra calls are acceptable; minimum is 6 |
| A `context_briefing` call is missing `session_id` | `grep` shows call without `session_id` | Flag for correction; `session_id` enables WA-2 histogram boost |
| A `context_briefing` call has `max_tokens` > 1000 | `grep` shows wrong value | Flag for correction (NFR-07 cap is 1000) |
| Phase sequence order matters | Visual inspection | `start` briefing first; `pr-review` briefing last |
