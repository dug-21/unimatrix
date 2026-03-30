## ADR-002: Constants Location — `unimatrix-store/src/read.rs` alongside `EDGE_SOURCE_NLI`

### Context

Three constants must be introduced for the promotion tick:

1. `EDGE_SOURCE_CO_ACCESS: &str = "co_access"` — identifies `GRAPH_EDGES` rows
   originating from co_access promotion (AC-08).
2. `CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` — the promotion threshold (AC-07). Currently
   exists as `const CO_ACCESS_BOOTSTRAP_MIN_COUNT: i64 = 3` in `migration.rs` but is
   `const` (file-private), not `pub const`.
3. `CO_ACCESS_WEIGHT_UPDATE_DELTA: f32 = 0.1` — churn-suppression guard for weight
   updates (SCOPE.md §Design Decision 1).

Two placement options were evaluated for items 1 and 2:

**Option A — Co-locate with `EDGE_SOURCE_NLI` in `read.rs`**: `EDGE_SOURCE_NLI` is
already defined at line 1630 of `unimatrix-store/src/read.rs` and re-exported from
`lib.rs` (`pub use read::{..., EDGE_SOURCE_NLI, ...}`). Adding `EDGE_SOURCE_CO_ACCESS`
and `CO_ACCESS_GRAPH_MIN_COUNT` directly below `EDGE_SOURCE_NLI` keeps all
GRAPH_EDGES source-identification constants in one place.

**Option B — New `constants.rs` sub-module in `unimatrix-store`**: Introduces a
`constants.rs` file in the store crate as a dedicated constants home. Adds a new
`pub mod constants` declaration and re-exports.

The migration's file-private `CO_ACCESS_BOOTSTRAP_MIN_COUNT` stays in `migration.rs`
unchanged (SCOPE.md §Non-Goals: no migration changes). The public `CO_ACCESS_GRAPH_MIN_COUNT`
is a separate symbol — it exists to be consumed by the tick; the migration can reference
it via `unimatrix_store::CO_ACCESS_GRAPH_MIN_COUNT` in the future (or continue using
its private constant for internal bootstrap code).

### Decision

Use **Option A**: add `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` to
`unimatrix-store/src/read.rs` immediately below `EDGE_SOURCE_NLI` at line 1630.
Re-export both from `lib.rs` in the existing `pub use read::{...}` block.

`CO_ACCESS_WEIGHT_UPDATE_DELTA` is placed as a module-private `const` inside
`services/co_access_promotion_tick.rs`. It is an internal churn-suppression parameter,
not a cross-crate identifier. It is not exported from `unimatrix-store` or
`unimatrix-server`.

### Consequences

- `EDGE_SOURCE_NLI`, `EDGE_SOURCE_CO_ACCESS`, and `CO_ACCESS_GRAPH_MIN_COUNT` are
  co-located — any future edge-source constants follow the same pattern.
- `read.rs` exceeds the 500-line guideline (already ~1630+ lines at the time of this
  writing). The existing NOTE comment at line 1627 documents the deferred split.
  Adding two constants does not worsen this materially.
- `CO_ACCESS_GRAPH_MIN_COUNT` is distinct from the migration's private
  `CO_ACCESS_BOOTSTRAP_MIN_COUNT`. If the threshold ever changes, both must be updated;
  this is a known duplication. The migration's constant is intentionally isolated
  (bootstraps are one-shot; changing the migration threshold would be incorrect).
- Option B (new `constants.rs`) is rejected as over-engineering for two constants that
  belong to an established pattern.
