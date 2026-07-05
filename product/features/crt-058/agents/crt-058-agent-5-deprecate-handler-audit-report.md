# Agent Report — crt-058-agent-5-deprecate-handler-audit (Wave B)

## Scope
Wave B: deprecate-handler (`tools.rs` `context_deprecate` step 6.5) + audit-emit
(`server.rs` `emit_edge_cleanup_audit`). Makes the crate compile (Wave A left the
two `context_deprecate` call sites on the OLD `format_deprecate_success` arity) and
wires the eager agent-edge cleanup + its distinct audit event.

## Files Modified
- `crates/unimatrix-server/src/server.rs` — NEW `emit_edge_cleanup_audit` helper
  beside `audit_fire_and_forget`; declared the `edge_cleanup_audit_tests` module.
- `crates/unimatrix-server/src/mcp/tools.rs` — `context_deprecate`: step-5 early
  return now passes `None`; identity clones captured before the flip audit moves
  `ctx.agent_id`; NEW step 6.5 (non-fatal eager delete + audit); step 8 threads
  `edges_removed`.
- `crates/unimatrix-server/src/background.rs` — extended `mod tests` with the AC-10
  subset keystone, chokepoint-exclusion/negative-mutation, synchronous-removal, and
  shared-edge tests (reusing existing graph fixtures).
- `crates/unimatrix-server/src/server_edge_cleanup_audit_tests.rs` — NEW audit-emit
  test module (`#[path]`-included).

## Implementation Notes
- **Identity-move hazard (handled):** `ctx.agent_id` is moved (not cloned) into the
  step-6 flip `AuditEvent`. Cloned `agent_id`/`session_id`/`client_type` into locals
  before that so step 6.5 can reuse them.
- **Distinct operation (R-08):** cleanup event is `context_deprecate.edge_cleanup`,
  separate from the flip's `context_deprecate`.
- **`{}` sentinel avoided:** metadata is `serde_json::to_string(&[RemovedEdge])`;
  `emit` guards `is_empty()` and, on a serialize `Err`, `warn!`s and skips the event
  rather than emitting the empty sentinel with a non-empty removal.
- **Non-fatal (C-01):** delete awaited inline (synchronous); helper `Err` → `warn!`
  (not `debug`, #3448), `edges_removed = None`, normal success; tick backstops.
- **Count = `tuples.len()`**, never `rows_affected()`.

## Tests (all passing)
Handler/subset (background.rs mod tests):
- `test_deprecate_eager_subset_of_tick_and_exactly_agent_edges` (AC-10 keystone,
  both real functions: `R ⊆ T` and `R` == exactly the two agent edges)
- `test_successor_bearing_edge_repointed_by_tick_but_eager_would_destroy`
  (chokepoint-exclusion R-01 + negative-mutation R-06)
- `test_deprecate_removes_agent_edges_synchronously` (AC-01/AC-09)
- `test_two_entries_sharing_edge_deprecated_in_sequence`

Audit-emit (server.rs `edge_cleanup_audit_tests`):
- `test_edge_cleanup_audit_record_content` (AC-03)
- `test_edge_cleanup_audit_metadata_tuple_set_equality` (AC-11)
- `test_edge_cleanup_audit_metadata_not_sentinel_on_nonempty`
- `test_edge_cleanup_audit_metadata_wellformed_with_unusual_relation_type` (security)
- `test_flip_and_cleanup_are_two_distinct_records` (R-08)
- `test_no_cleanup_audit_when_zero_agent_edges`
- `test_high_degree_audit_metadata_carries_all_tuples` (R-10)

**Build:** `cargo build -p unimatrix-server` — pass.
**Tests:** `cargo test -p unimatrix-server` — pass (4398 lib + 124 + 21; 0 failures;
12 new crt-058 tests green). Clippy clean on the crate.

## Flags / Out-of-scope
- **RequestContext limitation (not a gap):** `context_deprecate` / `context_correct`
  `#[tool]` methods are not constructible in unit scope (established codebase
  constraint, tools.rs:12907). The handler-level cases that REQUIRE the live handler
  — AC-06 fault-injected `warn!`+`None`+success, AC-07 step-5 idempotency emitting no
  second cleanup audit, and the "no `edge_cleanup` audit on the `context_correct`
  path" half of chokepoint-exclusion — are proven at reachable seams here where
  possible and otherwise deferred to the **Stage-3c Python integration suite** (the
  tester owns that layer). The structural guarantees (single production caller R-06;
  chokepoint never sets `superseded_by`) hold in code.
- **R-02 literal predicate-string pin:** implemented behaviorally (the AC-10
  exact-set + `R ⊆ T` assertions over both real functions catch any widening/
  narrowing). A literal SQL-string snapshot would require extracting the LOCKED
  predicate into a `const` in Wave A's `edge_write.rs` — out of my file scope; noted
  for the tester if a text pin is also wanted.
- Reverted pure fmt churn the on-save formatter applied to Wave A's committed
  `mcp/edge_write_delete_agent_tests.rs` (out of my scope). A pre-existing
  non-rustfmt-clean spot at `server.rs:1259` (`initialize` method) is untouched and
  outside my diff.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-001/002/003 (#5458-5460),
  the Wave-A helper pattern #5467 (edge_write atomicity), and #3910 (identical status
  filters across cleanup passes). Applied: distinct-operation, tuple-JSON metadata,
  eager⊆tick via both-real-functions test.
- Stored: entry #5468 "context_deprecate step-6.5 edge_cleanup audit: clone identity
  before flip-audit move; emit distinct op; never let non-empty metadata hit the {}
  sentinel" via context_store (pattern, topic unimatrix-server) — captures the
  identity-move hazard, the `{}`-sentinel trap, and the RequestContext test-seam
  reality, none visible in source.
