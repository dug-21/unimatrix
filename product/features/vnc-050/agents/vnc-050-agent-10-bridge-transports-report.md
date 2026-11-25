# vnc-050-agent-10-bridge-transports — C10 Bridge / Transports

## Report
- Modified: lib/hook-client/{mcp-bridge.js, mcp-bridge/http-session.js, transport-http.js, config.js, index.js (misconfig reason/detail only)}, lib/init.js (pingForInit extra headers). Created: lib/hook-client/tls-guard.js, test/helpers/ca-fixture.js, test/hook-client/ca-trust-tls.test.js. Updated test/hook-client/mcp-bridge.test.js (null-fp → CA per ADR-006; new "no MCP credential" message).
- CA branch: no fp → plain https, no rejectUnauthorized key, assertCaOptions guard; body deferred until secureConnect + authorized; CA verify error → bridge exit 1, code-only stderr. Pinned statements unchanged; cert-pin.js and pin test files: empty diff.
- Extra headers on initialize/POST/DELETE/observe/pingForInit; reserved names always client-owned. Bridge has no standalone SSE GET (gap noted in pseudocode).
- Hook: ResolveError → {ok:false, reason:"misconfig", detail}; exit 0, one stderr line naming the var.
- Tests: full suite 1342 pass / 0 fail (concurrency 4). ca-trust-tls 11/11 (zero-byte captures for self-signed and wrong-host across observe POST, pingForInit, bridge).
- Size gate: cap raised per human decision (vnc-050 #1010, PL-10 no-grow) to as-measured 113137 stripped / 200623 raw (was 110000/200000); size-gate.test.js updated in lockstep. PASS at 0 headroom.
- Rework (human decisions): CA path sets rejectUnauthorized:true explicitly (global NODE_TLS_REJECT_UNAUTHORIZED=0 ignored); parity-layer2-uds/precompact now pin via UNIMATRIX_REMOTE_FINGERPRINT=server.pinnedFp instead of disabling TLS; guard rejects missing/false rejectUnauthorized (ADR-006 #5829); new test global_tls_override_ignored_ca_path. Full suite 1343 pass / 0 fail / 1 skip.
- Not committed.

### Issues / decisions for review
1. Resolved: NODE_TLS_REJECT_UNAUTHORIZED=0 is ignored on the CA path (human decision).
2. config.js passes readStore:()=>null when UNIMATRIX_REMOTE_URL is set, so the env observe pair never touches the store (fixes test_unwritable_state_dir_exit0 regression; restores old "env wins outright").
3. classifyResponse 403 left to C6c (its deny flag already present).

## Knowledge Stewardship
- Queried: context_search pattern/decision vnc-050 — ADR-006/007 consistent with pseudocode.
- Stored: nothing stored yet — candidate pattern: "spawnSync in a node:test that hosts the server blocks the event loop; zero-byte TLS assertions pass vacuously — use async spawn".
