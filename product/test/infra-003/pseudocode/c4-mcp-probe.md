# C4 — MCP-write probe, both directions, per-route own `Mcp-Session-Id`

> Source: ARCH C4, ADR-003, SPEC FR-02.3/FR-07.3/AC-06/AC-15, RISK R-01/R-02/R-17.
> SR-10 — the **load-bearing half** (the `entry.store==adapter-store` invariant is
> a `debug_assert!` compiled OUT of release, `seam.rs:345`; the shipped artifact
> has zero MCP-isolation coverage without this probe).

## Purpose

Drive a real `context_store` MCP write through the production MCP route in **both
directions**, each over its **own** streamable-HTTP session, and prove each via
the C5/C6 content read of `entries` — **never** the RPC success code (SR-10). The
endpoint is rmcp's `StreamableHttpService` (`http/router.rs:299-375`), which
**forces SSE** (#5296/#5129) and requires a session: a bare `tools/call` is
rejected. Each probe runs its own handshake and captures its own `Mcp-Session-Id`;
**A's session is NEVER reused against B's route** (a crossed session
mis-attributes the very isolation under test — R-17/C-13).

This component issues the **write** only (handshake + `tools/call`). The barrier
read and verdict are C5/C7.

## Wire Contract (ADR-003 §Decision)

Per route, three frames over the same cert-pinned bearer path (one token, slug in
path):
1. `initialize` → capture **that route's** `Mcp-Session-Id` response header.
2. `notifications/initialized` (with that session header).
3. `tools/call` `context_store` (with that session header; marker in
   `content` and `topic`).

Required headers on every MCP request:
`Accept: application/json, text/event-stream` (rmcp forces SSE — a JSON-only
`Accept` is refused, #5296/#5129), `Authorization: Bearer ${TOKEN}`,
`Content-Type: application/json`, and on frames 2–3 the captured
`Mcp-Session-Id: <that route's id>`.

Response bodies are **SSE-framed**: the JSON-RPC result must be extracted from the
`data:` event lines, not parsed as a bare body.

## New Functions

### `mcp_handshake(slug)` → echoes that route's `Mcp-Session-Id`, or INFRA

```
mcp_handshake(slug):
    url := "https://localhost:${PORT}/v1/${slug}/mcp"

    # Frame 1: initialize — capture the per-route session id from RESPONSE HEADERS.
    # Dump headers (curl -D) so we can read the Mcp-Session-Id header (a UUID minted
    # at initialize, #4708 — distinct from any tool session_id param).
    resp_headers, resp_body := curl -sS --cacert "$TMP/cert.pem" -D - \
        -X POST url \
        -H "Authorization: Bearer ${TOKEN}" \
        -H "Accept: application/json, text/event-stream" \
        -H "Content-Type: application/json" \
        -d <jsonrpc initialize frame>
      on transport failure: infra_fail "MCP initialize <slug> transport failure (INFRA)"

    sid := extract_header(resp_headers, "Mcp-Session-Id")   # case-insensitive
    if empty(sid):
        infra_fail "MCP initialize <slug>: no Mcp-Session-Id minted — handshake
                    failed (INFRA, not RED — transport fault ≠ isolation fault)"

    # Frame 2: notifications/initialized with THIS route's session id.
    curl ... url -H "Mcp-Session-Id: ${sid}" -d <jsonrpc notifications/initialized>
      on failure / SSE error: infra_fail "MCP initialized <slug> failed (INFRA)"

    return sid                       # caller binds sid ONLY to this slug's route
```

### `mcp_write(slug, marker)` → asserts JSON-RPC success; returns on success

```
mcp_write(slug, marker):
    sid := mcp_handshake(slug)       # OWN session per route — never crossed (R-17)
    url := "https://localhost:${PORT}/v1/${slug}/mcp"

    # Frame 3: tools/call context_store with marker in content AND topic.
    # Body built with node (safe JSON); marker is already [a-z0-9-].
    frame := node-build(jsonrpc tools/call {
        name: "context_store",
        arguments: { content: marker, topic: marker, ...minimal valid args... }
    })

    sse := curl -sS --cacert "$TMP/cert.pem" \
        -X POST url \
        -H "Authorization: Bearer ${TOKEN}" \
        -H "Accept: application/json, text/event-stream" \
        -H "Content-Type: application/json" \
        -H "Mcp-Session-Id: ${sid}" \              # THIS route's id, never A's on B
        -d frame
      on transport failure: infra_fail "MCP tools/call <slug> transport failure (INFRA)"

    result := parse_sse_jsonrpc(sse)               # extract JSON-RPC from data: events
    if result has "error" (e.g. -32099 SESSION_NOT_FOUND) or no result:
        infra_fail "MCP context_store <slug> JSON-RPC error <code> — write did not
                    execute (INFRA, not RED). marker=<marker>"
    log "MCP write <slug> succeeded (own session ${sid}). marker=<marker>"
```

### `parse_sse_jsonrpc(sse_text)` → JSON-RPC object

```
parse_sse_jsonrpc(sse_text):
    # rmcp returns the JSON-RPC result inside SSE `data:` lines. Concatenate the
    # data: payloads and JSON-parse via node. A bare-body parse would choke on the
    # text/event-stream framing (R-01).
    data := join(lines of sse_text starting with "data:", stripped of "data:")
    return node-json-parse(data)   # on parse failure → INFRA (SSE-parse error)
```

> `context_store` (default, ADR-003) persists an `entries` row carrying the marker
> in `content`/`topic`. `context_correct` is permitted but needs a prior entry to
> correct (else the write no-ops, R-02 sc.2) — `context_store` is the simplest
> single-row marker. The exact JSON-RPC frame bytes are a tester detail; this
> component fixes the approach + integration surface (ADR-003 §Decision).

## Per-route session isolation (R-17 / AC-15 — load-bearing)

- `mcp_handshake(A)` and `mcp_handshake(B)` each mint and return their **own**
  `sid`. The script binds each `sid` to a distinct variable (`SID_A`, `SID_B`) and
  passes only `SID_A` to A's `tools/call`, only `SID_B` to B's. There is no shared
  session variable that could be crossed.
- A crossed/reused session is **structurally excluded** by construction, not by a
  runtime check — the gate never holds a session id it could send to the wrong
  route.
- Handshake/session failure on either route is **INFRA** (per direction), distinct
  from a wrong-store-marker **RED** (which is C6/C7).

## Data Flow

| In | Out |
|----|-----|
| `slug`, `marker` (`$M_MCP_A`/`$M_MCP_B`), `$PORT`, `$TOKEN`, `$TMP/cert.pem` | server persists `entries.content`/`entries.topic = marker` in that slug's store; per-route `sid` captured |

Marker → column mapping (R-09): `content`/`topic` → `entries.content TEXT` /
`entries.topic TEXT` (`db.rs:541-568`). C5/C6 query
`content LIKE '%marker%' OR topic = '%marker%'` (AC-07 canonical form).

## Error Handling

| Condition | Outcome |
|-----------|---------|
| initialize transport failure / no `Mcp-Session-Id` | INFRA (per direction) |
| `notifications/initialized` failure | INFRA |
| SSE parse failure | INFRA |
| `tools/call` JSON-RPC `error` (incl. `-32099`) | INFRA (write did not execute; never RED) |
| `tools/call` success | return; C5 barrier decides PRESENT vs INFRA |

RPC success is **necessary but not sufficient** — it is never the verdict (SR-10).
The verdict is the C5/C6 `entries` content read.

## Key Test Scenarios

1. Each direction drives the full handshake; `initialize` returns a non-empty
   `Mcp-Session-Id` replayed byte-stable on `tools/call` (R-01 sc.1).
2. `Accept: application/json, text/event-stream` is sent; the SSE-framed result is
   parsed from `data:` events, not a bare body (R-01 sc.2).
3. The session id used on `/v1/A/mcp` is the one minted by A's `initialize`, and
   likewise for B — no cross-route reuse (R-17 sc.1; AC-15).
4. A handshake failure (missing session id / `-32099` / SSE-parse error) is INFRA,
   attributed to the correct direction — never RED (R-01 sc.4 / R-17 sc.3).
5. Each MCP positive control is a genuine `entries` content read (C5), never
   RPC-success-only nor a `du` delta (R-02 sc.1; SR-10).
6. `context_store` actually persists an `entries` row carrying the marker (R-02
   sc.2); a deliberately wrong marker returns 0 rows and forces RED (teeth).
