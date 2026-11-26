# Connecting a Client to a Remote Unimatrix Server

Connect Claude Code, Codex CLI, or Gemini CLI to a remote Unimatrix instance over HTTPS. A client attaches with a single `init` command that wires both the MCP tool surface and the pure-JS HTTP hook client automatically. Telemetry then flows over the per-slug route `POST /v1/{slug}/observe` — there are no hand-rolled curl hook scripts and no local `unimatrix` platform binary on the client machine.

## Prerequisites

- Node.js >= 18 on the client machine, with `npx` access to `@dug-21/unimatrix`. The remote attach is pure JS — no platform binary and no ONNX model are required, so it runs on Linux, macOS (Apple Silicon), and Windows.
- A running Unimatrix server reachable over HTTPS (the container HTTPS posture: TLS-only port 8443, `GET /health` the only unauthenticated endpoint).
- At least one project registered on the server (`unimatrix project register <slug>`) and a connection bundle emitted for it (see below). A client never mints a slug; it attaches to one the operator has already registered.

---

## Attach modes

There are two ways to attach a client. The **canonical** path is the per-project connection bundle (`init --bundle <blob>`). The **legacy** `--remote <url> --token <tok>` path is documented for completeness only.

### Bundle attach (canonical)

The operator emits a per-project connection bundle on the server, then the client consumes it.

**1. Operator emits the bundle (on the server):**

```bash
unimatrix client-bundle <slug>
```

Stdout is a single opaque `unimatrix-bundle:` (`v:2`) blob — it carries the server-composed MCP and observe endpoint URLs (the slug is already baked in by the server), the bearer token, and the certificate's `sha256:` fingerprint. The token is never printed to stderr; stderr echoes only the decoded URLs and fingerprint for the operator to eyeball.

**2. Client attaches (on the client machine):**

```bash
npx @dug-21/unimatrix init --bundle <blob>
```

The bundle path takes **no `--slug`** — the blob already encodes the slug, and the client composes no paths. `init` wires the pure-JS HTTP hook client into `.claude/settings.json` and registers a `stdio` `unimatrix` MCP server in `.mcp.json` (a fingerprint-pinned stdio→HTTPS bridge), so a bundle attach gives the full `context_*` MCP tool set over HTTPS, not just telemetry. The bearer credential (token, `observe_url`, `mcp_url`, and the certificate fingerprint) is written out-of-tree to `~/.unimatrix/<projectHash>/remote.json` (mode 0600) — never inside the repo working tree — so a stray `git add -A` cannot commit a live credential. `init` validates connectivity with a pinned `Ping` over fingerprint-pinned HTTPS before writing config.

**3. Telemetry flows automatically.** The wired hook client reads `observe_url` and the certificate fingerprint from the credential store and POSTs `HookRequest` frames to the server-composed `POST /v1/{slug}/observe` route over fingerprint-pinned HTTPS. The client posts to the finished URL verbatim, so it can never mis-target another project's store.

### Direct attach (legacy)

> **Legacy.** This `--remote <url> --token <tok>` form is documented for completeness only. It is effectively unused, will not be invested in, and does **not** support cloud MCP — cloud MCP is bundle-only. Prefer bundle attach above.

```bash
npx @dug-21/unimatrix init --remote https://uni.example.com --token <token>
```

This wires the HTTP hook client for telemetry only. `init` does not register a `unimatrix` MCP server on this path and emits a loud, deterministic message stating that cloud MCP requires a `v:2` bundle. The path forward for a legacy client is to migrate to a `v:2` bundle.

---

## How telemetry works

The `init`-wired pure-JS hook client reads `observe_url` and the certificate fingerprint from the out-of-tree credential store and POSTs `HookRequest` frames to `/v1/{slug}/observe` over fingerprint-pinned HTTPS. Sync events (`UserPromptSubmit`, `PreCompact`, `SubagentStart`) request `Accept: text/plain` so the server formats injection text; fire-and-forget events stream transcript deltas in a separate POST so the server's per-session buffer stays authoritative. The client is fail-open (exit 0 always, never blocks the host CLI) and uses a disk-backed event queue for graceful degradation.

The client pins the server's exact leaf certificate by its `sha256:` fingerprint — there is no certificate-authority trust path — so a self-signed cert is trusted by pinning rather than by a CA. The published port is TLS-only 8443; `GET /health` is the only unauthenticated endpoint.

Multiple distinct LLM CLIs (Claude Code, Codex CLI, Gemini CLI) attach the same server identically, each as a separate client connection. Each client instance is bound to exactly one project; a different project means a separate bundle and a separate client instance.

---

## Certificate rotation

When the operator rotates the server certificate, re-emit the bundle and re-run the attach on each client:

```bash
unimatrix client-bundle <slug>          # operator: re-emit with the new fingerprint
npx @dug-21/unimatrix init --bundle <blob>   # each client: re-attach
```

A presented certificate that does not match the pinned fingerprint is rejected with a clear, diagnosable mismatch error directing you to re-bundle. See [docs/cert-rotation.md](cert-rotation.md) for the operator rotation procedure.

---

## Token Rotation

If you need to rotate the bearer token:

1. Stop the server
2. Delete `{data_volume}/token`
3. Restart the server — a new token is generated
4. Re-emit the bundle (`unimatrix client-bundle <slug>`) and re-run `init --bundle <blob>` on each client to pick up the new credential

---

## Proxy-Terminated Deployments

When TLS is terminated by a reverse proxy (nginx, Caddy, cloud load balancer), set `[tls] enabled = false` in the server's `config.toml`. The server binds plain HTTP; the proxy handles TLS. Bearer token auth still applies — the proxy does not handle authentication.

---

## Headless automation access

CI jobs, scripts, and other non-interactive agents attach with a **scoped headless token** minted per slug. Each token is bound to exactly one slug, carries a fixed mode (`read` or `read-write`), and is revocable by label without touching any other client. It never receives Admin.

### Mint, list, revoke (server host)

```bash
unimatrix project register alpha          # existing step; slugs are never minted by clients
unimatrix client-bundle alpha --headless --mode read --label ci-read --trust ca > ci-read.bundle
unimatrix client-bundle alpha --list      # stderr table: label, mode, slugs, created, state — no secrets
unimatrix client-bundle alpha --revoke ci-read
```

- Stdout is one `unimatrix-bundle:` blob and nothing else; store it as a CI secret immediately — it is shown once and the server keeps only a hash. Stderr echoes URLs, trust, label, mode and slug, never the token.
- Labels are unique forever: a revoked label cannot be reused. A revoked token is rejected with `401` on its next request; audit rows it wrote keep its `token:<label>` attribution.
- Set `UNIMATRIX_PUBLIC_URL` on the server before minting, or the bundle carries a placeholder URL.

### Mode semantics

| Mode | Capabilities | Observe / telemetry | Learning signals |
|---|---|---|---|
| `read` | Read + Search | rejected `403`; the client logs one warning and the job does **not** fail | none recorded |
| `read-write` | Read + Search + Write + SessionWrite | accepted | recorded |

Neither mode ever grants Admin. A token bound to `alpha` gets `403` on every `/v1/<other-slug>/…` route.

Residual gap: within a token's ceiling the `agent_id` argument is self-asserted (a `read-write` token with no `agent_id` writes as `anonymous`). The ceiling, not `agent_id`, is the security boundary.

### Trust: pinned vs CA

- **Pinned (default, `--trust pin`)**: the bundle carries the server leaf's `sha256:` fingerprint; use this with the self-signed cert the server provisions.
- **CA (`--trust ca`)**: the bundle carries `fp: null` and the client verifies the server cert against the CA store with hostname checking. Use it only when the server presents a CA-issued cert (for example behind a proxy with a public cert). For a private CA, point Node at it with `NODE_EXTRA_CA_CERTS=/path/to/ca.pem`.
- `NODE_TLS_REJECT_UNAUTHORIZED` is **ignored** on the CA path: certificate verification cannot be switched off. Never set it as a workaround — fix the trust chain instead.

### Attach with `UNIMATRIX_BUNDLE` (no `init`)

The bridge and the observe client read the bundle straight from the environment; no `init`, no `remote.json`, no project hash.

| Variable | Meaning |
|---|---|
| `UNIMATRIX_BUNDLE` | The headless bundle. Takes precedence over every other credential source. |
| `UNIMATRIX_PROJECT_ROOT` | Required with any headless variable. Absolute path of the repository the headless credential belongs to; headless env applies only when the working directory is inside it. Without it, hooks and the MCP bridge treat every directory as not a Unimatrix project and exit 0 silently. |
| `UNIMATRIX_REMOTE_HEADERS` | JSON object of extra request headers, e.g. for an access proxy. |
| `NODE_EXTRA_CA_CERTS` | Private CA bundle for `--trust ca` bundles. |

- `UNIMATRIX_BUNDLE` cannot be combined with `UNIMATRIX_REMOTE_MCP_URL`, `UNIMATRIX_REMOTE_URL`, `UNIMATRIX_REMOTE_TOKEN` or `UNIMATRIX_REMOTE_FINGERPRINT`; the client exits with an error naming the conflicting variable.
- An empty value is treated as unset (no trimming).
- PowerShell is not supported as a hook shell.
- `UNIMATRIX_REMOTE_MCP_URL` and `UNIMATRIX_REMOTE_URL` must be `https://` URLs; any other scheme is rejected (the token is never sent in cleartext). The hook logs one stderr line and exits 0; the bridge exits 1.

### Extra headers (`UNIMATRIX_REMOTE_HEADERS`)

```bash
export UNIMATRIX_REMOTE_HEADERS='{"CF-Access-Client-Id":"<id>","CF-Access-Client-Secret":"<secret>"}'
```

At most 32 headers, names must be valid HTTP tokens, values printable ASCII up to 8 KiB, and no case-insensitive duplicates. Reserved names cannot be set: `Authorization`, `Mcp-Session-Id`, `Host`, `Content-Length`, `Content-Type`, `Transfer-Encoding`, `Connection`, `Accept`. Header values are never logged.

### Direct HTTPS (curl)

Any HTTP client can speak MCP Streamable HTTP directly. Decode the bundle (Node is enough; `CA` is only needed for a private CA):

```sh smoke
MCP_URL=$(node -e 'console.log(JSON.parse(Buffer.from(process.argv[1].slice(17),"base64url")).mcp_url)' "$UNIMATRIX_BUNDLE")
TOKEN=$(node -e 'console.log(JSON.parse(Buffer.from(process.argv[1].slice(17),"base64url")).token)' "$UNIMATRIX_BUNDLE")
CURL="curl -sS --fail ${CA:+--cacert $CA} -H Content-Type:application/json -H Accept:application/json,text/event-stream"
```

Initialize and confirm the session, then call a tool:

```sh smoke
$CURL -H "Authorization: Bearer $TOKEN" -D headers.txt \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"curl","version":"0"}}}' \
  "$MCP_URL" > /dev/null
SID=$(grep -i '^mcp-session-id:' headers.txt | cut -d' ' -f2 | tr -d '\r')
$CURL -H "Authorization: Bearer $TOKEN" -H "Mcp-Session-Id: $SID" \
  -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$MCP_URL" > /dev/null
```

```sh smoke
$CURL -H "Authorization: Bearer $TOKEN" -H "Mcp-Session-Id: $SID" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context_lookup","arguments":{"query":"deploy"}}}' \
  "$MCP_URL"
```

With a pinned (self-signed) bundle, curl cannot pin by fingerprint; use the bridge below, or `--cacert` the server's own `cert.pem`.

### Generic MCP client (stdio bridge)

Any MCP client that launches a stdio server can use the bridge with the bundle in its environment:

```json mcp-config
{
  "mcpServers": {
    "unimatrix": {
      "command": "npx",
      "args": ["-y", "@dug-21/unimatrix", "mcp-bridge"],
      "env": { "UNIMATRIX_BUNDLE": "<bundle>", "UNIMATRIX_PROJECT_ROOT": "/absolute/path/to/repo" }
    }
  }
}
```

The bridge expects a real MCP client: it relays requests after the `initialize` handshake completes. For shell scripts and CI, use the curl recipe above.

### GitHub Actions example

Store the bundle as a repository secret. This is an example for your own workflow — Unimatrix ships no workflow file.

```yaml
jobs:
  knowledge:
    runs-on: ubuntu-latest
    env:
      UNIMATRIX_BUNDLE: ${{ secrets.UNIMATRIX_BUNDLE_CI_READ }}
      UNIMATRIX_PROJECT_ROOT: ${{ github.workspace }}
    steps:
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - name: Search Unimatrix
        run: |
          MCP_URL=$(node -e 'console.log(JSON.parse(Buffer.from(process.argv[1].slice(17),"base64url")).mcp_url)' "$UNIMATRIX_BUNDLE")
          TOKEN=$(node -e 'console.log(JSON.parse(Buffer.from(process.argv[1].slice(17),"base64url")).token)' "$UNIMATRIX_BUNDLE")
          CURL="curl -sS --fail -H Content-Type:application/json -H Accept:application/json,text/event-stream"
          $CURL -H "Authorization: Bearer $TOKEN" -D headers.txt \
            -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"gha","version":"0"}}}' \
            "$MCP_URL" > /dev/null
          SID=$(grep -i '^mcp-session-id:' headers.txt | cut -d' ' -f2 | tr -d '\r')
          $CURL -H "Authorization: Bearer $TOKEN" -H "Mcp-Session-Id: $SID" \
            -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' "$MCP_URL" > /dev/null
          $CURL -H "Authorization: Bearer $TOKEN" -H "Mcp-Session-Id: $SID" \
            -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context_search","arguments":{"query":"release checklist"}}}' \
            "$MCP_URL"
```

With a `read` bundle the search succeeds and `context_store` is rejected with a capability error. With a `read-write` bundle `context_store` on the bundle's slug succeeds; any other slug returns `403`.

### Disable the legacy token

The per-data-dir legacy bearer token (`{data_volume}/token`, used by `client-bundle <slug>` without `--headless`) stays enabled by default so existing clients keep working. Once every client uses scoped tokens, disable it:

```toml
[http]
legacy_token_enabled = false
```

or set `UNIMATRIX_LEGACY_TOKEN_ENABLED=false`. Requests with the legacy token then get `401` with a `legacy token disabled` indication; scoped tokens are unaffected.

---

## Verifying the connection

### 1. Liveness (server is up)

The `/health` endpoint requires no authentication:

```bash
curl https://<host>:8443/health
```

Returns:

```json
{"version": "x.y.z", "schema_version": 27}
```

Use this for Docker HEALTHCHECK or external monitoring. Note: `/health` proves only
that the server is *up* — not that *your client works*. For that, do step 2.

### 2. Client works (a real authenticated `context_*` op over the pinned-TLS bundle)

After `unimatrix init --bundle …` writes `.mcp.json`, confirm the attached client can
perform a real operation over the pinned-TLS bundle path — not just reach `/health`. This
reads the bridge command your `init --bundle` wrote into `.mcp.json` and drives one
stateless `context_status` call through it (requires `jq`):

```bash
printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"verify","version":"1.0.0"}}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context_status","arguments":{}}}' \
  | node "$(jq -r '.mcpServers.unimatrix.args[0]' .mcp.json)" \
         "$(jq -r '.mcpServers.unimatrix.args[1]' .mcp.json)"
```

A non-error JSON-RPC result (no `"error"` field, no `"isError": true`) confirms the client
performs real `context_*` operations over the pinned-TLS bundle. This is the same check the
release smoke's client-works gate runs (`docker-http-posture-smoke.sh`, Gate 9).

---

_Verified on v0.x.y_
