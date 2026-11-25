# vnc-050-agent-15-pr-blockers — PR #1022 blockers

## Report

### Root cause (Blocker 1)
Two independent CI causes; neither was the #1021 race in the failing tests:
1. **Node 18, all OSes** — test fixtures bound to `127.0.0.1` while clients dialed `https://localhost`. Runners have IPv6, so getaddrinfo (`ADDRCONFIG`) returns `::1` first; Node 18 has no happy-eyeballs fallback (`autoSelectFamily` defaults true only from Node 20) → `ECONNREFUSED`. The bridge then reported "endpoint unreachable" and exited 0 at EOF; the trusted-CA tests failed with `failureClass:"connect"`; the untrusted observe tests passed vacuously. The dev container has no global IPv6, so `ADDRCONFIG` drops `::1` and local Node 18 passed.
2. **Windows, all Node versions** — `observe-bundle.test.js` `writeStore` set only `HOME`; `os.homedir()` reads `USERPROFILE` on Windows → store written to the real profile → ENOENT.

#1021 is still real and security-relevant: `printf | bridge` hit EOF and exited 0 before the TLS-verify `exit(1)` fired. Fixed.

### Changes
- `lib/hook-client/mcp-bridge.js`: run loop tracks in-flight requests; non-initialize messages await a pending initialize; on EOF drain all, teardown, exit 0 after stdout flush. TLS verify failure exits 1 inside the drained request.
- `lib/hook-client/remote-resolve.js`: F1 — `UNIMATRIX_REMOTE_MCP_URL` / `UNIMATRIX_REMOTE_URL` must match `^https://` (case-insensitive); `ResolveError("bad_url", "<VAR> must be an https:// URL")`, value-free.
- Tests: `ca-trust-tls.test.js` (127.0.0.1, independent zero-bytes/exit/no-secret subtests, EOF-immediately regression, F1 bridge exit 1); `mcp-bridge.test.js` (run loop #1021: EOF drain, pre-initialize ordering); `observe-bundle.test.js` (127.0.0.1, USERPROFILE, F1 hook one-line exit 0); `remote-resolve.test.js` (F1 accept/reject matrix); `observe-latch.test.js`, `index.test.js`, `c14-verifier.js` moved from http stubs to TLS + local CA.
- Size gate: stripped 113137 → 113639, raw 200623 → 201399 (as-measured, no headroom).
- `docs/client-setup.md`: https-only env URL rule. `RISK-COVERAGE-REPORT.md` R-01 updated.

### Tests
Local `npm test` (Node 18.20.8): 961/960 pass+1 skip; layer2 24/24; package 406/405+1 skip; opencode 37/37; 0 fail.

## Knowledge Stewardship
- Queried: context_get 5829/5830 and briefing were not run in this rework pass; proceeded from the CI logs and source.
- Stored: none yet. Candidate pattern: "TLS fixtures must dial 127.0.0.1, not localhost: on Node 18 + IPv6 runners localhost goes to ::1 with no fallback".
