## ADR-001: `is_in_progress` as `Option<bool>`, Not `bool`

### Context

`context_cycle_review` needs to signal whether the cycle being reviewed is still active
(no `cycle_stop` event exists in `cycle_events`). The naive approach is `is_in_progress: bool`
with `#[serde(default)]`, which defaults to `false` when absent from JSON.

SR-03 identifies this as a semantic corruption hazard: all historical cycles predating col-024
(before cycle_events was introduced) have zero rows in `cycle_events`. Under `bool` semantics,
`#[serde(default)]` silently produces `is_in_progress = false` on every pre-col-024 retro call.
This incorrectly asserts "confirmed complete" for cycles where the evidence is simply absent.

Three states are required:
- `None` — no `cycle_events` rows exist for this cycle; in-progress status is unknown
- `Some(false)` — a `cycle_stop` event exists; cycle is confirmed complete
- `Some(true)` — a `cycle_start` exists but no `cycle_stop`; cycle is in progress

`bool` cannot represent the `None` case. `Option<bool>` maps to all three states cleanly.

### Decision

`is_in_progress: Option<bool>` on `RetrospectiveReport` with
`#[serde(default, skip_serializing_if = "Option::is_none")]`.

Derivation in handler (step 10i): after loading `events: Vec<CycleEventRecord>` in step 10g:
- If `events.is_empty()` → `report.is_in_progress = None`
- Else if `events.iter().any(|e| e.event_type == "cycle_stop")` → `Some(false)`
- Else → `Some(true)`

Formatter behavior:
- `Some(true)` → render `**Status**: IN PROGRESS` in header
- `Some(false)` → render `**Status**: COMPLETE`
- `None` → omit Status line entirely (no blank line)

JSON consumers: `None` deserializes as absent key (not `null`), consistent with all other
optional fields on `RetrospectiveReport`. Consumers who read `is_in_progress` as `bool` via
older schemas default to `false` on absent field — this is the same (harmless) behavior as
before, except they no longer receive a misleading `false` for in-progress cycles; they receive
nothing, prompting them to handle the optional case.

### Consequences

Easier:
- Semantics are unambiguous across three states.
- No false "confirmed complete" on pre-col-024 historical retros.
- Consistent with established `Option<T>` extension pattern on `RetrospectiveReport`.

Harder:
- Formatter must handle three branches (None/Some(true)/Some(false)) rather than one bool check.
- Tests must cover all three states.
