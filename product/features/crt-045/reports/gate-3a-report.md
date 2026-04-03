# Gate 3a Report: crt-045

> Gate: 3a (Design Review)
> Date: 2026-04-03
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | Component boundaries, sequencing, and interface contracts all match |
| Specification coverage | PASS | All 8 FRs and 6 NFRs addressed; no scope additions |
| Risk coverage | PASS | All 10 risks mapped to test scenarios; four non-negotiable scenarios present |
| Interface consistency | PASS | Shared types consistent; OVERVIEW.md contracts match per-component pseudocode |
| Key check: rebuild() before with_rate_config() | PASS | Step 5b precedes Step 13 explicitly in pseudocode |
| Key check: write-back idiom correctness | PASS | `*guard = state` inside `handle.write().unwrap_or_else(|e| e.into_inner())` |
| Key check: three-layer assertion | PASS | Layer 1 (handle state) + Layer 2 (graph connectivity) + Layer 3 (live search) all present |
| Key check: Active-entry + edge seeding (C-09) | PASS | Two Active entries + one S1/S2/S8 CoAccess edge explicitly seeded |
| Key check: TOML values (ADR-005) | PASS | `distribution_change = false`, `mrr_floor = 0.2651`, `p_at_5_min = 0.1083` |
| Key check: cycle-abort-safety test | PASS | Returns `Ok(layer)` with `use_fallback=true` asserted |
| Knowledge stewardship compliance | PASS | Both agent reports have stewardship blocks with Queried: and Stored: entries |

## Detailed Findings

### 1. Architecture Alignment

**Status**: PASS

**Evidence**:

- `EvalServiceLayer.md` Step 5b places `TypedGraphState::rebuild(&*store_arc).await` immediately after `SqlxStore::open_readonly()` (Step 5) and before `ServiceLayer::with_rate_config()` (Step 13). This matches ARCHITECTURE.md's component interaction diagram exactly: "Step 5b (NEW): TypedGraphState::rebuild(&store_arc).await" before "Step 13: ServiceLayer::with_rate_config(...)".

- `EvalServiceLayer.md` Step 13b places the write-back after `with_rate_config()` returns, using `inner.typed_graph_handle()` (which returns `Arc::clone` of the same allocation), then `handle.write().unwrap_or_else(|e| e.into_inner())`, then `*guard = state`. This matches ADR-001 (Option B post-construction write) and ARCHITECTURE.md's integration surface entry: "Write-lock swap idiom: `*guard = rebuilt_state` inside `handle.write().unwrap_or_else(|e| e.into_inner())`".

- The new `pub(crate) typed_graph_handle()` accessor delegates to `self.inner.typed_graph_handle()` with no new fields on `EvalServiceLayer`, consistent with C-08 and the precedent of `embed_handle()` / `nli_handle()` (ARCHITECTURE.md lines 113–114).

- Technology choices: no new crates, no `spawn_blocking`, `.await` called directly from async `from_profile()`. Consistent with C-01 and ADR-001.

- Component scope matches ARCHITECTURE.md table: only `layer.rs`, `ppr-expander-enabled.toml`, and `layer_tests.rs` are modified. `services/typed_graph.rs`, `services/mod.rs`, and `search.rs` are read-only references.

### 2. Specification Coverage

**Status**: PASS

**Evidence**:

| FR/NFR | Pseudocode Address | Location |
|--------|-------------------|----------|
| FR-01: rebuild() before with_rate_config() | Step 5b in `EvalServiceLayer.md` | `EvalServiceLayer.md` lines 62–99 |
| FR-02: Write-back via write-lock swap | Step 13b in `EvalServiceLayer.md` | `EvalServiceLayer.md` lines 144–167 |
| FR-03: Rebuild error → warn + Ok(layer) | Two match arms both leave rebuilt_state=None, continue | `EvalServiceLayer.md` lines 83–98 |
| FR-04: info! log on success | `tracing::info!` in Ok(state) arm | `EvalServiceLayer.md` lines 74–81 |
| FR-05: pub(crate) typed_graph_handle() | New accessor declared pub(crate) | `EvalServiceLayer.md` lines 192–203 |
| FR-06: TOML fix | Full TOML content specified | `ppr-expander-enabled-toml.md` lines 43–59 |
| FR-07: New integration test (two Active entries + edge + three-layer assertion) | Two new tests in `layer_tests.md` | `layer_tests.md` lines 117–404 |
| FR-08: Existing tests continue to pass | Existing helper reuse, no modification of make_snapshot_db() | `layer_tests.md` lines 463–465 |

NFR-01 (5s performance): No blocking path introduced; rebuild is pure async DB reads. NFR-02 (memory): One graph allocation; prior cold-start dropped after lock swap. NFR-03 (concurrency): Lock held only for swap duration; rebuild completes before lock is acquired. NFR-04 (observability): info! on success, warn! on failure. NFR-05 (API stability): with_rate_config() signature unchanged, no SearchService field changes. NFR-06 (test suite): No regressions indicated.

No scope additions found. The pseudocode does not implement any unrequested features.

### 3. Risk Coverage

**Status**: PASS

**Evidence**: All ten risks from RISK-TEST-STRATEGY.md are mapped to test scenarios in the test plans.

| Risk | Coverage | Test |
|------|----------|------|
| R-01 (write-back propagation) | Full | Layer 1 + Layer 3 of `test_from_profile_typed_graph_rebuilt_after_construction` |
| R-02 (wired-but-unused) | Full | Three-layer assertion (all layers required per ADR-003) |
| R-03 (Quarantined vacuous pass) | Full | Fixture explicitly uses Active entries + S1/S2/S8 edge; bootstrap_only=0; Quarantined exclusion noted |
| R-04 (rebuild error aborts) | Full | `test_from_profile_returns_ok_on_cycle_error` (called `test_from_profile_rebuild_error_degrades_gracefully` in pseudocode) |
| R-05 (TOML parse failure) | Full | `test_ppr_expander_enabled_profile_parses_cleanly` unit test in `eval/profile/tests.rs` |
| R-06 (baseline regression) | Full | Existing tests passed unchanged; make_snapshot_db() not modified |
| R-07 (rebuild timeout) | Accepted residual | Noted as residual; sqlx query timeout is implicit guard; deferred per SPECIFICATION.md |
| R-08 (accessor visibility) | Full | pub(crate) declared; compiler enforcement noted; PR review gate specified |
| R-09 (mrr_floor drift) | Manual | Pre-merge `unimatrix eval run --profile baseline.toml` confirmation required |
| R-10 (write-back before init) | Resolved | from_profile() is sequential; no concurrent access during construction |

The four non-negotiable gate-blocking scenarios from RISK-TEST-STRATEGY.md Coverage Summary are all present:
1. `use_fallback == false` AND non-empty `typed_graph` (AC-06) — Test 1 Layer 1
2. Live `search()` call returns `Ok(_)` (SR-05, ADR-003) — Test 1 Layer 3
3. `Ok(layer)` on cycle error with `use_fallback==true` (AC-05) — Test 2
4. All existing tests pass unchanged (AC-08) — explicit in layer_tests.md

### 4. Interface Consistency

**Status**: PASS

**Evidence**:

OVERVIEW.md defines the integration surface table and all shared types. Per-component pseudocode is consistent:

- `TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>` used identically in `EvalServiceLayer.md` (write-lock swap) and `layer_tests.md` (read-lock in assertions).
- `TypedGraphState::rebuild` signature `async fn rebuild(store: &Store) -> Result<TypedGraphState, StoreError>` appears consistently in OVERVIEW.md integration surface and `EvalServiceLayer.md` Step 5b.
- `ServiceLayer::typed_graph_handle` signature `pub fn typed_graph_handle(&self) -> TypedGraphStateHandle` used in `EvalServiceLayer.md` Step 13b and consistent with ARCHITECTURE.md integration surface table.
- New `EvalServiceLayer::typed_graph_handle` declared as `pub(crate)` in `EvalServiceLayer.md` and used in `layer_tests.md` for assertions — consistent.
- `ServiceSearchParams`, `AuditContext`, `CallerId`, `AuditSource` types listed in OVERVIEW.md shared types table and used in layer_tests.md Layer 3 assertion — consistent.
- No contradictions found across pseudocode files.

### 5. Key Check: rebuild() Before with_rate_config() (ADR-001)

**Status**: PASS

**Evidence**: `EvalServiceLayer.md` Step 5b (lines 62–99) places the rebuild call after Step 5 (`SqlxStore::open_readonly`) and before Step 13 (`ServiceLayer::with_rate_config`). The sequencing is explicit in both the pseudocode flow and OVERVIEW.md Sequencing Constraints item 2: "Step 5b precedes Step 13: rebuild() is called after store construction (Step 5) and before with_rate_config() (Step 13)". ARCHITECTURE.md component interaction diagram confirms this ordering.

### 6. Key Check: Write-Back Idiom

**Status**: PASS

**Evidence**: `EvalServiceLayer.md` Step 13b (lines 144–167) uses:
```
handle <- inner.typed_graph_handle()
guard  <- handle.write().unwrap_or_else(|e| e.into_inner())
*guard <- state
DROP guard
```
This precisely matches the idiom specified in ARCHITECTURE.md integration surface: "Write-lock swap idiom: `*guard = rebuilt_state` inside `handle.write().unwrap_or_else(|e| e.into_inner())`". The poison recovery pattern (`unwrap_or_else(|e| e.into_inner())`) matches the established convention in `typed_graph.rs`. The guard is explicitly dropped after the swap (NFR-03: lock held only for swap duration).

### 7. Key Check: Three-Layer Assertion (ADR-003)

**Status**: PASS

**Evidence**: Test 1 in `layer_tests.md` implements all three layers in sequence:

- **Layer 1 (handle state)**: `assert!(!guard.use_fallback)` + `assert!(guard.all_entries.len() >= 2)` (lines 176–184)
- **Layer 2 (graph connectivity)**: `find_terminal_active(id_a, &guard.typed_graph, &guard.all_entries)` asserted as `Some(id_a)` + `guard.typed_graph.edge_count() >= 1` (lines 197–207); fallback path specified if `find_terminal_active` not accessible
- **Layer 3 (live search)**: `layer.inner.search.search(params, &audit_ctx, &caller_id).await` asserted as `is_ok()` (lines 248–254)
- Read lock is explicitly dropped before Layer 3 search call (line 210 "DROP guard")

The test-plan `layer_tests.md` Three-Layer Assertion ADR Compliance Checklist (lines 249–259) requires all four sub-assertions (Layer 1a, Layer 1b, Layer 2, Layer 3) and states "A test that asserts only Layer 1 (handle state) is insufficient per ADR-003". This is enforced in both pseudocode and test-plan.

### 8. Key Check: Active-Entry + Edge Seeding (C-09)

**Status**: PASS

**Evidence**: `seed_graph_snapshot()` helper in `layer_tests.md` inserts:
- Two entries with `status: Status::Active` (lines 61–86)
- One CoAccess (S1-class) edge via raw SQL with `bootstrap_only=0` (lines 88–99)

The fixture notes explicitly state that Quarantined entries are excluded by `rebuild()` and would produce an empty graph (line 89: "bootstrap_only=0 ensures build_typed_relation_graph includes this edge"). This is consistent with C-09 from SPECIFICATION.md and R-03 from RISK-TEST-STRATEGY.md. The test-plan `layer_tests.md` Seeding requirement section also confirms bootstrap_only must be 0 and entries must be Active (lines 100–109).

### 9. Key Check: TOML Values (ADR-005)

**Status**: PASS

**Evidence**: `ppr-expander-enabled-toml.md` Required Content section (lines 43–59) specifies:
```toml
distribution_change = false
mrr_floor = 0.2651
p_at_5_min = 0.1083
```
These exact values match SPECIFICATION.md C-06 ("mrr_floor = 0.2651, p_at_5_min = 0.1083. distribution_change = false. These are human-approved (OQ-01, SCOPE.md)") and ADR-005. The TOML pseudocode also includes the required explanatory comment on `distribution_change = false` (SR-04 guard). Field-by-Field Rationale table in `ppr-expander-enabled-toml.md` confirms no deviation.

### 10. Key Check: Cycle-Abort-Safety Test

**Status**: PASS

**Evidence**: Test 2 (`test_from_profile_rebuild_error_degrades_gracefully`) in `layer_tests.md` (lines 261–404):
- Seeds a Supersedes cycle via two raw SQL insertions: A→B and B→A (lines 329–345)
- Calls `EvalServiceLayer::from_profile()` against the cycle-containing snapshot
- Asserts the result is `Ok(layer)` — any `Err(e)` other than environmental causes `panic!` with message "from_profile must return Ok(layer) on rebuild failure (AC-05)"
- After obtaining the layer, asserts `guard.use_fallback == true` (line 377)

This satisfies AC-05, R-04, and ADR-002. The "cycle-abort-safety test" requirement from the spawn prompt is explicitly implemented as a separate test.

### 11. Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**:

`crt-045-agent-1-pseudocode-report.md`: Contains `## Knowledge Stewardship` section with:
- `Queried:` entries documenting three separate Unimatrix searches (context_search, context_briefing) with specific entry IDs referenced (#4096, #2652, #2673, #4099, #4100, #4101, #4102, #3610)
- Stored: nothing novel to store — provides a specific reason (deviations from established patterns: none; write-back idiom matches existing pattern exactly)

`crt-045-agent-2-testplan-report.md`: Contains `## Knowledge Stewardship` section with:
- `Queried:` entries documenting context_briefing with specific entry IDs (#4096, #4099–#4102, #4097, #747, #238)
- `Stored: entry #4103 "Three-layer integration test for eval service layer graph wiring (wired-but-unused guard)"` via uni-store-pattern — active storage with reason

Both agents satisfying the obligation as read-only (pseudocode) agents: Queried entries present. The test-plan agent additionally stored a new pattern entry, which goes beyond the read-only minimum.

Note: The RISK-TEST-STRATEGY.md (risk agent) also has a Knowledge Stewardship section with Queried: and Stored: entries. The SPECIFICATION.md similarly has a stewardship block. All design-phase artifacts reviewed have compliant stewardship.

## Rework Required

None.

## Knowledge Stewardship

- nothing novel to store — this feature's gate-3a result is feature-specific. The design is clean and fully compliant; no recurring failure patterns to capture.
