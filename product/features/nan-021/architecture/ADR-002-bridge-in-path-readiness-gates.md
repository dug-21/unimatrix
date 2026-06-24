## ADR-002 nan-021: Drive the Cycle THROUGH mcp-bridge.js with Event-Driven Readiness Gates and Capture-First Stderr

### Context
D-2 LOCKS the bridge in path: drive the full `context_cycle(start) → tool calls (incl. a Bash command) →
stop` THROUGH `mcp-bridge.js` (the vnc-039 stdio→HTTPS bridge) — spawn it and speak stdio JSON-RPC to it
as the local MCP server. Do NOT POST `mcp_url` directly; bridge coverage is in-scope and must not be
optimized away. AC-02 requires the MCP traffic flow through the bridge over pinned HTTPS, and the hook
observations flow over pinned HTTPS `/observe` — not UDS, not stdio, not a direct `mcp_url` POST.

SR-01 (High/High) is the dominant brittleness risk: the live path chains image boot → slug register →
restart → busybox cert/bearer read → bundle emit → `init --bundle` → `mcp-bridge.js` spawn → pinned-HTTPS
SSE session. Each link is a flake/ordering hazard (cert-on-volume timing, credstore path
`~/.unimatrix/<projectHash>`, SSE `text/event-stream` framing #5129, bearer-after-pin). The smoke today
proves a cert-pinned `curl --cacert`, but the bridge over stdio JSON-RPC + SSE parse is an UN-SMOKED
surface (SR-02). vnc-039 ADR-001 (#5115) shipped the bridge with the residual hand-rolled SSE+session
risk explicitly flagged; this fixture is where that surface is exercised live end-to-end.

Lessons #5266/#5267: a release/smoke gate that discards its child's stderr is undiagnosable BY
CONSTRUCTION — and a release-only path reached for the first time has no green baseline, so blind guessing
burns a full tag cycle per wrong guess. The Constraint forbids fixed sleeps as the synchronization
mechanism.

R-05 (Critical, added at architecture-risk review). rmcp `keep_alive` EVICTS idle MCP sessions
(#5280/#830): an idle cloud session is dropped, and the bridge's next POST replaying the captured
`Mcp-Session-Id` 404s (`SESSION_NOT_FOUND` -32099). The readiness-gate wait between bridge spawn and the
first tool call is EXACTLY such an idle window — so a naive "spawn, wait, then drive" ordering re-creates
the precise failure #830 fixed. The cycle would either abort mid-flight (a fixture-timing RED, not a parity
finding) or silently lose observes (vacuous parity). #830 shipped a single-flight self-heal (re-init once
on `SESSION_NOT_FOUND`); the fixture must rely on that SHIPPED behavior, not re-implement it (AC-06).

### Decision
**Bridge-in-path with explicit event-driven readiness gates, a minimized idle window, and capture-first
child stderr — no sleeps.**

1. **The bridge carries the traffic, asserted positively (D-2, SR-02).** Spawn `node mcp-bridge.js
   <projectHash>`; speak stdio JSON-RPC to it (`initialize → tools/list → tools/call`). The cycle
   (`context_cycle` start/stop + Bash + other tool calls) and hook observes (→ pinned `POST
   /v1/{slug}/observe`) ALL traverse the live transport. AC-02 asserts the bridge ACTUALLY carried the
   MCP traffic — `Mcp-Session-Id` replay observed, SSE (`text/event-stream`, #5129) parsed, derived
   attribution present downstream — not merely a 200. The bridge MUST NOT be optimized to a direct
   `mcp_url` POST.

2. **Explicit readiness gates between every link (SR-01), event-driven not timed:**
   - **cert present:** the leaf cert (`vol cat .../tls/cert.pem`) is non-empty before any pinned client
     runs (it determines the pinned fp; vnc-034 #4948).
   - **listener bound:** after `docker restart`, poll for the HTTPS listener accepting (log line / connect
     probe) before register/curl/bridge — never a fixed sleep.
   - **HTTP transport active:** poll the daemon log for the existing `"HTTP transport active"` marker
     (Gate-1 reuse) before driving.
   - **credstore present:** `~/.unimatrix/<projectHash>/remote.json` (mode 0600) exists after `init
     --bundle` before the bridge spawn.
   - **session-id captured:** the bridge's `initialize` reply (Mcp-Session-Id) is observed before any
     `tools/call` is issued — the synchronization point, not a sleep.

3. **Capture-first child stderr (#5266/#5267).** Every child (`mcp-bridge.js`, `init`, the container)
   redirects stderr to a file under the hermetic `$SANDBOX`, dumped tail-bounded ON FAILURE ONLY, never
   `2>/dev/null` for a token-free child. The `emit_bundle` child stays suppressed (its blob carries the
   bearer). This makes the first real release-lane failure diagnosable in one round instead of N guesses.

4. **Minimize the idle window + rely on the shipped self-heal (R-05).** The `session-id captured` gate is
   the LAST wait — the first `tools/call` follows the `initialize` reply IMMEDIATELY, with NO interposed
   fixed wait between session capture and first drive (the idle window that triggers `keep_alive`
   eviction). All other readiness gates (cert, listener, transport-active, credstore) complete BEFORE the
   bridge is spawned, so spawning the bridge is the last step before driving — the session is never left
   idle waiting on a downstream gate. Should a mid-cycle eviction still occur, the fixture relies on the
   SHIPPED single-flight self-heal (#830: re-init once on `SESSION_NOT_FOUND` -32099) — it does NOT
   re-implement re-init (AC-06). The fixture must NOT depend on eviction never happening; a 404 that
   exhausts the heal surfaces as a HARD cycle failure with captured bridge stderr, never a silent dropped
   observe that shows up only as a short `MetricVector`.

   **Intentional coupling to #830 (NFR-7).** This fixture's reliability now DEPENDS ON the shipped
   single-flight `keep_alive` self-heal (#830) holding. The coupling is DESIRABLE, not incidental: if the
   self-heal regresses, the cloud cycle flakes HERE — so a flake in this gate correctly SIGNALS a #830
   regression. The fixture is therefore also a standing regression guard for the #830 self-heal; the
   dependency is recorded as intended rather than a hidden fragility.

5. **Trust contract preserved as-shipped (AC-06).** The bearer flushes ONLY after `verifyPeerFingerprint`
   matches (vnc-039 ADR-001 #5115); the fixture exercises this, never bypasses or re-implements it. A pin
   mismatch must surface loud (stderr + non-zero), never a silent degrade.

### Consequences
- **Easier:** the un-smoked SSE/session surface (SR-02) is now exercised live, closing the residual
  vnc-039 delivery risk with a real handshake. Event-driven gates make the chain deterministic instead of
  sleep-tuned (SR-01) — robust across slow CI runners. Minimizing the idle window plus leaning on the
  shipped self-heal (R-05) means the keep_alive eviction class (#830) cannot turn into a fixture-timing
  flake. Capture-first converts the inevitable first release-lane failure into a single diagnosable round
  (#5266).
- **Harder:** more wiring than a `curl` POST — each readiness gate is a poll loop with a bounded deadline
  and a clear failure diagnostic; the bridge's stdio JSON-RPC framing must be driven correctly (it is the
  real rmcp wire, not a JSON shortcut). Asserting "the bridge carried it" (session-id replay / SSE
  parsed) requires observing bridge internals or downstream effects, not just the tool result — a
  stronger but more involved assertion than a status code. The ordering constraint (spawn-bridge-last,
  drive-immediately) couples the readiness-gate sequence to the eviction hazard — a future edit that
  reintroduces a wait between capture and first call silently re-opens R-05.

Related: D-2, SR-01, SR-02, R-05; AC-02. Reuses vnc-039 ADR-001 (#5115, the bridge + pinned-flush + stable
session id), #5280/#830 (rmcp keep_alive idle eviction + shipped single-flight self-heal), #5129 (rmcp
forces SSE), entry #5098 (register-pre-serve / HTTPS-only / leaf-fp pin trap), lessons #5266/#5267
(capture-first, never-green-first-run). Pairs with ADR-006 (the durability barrier after the cycle) and
ADR-005 (the gate-level false-green discriminator) and ADR-001 (the workload this drives).
