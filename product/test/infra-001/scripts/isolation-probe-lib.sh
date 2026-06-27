#!/usr/bin/env bash
# isolation-probe-lib.sh (infra-003 #853) — the C3 observe + C4 MCP-write probes,
# factored out of multi-tenant-isolation-smoke.sh to keep BOTH files <=500 lines
# (workspace rule), mirroring nan-021's lib-split precedent (cloud-cycle-lib.sh was
# itself factored out of docker-http-posture-smoke.sh).
#
# SOURCED by multi-tenant-isolation-smoke.sh — it does NOT execute on its own; it
# only DEFINES functions. It relies on the parent gate providing log(),
# infra_fail(), and $PORT/$TOKEN/$TMP/$RUN/$SLUG_A/$SID_A/$SID_B (all in scope when
# the gate's run_cells calls these AFTER C2 established the live container).
#
# STUB SEAM: both write functions honor SMOKE_WRITE_CMD (argv run as
# CMD <surface> <slug> <marker>) so the off-Docker gate-logic test neutralizes the
# real curl/MCP path — the same real-vs-stub split the read seam uses.

# =====================================================================
# C3 — Observe write surface, both directions
# =====================================================================
observe_write() {
  local slug="$1" marker="$2"
  if [ -n "${SMOKE_WRITE_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_WRITE_CMD observe "$slug" "$marker"; return
  fi
  local url="https://localhost:${PORT}/v1/${slug}/observe"
  # Wire: HookRequest::RecordEvent { #[serde(flatten)] event: ImplantEvent } with
  # serde tag "type"="RecordEvent"; marker rides ImplantEvent.topic_signal
  # (wire.rs:251-267 — event_type/session_id/timestamp/payload required). Body is
  # node-built (existing JSON-shaping idiom) so the marker can never break quoting.
  local body code
  body="$(MARKER="$marker" SID="infra003-${RUN}" node -e 'const m=process.env.MARKER,s=process.env.SID;process.stdout.write(JSON.stringify({type:"RecordEvent",event_type:"tool_use",session_id:s,timestamp:0,payload:{},topic_signal:m}))')"
  code="$(curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{http_code}' \
            -X POST "$url" \
            -H "Authorization: Bearer ${TOKEN}" \
            -H "Content-Type: application/json" \
            -d "$body")" \
    || infra_fail "observe POST to $url failed (transport)"
  [ "$code" = "204" ] \
    || infra_fail "observe $slug returned HTTP $code (expected 204) — write did not enter the funnel; INFRA, not RED"
  log "observe write $slug accepted (204). marker=$marker"
}

# =====================================================================
# C4 — MCP-write probe, both directions, per-route OWN Mcp-Session-Id
# =====================================================================
parse_sse_jsonrpc() {
  # rmcp returns the JSON-RPC result inside SSE `data:` lines; concatenate them.
  printf '%s' "$1" | node -e 'let b="";process.stdin.on("data",c=>b+=c).on("end",()=>{const d=b.split(/\r?\n/).filter(l=>l.indexOf("data:")===0).map(l=>l.slice(5).trim()).join("");process.stdout.write(d)})' 2>/dev/null || true
}

mcp_handshake() {
  local slug="$1"
  local url="https://localhost:${PORT}/v1/${slug}/mcp"
  local hdr="${TMP}/mcp.hdr.$$"
  local init_frame
  init_frame='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"infra003-smoke","version":"0"}}}'
  # Accept MUST advertise text/event-stream (rmcp forces SSE — JSON-only is refused, #5296/#5129).
  curl -sS --cacert "$TMP/cert.pem" -D "$hdr" -o /dev/null \
    -X POST "$url" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Accept: application/json, text/event-stream" \
    -H "Content-Type: application/json" \
    -d "$init_frame" \
    || infra_fail "MCP initialize $slug transport failure (INFRA)"
  local sid
  sid="$(grep -i '^Mcp-Session-Id:' "$hdr" | head -1 | sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r\n')"
  rm -f "$hdr"
  [ -n "$sid" ] \
    || infra_fail "MCP initialize $slug: no Mcp-Session-Id minted — handshake failed (INFRA, not RED)"
  curl -sS --cacert "$TMP/cert.pem" -o /dev/null \
    -X POST "$url" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Accept: application/json, text/event-stream" \
    -H "Content-Type: application/json" \
    -H "Mcp-Session-Id: ${sid}" \
    -d '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    || infra_fail "MCP initialized $slug failed (INFRA)"
  MCP_SID="$sid"
}

mcp_write() {
  local slug="$1" marker="$2"
  if [ -n "${SMOKE_WRITE_CMD:-}" ]; then
    # shellcheck disable=SC2086
    $SMOKE_WRITE_CMD mcp "$slug" "$marker"; return
  fi
  mcp_handshake "$slug"
  # Bind the minted session to this route's OWN variable; never crossed (R-17).
  local sid
  if [ "$slug" = "$SLUG_A" ]; then SID_A="$MCP_SID"; sid="$SID_A"; else SID_B="$MCP_SID"; sid="$SID_B"; fi
  local url="https://localhost:${PORT}/v1/${slug}/mcp"
  local frame sse data
  frame="$(MARKER="$marker" node -e 'const m=process.env.MARKER;process.stdout.write(JSON.stringify({jsonrpc:"2.0",id:2,method:"tools/call",params:{name:"context_store",arguments:{content:m,topic:m,category:"pattern"}}}))')"
  sse="$(curl -sS --cacert "$TMP/cert.pem" \
           -X POST "$url" \
           -H "Authorization: Bearer ${TOKEN}" \
           -H "Accept: application/json, text/event-stream" \
           -H "Content-Type: application/json" \
           -H "Mcp-Session-Id: ${sid}" \
           -d "$frame")" \
    || infra_fail "MCP tools/call $slug transport failure (INFRA)"
  data="$(parse_sse_jsonrpc "$sse")"
  if [ -z "$data" ] || printf '%s' "$data" | grep -q '"error"'; then
    infra_fail "MCP context_store $slug JSON-RPC error / no result — write did not execute (INFRA, not RED). marker=$marker"
  fi
  log "MCP write $slug succeeded (own session ${sid}). marker=$marker"
}
