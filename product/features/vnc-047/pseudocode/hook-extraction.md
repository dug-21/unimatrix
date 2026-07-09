# C4 — Hook tag extraction (`build_cycle_event_or_fallthrough`)

**File:** `crates/unimatrix-server/src/uds/hook.rs` (~:769; beside goal block :843-880)
**ADR:** ADR-002. **Risks:** R-11, R-03 (integration contract). **AC:** AC-01, AC-02.

## Purpose

On a `context_cycle` **Start** only, extract `tags` from `tool_input`, drop empty/whitespace-only
strings (value-opacity: non-empty is the ONLY check), and — if any survive — set
`payload["tags"]` as a JSON array so the listener can persist them. Parity with how `goal` is read
from `tool_input` (:844) and written to `payload["goal"]` (:877-880). The hook runs OUTSIDE the tokio
runtime and MUST NEVER fail — extraction is a pure filter, infallible.

## Placement

Add a "Step 4c" block beside the existing goal "Step 4b" (:843-858), and a payload write beside the
`payload["goal"]` set (:877-880). Read from `tool_input` (the `HookInput.extra["tool_input"]` already
bound at :792), NOT from `CycleParams` (SR-03 — CycleParams never carries the persisted value).

## Pseudocode

```
# Step 4c: extract tags for cycle_start events only (parity goal Step 4b).
# tool_input is the serde_json::Value bound at hook.rs:792.
let tags_vec: Vec<String> =
    if validated.cycle_type == CycleType::Start:
        tool_input.get("tags")
            .and_then(|v| v.as_array())            # non-array / missing → None → []
            .map(|arr|
                arr.iter()
                   .filter_map(|v| v.as_str())      # non-string elements dropped
                   .map(|s| s.to_string())
                   .filter(|s| !s.trim().is_empty()) # value-opacity: only non-empty survive
                   .collect())
            .unwrap_or_default()
    else:
        Vec::new()                                  # PhaseEnd / Stop never extract tags (FR-4)

# … existing payload construction (feature_cycle, phase, outcome, next_phase, goal) …

# beside payload["goal"] (:877-880): only set the key when at least one tag survived,
# so a tagless / all-blank start omits the key entirely (keeps EXISTS-false → tagless-no-lock).
if !tags_vec.is_empty():
    payload["tags"] = serde_json::Value::Array(
        tags_vec.into_iter().map(serde_json::Value::String).collect())
```

### Notes

- **NO length cap / truncation** (contrast the `goal` `MAX_GOAL_BYTES` truncation at :845-851) —
  value-opacity means no byte cap for tags (vnc-045 SD-8).
- **NO namespace parsing / no `:` handling** — a colon-prefixed tag is treated as an ordinary opaque
  string (AC-07).
- Use `eprintln!` (not `tracing!`) if any diagnostic is ever needed here — the hook runs outside
  tokio (parity :841/:846). In practice this block needs no logging (infallible filter).
- Omitting the key when empty is important: it keeps `payload["tags"]` absent for tagless starts so
  C5 routes them to the unchanged `insert_cycle_event` arm and the whole-set-once lock is not burned.

## Data flow

- **Input:** `tool_input["tags"]` — untrusted JSON (any type).
- **Output:** `payload["tags"]` = JSON array of non-empty strings, OR key absent.
- **Contract with C5:** array-of-strings or absent; any malformed shape already degraded to "no tags"
  here, so C5 also defends but never sees a surprise type.

## Error handling

None — the block cannot fail. Malformed input (`tags` not an array, elements not strings, all blank)
degrades silently to "no tags" (key omitted). Consistent with the hook's must-never-fail contract
(:824).

## Key test scenarios (hints)

1. Start with `tags=["workflow:v1.3","","foo"]` → `payload["tags"] == ["workflow:v1.3","foo"]`
   (empty dropped, others verbatim).
2. Start with `tags=[]` or all-whitespace → key absent from payload.
3. Non-start event (phase-end/stop) carrying `tags` → key absent (FR-4).
4. `tags` present but not an array (e.g. a string/object) → key absent, no panic.
5. Colon-prefixed and bare tags survive identically (no namespace branching, AC-07).
