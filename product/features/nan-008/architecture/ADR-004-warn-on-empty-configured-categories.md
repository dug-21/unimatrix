## ADR-004: Emit `tracing::warn!` When `configured_categories` Is Empty in `compute_cc_at_k`

### Context

`compute_cc_at_k` must return 0.0 when `configured_categories` is empty (SCOPE
constraint 6: division-by-zero guard). The risk (SR-02) is that a profile
TOML that accidentally omits the `[knowledge]` section or specifies an empty
categories list will produce CC@k = 0.0 across all scenarios — a plausible
value that looks like "no categories covered" but actually means
"misconfigured denominator."

`KnowledgeConfig::default()` populates categories with the 7
`INITIAL_CATEGORIES`, so an empty list only arises when a profile explicitly
sets `categories = []` or when the TOML parser produces an empty default in an
unusual path. The risk is low likelihood but high confusion when it occurs.

Two options were considered:
1. `debug_assert!(!configured_categories.is_empty(), "...")` — only fires in
   debug builds; silent in release.
2. `tracing::warn!` — fires in both debug and release builds; appears in
   stderr during `eval run`, surfacing the misconfiguration to the operator
   without panicking.

A panic was rejected because `eval run` should not abort on a
misconfigured profile; the operator should see the warning, inspect the
profile TOML, and re-run. Returning 0.0 silently was rejected because it
produces misleading baseline records.

### Decision

In `compute_cc_at_k`, before the division:

```rust
if configured_categories.is_empty() {
    tracing::warn!(
        "compute_cc_at_k: configured_categories is empty; \
         returning 0.0. Check [knowledge] categories in the profile TOML."
    );
    return 0.0;
}
```

The function remains a pure computation function (no I/O) in the sense that
`tracing::warn!` does not block or allocate in the hot path and is the
project-standard diagnostic mechanism (per rust-workspace.md: "Logging uses
`tracing` macros").

### Consequences

- Empty `configured_categories` is surfaced to the operator via the tracing
  subscriber during `eval run`, not silently swallowed.
- The function still returns 0.0 safely; no panic.
- Unit tests for the empty-categories case verify both the 0.0 return and
  do not assert on tracing output (tracing output is a diagnostic, not a
  semantic contract).
- `tracing` is already a dependency in `unimatrix-server`; no new dependency
  introduced.
