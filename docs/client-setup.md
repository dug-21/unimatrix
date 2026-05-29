# Client Setup — Remote Unimatrix Server

Connect Claude Code, Codex CLI, or Gemini CLI to a remote Unimatrix instance over HTTPS. All three clients connect via MCP (tool calls) and use curl-based shell hooks for telemetry. No local `unimatrix` binary is required on the client machine.

**Prerequisites:**
- A running Unimatrix server with `[http] enabled = true` (see SCOPE.md or server config docs)
- The bearer token printed at first server start (`[UNIMATRIX TOKEN] <64-hex-chars>`)
- The server's hostname or IP and content port (default 8443)
- `curl` available on the client machine (POSIX systems only)

---

## Claude Code

### MCP Connection

Use `claude mcp add` with the `-H` flag to attach the authorization header directly. This is required due to [anthropics/claude-code#28293](https://github.com/anthropics/claude-code/issues/28293) — headers defined in `.mcp.json` are not forwarded on tool call POSTs.

```bash
claude mcp add unimatrix \
  -H "Authorization: Bearer <token>" \
  -- https://<host>:8443
```

Replace `<token>` with the 64-character hex token from the server. Replace `<host>` with the server hostname or IP.

### Shell Hook (curl-based)

Add to your Claude Code hook configuration. The hook POSTs observation events to the remote `/observe` endpoint. No local binary needed.

```bash
#!/bin/sh
# .claude/hooks/unimatrix-observe.sh
# Posts hook events to remote Unimatrix /observe endpoint.
# Returns 501 until W2-7 ships the remote telemetry handler.

UNIMATRIX_URL="https://<host>:8443"
UNIMATRIX_TOKEN="<token>"

curl -s -X POST "${UNIMATRIX_URL}/observe" \
  -H "Authorization: Bearer ${UNIMATRIX_TOKEN}" \
  -H "Content-Type: application/json" \
  -d @- < /dev/stdin
```

Make the script executable:

```bash
chmod +x .claude/hooks/unimatrix-observe.sh
```

Reference this script in your Claude Code hook configuration for the events you want to observe (e.g., `UserPromptSubmit`, `SubagentStart`, `PreCompact`, `Stop`).

> **Note:** The `/observe` endpoint returns HTTP 501 until W2-7 (remote telemetry transport) is shipped. The hooks are ready — install them now so they activate automatically when the server-side handler lands.

---

## Codex CLI

### MCP Connection

Add the Unimatrix server to your Codex CLI MCP configuration:

```json
{
  "mcpServers": {
    "unimatrix": {
      "url": "https://<host>:8443",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

Replace `<token>` and `<host>` with your server's values.

### Shell Hook (curl-based)

```bash
#!/bin/sh
# .codex/hooks/unimatrix-observe.sh
# Posts hook events to remote Unimatrix /observe endpoint.
# Returns 501 until W2-7 ships the remote telemetry handler.

UNIMATRIX_URL="https://<host>:8443"
UNIMATRIX_TOKEN="<token>"

curl -s -X POST "${UNIMATRIX_URL}/observe" \
  -H "Authorization: Bearer ${UNIMATRIX_TOKEN}" \
  -H "Content-Type: application/json" \
  -d @- < /dev/stdin
```

Make the script executable and reference it in `.codex/hooks.json` for the desired events.

> **Note:** The `/observe` endpoint returns HTTP 501 until W2-7. Codex CLI requires `--provider codex-cli` when using the local `unimatrix hook` subcommand, but the curl-based remote hook does not need this flag — the server identifies the caller from the MCP session's `clientInfo.name`.

---

## Gemini CLI

### MCP Connection

Add the Unimatrix server to your Gemini CLI MCP configuration (`.gemini/settings.json`):

```json
{
  "mcpServers": {
    "unimatrix": {
      "url": "https://<host>:8443",
      "headers": {
        "Authorization": "Bearer <token>"
      }
    }
  }
}
```

Replace `<token>` and `<host>` with your server's values. Requires Gemini CLI v0.31+.

### Shell Hook (curl-based)

```bash
#!/bin/sh
# .gemini/hooks/unimatrix-observe.sh
# Posts hook events to remote Unimatrix /observe endpoint.
# Returns 501 until W2-7 ships the remote telemetry handler.

UNIMATRIX_URL="https://<host>:8443"
UNIMATRIX_TOKEN="<token>"

curl -s -X POST "${UNIMATRIX_URL}/observe" \
  -H "Authorization: Bearer ${UNIMATRIX_TOKEN}" \
  -H "Content-Type: application/json" \
  -d @- < /dev/stdin
```

Make the script executable and reference it in your Gemini CLI hook configuration for the desired events (`BeforeTool`, `AfterTool`, `SessionEnd` — these are normalized to canonical Unimatrix names server-side).

> **Note:** The `/observe` endpoint returns HTTP 501 until W2-7. Install hooks now; they activate when the server-side handler ships.

---

## Token Rotation

If you need to rotate the bearer token:

1. Stop the server
2. Delete `{data_volume}/token`
3. Restart the server — a new token is generated and printed to stdout
4. Update all client configurations with the new token

---

## Proxy-Terminated Deployments

When TLS is terminated by a reverse proxy (nginx, Caddy, cloud load balancer), set `[tls] enabled = false` in the server's `config.toml`. The server binds plain HTTP; the proxy handles TLS. Bearer token auth still applies — the proxy does not handle authentication.

Update client URLs to match your proxy's external address (the proxy forwards to the Unimatrix content port).

---

## Health Check

The `/health` endpoint requires no authentication:

```bash
curl https://<host>:8443/health
```

Returns:

```json
{"version": "x.y.z", "schema_version": 27}
```

Use this for Docker HEALTHCHECK or external monitoring.
