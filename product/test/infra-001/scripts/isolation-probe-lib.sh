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
#
# It ALSO houses the construction-safe nonce + PII-shape canary helpers (#859) so
# multi-tenant-isolation-smoke.sh stays <=500 lines; they are defined here (sourced
# before derive_markers/warmup_barrier RUN) and consumed there.

# =====================================================================
# Construction-safe nonce (B1/B3, #859) — letter-dominant, shape-safe BY DESIGN
# =====================================================================
# _b36(n): base36-encode a non-negative integer to lowercase [0-9a-z] (R-12-safe).
# bash has no printf base36; divide-by-36 into the digit table. base36 is 26:10
# letter-dense, so a <=6-char component cannot itself form a 10-digit phone run.
_b36() {
  local n="${1:-0}" out="" d
  local tbl="0123456789abcdefghijklmnopqrstuvwxyz"
  case "$n" in ''|*[!0-9]*) n=0;; esac
  [ "$n" -eq 0 ] && { printf '0'; return; }
  while [ "$n" -gt 0 ]; do
    d=$(( n % 36 )); out="${tbl:d:1}${out}"; n=$(( n / 36 ))
  done
  printf '%s' "$out"
}

# Fixed all-digit feature-id token (#859 two-filter root cause). EVERY derived marker
# (the 4 cell markers + the warmup marker) carries this SHORT, FIXED all-digit hyphen-
# segment so the server's looks_like_feature_id (uds/listener.rs:289-303, bugfix-832)
# is TRUE and the observe path PERSISTS topic_signal (instead of dropping it to NULL —
# the regression that broke the C5 observe positive controls + the warmup-via-observe
# barrier). It stays PII-safe: one fixed digit, hyphen-separated from the letter-
# dominant b36 nonce, is structurally incapable of forming the PhoneNumber/SSN digit
# runs the MCP context_store content scanner rejects. The two server filters pull in
# OPPOSITE directions on digit runs; this token is what threads BOTH. Defined here (in
# the sourced lib) so multi-tenant-isolation-smoke.sh stays <=500 lines; consumed by
# the parent gate's derive_markers/warmup_barrier (cross-file use; SC2034 false positive).
# shellcheck disable=SC2034
MARKER_FID_TOKEN="1"

# _default_nonce -> <b36(pid)>x<b36(epoch)>: pid and epoch base36-encoded
# SEPARATELY and joined with the LETTER 'x' (NOT a hyphen, NOT a digit). Each
# component is <=6 chars (pid <36^5, a 10-digit epoch <36^7 but realistically
# <=6), so neither the 10-digit phone grouping nor the \d{3}-\d{2}-\d{4} SSN shape
# can form within a component, and the letter separator blocks any run across the
# boundary — the PII shapes are structurally unreachable, not merely improbable.
# Honors PID_OVERRIDE/EPOCH_OVERRIDE (default $$/date) so the off-Docker logic test
# drives an adversarial epoch/pid battery through the REAL default path (B3); the
# single seam routes BOTH derive_markers and warmup_barrier so they cannot diverge.
_default_nonce() {
  printf '%sx%s' "$(_b36 "${PID_OVERRIDE:-$$}")" "$(_b36 "${EPOCH_OVERRIDE:-$(date +%s)}")"
}

# assert_marker_pii_safe(marker) — REGRESSION CANARY (B2/N4, #859). Runs AFTER the
# R-12 [a-z0-9-] charset guard, so its input is charset-reduced. Of the six
# production ContentScanner patterns (crates/unimatrix-server/src/infra/scanning.rs)
# only TWO are reachable under [a-z0-9-]; the other four need chars outside it:
#   EmailAddress  needs '@' and '.'        (out of charset)
#   ApiKey bearer needs a space            (out of charset)
#   ApiKey AWS    needs uppercase 'AKIA…'  (out of charset)
#   ApiKey GitHub needs '_'                (out of charset)
# Charset-reduced ERE projections (bash [[ =~ ]] has no \d / \s / \b / (?:)):
#   PhoneNumber  scanning.rs:300-304  (?:\+?1[\s.-]?)?\(?[2-9]\d{2}\)?[\s.-]?\d{3}[\s.-]?\d{4}
#     -> \d=>[0-9]; \s,(,),+ unreachable; '-' IS in-charset (a valid separator):
#        1?[2-9][0-9]{2}-?[0-9]{3}-?[0-9]{4}    (the leading [2-9] anchor means the
#        '003' in the infra003 prefix cannot be read as a phone start — N3.)
#   SocialSecurityNumber  scanning.rs:306-309  \b\d{3}-\d{2}-\d{4}\b
#     -> \d=>[0-9]; \b has no ERE equiv:  [0-9]{3}-[0-9]{2}-[0-9]{4}
# With the construction-safe nonce these shapes are unreachable, so a match here can
# ONLY mean a future derivation regression — this is a CANARY, not a faithful
# regex re-implementation (do NOT chase verbatim faithfulness). Fail loud INFRA and
# report the shape CATEGORY only, NEVER the offending digits (N4).
assert_marker_pii_safe() {
  local m="$1"
  local phone_shape='1?[2-9][0-9]{2}-?[0-9]{3}-?[0-9]{4}'
  local ssn_shape='[0-9]{3}-[0-9]{2}-[0-9]{4}'
  if [[ "$m" =~ $phone_shape ]]; then
    infra_fail "a derived marker matched the phone-shape guard (REGRESSION CANARY; scanning.rs:300-304) — derivation regression; digits withheld"
  fi
  if [[ "$m" =~ $ssn_shape ]]; then
    infra_fail "a derived marker matched the SSN-shape guard (REGRESSION CANARY; scanning.rs:306-309) — derivation regression; digits withheld"
  fi
}

# assert_marker_feature_id_shaped(marker) — SYMMETRIC CANARY (#859 two-filter root
# cause). The OBSERVE path persists topic_signal ONLY when the server's
# looks_like_feature_id (uds/listener.rs:289-303, bugfix-832) is TRUE — it requires
# >=1 hyphen-segment that is ENTIRELY ascii digits AND >=1 segment carrying an alpha
# char; a marker failing it is silently dropped to NULL, so the C5 observe read-as-
# barrier never sees it => deterministic INFRA at ANY deadline (the regression the
# #859 PII-safe nonce introduced by removing the all-digit segment). This is the
# MIRROR of assert_marker_pii_safe: the two filters pull in opposite directions on
# digit runs, so a future nonce change that satisfies one can silently break the
# other. Faithful split-on-'-' re-implementation of the predicate (charset is
# [a-z0-9-], so a non-all-digit segment necessarily carries an alpha char). Fail loud
# INFRA and report the shape CATEGORY only, NEVER the marker value (N4 parity).
assert_marker_feature_id_shaped() {
  local m="$1" seg has_digit=0 has_alpha=0
  local IFS=-
  for seg in $m; do
    [ -z "$seg" ] && continue
    case "$seg" in
      *[!0-9]*) case "$seg" in *[a-z]*) has_alpha=1;; esac ;;
      *)        has_digit=1 ;;
    esac
  done
  if [ "$has_digit" -eq 0 ] || [ "$has_alpha" -eq 0 ]; then
    infra_fail "a derived marker lacks the feature-id shape (no all-digit hyphen-segment + alpha segment; mirrors uds/listener.rs:289-303 looks_like_feature_id) — observe topic_signal would be dropped to NULL and the C5 read-as-barrier would never see it; CATEGORY reported, value withheld"
  fi
}

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
