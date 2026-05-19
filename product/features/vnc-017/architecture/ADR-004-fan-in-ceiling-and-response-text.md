## ADR-004: Fan-in Ceiling at N=50 and Zero-Edge Response Text Behavior

### Context

**SR-01 — Fan-in ceiling**
`redirect_graph_edge` opens one RAII `sqlx::Transaction` per call. Under high incoming
edge cardinality (N incoming edges), the redirect loop executes N sequential transactions
inline on the `context_correct` MCP call path. SQLite WAL mode handles small transactions
efficiently; no production entry has been observed with more than a handful of incoming
edges. However, no enforcement mechanism exists. A pathological case (e.g., a hub entry
with hundreds of incoming edges) could add significant latency to the MCP call.

Options considered:
1. Accept unbounded fan-in — consistent with ADR-003 posture, rely on observed cardinality.
2. Warn and skip all redirects above a ceiling — protect latency, surface the issue.
3. Batch redirects into a single transaction — requires refactoring `redirect_graph_edge`
   to accept `&mut Transaction` — out of scope.

**SR-05 — Zero-edge response text**
The resolved OQ-01 appends `"Redirected N incoming edges (M failed, see logs)"` to the
response text. If zero incoming edges exist (AC-08 zero-overhead path), appending
`"Redirected 0 incoming edges"` is noisy. Two options:
1. Emit `"Redirected 0 incoming edges"` unconditionally when the redirect loop runs.
2. Omit the redirect line entirely when found=0.

### Decision

**Fan-in ceiling:** Warn and truncate at N=50.

When `query_incoming_edges` returns more than 50 rows, emit a single `tracing::warn!`
(log the entry ID and total found count), then process only the first 50 rows. The
response text reflects the truncated count: `"Redirected N incoming edges (truncated
from M, see logs)"` if truncation occurs.

The ceiling of 50 is based on:
- No observed production entry exceeds single-digit incoming edge counts.
- 50 sequential transactions is the worst-case latency budget acceptable inline on an
  MCP call (~50 * ~1ms = ~50ms overhead in the worst case with WAL).
- The ceiling is explicit and documentable; `tracing::warn!` makes it observable without
  aborting the correction.

A future batching optimization (pass `&mut Transaction` to `redirect_graph_edge`) can
raise or remove the ceiling without changing the decision posture.

**Zero-edge response text:** Omit the redirect line entirely when found=0.

The response text is unchanged from the current `format_correct_success` output when no
incoming edges are found. This preserves the zero-overhead path (AC-08) at the response
layer, not just at the loop layer.

### Consequences

Easier: the fan-in ceiling provides an explicit latency bound and surfaces pathological
cases in logs; the zero-edge path is silent and backward-compatible.

Harder: entries with more than 50 incoming edges receive only partial auto-redirect;
the remaining stale edges persist until manually redirected or the ceiling is raised.
The `DependencyOnDeprecated` rule will continue to detect the unredirected edges. This
is an accepted degraded state — the correction itself always succeeds.

**Constant to define in tools.rs:**
```rust
/// Maximum incoming edges to auto-redirect per context_correct call (SR-01 ceiling).
/// Entries with more than this many incoming edges emit tracing::warn! and redirect
/// only the first REDIRECT_CEILING rows. See ADR-004 vnc-017.
const REDIRECT_CEILING: usize = 50;
```
