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
