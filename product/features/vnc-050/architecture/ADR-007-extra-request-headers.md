## ADR-007: `UNIMATRIX_REMOTE_HEADERS` — Strict JSON, Reserved Names, Applied on Every Request, Never Logged

### Context
Callers behind Cloudflare Access need headers such as `CF-Access-Client-Id` and `CF-Access-Client-Secret` on every request (AC-06, OQ-2). SR-04: malformed JSON, non-string values and CRLF injection. SR-16: if the headers reach initialize but not the SSE GET or the observe POST, Access blocks those requests silently.

### Decision
- `remote-resolve.js parseExtraHeaders(raw) -> Array<[name, value]>`. The value is caller-owned and separate from the bundle, and it applies to every source tier (bundle, env, store). Bundle + `UNIMATRIX_REMOTE_HEADERS` is an explicitly allowed combination (ADR-006, V-2b).
- **Validation** is fail-loud, exit 1, and the message names the header but never its value:
  - The value must be a JSON object, with at most 32 entries.
  - Names must match the RFC 7230 token grammar `^[!#$%&'*+.^_\`|~0-9A-Za-z-]+$`.
  - Values must be strings, at most 8 KiB, and contain no CR, LF or NUL.
  - Names are compared case-insensitively against a reserved set, and a reserved name is rejected: `authorization`, `mcp-session-id`, `host`, `content-length`, `content-type`, `transfer-encoding`, `connection`, `accept`.
  - Duplicate names that differ only in case are rejected.
- **Application**: the headers are merged first and the reserved headers are set after them, as a second line of defense:
  - `HttpSession._headers` covers initialize, every POST, the SSE GET and session DELETE.
  - `transport-http.js` covers the observe POST and `pingForInit`.
- **Logging**: no code path logs header values. Errors and debug output print header names only. A test greps captured stderr for a sentinel value.

### Consequences
- The bridge can work behind identity-aware proxies without code changes per proxy.
- A typo in the header JSON stops startup instead of producing a silent 403 later.
