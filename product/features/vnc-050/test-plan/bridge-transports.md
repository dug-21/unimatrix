# C10 Bridge / Transports — `mcp-bridge.js`, `mcp-bridge/http-session.js`, `transport-http.js`, `config.js`

Risks: R-01, R-07, R-08, R-09, R-11, R-18. ACs: AC-02, AC-03, AC-04, AC-05, AC-06. All tests spawn real processes with clean HOME (`packages/unimatrix/test/hook-client/`, new `headless-bridge.test.js`, `ca-trust-tls.test.js`).

## Helpers (extend `test/helpers/`)
- `ca-fixture.js`: generate local CA + leaf for `localhost` and a wrong-host leaf (node `crypto`/openssl, no new deps); exposes `NODE_EXTRA_CA_CERTS` path.
- `byte-capture-tls.js`: raw `tls.createServer` recording every decrypted application byte.
- `capturing-proxy.js`: TLS server that records request method/path/headers then forwards to `real-server.js`.

## R-01 CA branch (Critical, real socket)
| Test | Assertion |
|---|---|
| `untrusted_self_signed_zero_bytes_bridge` | no fp; capture server bytes == 0; exit 1; stderr lacks token |
| `ca_trusted_wrong_host_zero_bytes_bridge` | same |
| `..._observe_post`, `..._ping_for_init` | same for `transport-http.js` observe and `pingForInit` |
| `guard_throws_on_reject_unauthorized_false` | unpinned request options with `rejectUnauthorized:false` → throws before socket |
| `ca_branch_no_cert_pin_agent` | companion: CA-trusted correct host → connects (AC-02) |

## R-11 Pinned unchanged
- `pin_test_files_unchanged`: shell gate `git diff main -- packages/unimatrix/test/cert-pin-tls.test.js packages/unimatrix/test/hook-client/mcp-bridge-sse.test.js <other existing pin tests>` empty; they pass.
- `fp_source_selects_pinned` × {bundle, `UNIMATRIX_REMOTE_FINGERPRINT`, store}: matching fp vs self-signed server connects; mismatched fp → refused, zero bytes.

## R-07 Parity (Critical)
- `precedence_matrix_bridge_and_config_agree`: each remote-resolve.md precedence case run through spawned `mcp-bridge.js` AND hook (`config.js`) — same outcome (attach target / exit code / named var).
- `bundle_only_observe_lands`: `UNIMATRIX_BUNDLE` only, rw → observe event arrives at bundle `observe_url` with bundle trust; DB row present; pinned and CA runs.
- `env_pair_attach_ca` (AC-02): `UNIMATRIX_REMOTE_MCP_URL`+`_TOKEN`, CA via `NODE_EXTRA_CA_CERTS`: initialize + tools/call OK; no `remote.json`.
- `partial_pair_exit_nonzero` (AC-03): each var alone → non-zero, stderr names missing var, sentinel scan clean.

## R-18 conflicts through processes
Bundle + each of 4 conflict vars → bridge exits 1 naming var; hook exits 0 (`{ok:false, reason:"misconfig"}`, FR-24a) with exactly one stderr line naming the var; neither leaks credential/value (sentinel scan); bundle + headers → accepted; empty conflict var → attach OK; partial empty pair → error (AC-28/29).

## R-08 Headers (capturing proxy)
- `headers_on_every_request_type`: initialize, tools/call POST, SSE GET, DELETE, observe POST, `pingForInit` all carry `X-Sentinel`.
- `reserved_header_rejected_exit_1`: `{"authorization":..}` → exit 1, name printed, value not.
- `reserved_headers_own_even_if_parser_bypassed`: inject entries directly into `HttpSession.create({extraHeaders})` → Authorization/mcp-session-id on wire are the bridge's.

## R-09 Leakage
`no_sentinel_in_any_output`: across all above error paths capture stdout, stderr, debug logs (`UNIMATRIX_DEBUG=1`) → zero sentinel token/header hits.
