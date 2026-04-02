## ADR-004: nli_detection.rs Module Merge — Deferred to Group 2

Feature: crt-038 — conf-boost-c formula and NLI dead-code removal. Status: Accepted.

### Context

After removing the three dead functions from `nli_detection.rs` (lines ~39–625),
the file retains only:
- `pub(crate) fn format_nli_metadata` (~line 628, 10 lines)
- `pub(crate) fn current_timestamp_secs` (~line 639, 8 lines)
- `pub(crate) async fn write_nli_edge` (~line 532, ~90 lines)
- `async fn write_edges_with_cap` (~line 456, ~80 lines, internal — only called by
  `run_post_store_nli` which is being deleted)

Critically: `nli_detection_tick.rs` line 34 imports three of these from
`nli_detection.rs`:

```rust
use crate::services::nli_detection::{current_timestamp_secs, format_nli_metadata, write_nli_edge};
```

The three symbols must remain in `nli_detection.rs` in their current locations
(as `pub(crate)` functions) for `nli_detection_tick.rs` to compile without changes.

After removal, `write_edges_with_cap` will have no callers (it was only called by
`run_post_store_nli`). The function should also be deleted as dead code.

The question is whether to: (a) leave `nli_detection.rs` as a small module with
three retained helpers, or (b) merge the retained helpers into `nli_detection_tick.rs`
and delete the now-minimal file.

The SCOPE.md Constraints note: "If remaining code drops below 200 lines, consider
merging into `nli_detection_tick.rs` — but do not do so unless the resulting file
length and module boundary are clean. Not required for this feature."

### Decision

**Defer the module merge to Group 2 (tick decomposition, separate feature)**. Do not
move the three retained helpers in crt-038.

Rationale:
1. The three retained helpers are already correctly located and accessible to
   `nli_detection_tick.rs` via the existing cross-module import. Moving them
   introduces a rename in `nli_detection_tick.rs` (import path change) with no
   functional benefit in this feature.
2. The scope of Group 2 (tick decomposition) explicitly includes rationalizing the
   module boundaries between `nli_detection.rs` and `nli_detection_tick.rs`. The
   merge belongs in that feature where the full module structure is being evaluated.
3. Moving files/helpers in a correctness-focused removal feature adds unnecessary
   merge surface and test churn with no formula or reliability benefit.
4. `nli_detection_tick.rs` at its current line count does not need additional
   content pushed into it under the 500-line constraint.

**crt-038 delivery action**: Delete `write_edges_with_cap` (no callers after
`run_post_store_nli` is removed) alongside the three dead functions. Retain
`format_nli_metadata`, `write_nli_edge`, and `current_timestamp_secs` in place.
The module file name and path are unchanged.

The module-level doc comment in `nli_detection.rs` must be updated to reflect the
new state: the file no longer contains `run_post_store_nli` or
`maybe_run_bootstrap_promotion`. The updated doc should describe the file as a
shared helpers module for graph edge operations used by `nli_detection_tick.rs`.

### Consequences

Easier:
- crt-038 delivery scope is bounded; no import path changes in `nli_detection_tick.rs`.
- Group 2 inherits a clean decision record for the merge question.
- `write_edges_with_cap` is deleted alongside the dead functions, keeping the file
  from accumulating callerless helpers.

Harder:
- `nli_detection.rs` will be a small file (likely under 150 lines) with a doc
  comment describing it as a helpers module — an unusual structure that may surprise
  future readers until Group 2 runs.
