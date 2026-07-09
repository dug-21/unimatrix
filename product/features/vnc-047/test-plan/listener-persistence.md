# C5 — Listener persistence (`handle_cycle_event` step-5) — ASSEMBLED PATH

> File: `crates/unimatrix-server/src/uds/listener.rs` (`handle_cycle_event`, step-5 spawn ~:3035).
> Reads `payload["tags"]`; routes `Start && !tags.is_empty()` → `insert_cycle_start_with_tags`, else
> → `insert_cycle_event` (UNCHANGED). Gate: `!feature_cycle.is_empty()`.
> Risks: **R-03 (Critical)**, **R-04 (High)**, R-06 (High), R-09 (Med), R-08 (Med). ACs: **AC-02,
> AC-02a [assembled-path]**, AC-EXTRA-1, AC-EXTRA-2.
>
> **This is the gate-critical assembled-path file. `proven_by` for AC-02/AC-02a MUST cite tests here
> (or review-handler.md for AC-05), NEVER a store-only test (R-03/SR-08).**

## Reuse (mandatory — do not build isolated scaffolding)
Drive the real chain via `dispatch_request(HookRequest::RecordEvent { event }, …)` in the `listener.rs`
test module. Canonical model: `test_cycle_start_goal_flows_from_hook_payload_to_db` (`listener.rs:7934`,
T-389-02). Helpers: `make_store()` (:3425), `make_registry()` (:3438), `make_pending()` (:3442),
`make_dispatch_deps(&store)` (:3446), `make_services(...)` (:3468), `make_cycle_event(event_type,
session_id, payload, topic_signal)` (:5938), `crate::uds::UDS_CAPABILITIES`. After firing, `sleep`
~100ms for the fire-and-forget spawn (as T-389-02 does), then read back via
`store.get_cycle_tags(fc)`.

## AC-02 — whole-set-once at start via the hook (assembled)
- `test_cycle_start_tags_flow_from_hook_to_cycle_tags` — fire a Start `RecordEvent` with
  `payload["tags"]=["arm:A","workflow:v1.3"]` for `fc`; settle; assert `get_cycle_tags(fc)` returns
  exactly that set. **This is the AC-02 anchor.**
- `test_non_start_tags_not_persisted` — fire a non-start event carrying `tags`; assert
  `get_cycle_tags(fc)` is empty (FR-4 via assembled path, not a unit stub) (R-09).
- `test_duplicate_start_no_dup_no_error` — fire the same Start twice; assert no duplicate rows, no
  error surfaced.

## AC-02a — whole-set no-op (assembled, EXACT stored-set equality)
- `test_whole_set_once_changed_set_noop` — start `{arm:A}`, then start `{arm:B}` → stored EXACTLY
  `{arm:A}`, no error.
- `test_whole_set_once_superset_noop` — start `{A,B}`, then start `{C}` → EXACTLY `{A,B}` (no
  accumulation).
- `test_whole_set_once_subset_and_expansion` — `{A,B}` then `{A}` → `{A,B}`; separately `{A}` then
  `{A,B}` → `{A}` (both directions whole-set no-op).
- `test_tagless_start_does_not_lock` — tagless Start first, then Start `{A}` → `{A}` locks (first
  *tags* win, not first start).

## AC-EXTRA-2 / R-04 — absent-session durability + degrade (assembled)
- `test_evicted_session_tags_persist` — Start with tags where the session is ABSENT from the registry
  (evicted); assert the #519 pre-register (step-1b) fires and tags still land in `cycle_tags` for the
  correct `fc` (NFR-6/ADR-003). Gate is `!feature_cycle.is_empty()`, NOT registry presence.
- `test_empty_feature_cycle_no_orphan_rows` — Start with tags but empty/NULL `feature_cycle` →
  persistence gated off, zero `cycle_tags` rows (the SINGLE documented silent drop). Every other
  input persists.
- `test_db_error_in_spawn_warns_no_panic` — force a store error inside the fire-and-forget spawn
  (closed pool, model T-RES-03 :8590) → `tracing::warn`, spawned task does not panic, no
  caller-visible signal.

## AC-EXTRA-1 / R-06 — no second persistence route
- `test_bare_mcp_handler_persists_no_tags` — call the bare MCP `context_cycle` handler (no hook) with
  tags → assert nothing persists to `cycle_tags` (session-unaware by design). (Python-harness
  counterpart in OVERVIEW §5.)
- Grep assertion (record in coverage report): `cycle_tags` INSERT appears ONLY in
  `insert_cycle_start_with_tags`, reached only from this step-5 Start branch — no other writer.

## R-09 — 15 untouched `insert_cycle_event` call sites
- `test_start_without_tags_routes_to_insert_cycle_event` — Start with no tags → routed to the
  UNCHANGED `insert_cycle_event` arm; normal cycle_start row, zero `cycle_tags` rows.
- `test_non_start_event_unchanged` — phase/outcome/next_phase events behave exactly as before.
- `test_goal_persists_with_and_without_tags` — existing col-025 goal end-to-end still green; goal
  persists whether or not tags are supplied (goal rides the same start row via the new primitive when
  tags present). Assert against `get_cycle_start_goal`.

## Payload contract robustness (C4→C5)
- `test_malformed_tags_payload_degrades` — `payload["tags"]` as an object (not array) or wrong type →
  treated as "no tags", routed to `insert_cycle_event`, no panic.
