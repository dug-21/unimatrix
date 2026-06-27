# Agent Report: infra-003-agent-3-risk (architecture-risk mode)

**Deliverable:** `product/test/infra-003/RISK-TEST-STRATEGY.md`
**Revision:** final design-gate pass — three hazards now resolved-by-design in
ARCH/ADR-002/ADR-003, R-15/R-16 concretized. (Prior revision: bidirectional 2×2.)

## Summary
18 architecture-level risks (R-01…R-18): 4 Critical, 8 High, 4 Medium, 2 Low.
Dominant risk class remains false-GREEN / vacuous-pass. All 12 scope risks traced;
R-05/R-11/R-17/R-18 resolved-by-design; R-15/R-16 carry concrete GitHub linkage.

## What changed in this pass
- **R-05 reclassified (Critical → Med):** the old aggregate `store_size`
  ("A grew AND B grew") barrier was unsound — satisfied by the FIRST of a store's
  two writes, saying nothing about the second's durability (tokio::spawn
  fire-and-forget + synchronous=NORMAL → content read races an unsynced write →
  positive control FALSE-RED). Resolution recorded: marker-keyed **read-as-barrier**
  (strictly sequential per-store writes; bounded retry-until-present; own-store
  timeout = INFRA never RED; a genuine mis-route still RED at the cross-store cell);
  `store_size` demoted to liveness-only. Residual = sound INFRA-vs-RED discrimination
  + bounded retry.
- **R-17 added (High, sibling of R-01) — crossed/reused `Mcp-Session-Id`:** with the
  handshake run ×2, A's session replayed against B's route mis-attributes the
  isolation under test → false verdict (distinct from R-01's "handshake doesn't
  work"). Resolved-by-design: each probe captures/uses its own session;
  INFRA-vs-RED holds for both (session failure = INFRA; wrong-store marker = RED).
- **R-18 added (Med) — marker substring collision:** MCP read is
  `content LIKE '%marker%'`; "distinct" is insufficient — a substring marker
  false-matches a cross-cell → GREEN on a real leak. Resolved-by-design: four
  mutually NON-SUBSTRING literals `infra003-{obs,mcp}-{a,b}-<run>` (shared run-nonce
  + disjoint per-cell tag). (Distinct from R-12, which is SQL/LIKE metacharacters
  inside a marker.)
- **R-15 concretized:** mitigation is now in-PR lockstep — the smoke-script
  invariant update lands in the SAME delivery PR as the new script, cross-linked on
  #815 (issue comment by the leader). Real linkage, not a doc row.
- **R-16 concretized:** a durable linkage comment was posted on #788 requiring
  N5/#788 to adopt infra-003's gate into the recurring lane (advances N3
  point-in-time → maintained). Real #788 linkage, not just a feature-doc row.

## Final risk register totals
18 risks (R-01…R-18): **4 Critical** (R-01 MCP handshake, R-02 load-bearing MCP
vacuity, R-03 positive-gates-negative ×4, R-04 WAL false-empty), **8 High**
(R-06 read-deps, R-07 liveness-as-verdict/missing-B, R-08 stale/non-unique markers,
R-09 column round-trip, R-10 tri-state collapse, R-15 #815 invariant, R-16 #788
standing-lane, R-17 crossed session), **4 Medium** (R-05 barrier soundness residual,
R-12 SQL metacharacters, R-13 cumulative coupling, R-18 substring collision),
**2 Low** (R-11 slug-B resolved, R-14 overclaim/parity).

## Scope-risk traceability status
Complete. Updated edges: SR-03→R-05/R-10 (read-as-barrier), SR-07→R-08/R-18
(non-substring markers), SR-10→R-01/R-02/R-03/R-17 (own-session per direction),
SR-05→R-14/R-16 (#788 linkage). All SR-01…SR-12 traced.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for vacuous-pass/false-GREEN and test-harness risk
  patterns. Applied #3624, #5180, #5177/#5173, #5296+#5129 (rmcp forces SSE),
  #4708 (Mcp-Session-Id UUID per session — directly informs R-17), #5193 (WAL-robust
  store-grew = du over DIR — the demoted-to-liveness signal in R-05).
- Stored: nothing novel — load-bearing patterns already exist. Candidates still
  infra-003-specific (read-as-barrier for N-sequential-writes-one-container;
  non-substring marker discipline under LIKE; point-in-time-gate-orphan handoff to a
  standing lane). Below the 2+-feature bar; revisit at retro if they recur.
