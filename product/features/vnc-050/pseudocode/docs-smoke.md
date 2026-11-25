# C11 Docs + Scripted Smoke

FR-31, FR-32 (local equivalent only), AC-12, AC-13 (delivery part), HD 9. Depends on all components. Live AC-13 (GH Actions) and AC-14 (Tailscale) are post-delivery operator checks; delivery adds no GH Actions workflow and no Tailscale work.

## Deliverables
1. `docs/client-setup.md` (existing remote-client guide) — new section "Headless automation access". No parallel guide file.
2. Smoke script `packages/unimatrix/test/headless-smoke.test.js` (node:test, existing runner) driving the built server binary + docs recipe.
3. Additions to existing fixtures: local CA helper in `packages/unimatrix/test/helpers/` (extend existing TLS helpers used by `cert-pin-tls.test.js`; no isolated scaffolding).

## Doc Structure (each runnable block fenced as ```sh smoke``` so the smoke test can extract it verbatim)
```
## Headless automation access
### Mint a bundle (server host)
  unimatrix project register alpha                                   (existing)
  unimatrix client-bundle alpha --headless --mode read --label ci-read [--trust ca]
  unimatrix client-bundle alpha --list
  unimatrix client-bundle alpha --revoke ci-read
### Trust: pinned vs CA (`--trust ca` only behind a CA-issued cert; private CA via NODE_EXTRA_CA_CERTS)
### Attach with UNIMATRIX_BUNDLE (bridge)  — env var table incl. conflict list (FR-20) and empty=unset (FR-40)
### Extra headers (UNIMATRIX_REMOTE_HEADERS, Cloudflare Access example; reserved names list)
### Direct HTTPS (curl)                                   ```sh smoke```
  curl -sS --cacert "$CA" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
       -H 'Accept: application/json, text/event-stream' -D headers.txt \
       -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}' "$MCP_URL"
  SID=$(grep -i '^mcp-session-id:' headers.txt | cut -d' ' -f2 | tr -d '\r')
  curl ... -H "Mcp-Session-Id: $SID" -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$MCP_URL"
  curl ... -H "Mcp-Session-Id: $SID" -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context_search","arguments":{"query":"x"}}}' "$MCP_URL"
### Generic MCP client (stdio bridge)                     ```sh smoke```
  UNIMATRIX_BUNDLE="$BUNDLE" npx -y @dug-21/unimatrix mcp-bridge    (exact launch form per C10 argv decision)
### GitHub Actions example (YAML in docs only, NOT a workflow file in .github/)
  read bundle: search succeeds, store rejected; rw bundle: store on its slug succeeds, other slug 403
### Mode semantics: read = Read+Search, no observe (one warning, job not failed), no learning; read-write = + Write, SessionWrite; never Admin
### Residual gap (R-20/R-22): agent_id is self-asserted within a token's ceiling
### Disable the legacy token: http.legacy_token_enabled = false / UNIMATRIX_LEGACY_TOKEN_ENABLED=false
```
Protocol version string must be the one rmcp 1.7.0 accepts (copy from existing bridge `initialize` test fixture).

## Smoke Algorithm (`headless-smoke.test.js`)
```
before:
  ca = helpers.makeLocalCA(); leaf = ca.issue("localhost")           // CA-signed server cert
  tmp data dir; write leaf into {data_dir}/tls; boot server binary (http.enabled, UNIMATRIX_PUBLIC_URL=https://localhost:<port>)
  run `unimatrix project register alpha` and `beta`
  readBundle = run `client-bundle alpha --headless --mode read --label ci-read --trust ca` (stdout)
  rwBundle   = run `client-bundle alpha --headless --mode read-write --label ci-rw --trust ca`
  decode bundles (bundle.js) -> MCP_URL, TOKEN; CA path exported for curl; NODE_EXTRA_CA_CERTS for node
test "recipe verbatim (AC-12)":
  blocks = extract ```sh smoke``` fences from docs/client-setup.md (fail if zero blocks)
  for each block: run via `sh -euc` with env {CA, TOKEN, MCP_URL, BUNDLE}; assert exit 0; tools/call result has no error
test "local AC-13 equivalent":
  bridge with readBundle: context_search ok; context_store → CapabilityDenied; DB entries count unchanged
  bridge with rwBundle: context_store on alpha ok; curl with rw token to /v1/beta/... → 403
test "revoke (AC-16)": --revoke ci-read → next curl tools/call → 401
after: kill server, rm tmp
```
Skips (with explicit reason, never silent pass) only when the server binary is not built; CI builds it first.

## Post-delivery (not produced by delivery)
Tailscale findings doc (AC-14) and live GH Actions run (AC-13 live) — operator checks; C19 (#5792) stays unproven until both run.

## Key Test Scenarios
- Docs block extraction finds ≥ 3 blocks; any edit to doc commands is exercised automatically.
- AC-12, local AC-13, AC-16 as above.
