# Test Plan — project-resolver (`MultiProjectRouter` / `ProjectEntry`)

Source: `http/router/project_resolver.rs`. `ProjectEntry` gains `session_registry` /
`pending_entries_analysis` / `services`; `from_server` `Arc::clone`s them off `server`
**before** it moves into `McpAdapter::new`; new methods resolve O(1). Risks: R-06, R-03
(convergence-by-construction), R-11.

## Unit Test Expectations

1. **`test_from_server_clones_handles_before_move`** (convergence — R-03/ADR-002) — after
   `ProjectEntry::from_server`, assert the entry's `session_registry` is
   `Arc::ptr_eq` with the same `SessionRegistry` the entry's `McpAdapter` → `UnimatrixServer`
   holds. Same for `pending_entries_analysis`. This is the structural proof that write-side
   (`registry_for`) and read-side (adapter's server) meet on ONE instance per slug. A
   clone-after-move or a re-mint breaks it and this test RED.
2. **`test_services_clone_is_slug_config_driven`** — the entry's `services: ServiceLayer` is the
   slug's config-driven layer (crt-056), `Arc::ptr_eq`/value-equal to the server's, not a global.
3. **`test_registry_for_resolves_entry_handle`** — `MultiProjectRouter::registry_for(&key)`
   returns `entry.session_registry.clone()`; `Arc::ptr_eq` against the stored entry handle.
   Same for `pending_for` / `services_for`. (Complements resolution-funnel.md #3 at the impl.)
4. **`test_from_servers_builds_n2_distinct_entries`** — build a router from ≥2 slug servers;
   each entry's handles are distinct across slugs (A's registry ≠ B's registry by `ptr_eq`).
   N≥2 is required (N=1 cannot distinguish a real map from a global handle, #4974).

## Integration Expectations (through the funnel)

5. Via the behavioral suite (isolation-suite.md): a delta driven to `/v1/{A}/observe` writes into
   the registry that A's `McpAdapter.cycle_review` reads — proving `from_server`'s clone-order
   and the map wiring are correct end-to-end, not just by `ptr_eq`.

## Blast-Radius / Integration Risks
- `ProjectEntry` field additions ripple to every constructor call site and the test doubles
  (R-06 — audited in resolution-funnel.md). Flag any adjacent breakage; do not silently patch a
  double to return a global handle.
- Ordering hazard shared with project-provisioner + main.rs tick loop: the handles must be
  cloned before the `server` move; pinned by #1.

## Coverage Trace
| Risk | Test |
|------|------|
| R-03 (convergence) | #1, behavioral back-stop #5 |
| R-06 | #3, #4 |
| R-11 | O(1) resolve confirmed by #3 (no rebuild) |
