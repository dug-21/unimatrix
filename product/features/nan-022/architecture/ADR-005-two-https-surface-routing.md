## ADR-005 nan-022: Two-HTTPS-Surface Routing Keyed by the Dimension Registry, with Never-Empty / #5298-Conformant Capture

### Context
SR-08: the HTTPS leg has TWO distinct wire surfaces, and the six dimensions split across them.
Routing a dimension to the WRONG surface silently records NOTHING — a vacuous PASS, not a loud
fail (the #5298 legacy/rework-frame gotcha: the rework `post_tool_use_rework_candidate` variant and
the legacy `{"type":"PostToolUse"}` tag both record nothing). The two surfaces:
- **MCP bridge** (`mcp-bridge.js` JSON-RPC `tools/call` over pinned HTTPS, D-2 bridge-in-path):
  anything that is a `context_*` MCP tool — retrieval, briefing, `context_cycle_review`, edges.
- **Hook `/observe` route** (pinned HTTPS POST, per-slug funnel): observe/attribution, PreCompact,
  cycle_start/cycle_stop record events.
The UDS leg mirrors this (`UnimatrixUdsClient` MCP socket vs `UnimatrixHookClient` hook IPC). The
nan-021 drivers already use BOTH; each NEW dimension must be routed to the correct surface on BOTH
legs, or it silently captures nothing.

### Decision
Make surface routing EXPLICIT data on the registry and make a misroute / missing capture ERROR
(never empty-pass).

(1) **`Dimension.wire_surface`** is `WIRE_MCP_BRIDGE` or `WIRE_HOOK_OBSERVE` (a dimension touching
both — analytics, isolation — declares its primary capture surface and the leg driver fans out
explicitly). The leg drivers (`drive_uds_leg` extension on UDS; `bridge-cycle-driver.js` +
`cloud_cycle_gates` on HTTPS) dispatch each dimension's capture by THIS field — no implicit
routing. Mapping: retrieval/proactive → MCP bridge; behavioral/precompact → /observe;
analytics → review read (MCP) over cycle_events written via /observe; isolation → write
(/observe) + read-back (MCP).

(2) **#5298 conformance for every observe-driven dimension.** Behavioral, precompact, analytics
(cycle frames), and isolation (write) emit the BYTE-IDENTICAL 11-frame `RecordEvent` sequence on
BOTH legs (`SessionRegister`→`cycle_start`→`PreToolUse(TaskCreate phase-set)`→per-observe
`Pre`+`Post`→`cycle_stop`→`SessionClose`), NEVER the rework/legacy variants (#5298). The shared
`PARITY_PHASE` + the single manifest keep the framing symmetric (nan-021 ADR-001).

(3) **Never-empty / missing-capture ERRORS.** Each dimension's capture is asserted PRESENT and
NON-EMPTY before its comparator runs; a missing capture is an `InfraError` (ADR-002 INFRA-ERROR
class), never a vacuous pass — the nan-021 R-03 discipline carried to all six (SR-08). The
generalized `load_https_bundle` rejects a bundle missing any required `capture_key`.

(4) **Bridge-in-path preserved (D-2):** retrieval/briefing are added as additional `tools/call`
envelopes inside the EXISTING `bridge-cycle-driver.js` (which already speaks `context_cycle`
`tools/call` and proves SSE + Mcp-Session-Id replay); NO direct `mcp_url` POST, NO net-new
transport/cert/spawn code (a fork smell to FLAG, nan-021 ADR-002 #5294 / AC-11).

### Consequences
Easier: routing is auditable data (the registry), not buried control flow, so a misroute is caught
by the ADR-003 drift guard / never-empty assertion rather than surfacing as a silent vacuous pass;
the #5298 contract is one shared sequence both legs replay; the bridge stays in path so AC-02's
"the bridge ACTUALLY carried it" assertion extends to the new retrieval/briefing calls for free.
Harder: dimensions touching both surfaces (analytics, isolation) need explicit fan-out in the leg
drivers; the #5298 11-frame sequence is exacting and a future edit that swaps in a rework/legacy
frame re-opens the silent-nothing trap (guarded by the hook-Error-frame assertion `_assert_hook_ok`
+ never-empty); adding retrieval/briefing to `bridge-cycle-driver.js` widens the JS surface that
must stay free of re-implemented transport code (FLAG on review).

Related: SR-08; AC-02, AC-03. Conforms to the #5298 RecordEvent contract. Keeps nan-021 ADR-002
(#5294) bridge-in-path. Depends on nan-022 ADR-001 (registry `wire_surface`). Pairs with ADR-002
(missing capture → INFRA-ERROR).
