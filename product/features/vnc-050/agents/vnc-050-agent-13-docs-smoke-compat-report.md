# vnc-050-agent-13-docs-smoke-compat — Report (Wave 4, C11 + compat matrix)

## Files
- docs/client-setup.md — new "Headless automation access" section (mint/list/revoke, modes, trust pin vs CA, NODE_EXTRA_CA_CERTS, NODE_TLS_REJECT_UNAUTHORIZED ignored, UNIMATRIX_BUNDLE + conflicts + empty=unset, UNIMATRIX_REMOTE_HEADERS + reserved names, curl recipe as 3 `sh smoke` blocks, generic MCP client `json mcp-config`, GH Actions YAML (docs only), R-20/R-22 residual, legacy_token_enabled).
- packages/unimatrix/test/headless-smoke.test.js (new) — AC-12, local AC-13, AC-16 (ac16_end_to_end), AC-17 (audit_attribution_retained_after_revoke), doc grep checks.
- packages/unimatrix/test/compat-matrix.test.js (new) — R-21 matrix, FR-34..39.
- packages/unimatrix/test/helpers/old-client.js (new) — 0.11.3 tarball, sha512 integrity pinned, cache outside package tree, git-archive v0.11.3 fallback flagged.
- packages/unimatrix/test/helpers/real-server.js — extended: opts tlsPair, extraSlugs, publicUrl, env, extraToml, port, bin; `cli()`/`dataDir` exposed; binary resolution picks newest build (stale release was shadowing debug).
- packages/unimatrix/test/fixtures/pre-vnc050-bundle.txt (new) — frozen pre-vnc-050 v:2 bundle.
- packages/unimatrix/bin/unimatrix.js — `mcp-bridge` no longer demands projectHash when UNIMATRIX_BUNDLE / UNIMATRIX_REMOTE_MCP_URL set (FR-21 gap: C10 changed lib main() but bin still exited 2). lib/ untouched.

## Tests
- headless-smoke: 9/9 pass. compat-matrix: 5 pass, 1 skip (fr39 diff, needs UNIMATRIX_COMPAT_BASELINE_BIN = main-built server).
- Affected suites (bin-mcp-bridge, cli-routing, shim, all real-server users): 74 pass, 1 skip, 1 fail (parity-layer2 AC-04 cross-transport replay) under parallel load; passes in isolation with and without the real-server.js change, so it is a load flake. Full `npm test` exceeded 900 s locally. Size gate is OK at 0 headroom; lib/ is untouched.

## Issues
1. Bridge (pre-existing, lib/, not fixed — size gate): stdin EOF exits before in-flight responses, and concurrent requests before initialize completes yield `id:null "unexpected content-type"`. The existing "Verifying the connection" `printf | node bridge` recipe in client-setup.md is affected. Docs now steer shell/CI to curl; bridge reserved for real MCP clients.
2. 0.11.3 env observe ignores UNIMATRIX_REMOTE_FINGERPRINT (unpinned) — works only with a CA-trusted cert; compat cell uses local CA. Matrix doc row "± fp" for old client is effectively "fp ignored".
3. AC-20 old-client *write* not exercised (context_store needs embedding; old cells use context_status + context_lookup). Smoke covers writes on new client with model cache (UNIMATRIX_MODEL_CACHE → ~/.cache/unimatrix/models, polled up to 120 s).
4. FR-39 diff needs a main-built binary; skipped with explicit reason by default.
5. AC-17 test reads audit_log via python3 sqlite3 (skips with reason if absent).

## Knowledge Stewardship
- Queried: context_get #5828 (embedding never ready in-process — applied: lookup/status for read cells, poll for store), #5830 (no writes into package dirs — old client cache + shims in os.tmpdir).
- Stored: nothing novel stored in Unimatrix — findings (bridge EOF race, 0.11.3 env unpinned) are reported as issues for the SM to route; storing deferred pending fix decision.
