# C9 Remote Resolver — `lib/hook-client/remote-resolve.js`

Risks: R-07, R-08, R-18, R-09. ACs: AC-03, AC-06, AC-28, AC-29. Unit tests here are seams; each behavioral row is re-proven by a spawned process in bridge-transports.md.

## Precedence (`resolveRemote`)
| Case | Expected |
|---|---|
| bundle only | mcp+observe from bundle, pinnedFp = bundle fp |
| MCP env pair only | mcp from env; observe from observe tier |
| store only | from credential store |
| bundle + `UNIMATRIX_REMOTE_MCP_URL` / `_URL` / `_TOKEN` / `_FINGERPRINT` | `ResolveError` naming var, message has no token |
| bundle + `UNIMATRIX_REMOTE_HEADERS` | accepted, headers returned |
| bundle + each conflict var `""` | no conflict (AC-28) |
| URL set + TOKEN `""` (and reverse), MCP and observe pairs | partial-pair error naming both vars; no credential bytes (AC-29) |
| both halves `""` | store used |
| fingerprint without URL | error |
| MCP env tier inactive without `UNIMATRIX_REMOTE_MCP_URL` | falls to store |

## `parseExtraHeaders`
Reject (ResolveError, header name in message, value never): invalid JSON, array, non-string value, CR, LF, NUL, value > 8 KiB, > 32 entries, bad name token, each reserved name (authorization, mcp-session-id, host, content-length, content-type, transfer-encoding, connection, accept) in mixed case, case-duplicate names. Accept: undefined, `""`, `{}` → `[]`; 32 entries; 8 KiB value exactly; `\t` inside value accepted. Non-ASCII (ADR-007, values must be HTAB or 0x20–0x7E): `é` (U+00E9), `€` (U+20AC), emoji, `\x7f` (DEL), `\x01` → `bad_headers` naming header, value absent.

`errors_sentinel_free`: every ResolveError message lacks token/header sentinel.
