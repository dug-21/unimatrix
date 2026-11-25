# C3 Slug Guard + Access Audit — `http/router/slug_guard.rs`, `http/router.rs`

Risks: R-03, R-16, R-09. ACs: AC-09, AC-11.

## HTTP integration (real listener, two slugs A,B; plus infra-001 `test_scoped_token_other_slug_returns_403`)
| Test | Assertion |
|---|---|
| `route_matrix_slug_binding` | {MCP POST, SSE GET, DELETE session, observe POST} × {legacy, read@A, rw@A} × {A,B}: B → 403 `{"error":"token not permitted for project"}` for scoped; legacy B passes; A passes for all (companion). Observe on A with read token → 403 SessionWrite (owned by observe-gate) |
| `path_variants_never_200` | `/v1/B/`, `/v1/B`, `/v1/%42/mcp`, `/v1//B/mcp`, `/v1/tools/...`, `/v1/b` vs `/v1/B` → 403 or existing 400/404, never 200, never reaches `resolve_store` (`RESOLVE_SPY` has no entry for test slug) |
| `403_body_identical_existing_vs_missing_slug` | byte-equal bodies + same status for B (exists) and Z (not provisioned) |
| `guard_runs_before_resolve_store` | `RESOLVE_SPY` has no entry for test slug on denial |
| `missing_principal_returns_500` | `StripPrincipalLayer` strips principal → 500, not pass-through |
| `ceiling_and_guard_apply_to_slug_provisioned_after_boot` | provision C after boot; rw@A on C → 403; rw@C token on C → 200 |

## Audit (R-16)
| Test | Assertion |
|---|---|
| `http_access_row_per_request` | legacy, scoped MCP, observe → one `http_access` row each, `token_label` = label / `legacy`, method, route; never secret |
| `denial_logged_to_bound_slug_only` | rw@A on B → row in A's audit; B's audit row count unchanged |
| `audit_sink_failure_does_not_fail_request` | failing sink injected → request 200 |
| `sse_session_one_row_not_per_event` | long SSE GET with ≥10 events → 1 row |
| `audit_rows_sentinel_free` | grep rows for token/sha256 sentinel → 0 |
