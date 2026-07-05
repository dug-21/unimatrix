# Scope Risk Assessment: vnc-043

Feature: context_graph subgraph — Class-1 doc fix + live depth-1 read (GH #903).
Two co-equal deliverables: (1) correct three mis-documented discoverable surfaces; (2) route `max_depth==1` to the existing live `subgraph_via_db`. No wire/struct change.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Mirror-const duplication: `CONTEXT_GRAPH_DESCRIPTION` + its mirror copy both carry the subgraph text. Editing one and not the other re-creates the exact drift the zero-reviewer flagged and that is the root cause of #903. Two surfaces (filter-availability + staleness) each edited in two places = 4 edit points that must agree. | High | Med | Architect/spec: name both const locations explicitly; require a same-body invariant (single source-of-truth const, or a test asserting the two copies match) so drift cannot silently reopen. |
| SR-02 | `subgraph_via_db` reuse exposes a path today reached only on `use_fallback==true` (cold-start/cycle, #4562). Routing every `max_depth==1` call through it makes a formerly-rare branch the hot default — any latent bug there (dedup R-02, dangling filter R-05, metadata cap) now fires on normal board reads. | High | Med | Spec: treat depth-1 live as a newly load-bearing path; require regression coverage of dedup/dangling/hydration on it, not just the happy DoD one-shot. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Ordering non-determinism: live-DB path may order nodes/edges differently from the cached BFS. SCOPE resolves this (AC-14) but leaves the exact key to the architect. If order is not pinned deterministically, the DoD one-shot flakes and depth-1/depth>1 outputs diverge confusingly. | Med | Med | Architect: pin documented order (asc `id` nodes, canonical `(source,target,relation)` edges) and apply it to BOTH depth-1 and depth>1 so callers see one contract, not two. |
| SR-04 | Open Q4 unresolved: an external snapshot/schema test may pin the exact `CONTEXT_GRAPH_DESCRIPTION` string or `GraphParams` JSON schema. If one exists and is not updated in-scope, the doc fix red-bars CI; if it is stale-pinned to old text it masks the drift (cf. #4085 — snapshot must track its source). | Med | Med | Spec writer: make "locate + update or confirm-absent any description/schema snapshot" an explicit in-scope task, not a footnote. Resolve before description edits land. |
| SR-05 | Open Q5 unresolved: `subgraph_via_db` honors `max_nodes` and sets `truncated`. AC-15 asserts `truncated==false` at realistic fan-in, but a very-high-fan-in goal can still cap seed+one-hop, silently returning a partial board. Threshold ("realistic") is undefined. | Med | Med | Architect: decide the contract — board caller raises `max_nodes` vs. accepts+surfaces `truncated`. Define "realistic fan-in" as a concrete number so the test is not aspirational. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Regression to the already-shipped filter path: `edge_types`/`direction` are honored today on both cache and live paths. The dispatch change must not alter filter/`resolve_supersessions` semantics; a subtle divergence (e.g. absent/`[]` handling, Supersedes exclusion) between the two paths would ship as a silent behavior change. cf. #4167 (inclusive SQL filter silently undercounts). | High | Low | Spec: require parity assertions — same seed+filter yields the same node/edge SET at depth-1 live vs the prior cache path (order aside). |
| SR-07 | Regression to depth>1 cache path: SCOPE mandates depth>1 unchanged incl. the `use_fallback`→live cold-start branch (#4562). Inserting a `max_depth==1` early dispatch ahead of the `use_fallback` branch risks accidentally capturing depth>1 or breaking the cold-start fallback. | High | Low | Spec: pin the dispatch as `max_depth==1` exact-match before the fallback branch; add a depth>1 cold-start regression test (empty TypedRelationGraph) asserting the fallback still fires. |
| SR-08 | Behavioral-split test debt: the depth-1-live / depth>1-cache asymmetry (ADR-005 vnc-018 #4479, ADR-004 vnc-019 #4493) is disclosed only in description text. If the freshness split is not tested both ways (write-then-read visible at d1; within-tick write NOT visible at d>1), the contract silently rots as future edits touch the tick path. | Med | Med | Tester (downstream): both-direction freshness test is mandatory, per the ADR-005 precedent. Flagged here so it is budgeted in the test plan. |

## Assumptions

- **A1** (SCOPE §Background Research, §Open Q1): `edge_types`/`direction` filtering already ships and is correct on subgraph today. If false, the "doc-only" half becomes a code change and scope expands. Grounded by git blame (#597) — low risk, but the doc fix asserts a behavior; if the code ever regressed, the doc would lie.
- **A2** (SCOPE §Proposed Approach, line 53): `subgraph_via_db` already satisfies every non-freshness contract (filter, dedup, dangling, hydration, metadata, max_nodes). The whole reuse rationale rests on this; SR-02 is the failure mode if any sub-contract is incomplete.
- **A3** (SCOPE §Constraints, line 82): depth-1 live routing needs no `TypedGraphState` lock. If the live path still touches the lock, the "no hot-path touch" claim (AC-10) weakens.

## Design Recommendations

1. **Single source of truth for the description text** (SR-01) — collapse the mirror-const or add a same-body test; do not rely on humans keeping 4 edit points in sync.
2. **Elevate depth-1 live from fallback to load-bearing** (SR-02, SR-06, SR-07) — the spec's acceptance set must assert path PARITY (same set, filter, supersession) and depth>1 non-regression, not only the DoD happy path.
3. **Close both open questions in design, not delivery** (SR-04, SR-05) — snapshot-pin discovery and the `max_nodes`/"realistic fan-in" number are contract decisions; leaving them to the coder invites a CI red-bar or a silently-truncated board.
4. **Pin one ordering contract across both depths** (SR-03) so callers never see two orderings.
