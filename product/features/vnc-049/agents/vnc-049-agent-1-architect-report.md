# Agent Report — vnc-049-agent-1-architect

Task: Architecture for vnc-049 (OpenCode as fourth observation harness, C18).

## Deliverables
- ARCHITECTURE.md: `product/features/vnc-049/architecture/ARCHITECTURE.md`
- ADRs (files + Unimatrix):
  - ADR-001 source-domain persisted at ingest (#5738)
  - ADR-002 model_id carrier (#5739)
  - ADR-003 E2 sizing: AC-06 IN this cycle (#5740)
  - ADR-004 plugin shim + provider arm + parity corpus (#5741, Prerequisite→#5737)
  - ADR-005 modularity/carve (#5742)
  - ADR-006 installer OpenCode branch (#5743)
  - ADR-007 subagent alignment (#5744)
  - ADR-008 AC-07 forward-compat (#5745)
  - ADR-009 degraded-legs posture (#5746)

## Key decisions
- **Load-bearing finding**: `observations` table has NO source_domain/provider/model column; source_domain is read-derived from event_type. OpenCode shares canonical event names with claude-code → event-derived resolution reads back `claude-code`. AC-03/AC-06 therefore require **persisting attribution at ingest** (schema migration), not just a provider flag + domain pack. This corrects the SCOPE framing.
- **E2 sizing (AC-06): IN this cycle.** Rides ADR-001's forced migration + plumbing at marginal cost; ledger guardrail gives zero credit for deferral; deferring doubles migrations and re-opens the same over-cap files + split-brain surface.
- **Carve (Q2): background.rs NOT carved — it is not touched by C18** (its observations INSERT is test-only; production write path is listener.rs). Re-measured code-lines: hook.rs 804, listener.rs 2504, observation.rs 975, db.rs ~1250, migration.rs ~1500 (all over cap and edited); wire.rs 237, domain/mod.rs 106, observations.rs 185 (under). Decision: new-module-with-thin-wiring + scheduled decomposition issues for the over-cap files modified; NO ad-hoc mid-delivery split.
- Model carrier: new `model_id: Option<String>` on HookInput + ImplantEvent + new observations column, plumbed plugin→wire→DB, queryable-distinct (asserted end-path).
- AC-07: seam kept viable by not folding validated session.agent into MCP tool args and leaving mcp.unimatrix additive; no enforcement, no N=1 ceremonial test.

## Open questions (routed)
1. session.idle→Stop semantics — delivery-time PoC (SR-04).
2. Existing-provider source_domain impact — tester decides opencode-only vs generalized write-stamp; T-SEC-12/13 may need updates.
3. Subagent MCP connection sharing — gates AC-07 channel (i) vs proxy (ii); E1 cycle, not now.
4. PreCompact experimental flag default (SR-03).
5. Confirm next migration version (latest observed v27→v28).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search/context_get — applied pattern #5737 (three coupled touchpoints), col-022 split-brain via #4751 (parity corpus oracle) and lesson #5670 (4-layer hook-frame landing), ADR #4306 (vnc-013 provider field), #4357 (external_identity seam), PL-10 #5715 (modularity), PL-4 #5705.
- Stored: entries #5738–#5746 "ADR-001..009 vnc-049" via /uni-store-adr (context_store, category=decision, topic=vnc-049). ADR-004 (#5741) asserted Prerequisite→#5737 (traversal-necessary: the multi-touchpoint contract; missing it reintroduces the single-flag silent-degradation defect). No supersession — prior ADRs built upon, not contradicted.
