## ADR-003: IndexBriefingService Replaces BriefingService — Dependencies and k Default

### Context

`BriefingService` carries three design liabilities that crt-027 resolves:

1. **Semantic k=3 default** via `UNIMATRIX_BRIEFING_K` (default 3, clamped [1,20]). This was
   appropriate for the original human-oriented orientation briefing but is too small for the
   "comprehensive phase handoff" use case that WA-4b introduces. k=20 is needed.

2. **Section-oriented output** (`decisions` / `injections` / `conventions` sections, role-framed
   headers). The section structure was designed for human-readable briefings; it is an obstacle
   for the WA-5 transcript prepend use case, which needs a flat indexed surface it can prepend
   to without parsing structure.

3. **`UNIMATRIX_BRIEFING_K` env var** controls k for the old service. If this var is set to 3
   in a deployment and silently inherited by the new service, the new service returns only 3
   entries instead of 20, defeating the high-k design. The env var must be explicitly deprecated
   and not read by the new service.

**Why full replacement rather than extension:**
- `BriefingService.assemble()` has two callers (confirmed Q4 in SCOPE.md). Both can be
  migrated in this feature. Leaving `BriefingService` as dead code violates the project
  anti-stub rule.
- The new output format (`Vec<IndexEntry>`) is incompatible with the existing
  `BriefingResult` type. An adapter layer adds complexity without benefit.
- `OQ-4` in SCOPE.md explicitly resolves to "delete in this feature."

**EffectivenessStateHandle dependency (SR-03):**
`BriefingService` currently holds `EffectivenessStateHandle` as a required, non-optional
constructor parameter per ADR-004 crt-018b (entry #1546). This invariant must carry forward
to `IndexBriefingService`. Missing wiring is a compile error under this pattern.

`IndexBriefingService` delegates all ranking to `SearchService`, which already carries its
own `EffectivenessStateHandle`. However, `IndexBriefingService` also applies effectiveness
classification directly for sort tiebreakers when ranking injection-history entries and
convention entries outside the semantic search path. This direct usage requires the handle
independently of `SearchService`.

### Decision

**Full replacement:** `services/briefing.rs` is rewritten entirely. The old content
(struct, methods, tests, module-level docs) is deleted. `IndexBriefingService` replaces it.

**Constructor:**
```rust
impl IndexBriefingService {
    pub(crate) fn new(
        entry_store: Arc<Store>,
        search: SearchService,
        gateway: Arc<SecurityGateway>,
        effectiveness_state: EffectivenessStateHandle,  // required, non-optional
    ) -> Self {
        IndexBriefingService {
            entry_store,
            search,
            gateway,
            default_k: 20,   // hardcoded, not from env var
            effectiveness_state,
            cached_snapshot: EffectivenessSnapshot::new_shared(),
        }
    }
}
```

**`UNIMATRIX_BRIEFING_K` fate:** The env var is **deprecated and not read** by
`IndexBriefingService`. The `parse_semantic_k()` function is deleted with `briefing.rs`. The
deprecation is documented in the removal commit message and in a comment in `services/mod.rs`
where the old call to `parse_semantic_k()` was. Operators who have `UNIMATRIX_BRIEFING_K=3`
set in production will see the new k=20 behavior take effect silently on upgrade — this is
the intended behavior.

**ServiceLayer field:** `briefing: BriefingService` → `briefing: IndexBriefingService`.
`with_rate_config()` construction block:
```rust
// OLD:
let semantic_k = briefing::parse_semantic_k();
let briefing = BriefingService::new(
    Arc::clone(&entry_store), search.clone(), Arc::clone(&gateway),
    semantic_k, Arc::clone(&effectiveness_state),
);

// NEW:
let briefing = IndexBriefingService::new(
    Arc::clone(&entry_store), search.clone(), Arc::clone(&gateway),
    Arc::clone(&effectiveness_state),
);
```

The `mcp-briefing` feature flag gates the MCP tool endpoint (unchanged per ADR-001 from
vnc-007, entry #283). `IndexBriefingService` itself is NOT gated — it is always compiled
(the UDS `handle_compact_payload` path is always active). Only the MCP handler code inside
`#[cfg(feature = "mcp-briefing")]` uses the briefing service conditionally.

### Consequences

- `UNIMATRIX_BRIEFING_K` env var is silently deprecated. Operators relying on it to limit
  briefing results should use `max_tokens` parameter instead.
- `IndexBriefingService` tests do not require `parse_semantic_k()`. The semantic k env var
  test (`test_parse_semantic_k_*`) is removed. k behavior is tested by checking default=20
  when no `k` param is supplied.
- `EffectivenessStateHandle` wiring is a compile error if missed — same guarantee as the
  prior `BriefingService` had.
- `services/mod.rs` re-export changes from `pub(crate) use briefing::BriefingService` to
  `pub(crate) use briefing::IndexBriefingService`. Any code referencing `BriefingService`
  by name gets a compile error pointing to the rename.
- `cargo test` without `mcp-briefing` feature must still exercise the UDS CompactPayload
  path (which calls `IndexBriefingService::index`). The CompactPayload path is not
  feature-gated.
