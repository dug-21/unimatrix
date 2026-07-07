# Agent Report — vnc-045-agent-1-architect

**Task:** Architecture + ADRs for `context_tag` (domain-agnostic in-place tag-mutate MCP op, parallel fast path to `context_correct`, with per-slug `protected_tags` value-hygiene policy).

## Deliverables
- `product/features/vnc-045/architecture/ARCHITECTURE.md`
- ADR-001 .. ADR-008 (files below; Unimatrix #5599–#5606)

| File | Unimatrix | Title |
|------|-----------|-------|
| ADR-001-direct-entry-tags-write.md | #5599 | Direct entry_tags write via new single-row primitive, not update() |
| ADR-002-no-inmemory-invalidation.md | #5600 | No in-memory invalidation — entry_tags read strictly live (SR-01) |
| ADR-003-value-hygiene-separate-check.md | #5601 | Value-hygiene separated from validate_outcome_tags (SR-07) |
| ADR-004-atomic-single-value-replace.md | #5602 | single_value replace atomic + counts as one cadence event (SR-02) |
| ADR-005-per-slug-threading-all-paths.md | #5603 | Per-slug threading, five sites, all paths honored, behavioral verify (SR-06) |
| ADR-006-merge-configs-replace-not-merge.md | #5604 | merge_configs replaces protected_tags list; no inheritance (SR-09) |
| ADR-007-cadence-guard-state-model.md | #5605 | Cadence guard: in-memory, restart-reset, per-(slug,entry,prefix) (SR-08) |
| ADR-008-authorization-posture-and-seams.md | #5606 | Capability::Write only; agent_id audit-only; min_trust_level inert seam (SR-03/04) |

## Risk-assessment resolution
- **SR-06 (High) — daemon-path divergence:** Explore evidence inverted the assumed risk. `build_project_server` (http_provision.rs:261) sets NO server-state config field (not even `store_config`); the silent-`::default()` trap falls on the **per-slug HTTP path**, not the daemon path. Decision (ADR-005): honor `protected_tags` on ALL THREE construction paths — daemon (main.rs:982) + stdio (main.rs:1701) from GLOBAL config, per-slug HTTP by threading a new param through `build_project_server` and setting it before `ProjectServerInput` returns. None inert. Verification is a **behavioral per-path matrix** (config → handler `self.protected_tags` → rejects `delivery:provn`), not source-assertion counting (classification drift-guard catches absent key only).
- **SR-01 (High) — derived-state blast radius:** Enumerated exhaustively (ARCHITECTURE §3). `entry_tags` is read strictly LIVE everywhere (`load_tags_for_entries`/`apply_tags` per query); analytics caches hold derived ranking signals only, no tags; NO tag-derived edges; only SQLite indices, maintained automatically. Op needs ZERO in-memory refresh (ADR-002). Bounded by A1 (tags stay out of hash + embedding).
- **SR-04 (Med):** Hygiene is fast-path-only; `context_correct` bypasses by design. No downstream logic may assume the tag lane is validated (ADR-008 §7, ARCHITECTURE §7).
- **SR-02/07/08/09:** ADR-004 / ADR-003 / ADR-007 / ADR-006 respectively.
- **A3 confirmed inert-and-safe:** cross-project isolation is structural (1-client:1-project + per-slug DB, vnc-034); no cross-project retrieval in flight; guard flagged if introduced (ARCHITECTURE §8).

## Integration points
vnc-040 per-slug config plumbing (#5209/#5199/#5217/#5211), vnc-034 per-slug DB, ass-093/094 research, nxs-008 entry_tags FK, uni-capability skill + #5505 (worked example ships `delivery:` as example config).

## Open questions (non-blocking)
Cadence threshold N + window (operational tuning); whether delivery prefers an explicit client `replace` action (resolved: server-side realization of `add` under single_value).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_lookup(vnc-040) + context_search — found vnc-040 per-slug config ADRs #5209 (resolve_slug_config seam), #5217 (classification registry + drift-guard), #5199 (post-merge re-validation), #5211 (drift-guard GlobalLocked split pattern); applied all four to ADR-005/006. Prior-decision scan found no conflict/supersession — vnc-045 extends vnc-040 plumbing, does not replace it.
- Stored: entries #5599–#5606 (ADR-001..008) via context_store (category: decision, topic: vnc-045). Edges asserted at the HIGH bar: #5603 Prerequisite→#5209 and →#5217 (must-read-first to thread the seam correctly); #5604 Supports→#5211 (drift-guard pattern validates the merge-behavior classification). Intra-feature ADR spine left for retro (sibling IDs did not exist at authoring). No deprecations required.
