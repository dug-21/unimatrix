# Test Plan — C4: MCP-write probe, both directions (load-bearing)

> Pseudocode: `pseudocode/c4-mcp-probe.md`. Risks: **R-01** (handshake mis-built),
> **R-17** (crossed/reused session), R-02 (vacuous MCP pass), R-09. ACs:
> **AC-06**, **AC-15**.

C4 is the load-bearing half: the `entry.store == adapter-store` invariant is a
`debug_assert!` compiled **out** of the release container (`seam.rs:345`), so the
shipped artifact has **zero** behavioral MCP-isolation coverage without this probe.
C4 drives `context_store` over the streamable-HTTP handshake against **both**
`/v1/A/mcp` and `/v1/B/mcp`, **each with its own `Mcp-Session-Id`**. The test of C4
proves the handshake actually executes (R-01), uses the correct per-route session
(R-17), and that handshake/session failure is **INFRA, not RED**.

## What C4 must do (behavior under test)

- Per route: `initialize` → capture **that route's** `Mcp-Session-Id` (a UUID
  minted at `initialize`, #4708) → `notifications/initialized` → `tools/call`
  `context_store` with the marker in `content` (+ `topic`).
- `Accept: application/json, text/event-stream` required (rmcp forces SSE,
  #5296/#5129); the response is SSE-framed and the JSON-RPC result is extracted
  from the SSE event, not a bare body.
- A's session id is **never** reused against B's route, and vice-versa.
- `context_store` (default) persists an `entries` row; marker lands in
  `entries.content`.

## Verification tier 1 — off-Docker / static + stub

- `test_c4_handshake_sequence_present` (R-01) — inspection: each direction issues
  `initialize` → `initialized` → `tools/call` in order; the `Accept` header
  carries `text/event-stream`.
- `test_c4_session_captured_per_route` (R-17) — the session id replayed on
  `/v1/A/mcp` `tools/call` is the one minted by A's `initialize`; same for B. No
  single captured id is reused across both routes (grep/var-flow: distinct
  `A_SESSION`/`B_SESSION` variables, never crossed).
- `test_c4_jsononly_accept_negative` (R-01 sc.3) — a JSON-only `Accept` would be
  refused by rmcp; assert the gate sends the SSE-capable `Accept` (proves real
  SSE, not a JSON shortcut).
- `test_c4_handshake_failure_is_infra` (R-01 sc.4 / AC-15) — stub a handshake
  failure (missing session id, `-32099 SESSION_NOT_FOUND`, SSE-parse error) →
  **INFRA** for that direction, attributed to the correct route, **never RED**.
- `test_c4_context_store_persists_row` (R-02 sc.2) — confirm `context_store` (not
  `context_correct`, which needs a prior entry) is the verb; if `context_correct`
  is ever chosen, a prior entry must exist or the write silently no-ops.

## Verification tier 2 — live run

- `test_c4_mcp_a_success` / `test_c4_mcp_b_success` (AC-06) — each `tools/call`
  returns JSON-RPC success (no `error`) parsed from the SSE frame.
- `test_c4_session_uuid_nonempty_and_stable` (R-01 sc.1) — `initialize` returns a
  non-empty `Mcp-Session-Id`; that exact header is replayed byte-stable on
  `tools/call` (wire-witness, #5296).
- `test_c4_marker_roundtrips_to_entries` (R-09) — the marker appears verbatim in
  `entries.content` of the **own** store (column mapping vs `db.rs:541-568`); this
  is the C5 MCP positive-control read.

## Coverage requirement

Both MCP writes provably execute through a real streamable-HTTP session (minted,
replayed, SSE-parsed) using each route's **own** `Mcp-Session-Id` before any
verdict; transport/session failure is INFRA (distinct from RED) per direction
(R-01/R-17); the marker genuinely lands in `entries.content` (R-02/R-09).
