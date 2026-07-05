# Test Plan — audit-emit (`server.rs:650` `audit_fire_and_forget`, `edge_cleanup` event)

**Unit under test:** one fire-and-forget `AuditEvent`, emitted ONLY on `Ok(tuples)` AND `!tuples.is_empty()`:
- `operation`: `"context_deprecate.edge_cleanup"` (distinct from the flip's `"context_deprecate"`)
- `target_ids`: `[entry_id]`
- `detail`: `"eager edge cleanup: removed {count} agent-authored edge(s) for deprecated entry #{id}"`
- `metadata`: JSON array `[{"source_id":..,"target_id":..,"relation_type":".."}, ...]` — via the JSON encoder, never string interpolation.

**Placement:** in-process server fixture (`make_server()`); read back with the `server.rs:2205` pattern, extended to project `metadata`: `SELECT operation, target_ids, detail, metadata FROM audit_log WHERE operation = 'context_deprecate.edge_cleanup'`. Because the write is fire-and-forget (`tokio::spawn`), sleep ~50ms before read-back (existing convention, `server.rs:2201`).

**Critical infra fact:** `audit.rs:33-43` substitutes the `"{}"` sentinel when `metadata` is EMPTY. Non-empty removals MUST serialize real tuple JSON and must NOT fall through to the sentinel — AC-11 asserts against this.

## Test Expectations

### AC-03 / FR-04 — audit record content — **R-08**
- `test_edge_cleanup_audit_record_content`
  - Arrange: deprecate E with N=3 agent edges of known tuples.
  - Act: read back the record filtered `operation == "context_deprecate.edge_cleanup"`.
  - Assert: `target_ids == [E]`; `detail` contains the count `3` and `#E`; exactly ONE such record.

### AC-11 — removed-edge tuples in audit (firm) — **R-03 / SR-01**
- `test_edge_cleanup_audit_metadata_tuple_set_equality`
  - Arrange: deprecate E with N agent edges of known `(source_id, target_id, relation_type)`.
  - Assert: parse `metadata` as JSON; it is a well-formed array of N objects; the SET of tuples equals EXACTLY the pre-delete agent-edge set (set-equality, order-independent). Not a count-only check.
- `test_edge_cleanup_audit_metadata_not_sentinel_on_nonempty`
  - Assert on a non-empty removal: `metadata != "{}"` (did not fall through the empty-sentinel guard at `audit.rs:35`), and is valid JSON.
- `test_edge_cleanup_audit_metadata_wellformed_with_unusual_relation_type`
  - Seed an agent edge with an unusual `relation_type` string; assert `metadata` is still well-formed JSON (serialized via the encoder, no interpolation corruption — security §audit-metadata-attack-surface).

### AC-07 / R-08 — distinct events, no emit on empty / re-deprecate
- `test_flip_and_cleanup_are_two_distinct_records`
  - Deprecate E with agent edges; assert TWO distinct records exist: `"context_deprecate"` (flip) AND `"context_deprecate.edge_cleanup"`. Tests keying on "an audit record exists" must not match the wrong one.
- `test_no_cleanup_audit_when_zero_agent_edges`
  - Deprecate E with zero agent edges; assert NO `"context_deprecate.edge_cleanup"` record (emitted only on non-empty), even though `edges_removed = Some(0)` renders in the response. (Response advisory and audit emission diverge at zero — response shows `0`, audit stays silent.)
- `test_no_cleanup_audit_on_redeprecate` — the idempotent second call (AC-07) emits neither the flip cleanup nor a cleanup event.

### AC-06 / R-03 — no audit on failure path
- `test_no_cleanup_audit_on_eager_delete_error`
  - Fault-inject the eager failure; assert `edges_removed = None`, NO `edge_cleanup` audit record. Pairs with the R-03 atomicity guarantee (eager-delete-helper plan): because delete+capture is one atomic RETURNING, an `Err` means the rows were NOT committed → "edges still present, tick backstops," never "gone with no record."

### R-10 — high-degree audit-JSON completeness
- `test_high_degree_audit_metadata_carries_all_tuples`
  - Deprecate a high-degree E (e.g. 50 agent edges); assert `metadata` array carries all 50 tuples; `detail` count == 50. Bounded by degree (NFR-03).

## Notes for delivery
- `audit_fire_and_forget` drops the `JoinHandle` (#579) — the delete already succeeded and the caller already got its count before the audit lands; an audit-write drop is degraded observability, not data loss. Do not assert the record is present without the ~50ms settle sleep.
- Count in `detail` and tuple count in `metadata` must agree with `edges_removed` (`tuples.len()`, the single source of truth).
