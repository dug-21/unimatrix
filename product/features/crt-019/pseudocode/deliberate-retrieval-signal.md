# Component: deliberate-retrieval-signal

**Files**:
- `crates/unimatrix-server/src/mcp/tools.rs`
- `crates/unimatrix-server/src/services/usage.rs`

## Purpose

Two injection points that activate dormant signaling infrastructure:

1. **context_get**: When `params.helpful.is_none()`, fold `helpful = Some(true)`
   into the existing `UsageContext` before the existing `spawn_blocking`. Zero
   new tasks spawned (C-04).

2. **context_lookup**: Pass `access_weight: 2` on `UsageContext`. The
   `record_mcp_usage` path multiplies the access increment by this weight.
   `UsageDedup.filter_access` fires BEFORE the multiplier (C-05).

Both require the `access_weight: u32` field on `UsageContext`, and a capturing
closure for `compute_confidence` in `record_mcp_usage` (R-01).

## Change 1: UsageContext.access_weight Field (services/usage.rs)

Add field to the struct:

```
pub(crate) struct UsageContext {
    pub session_id:    Option<String>,
    pub agent_id:      Option<String>,
    pub helpful:       Option<bool>,
    pub feature_cycle: Option<String>,
    pub trust_level:   Option<TrustLevel>,
    pub access_weight: u32,   // NEW: 1 = normal, 2 = deliberate retrieval
}
```

`access_weight` default is 1, not 0. All existing construction sites must add
`access_weight: 1`. The Rust compiler will reject struct construction with a
missing field, so no site will compile if omitted — but if the codebase uses
`..Default::default()` anywhere, `UsageContext` must NOT implement `Default`
with `access_weight: 0`. Either:
- Do not implement `Default` for `UsageContext`; OR
- Implement `Default` explicitly with `access_weight: 1`.

The safer option is: do not add a `Default` impl — struct literal construction
with all fields explicit is already the pattern in the codebase (IR-03).

## Change 2: record_mcp_usage Access Multiplier (services/usage.rs)

The multiplier is applied inside `record_mcp_usage`, AFTER `filter_access`
(C-05). The dedup-before-multiply ordering is essential: a deduped entry (already
seen by this agent) produces 0 increments, not 2.

```
fn record_mcp_usage(&self, entry_ids: &[u64], ctx: UsageContext):
    let agent_id = ctx.agent_id.clone().unwrap_or_default()

    // Step 1: Dedup access counts FIRST (C-05: dedup before multiply)
    let access_ids = self.usage_dedup.filter_access(&agent_id, entry_ids)
    // access_ids: entries not previously seen by this agent

    // Step 2: Apply access_weight multiplier (NEW)
    // R-11 GATE: Before committing this flat_map approach, verify the store
    // does NOT deduplicate IDs internally. Run the store-layer unit test:
    //   record_usage_with_confidence([42, 42], ...) -> access_count += 2
    // If the store deduplicates, switch to explicit (id, increment) pairs.
    let multiplied_access_ids: Vec<u64> = if ctx.access_weight == 1:
        access_ids.clone()    // no change for normal access
    else:
        // flat_map repeat: each ID appears access_weight times
        // This works ONLY if the store does not deduplicate IDs in its UPDATE loop
        access_ids.iter()
            .flat_map(|&id| std::iter::repeat(id).take(ctx.access_weight as usize))
            .collect()

    // Step 3: Vote processing (unchanged)
    let mut helpful_ids          = Vec::new()
    let mut unhelpful_ids        = Vec::new()
    let mut decrement_helpful_ids   = Vec::new()
    let mut decrement_unhelpful_ids = Vec::new()

    if let Some(helpful_value) = ctx.helpful:
        let vote_actions = self.usage_dedup.check_votes(&agent_id, entry_ids, helpful_value)
        for (id, action) in vote_actions:
            match action:
                VoteAction::NewVote =>
                    if helpful_value: helpful_ids.push(id)
                    else: unhelpful_ids.push(id)
                VoteAction::CorrectedVote =>
                    if helpful_value:
                        helpful_ids.push(id)
                        decrement_unhelpful_ids.push(id)
                    else:
                        unhelpful_ids.push(id)
                        decrement_helpful_ids.push(id)
                VoteAction::NoOp => {}

    // Step 4-5: spawn_blocking with all DB writes
    let store      = Arc::clone(&self.store)
    let all_ids    = entry_ids.to_vec()

    // R-01 CRITICAL: Capture alpha0/beta0 from ConfidenceState BEFORE spawn
    // (on async thread — no blocking concern here)
    let (alpha0, beta0) = {
        let guard = self.confidence_state
            .read()
            .unwrap_or_else(|e| e.into_inner())
        (guard.alpha0, guard.beta0)
    }

    // Pre-compute co-access pairs (unchanged)
    let co_access_pairs = ...  // existing logic

    // Pre-compute feature recording eligibility (unchanged)
    let feature_recording = ...  // existing logic

    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = store.record_usage_with_confidence(
            &all_ids,
            &multiplied_access_ids,   // CHANGED: was &access_ids, now multiplied
            &helpful_ids,
            &unhelpful_ids,
            &decrement_helpful_ids,
            &decrement_unhelpful_ids,
            // R-01 CRITICAL: capturing closure, not bare function pointer
            Some(Box::new(move |entry: &EntryRecord, now: u64| -> f64 {
                compute_confidence(entry, now, alpha0, beta0)
            })),
        ):
            tracing::warn!("usage recording failed: {e}")

        // existing feature_recording and co_access_pairs writes unchanged
    })
```

### R-01: Closure Signature Change

The store's `record_usage_with_confidence` currently takes
`Option<&dyn Fn(&EntryRecord, u64) -> f64>` (a reference to a function pointer).
With the new `alpha0`/`beta0` capture requirement, this must change to
`Option<Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>>`.

The implementation agent must update the store signature in
`crates/unimatrix-store` (or whichever crate owns `record_usage_with_confidence`)
to accept a `Box<dyn Fn>`. Then update all call sites.

The existing call sites that pass `Some(&crate::confidence::compute_confidence)`
(bare function pointer) must become:
```
Some(Box::new(move |entry, now| compute_confidence(entry, now, alpha0, beta0)))
```

The `UsageService::record_mcp_usage` change above is the primary site.
`record_briefing_usage` also calls `record_usage_with_confidence` with a
confidence function — it must similarly be updated to pass a capturing closure
with `alpha0`/`beta0` from `ConfidenceState`.

### UsageService Constructor Update

`UsageService` must gain a `ConfidenceStateHandle` field:

```
pub(crate) struct UsageService {
    store:            Arc<Store>,
    usage_dedup:      Arc<UsageDedup>,
    confidence_state: ConfidenceStateHandle,   // NEW
}

fn UsageService::new(
    store: Arc<Store>,
    usage_dedup: Arc<UsageDedup>,
    confidence_state: ConfidenceStateHandle,   // NEW
) -> Self:
    UsageService { store, usage_dedup, confidence_state }
```

`ServiceLayer::with_rate_config` must pass the handle when constructing
`UsageService`:
```
let usage = UsageService::new(
    Arc::clone(&store),
    usage_dedup,
    Arc::clone(&confidence_state_handle),  // NEW
)
```

## Change 3: context_get Handler (mcp/tools.rs)

The existing handler at line ~601 constructs `UsageContext` with `helpful: params.helpful`.
The change is one line: fold `or(Some(true))` onto the helpful field.

```
// In context_get handler, Step 6 (Usage recording):
// OLD:
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
}

// NEW (C-04: fold before existing spawn, zero new tasks):
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful.or(Some(true)),   // CHANGED
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 1,   // NEW field, normal access for get
}
```

The `.or(Some(true))` semantics:
- `params.helpful = None` -> `helpful = Some(true)` (implicit helpful vote)
- `params.helpful = Some(true)` -> `helpful = Some(true)` (explicit helpful)
- `params.helpful = Some(false)` -> `helpful = Some(false)` (explicit unhelpful honored)

`UsageDedup.check_votes` enforces one vote per agent-entry pair — repeated
`context_get` by the same agent in the same session produces at most one
`helpful_count` increment.

No new `spawn_blocking` or `tokio::spawn` calls are added to the handler (C-04,
R-08 compliance). The diff for this handler must show zero new task spawns.

## Change 4: context_lookup Handler (mcp/tools.rs)

The existing handler at line ~456 constructs `UsageContext`. Add `access_weight: 2`:

```
// In context_lookup handler, Step 6 (Usage recording):
// OLD:
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
}

// NEW:
UsageContext {
    session_id:    ctx.audit_ctx.session_id.clone(),
    agent_id:      Some(ctx.agent_id.clone()),
    helpful:       params.helpful,       // no implicit vote for lookup
    feature_cycle: params.feature.clone(),
    trust_level:   Some(ctx.trust_level),
    access_weight: 2,   // NEW: deliberate retrieval doubles access signal
}
```

No helpful vote is injected for lookup (only access count is doubled per SPEC
FR-07 / domain model table).

## Change 5: All Other UsageContext Construction Sites (mcp/tools.rs)

Every other `UsageContext { ... }` construction in `tools.rs` must add
`access_weight: 1`. These are for `context_search`, `context_briefing`, and any
other tools that call `record_access`. The compiler will catch missing fields.

In `services/usage.rs` tests:
- All `UsageContext { ... }` in test helpers must add `access_weight: 1`.

In `services/usage.rs` `record_briefing_usage`:
- `UsageContext` is not constructed there (it is passed in) — but the function
  itself is called with a `UsageContext`; callers must ensure `access_weight` is
  set appropriately (1 for briefing).

## R-11 Store-Layer Verification Gate

Before the `flat_map` repeat approach is committed, the following store-layer
unit test MUST pass:

```
store_layer_duplicate_id_increments_by_two:
    let store = Store::open(tempdir.path().join("test.db")).unwrap()
    let id = store.insert(test_entry()).unwrap()
    let initial = store.get(id).unwrap().access_count
    // Pass the same ID twice — verifies store does NOT deduplicate
    store.record_usage_with_confidence(
        &[id, id],           // all_ids
        &[id, id],           // access_ids (the doubled list)
        &[], &[], &[], &[],  // no vote changes
        None,                // no confidence fn
    ).unwrap()
    let after = store.get(id).unwrap().access_count
    assert_eq!(after, initial + 2, "store must not deduplicate IDs in access list")
```

If this test fails (store deduplicates), the fallback is to change the store's
`record_usage_with_confidence` signature to accept `&[(u64, u32)]` pairs where
the `u32` is the increment amount, and pass `&[(id, 2)]` for lookup entries.
The implementation agent MUST run this test first and document the result.

## Error Handling

All error handling in this component is fire-and-forget:
- `record_mcp_usage` returns `()` — errors from `spawn_blocking` are logged at
  warn level, never propagated to the MCP caller.
- `ConfidenceState` read lock failure: `unwrap_or_else(|e| e.into_inner())`
  recovers with last-written values (FM-03).
- `access_weight: 0` edge case: `flat_map(iter::repeat(id).take(0))` produces
  an empty access list, suppressing the increment — this is a silent data loss
  (EC-04). Prevent by ensuring `access_weight` is always >= 1 at construction
  sites.

## Key Test Scenarios

```
// AC-08a: context_get implicit helpful vote:
test_context_get_implicit_helpful_vote:
    // Construct handler, call context_get with params.helpful = None
    // Wait for spawn_blocking
    // Assert entry.helpful_count == 1
    // Also verify entry.access_count == 1

test_context_get_explicit_unhelpful_honored:
    // Call context_get with params.helpful = Some(false)
    // Assert entry.unhelpful_count == 1, helpful_count == 0

// AC-08b: context_lookup doubled access:
test_context_lookup_doubled_access_new_entry:
    // New agent, new entry, one lookup call
    // Assert access_count == 2, helpful_count == 0

test_context_lookup_dedup_prevents_second_increment:
    // Same agent, same entry, two lookup calls
    // Assert access_count remains 2 after second call (C-05)
    // (The second call: dedup suppresses access_ids -> empty list -> 0 increment)

test_context_lookup_two_agents_double_count:
    // Two different agents each call context_lookup once
    // Assert access_count == 4 (2 per agent)

// UsageContext default access_weight:
test_usage_context_access_weight_default_is_one:
    // All existing UsageContext construction sites use access_weight: 1
    // Verified by reviewing the diff — compiler catches missing fields

// R-01 integration: capturing closure carries empirical prior:
test_empirical_prior_flows_to_stored_confidence:
    // Setup: create 10 voted entries all helpful
    // Trigger maintenance tick -> ConfidenceState.alpha0 should shift from 3.0
    // Then record access for an unvoted entry
    // Assert stored confidence reflects shifted prior (not cold-start 0.5)
    // (Integration test — see test-infrastructure.md T-INT-04)
```
