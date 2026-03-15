## ADR-004: EffectivenessStateHandle as Non-Optional Constructor Parameter on BriefingService

### Context

SR-06 identifies a wiring risk: `BriefingService` needs `EffectivenessStateHandle` to apply
the effectiveness tiebreaker to injection history and convention lookup sorts. However, the
briefing semantic search path already delegates to `SearchService`, which holds its own
`EffectivenessStateHandle`. If the constructor wiring for `BriefingService` is missed or
deferred during implementation, the semantic path gains effectiveness but the injection-history
and convention paths regress silently — no compile error, no panic, just wrong behavior.

Two approaches were considered for adding `EffectivenessStateHandle` to `BriefingService`:

1. **Non-optional constructor parameter**: `BriefingService::new()` signature requires
   `EffectivenessStateHandle`. Any caller that does not provide it fails to compile.
   ```rust
   pub(crate) fn new(
       entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
       search: SearchService,
       gateway: Arc<SecurityGateway>,
       semantic_k: usize,
       effectiveness_state: EffectivenessStateHandle,  // required
   ) -> Self
   ```

2. **Optional with graceful degradation**: `BriefingService` takes `Option<EffectivenessStateHandle>`.
   If `None`, the sort uses confidence only (pre-crt-018b behavior). No compile error on missing
   wiring; the old behavior is silently preserved.

The `ConfidenceStateHandle` precedent in `SearchService` is instructive: it was added as a
non-optional parameter when crt-019 was delivered. There is no `Option<ConfidenceStateHandle>` in
`SearchService`. This is deliberate — making it optional would mean search could silently revert
to a hardcoded confidence weight if wiring were missed.

### Decision

`EffectivenessStateHandle` is a **required, non-optional constructor parameter** on
`BriefingService::new()`. Missing wiring is a compile error.

The `ServiceLayer::with_rate_config()` constructor (in `services/mod.rs`) is the call site that
constructs `BriefingService`. It already constructs and passes `ConfidenceStateHandle` to both
`SearchService` and `StatusService`. `EffectivenessStateHandle` follows the same pattern:

1. `EffectivenessService::new_handle()` creates the `Arc<RwLock<EffectivenessState>>`.
2. `ServiceLayer::with_rate_config()` constructs the handle once and passes `Arc::clone(&handle)`
   to `SearchService`, `BriefingService`, and `spawn_background_tick`.
3. The handle is stored in `ServiceLayer` for access by external callers (e.g., main.rs passing
   it to the background tick).

This mirrors the `confidence_state_handle` field on `ServiceLayer` and the
`ServiceLayer::confidence_state_handle()` accessor method.

When `EffectivenessState` is empty (cold start), `BriefingService` receives `None` from
`HashMap::get()` lookups and applies `effectiveness_priority(None) = 0`, which degrades to
confidence-only sort. This is the correct cold-start behavior with no special-casing needed.

### Consequences

Easier:
- Incomplete wiring is caught at compile time, not at runtime or in production.
- The effectiveness signal is guaranteed to be applied uniformly across all three briefing
  paths (injection history, convention lookup, semantic search) or not at all.
- No conditional branches in `BriefingService` for "was the handle provided?"

Harder:
- `ServiceLayer::with_rate_config()` signature grows by one parameter (the handle).
- Any existing test that constructs `BriefingService` directly must be updated to provide the
  handle. Tests that use `ServiceLayer::new()` or `with_rate_config()` require the handle at
  that layer.
- The `spawn_background_tick` function signature also grows by one parameter, which propagates
  to `main.rs`.
