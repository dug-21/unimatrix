## ADR-003: Bidirectional MCP-write probe over streamable HTTP, per-route session isolation, load-bearing

### Context

SCOPE Q1 is resolved (option a): infra-003 covers **both** the observe surface and
the HTTP MCP-write surface in the same test, and **bidirectionally** — through
**both** `/v1/A/mcp` and `/v1/B/mcp`. The MCP surface is the **load-bearing half**
(SR-10). Observe and MCP-write share the same isolation seam (the same
`parse_project_key`, the same `Arc<dyn StoreResolver>` instance, invoked per
request), but after the shared key/lookup the dispatch diverges: MCP-write goes
through the per-slug `McpAdapter` that holds its **own store captured at boot**
(`ProjectEntry{store, adapter}`, `project_resolver.rs`). The `entry.store ==
adapter-store` equality is a construction invariant guarded only by
`debug_assert!(adapter.wraps_store(&store))` (`seam.rs:345`) — **compiled out of
the release container**. So the shipped artifact has **zero behavioral coverage**
of MCP-write isolation, in either direction.

A single-direction MCP probe (write only through A) cannot catch B's `McpAdapter`
mis-resolving into A's store: B's store reads empty and the negative control
false-passes. The B-direction MCP write earns the same behavioral proof.

Construction nuance: unlike `/observe` (a single cert-pinned `POST`), the MCP
endpoint is served by rmcp's `StreamableHttpService` (`http/router.rs:299-375`),
which requires an MCP **session**. A bare `tools/call` without a session is
rejected, so a single POST cannot drive `context_store`. With **two** routes
exercised, the session handling itself becomes a correctness hazard: a session id
obtained from `/v1/A/mcp` reused against `/v1/B/mcp` would mis-attribute the very
isolation under test.

### Decision

Drive a real MCP `context_store` write through the production MCP route **in both
directions**, each with its **own session**, and prove each via a **content read
of `entries`** (ADR-002), never via the RPC success code.

- **Routes & markers:** `POST /v1/A/mcp` (`infra003-mcp-a-<run>`) and
  `POST /v1/B/mcp` (`infra003-mcp-b-<run>`), over the **same** cert-pinned bearer
  path as observe (no new transport, no new spawn path; one bearer token, slug in
  the path) — the genuine `parse_project_key → resolve_store → adapter_for`
  dispatch into each slug's `McpAdapter`.
- **Per-route session isolation (load-bearing):** each probe runs its **own**
  handshake and captures its **own** `Mcp-Session-Id`:
  1. `initialize` against the route → capture **that route's** `Mcp-Session-Id`
     response header.
  2. `notifications/initialized` with that session header.
  3. `tools/call` `context_store` with that session header and the route's marker.
  **A's session id is NEVER reused against B's route, and vice-versa** — a crossed
  session mis-attributes the isolation being measured. `context_correct` is an
  acceptable alternative verb; the spec pins it (default `context_store`).
- **INFRA-vs-RED discrimination holds for BOTH sessions.** Per direction: a failed
  handshake / missing session id / RPC transport error is **INFRA** (a non-verdict
  for that direction), distinct from a **RED** isolation failure; and the
  read-as-barrier positive control (ADR-002) is per direction — own-store
  marker-absent timeout = INFRA, wrong-store marker present = RED.
- **Verdict (load-bearing), the full 2×2:** the C5/C6 two-store content read of
  `entries` — for each direction the slug's own marker reaching PRESENT in its own
  store **gates** the cross-contamination cell (the other store must not contain
  it). A `du` delta or a success-RPC-only check is **insufficient** and explicitly
  rejected: it would leave the shipped artifact with the same zero coverage the
  `debug_assert!` gap creates (SR-10).
- **Independence:** the four mutually non-substring markers (ADR-002/SR-07) keep
  every MCP and observe cell separately attributable.
- The exact JSON-RPC frame bytes are a tester implementation detail; this ADR fixes
  the approach (real per-route handshake + own session + `context_store` +
  bidirectional read-as-barrier verdict) and the integration surface.

### Consequences

- **Easier:** the shipped release artifact gains genuine behavioral coverage of
  MCP-write isolation **in both directions** for the first time — closing the
  `debug_assert`-compiled-out gap that motivated the whole feature.
- **Easier:** reuses the existing cert/token/`vol`/sqlite3 primitives; no new
  transport or client process; the second direction is one extra handshake.
- **Harder:** the streamable-HTTP handshake (initialize → session header →
  initialized → tools/call) is more construction than observe's single POST, now
  run twice with **two distinct sessions** that must not be crossed. Per-session
  capture/ordering and INFRA-vs-RED discrimination on both sessions are the main
  fragility.
- **Harder:** couples the probe to the rmcp streamable session protocol; an rmcp
  protocol change could require updating the handshake. Bounded to the C4 probes.
- N3 (#5161) remains `partial` regardless — this is a point-in-time proof; the N5
  (#788) regression gate is unwired (SR-05).

Related: ADR-001 (shell host), ADR-002 (the non-substring four-marker read-as-barrier
verdict this uses), ADR-004 (both routes must exist before these writes; liveness
is a precondition only).
