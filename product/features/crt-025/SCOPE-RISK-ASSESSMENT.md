# Scope Risk Assessment: crt-025

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `current_phase` is set via fire-and-forget in the UDS listener — if the daemon's async task scheduler drops or delays the write, `context_store` reads stale (`None`) phase and tags the entry incorrectly. The phase label is a GNN training signal; systematic null tagging silently degrades W3-1 data quality. | High | Med | Architect must ensure `SessionState.current_phase` mutation is synchronous within the UDS handler's own task — not queued behind the analytics drain. Fire-and-forget is for DB writes, not in-memory state. |
| SR-02 | `seq` monotonicity uses `SELECT MAX(seq)+1` — safe only if the UDS listener serializes all events for a given `cycle_id` through a single task. If two sessions sharing a feature concurrently emit `phase-end`, both reads can return the same MAX, producing duplicate seq values. SCOPE Decision §2 assumes per-`cycle_id` serialization, but this is not enforced structurally. | Med | Med | Architect should verify the UDS listener concurrency model guarantees per-feature-cycle write serialization, or switch to SQLite `AUTOINCREMENT` + a per-cycle ordering query at review time. |
| SR-03 | `outcome` removal from `CategoryAllowlist` is a silent ingest breakage for any caller currently using category `"outcome"`. No existing stored entries are deleted, but new stores silently fail with a category-rejected error. Existing test fixtures that assert on category `"outcome"` storage will break. | Med | Low | Architect must audit call sites and test fixtures that use `outcome` category before removing it from the allowlist. Confirm no active protocol emits `outcome`-category stores. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `keywords` removal is declared backward-compatible (unknown fields silently ignored), but this relies on serde behavior with no `deny_unknown_fields`. If any call site passes `keywords` as the sole intent signal, that intent is now silently dropped. The scope confirms `keywords` is inert — this assumption must hold. | Low | Low | Confirm via code search that no consumer reads `sessions.keywords` at query time before treating removal as zero-risk. |
| SR-05 | `context_cycle_review` cross-cycle comparison is listed as a non-goal, but the product vision (WA-1 section) explicitly lists "cross-cycle comparison: category distribution per phase across multiple features" as a WA-1 deliverable. Scope and vision disagree on this boundary. | Med | Med | Clarify with product before architecture begins. If cross-cycle is deferred to a follow-up, update the product vision accordingly. |
| SR-06 | Phase string enforcement (lowercase, no spaces, max 64 chars) is applied only at ingest via `validate_cycle_params`. Nothing prevents protocol scripts from passing inconsistent casing or hyphens vs underscores across sessions — silently producing fragmented training labels (e.g., `"implementation"` vs `"impl"` vs `"implement"`). | Med | High | Spec writer should define the canonical phase vocabulary the delivery protocol will use and enforce it at the protocol level. Engine stores opaque strings; upstream consistency is not the engine's problem but it IS the GNN's problem. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `record_feature_entries` is called on two write paths: direct write pool (usage recording) and `AnalyticsWrite::FeatureEntry` (analytics drain). Both must receive `current_phase` from `SessionState`. The analytics drain path is batched and async — by the time the drain fires, `SessionState.current_phase` may have advanced to a later phase. Entry would be tagged with the wrong phase. | High | Med | Architect must resolve phase capture at event-enqueue time, not at drain-flush time. The phase must travel with the `FeatureEntry` event through the queue, not be read from live `SessionState` at flush. |
| SR-08 | Schema v14→v15 migration adds a column to `feature_entries` and a new `CYCLE_EVENTS` table. The established pattern (#681, #836) requires `pragma_table_info` pre-check. Migration tests exist for v13→v14; a v14→v15 test is required. Fresh-DB path (`create_tables_if_needed`) must also include both. Missing either path will cause test flakiness under concurrent test runs (see #303). | Med | Low | Follow pattern #836. Ensure the migration integration test list is extended — not just unit tests. |

## Assumptions

- **SCOPE §Background / Keywords**: Assumes `sessions.keywords` has no downstream consumers. If any external tooling reads it via direct SQL, the stop-populating decision is silent breakage. (Low risk given it was never documented.)
- **SCOPE §Decisions #1**: Assumes `next_phase` on `start` immediately sets `current_phase`. If `start` and the first `context_store` race in a concurrent session, the entry may still get `NULL`. This is accepted behavior but should be stated explicitly in AC-08.
- **SCOPE §Decisions #3**: Assumes `phase-end` with no prior `start` is valid. This means CYCLE_EVENTS can have orphaned events with no corresponding `cycle_start` row — `context_cycle_review` must handle this without a crash.
- **SCOPE §Non-Goals**: Assumes W3-1 training is tolerant of `NULL` phase labels on pre-WA-1 entries. If W3-1 requires complete phase labels for all training rows, backfill becomes blocking, not optional.

## Design Recommendations

- **SR-01 / SR-07**: The architect should explicitly model where `SessionState.current_phase` is read relative to when `record_feature_entries` is called. Phase capture must be snapshot-at-enqueue, not read-at-flush.
- **SR-06**: The spec writer should add an explicit constraint table of the canonical phase vocabulary (`scope`, `design`, `implementation`, `testing`, `gate-review`) that the delivery protocol must use. This is not an engine constraint but must be stated somewhere.
- **SR-05**: Resolve the scope/vision boundary mismatch on cross-cycle comparison before architecture begins. If deferred, update PRODUCT-VISION.md to move it to a follow-up.
- **SR-02**: The architect should decide during design — not implementation — whether per-cycle seq monotonicity is enforced by concurrency control or by making `seq` advisory (ordering by `timestamp` at query time as the true ordering).
