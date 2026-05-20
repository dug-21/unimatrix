## ADR-002: Truncated Response Envelope — Per-Direction Struct

### Context

`chain` mode with `direction="both"` runs two independent CTE sub-queries (forward
and backward from the seed). The 50-hop safety cap applies independently to each
direction branch. AC-03b requires that agents can determine which direction was
capped — a single `truncated: bool` cannot express this.

Three options were considered:

**Option A — Single bool** (`truncated: bool`)
Sets `true` if either direction was capped. Agent cannot tell which direction was
truncated or whether the other direction is complete. AC-03b is untestable — there
is no way to assert "forward was capped but backward was not" against a single bool.

**Option B — Per-direction struct** (`truncated: { forward: bool, backward: bool }`)
Each field is independently set based on whether the corresponding CTE reached its
cap. `direction="forward"` only sets `truncated.forward`; `direction="backward"` only
sets `truncated.backward`; `direction="both"` can set either or both independently.
AC-03b is directly testable.

**Option C — Nullable per-direction** (`truncated: { forward: Option<bool>, backward: Option<bool> }`)
`null` when the direction was not queried. Adds a third state that offers marginal
extra information (was the direction not requested vs. was it requested and not
truncated?) but increases deserialization complexity for callers.

SR-05 in the scope risk assessment explicitly calls out the single-bool as a
spec-blocking ambiguity: "if spec interprets this as a single bool, AC-03b is
untestable."

### Decision

The `chain` mode response carries a `Truncated` struct:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Truncated {
    /// True if the forward direction (descendants) hit the 50-hop cap.
    pub forward: bool,
    /// True if the backward direction (ancestors) hit the 50-hop cap.
    pub backward: bool,
}
```

For `direction="forward"`: only `forward` is set; `backward` is always `false`.
For `direction="backward"`: only `backward` is set; `forward` is always `false`.
For `direction="both"`: both fields are independently set.

`chain` mode response envelope:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ChainResult {
    /// Entries in the chain, ordered by depth from the seed.
    pub entries: Vec<EntryRecord>,
    /// Per-direction truncation status. Both false = full chain returned.
    pub truncated: Truncated,
}
```

`truncated` is always present in the response — never omitted. When the cap does not
fire, `{ forward: false, backward: false }` is returned.

The `current` mode does NOT include a `truncated` field. When the 50-hop cap fires
in `current` mode, the handler returns a structured error (AC-07), not a result with
a truncation indicator. Rationale: `current` mode is a point lookup — returning a
partial result with no terminal would be misleading. An error is the correct signal.

The `neighbors` mode does NOT include a `truncated` field in vnc-018. The 50-hop cap
in `neighbors` mode applies only to `follow_to_current` per-hop walks
(`resolve_supersessions=true`), not to the BFS frontier size. BFS is capped by
`depth` (max 10). A truncation signal for the `follow_to_current` sub-walks would
be noise for callers in the common case; this is left as a future enhancement if
needed.

### Consequences

Easier: AC-03b is directly testable — integration tests can assert
`truncated.forward == true && truncated.backward == false` on a chain where only the
forward branch exceeds 50. Agent callers know exactly which direction to re-query
with a narrower depth.

Harder: the response shape is slightly more complex than a single bool. Serializes
as `"truncated": {"forward": false, "backward": false}` in JSON, not
`"truncated": false`. Callers on #597/#598 that inspect `chain` mode results must
destructure the struct. This is a wire contract — changing it after delivery is a
breaking change.
