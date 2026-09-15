## ADR-007: Subagent alignment via validated `session.agent` on the observe channel, not tool args

### Context
AC-04 (show-stopper): subagent telemetry must be captured and aligned to the owning feature/cycle.
OpenCode has no dedicated pre-spawn hook (upstream FR #20387 open); a subagent spawn is reachable only
as a bus-derived child `session.created` with `parentID != null` (observe-only). OpenCode validates the
subagent type via `agent.get()` (rejects unknown types) and stamps the validated `agent` name onto the
child session — a harness-trusted identity with the LLM out of the loop (ass-106 §E1). SR-09 warns
that if parent→feature/cycle correlation fails, telemetry lands orphaned, and that the validated
`session.agent` must **not** be folded into spoofable tool args (that would both mis-attribute and
burn the AC-07 seam, SR-12).

### Decision
1. C1 maps the child `session.created` (with `parentID` + validated `session.agent`) to a
   `SubagentStart` `HookInput`, carrying the validated agent as `extra.agent_type` — the **observe
   channel** the pipeline already understands (existing SubagentStart handling; no server rewrite).
2. **Feature/cycle alignment**: the plugin resolves `parentID → parent session → parent's
   feature/cycle` (via the SDK `client` / the parent's cycle stamp) and stamps it so the child's
   records align to the owning cycle, not orphaned. Validated + tested (AC-04).
3. The validated `session.agent` is carried **only** on the observe channel. It is **never** written
   into MCP `context_*` tool args (the server treats those as self-declared/spoofable, SD-9 / #1301).
   This keeps the trusted identity off the spoofable channel and preserves it as a future
   `external_identity` source (ADR-008).
4. **Parity gap** (recorded, not a defect): retrieval **injection** into the subagent is unreachable
   (observe-only bus event). Observation of the spawn is in scope; injection is not (SCOPE Non-Goal).

### Consequences
Easier: reuses the existing SubagentStart observe path; the validated agent is richer/more trustworthy
than claude-code's stdin `agent_type`; alignment is deterministic via stable `sessionID`/`parentID`.
Harder: alignment correctness depends on the plugin resolving the parent's cycle reliably (tested
explicitly, SR-09); Stop/SessionStart for subagents inherit the bus-derived degradation (ADR-009).
Cross-references ADR-008 (the seam this protects), ADR-004 (plugin).
