## ADR-004: Reuse depth Field for path Mode Max-Hop Limit

### Context

path mode needs a caller-supplied hop limit (default 5, range [1, 10]). Two options:

- **Option A**: Add a new field to `GraphParams` (e.g., `path_max_depth: Option<u8>`).
- **Option B**: Reuse the existing `depth: Option<u8>` field.

`depth` was introduced in vnc-018 for neighbors mode (hop depth 1..=10, default 1).
`max_depth` was introduced in vnc-019 for subgraph mode (BFS max depth 1..=10, default 3).
The semantics are identical across all three: a caller-supplied upper bound on BFS hop
count before the traversal terminates.

The existing distinction between `depth` (neighbors) and `max_depth` (subgraph) exists
because:
1. `depth` was in `GraphParams` before `max_depth` was needed.
2. subgraph mode added `max_depth` as a new field (ADR-001 vnc-019) because `depth` was
   already semantically owned by neighbors mode.

For path mode, `depth` is the natural choice:
- neighbors uses `depth` for an equivalent hop limit.
- path mode has no other use for `max_depth` (which is semantically subgraph-specific
  and defaults to 3 rather than the path-appropriate 5).
- Adding `path_max_depth` to `GraphParams` would introduce a fourth name for the same
  concept across four modes — needless struct pollution.

The corrective action required by this decision: `validate_no_unsupported_params` must
now explicitly **reject** `depth` on modes that do not accept it (`chain`, `current`,
`subgraph`, `inverse`, `filter`) with an error naming the correct modes. Previously,
`depth` was silently ignored on these modes — a soft inconsistency that AC-25 requires
correcting. Any caller currently passing `depth` to a non-accepting mode was receiving
incorrect (silent ignore) behavior; they will now receive a helpful error.

### Decision

Reuse `depth: Option<u8>` for path mode. path mode interprets `depth` as the BFS hop
limit with default 5 and range [1, 10].

`validate_no_unsupported_params` is updated:
- `path` arm: accepts `depth` (alongside `from_id`, `to_id`, `edge_types`,
  `resolve_supersessions`).
- `chain`, `current`, `subgraph`, `inverse`, `filter` arms: reject `depth` with message
  "depth is not supported in {mode} mode — use neighbors or path mode".
- `neighbors` arm: continues to accept `depth` (no change).

No new field is added to `GraphParams`.

### Consequences

Easier: No new field in `GraphParams`. Consistent semantics across neighbors and path.
Corrects the silent-ignore inconsistency on non-neighbors modes (AC-25).

Harder: Callers that were silently passing `depth` to chain/current/subgraph/inverse/filter
modes will now receive a validation error. This is intentional and documented — the prior
behavior was silent-ignore which is worse than a helpful error message.
