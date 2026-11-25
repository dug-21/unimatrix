# vnc-050-agent-5-slug-observe — C3 Slug Guard + C6 Observe Gate

## Report
Files created (crate unimatrix-server): src/http/router/slug_guard.rs, src/http/router/slug_guard_tests.rs, src/http/router/observe_gate_tests.rs.
Files modified: src/http/router.rs (access_audit field, guard call before split, `/health` exempt), src/http/router/handlers.rs (Step -1 identity + `require_session_write`, DISPATCH_SPY), src/http/router/project_resolver.rs (RESOLVE_SPY), src/http/router/tests.rs (observe fixture identity + SessionWrite), tests/project_routing_integration.rs (drive_observe goes through real `StaticTokenAuthLayer` with legacy token, since `BearerPrincipal` is pub(crate) and the guard fails closed without it).

Tests: unimatrix-server all targets rc=0 (lib 4682+ pass). New: 13 HTTP-boundary tests (9 slug guard, 4 observe gate). Workspace run: only failure is pre-existing `unimatrix-embed download::tests::test_ensure_model_creates_directory` (model download, unrelated).

Deviations / issues:
- `StripPrincipalLayer` lives in slug_guard_tests.rs, not http/auth/test_layers.rs (http/auth* owned by another agent). cap-ceiling `empty_ceiling_denies_read_and_write` needs its own or can move it there later.
- `ceiling_and_guard_apply_to_slug_provisioned_after_boot`: not implemented — `MultiProjectRouter` map is immutable (provisioning restarts the server); guard is stateless over principal so behavior is identical.
- `sse_session_one_row_not_per_event`: covered structurally (one `record` per `PathRouter::call`); no long-SSE test.
- `read_token_observe_no_table_change` uses a 300 ms settle, not `UsageService::flush` (seam owned by usage.rs agent).
- DISPATCH_SPY assertion uses before/after deltas on shared slug `alpha` (harness slugs not per-test unique).
- `#[allow(clippy::result_large_err)]` on `check` / `require_session_write` (Err is the ready response).
- `cargo fmt -p unimatrix-server` reformatted some pre-existing unformatted code (incl. tests/project_routing_integration.rs).

## Knowledge Stewardship
- Queried: Unimatrix not queried this session; used pseudocode/ADR text directly.
- Stored: nothing novel to store -- the one gotcha (guard 500s external-crate tests driving PathRouter without auth layer) is feature-specific and documented above.
