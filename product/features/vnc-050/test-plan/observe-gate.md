# C6 Observe Gate — `http/router/handlers.rs`

Risks: R-17, R-16. ACs: AC-09, AC-18.

## HTTP integration
| Test | Assertion |
|---|---|
| `read_token_observe_403_pre_dispatch` | POST `/v1/A/observe` → 403 `{"error":"token lacks SessionWrite"}`; `DISPATCH_SPY` has no entry for test slug |
| `read_token_session_telemetry_routes_403` | each session telemetry route → 403, `DISPATCH_SPY` no entry for test slug |
| `read_token_observe_no_table_change` | after extraction tick + channel drain: observations, sessions, signal_queue, cycle_events, cycle_tags, co_access, injection_log, query_log, entries unchanged |
| `rw_token_observe_lands` | rw token → 2xx; observation row present (companion) |
| `legacy_observe_lands` | companion |
| `observe_403_audited_in_bound_slug` | `http_access` row written |
| `observe_and_tool_ceiling_agree_per_mode` | shared with principal.md |
