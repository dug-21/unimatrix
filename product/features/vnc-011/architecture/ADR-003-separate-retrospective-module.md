## ADR-003: Separate Retrospective Formatter Module

### Context

The existing `format_retrospective_report` function lives in `briefing.rs` (28 lines). The new markdown formatter will be substantially larger (~250-350 lines: header, session table, baseline filtering, finding collapse, phase outliers, knowledge reuse, recommendations, attribution note, plus helper functions and an internal `CollapsedFinding` struct).

Two options:
1. **Extend briefing.rs**: Add the markdown formatter alongside the existing JSON formatter in `briefing.rs`. Keeps retrospective formatting in one file.
2. **New retrospective.rs module**: Create `crates/unimatrix-server/src/mcp/response/retrospective.rs` as a dedicated sub-module of the response layer. `briefing.rs` keeps the JSON formatter and briefing logic.

### Decision

New `retrospective.rs` module. The markdown formatter, its internal types (`CollapsedFinding`), and all helper render functions live in this dedicated module. The existing `format_retrospective_report` (JSON) stays in `briefing.rs` unchanged.

Module registration in `response/mod.rs`:
```rust
#[cfg(feature = "mcp-briefing")]
mod retrospective;

#[cfg(feature = "mcp-briefing")]
pub use retrospective::format_retrospective_markdown;
```

Rationale:
- `briefing.rs` is currently 102 lines serving two distinct concerns (briefing + retrospective JSON). Adding 300+ lines of markdown formatting would make it the largest response module by far.
- The response layer already follows a one-concern-per-module pattern: `entries.rs`, `mutations.rs`, `status.rs`, `briefing.rs`. A dedicated `retrospective.rs` fits this pattern.
- The markdown formatter has its own internal types and helpers that would clutter `briefing.rs`.
- The existing JSON formatter `format_retrospective_report` remains in `briefing.rs` unchanged -- no moves, no breakage.

### Consequences

- Clean separation: `briefing.rs` = briefing + retrospective JSON; `retrospective.rs` = retrospective markdown.
- Two files export retrospective-related functions, which could confuse future developers. Mitigated by the re-export in `mod.rs` which makes both available from the same `response` namespace.
- The `#[cfg(feature = "mcp-briefing")]` gate applies to both modules, keeping the feature flag consistent.
