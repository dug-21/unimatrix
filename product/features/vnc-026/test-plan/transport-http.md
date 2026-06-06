# Test Plan: transport-http.js

New module (no Rust oracle). ADR-005 timeouts (ACCEPTED: 750/2,000/3,000 ms — do not flag vs
NFR-02 500 ms; different regimes). Risks: R-10, R-15, R-16, AC-02, AC-03, AC-09.
Suite: `test/hook-client/transport-http.test.js` against the stub-server helper.

## Request Shape (AC-02 / AC-03)

- `test_post_method_and_path` — POST to `{url}/observe`; body = exact HookRequest JSON.
- `test_headers_fnf` — `Authorization: Bearer <token>`, `Content-Type: application/json`, `Accept: application/json` (or absent per pseudocode) — NO `text/plain` on FNF.
- `test_headers_sync_every_arm` — `Accept: text/plain` present on ALL THREE sync arms (ContextSearch, CompactPayload, Ping) — per-sync-event assertion (the #4703 integration-risk shape: one missed arm prints raw JSON).
- URL forms (edge cases): trailing slash (`https://h/` → `/observe` not `//observe`), `http://` vs `https://` module selection, explicit port, path prefix (`https://h/base` → `/base/observe`), IPv6 literal. One test per form asserting the stub receives the right path.

## Timeouts (ADR-005 — values accepted, structure tested)

- `test_connect_timeout_750ms` — stub refusing to accept within 750 ms → failure class `connect`; measured wall time < ~1 s (no hang).
- `test_sync_total_timeout_2000ms` — stub delays response 3 s on a sync request → request aborted ~2,000 ms, no stdout, exit-path clean (timeout-expiry timing test, AC-09).
- `test_fnf_total_timeout_3000ms` — analogous on FNF.
- `test_timeout_overrides_from_config` — overrides honored.

## Response Classification (R-10 input; breadcrumb writing asserted in state.md)

| Stub behavior | Expected class |
|---|---|
| ECONNREFUSED | `connect` |
| connect/total deadline exceeded | `timeout` |
| 401, 403 | `auth` |
| 404, 413 | `http_4xx` |
| 500, 503 | `http_5xx` |
| 200, 204 | success (2xx incl. 204 = success, AC-02) |

- `test_classification_matrix` — table-driven over the rows above; classification result returned to the caller (used by state.md breadcrumb matrix and queue.md enqueue decisions).

## Sync Response Defense (R-15)

- `test_sync_200_nontext_content_type_dropped` — 200 with `Content-Type: application/json` (and with header absent) → treated as no-output; no stdout (transform.md asserts the stdout side).
- `test_sync_200_empty_body_no_output` — silent-skip parity.
- `test_sync_oversized_200_body` — large 200 text body handled without throw (server gates at MAX_INJECTION_BYTES; client prints verbatim — bounded-read behavior per pseudocode must not hang).

## Security (R-16)

- `test_no_token_in_errors` — across the classification matrix, token string never appears in the error object's message/stack surfaced to stderr.
- `test_https_used_for_https_urls` — no silent http downgrade.

## Concrete Assertions

- `send(request, {sync, config}) -> {ok, status, contentType, body} | {ok:false, class}` — never throws; never writes stdout/stderr itself (caller owns observability).
