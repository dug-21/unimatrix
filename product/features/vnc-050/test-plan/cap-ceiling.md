# C4b Capability Ceiling — `mcp/cap_ceiling.rs`, `server.rs`, `mcp/context.rs`, `tools.rs`, `services/usage.rs`

Risks: R-02, R-04, R-20, R-22. ACs: AC-08, AC-15, AC-25, AC-26, AC-27.
All behavioral tests drive real HTTP (`tests/headless_access_http.rs`, rmcp streamable-HTTP client, stateful session). Never call `require_cap` directly to discharge.

## Step 0 — PERMANENT (runs first)
`principal_visible_in_tool_request_context_over_http`: boot server; scoped read token; POST `initialize`, then `tools/call context_search` in the same `mcp-session-id`. The `PROBE` seam (`mcp/cap_ceiling.rs`, cfg(test)) in `build_context_with_external_identity` records `ceiling_from(rc)`; assert `Some([Read, Search])`. Repeat with legacy token → `None`. Failure → implement ADR-003 `McpAdapter::call` re-insert, re-run. Comment in test: "rmcp upgrade guard — do not remove".

## Unit (supplementary)
| Test | Assertion |
|---|---|
| `ceiling_from_none_without_parts` | stdio/UDS → `None` |
| `ceiling_from_empty_when_parts_without_principal` | `Some(empty)`, not `None` |
| `permits_matrix` | None→true for all; read ceiling: Read/Search true, Write/Admin/SessionWrite false |
| `records_learning_matrix` | None→true; read→false; rw→true; empty→false |

## HTTP integration
| Test | Assertion |
|---|---|
| `read_token_write_tool_agent_id_table` | tools {store, correct, deprecate, quarantine, edge, tag, enroll, cycle writes} × agent_id {omitted, `human`, registry Write-holder, `token:<label>`} → every cell error (`CapabilityDenied`, or `InvalidInput` for `token:`); DB row counts + entries table hash unchanged (AC-08, AC-25) |
| `read_token_read_tools_succeed` | search/lookup/get/briefing OK (companion) |
| `rw_token_write_tools_succeed` | same write calls with Write-holder agent succeed (#4416 companion) |
| `rw_token_human_admin_tool_denied` | rw + `human` on `context_quarantine` and on `context_enroll` (the only `Capability::Admin` sites) → both denied, DB unchanged; delivery re-greps `Capability::Admin` and adds any new site |
| `empty_ceiling_denies_read_and_write` | `StripPrincipalLayer` removes principal after auth → Read and Write denied |
| `narrow_only_intersection` | registry agent without Write + rw token → Write denied |
| `require_cap_single_check_point` | source test over `tools.rs`: every capability check is `require_cap(&ctx, ..)`; no direct `registry.*has_capability` calls (R-02.5) |

## Learning suppression (R-04)
| Test | Assertion |
|---|---|
| `read_token_learning_tables_unchanged` | snapshot access_count/helpful_count, co_access, feature_entries, query_log, agents, sessions, observations, signal_queue, implicit signals; run search, lookup, get, briefing with `helpful:true`, `feature` set, `agent_id:"human"`; drain usage + background channels (`UsageService::flush()` test counter); snapshot equal. `http_access` and tool audit rows DID increase (AC-15, AC-26) |
| `rw_token_learning_tables_change` | same batch → tables differ (gate not vacuous) |
| `legacy_and_stdio_learning_recorded` | companions |
| `usage_context_inventory` | source test: every `UsageContext {` in `tools.rs` sets `learning_allowed` from ctx; `UsageContext` has no `Default` impl yielding true |
| `gate_before_enqueue` | `UsageService::enqueued() == 0` for read token |

## Accepted residual pins
| Test | Assertion |
|---|---|
| `r20_rw_token_human_non_admin_write_succeeds_with_label` | write OK, tool audit `human`, `http_access.token_label == label` |
| R-22 tests | see principal.md |
