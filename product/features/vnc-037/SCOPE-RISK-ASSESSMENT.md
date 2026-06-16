# Scope Risk Assessment: vnc-037

Next-hop navigation affordance: surface a **ranked, capped (≤3)** set of an entry's depth-1 typed edges on `context_get`, with honest uncapped totals. REFRAMED from edge-dump to ranking-as-core (cap 10→3). New material: D-05 cap=3, D-09 ranking, D-10 symmetric canonicalization, SQL rank-and-limit, AC-12 latency, OQ-03. Read-path only, no migration. Supersedes the prior assessment; still-valid risks (SR-01..SR-03, SR-05, SR-06) retained.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | **Symmetric double-count/double-render (D-10 blocker).** `Contradicts`/`CoAccess`/`Informs` store as 2 reciprocal rows; `query_direct_neighbors(Both)` does `extend` with no dedup. A canonicalization miss renders the edge twice AND double-counts BOTH the displayed set and the split totals. Empirically grounded: #4083 (LIMIT caps rows not edges → cap=N yields 2N for symmetric), #3618 (double-count needs explicit JOIN/CASE guards). | High | High | Architect: canonicalize symmetric types to one `↔` edge in SQL **before** both `ORDER BY...LIMIT 3` and the `COUNT(*)`. Make "symmetric counted once" an invariant tested on display AND totals separately (#4083). |
| SR-09 | **Ranking correctness silently degrades (D-09).** Wrong `ORDER BY` (e.g. ranking by `graph_edges.weight` instead of `target.confidence`, or dropping `authored DESC`) produces a plausible-but-wrong set. With only 3 slots a bad rank is highly visible to the reader yet invisible to a passing build — no error, just worse next-hops. | High | Med | Spec writer: lock the exact key `ORDER BY (source='agent') DESC, t.confidence DESC LIMIT 3` as an AC with a discriminating test (higher-confidence target ranks first; weight does NOT decide). |
| SR-10 | **Carried-forward/`context_edge` edges mis-classified as inferred (D-09 + vnc-035).** If a carry-forward or `context_edge` write path sets `source` to anything but `'agent'`, those edges fall out of the authored bucket and lose slot priority — silently degrading the affordance for exactly the corrected entries. #4425 confirms `source='agent'` is the predicate; #4984 confirms vnc-035 carries only agent-declared edges — but correctness rests on the write path actually stamping `'agent'`. | High | Med | Architect/Spec: assert `source='agent'` on carry-forward + `context_edge` outputs; lock AC-10's "carried-forward classifies as authored" test by name (silent-degrade, no build signal). |
| SR-11 | **Confidence JOIN skews ranking on bad endpoints (D-09).** `JOIN entries t ON t.id = target_id ORDER BY t.confidence` — a dangling target (no row) or deprecated target (status≠0) yields NULL/absent confidence. An inner JOIN silently drops the edge (D-02 says retain dangling); NULL confidence sorts unpredictably. #3618: endpoint JOINs need explicit null/status guards. | Med | Med | Architect: LEFT JOIN with explicit NULL-confidence ordering (e.g. `COALESCE`/NULLS LAST) so dangling targets are retained (D-02) and rank deterministically. |
| SR-01 | Shared serializer (`entry_to_json`/markdown) is used by search/lookup/store/correct; a non-`None`-key-absent regression makes those payloads non-byte-identical (D-07). | High | Med | Architect: `None ⇒ key absent` is an invariant, not a convention. Spec: assert byte-identical payloads for the 4 list-view tools. |
| SR-02 | Adding `source` to the neighbor SELECT/`RawEdgeRow` touches a query shared with `context_graph` neighbors (#4479). A struct/SELECT change can break the existing contract. | High | Med | Architect: extend additively, read-only, no DDL; re-run existing neighbors tests empirically. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-12 | **AC-12 latency budget is unbacked.** Default-on adds a confidence-JOIN `LIMIT 3` select + split `COUNT(*)` to the hottest read tool — which also feeds the co-access loop, so cost compounds. The proposed ≤5ms p50 / ≤15ms p95 has no measured baseline yet. Fan-in precedent (#4463: 50 sequential txns ≈ 50ms) shows hub-node cost is real. | High | Med | Spec: require a measured edge-free baseline before locking numbers; mandate a high-degree-node case in the latency AC. Architect: rank-and-limit + count must use `read_pool()` and indexed JOINs; no full fan-out. |
| SR-13 | **Cap 3 raises the cost of every other defect.** Halving slots (10→3) means canonicalization (SR-08), ranking (SR-09), and classification (SR-10) errors each consume a precious slot — a single wrong edge is 1/3 of the visible affordance. Prior assessment's risks are now higher-stakes under the reframe. | Med | High | Spec: elevate SR-08/09/10 to mandatory discriminating tests, not smoke tests. Treat "which 3" as the acceptance surface. |
| SR-05 | Provenance reduced to `authored` boolean (D-03) on the NLI-dark premise; if a non-statistical inferred source revives it silently mislabels. | Low | Low | Keep `source` string underneath; document the NLI-dark precondition as the revival trigger. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-14 | **SQL rank-and-limit must bound fan-out, not pull 1000 rows.** Goal §6/D-01 require the ranked select + count to never materialize a hub node's full neighbor set. A naive "fetch all then sort/slice in Rust" satisfies the output contract but violates the memory/latency intent invisibly. | High | Med | Spec: AC-10 already names "high-degree node hits SQL LIMIT not memory" — lock it; assert 3 rows + 2 counts returned, full set never read. Architect: `LIMIT`/`COUNT(*)` in SQL, not Rust. |
| SR-06 | Get edge vocabulary must align to `context_graph`'s `EdgeRecord` (D-02, AC-09) while dropping `source_id`/`depth` and adding `target_title`/`authored`/`↔` direction semantics. Divergence = two inconsistent edge shapes. | Med | Med | Architect: document the get shape as an explicit projection of `EdgeRecord` (ADR #5010 already locks this); symmetric `↔` direction is get-only, must not leak into the neighbors contract. |
| SR-07 | Dangling-title (retained per D-02) and corrected-entry transient (sparse Inferred section) can be misread as bugs by readers/test authors. | Low | Med | Spec: encode dangling-title and zero-edge as explicit acceptance cases (AC-02, AC-06). |

## Assumptions

- **A1 (Background, D-04):** `query_direct_neighbors` keeps the `!= 'Supersedes'` filter and is reusable with empty `edge_types`+`Both`. The reframe ADDS that the *plain* call is no longer the reuse target (D-01) — a new ranked/limited/canonicalized SQL path is needed; if the design instead reuses the unbounded call and post-filters in Rust, SR-14 materializes. (SR-02, SR-14)
- **A2 (D-10, Background):** exactly three relation types (`Contradicts`/`CoAccess`/`Informs`) are symmetric two-row; all others are single-row asymmetric. If a future symmetric type is added without updating canonicalization, it double-counts. (SR-08)
- **A3 (D-09, #4425/#4984):** carry-forward and `context_edge` writes stamp `source='agent'`, so carried edges classify as authored. If any write path uses a different source value, ranking silently demotes them. (SR-10)
- **A4 (D-09, ass-079):** `target.confidence` is a meaningful discriminator and `graph_edges.weight` is not (frozen first-write-wins, ~always success). If confidence is uniformly 0.0 (cold-start default `db.rs:549`), inferred ranking degenerates to arbitrary order. (SR-09, SR-11)
- **A5 (AC-12):** the default-on path stays within a real latency budget on hub nodes. Unmeasured. (SR-12)

## Design Recommendations

1. **Canonicalize symmetric edges in SQL before ranking AND counting, and test both surfaces independently (SR-08).** #4083/#3618 show this is the highest-probability silent defect; a passing build with double-counted totals is the failure mode.
2. **Lock the D-09 `ORDER BY` and the authored predicate as named ACs with discriminating tests (SR-09, SR-10).** Ranking errors and source-misclassification produce no build signal — only a worse affordance. Verify carried-forward `source='agent'` by name (#4425/#4984).
3. **Require a measured edge-free `context_get` baseline before locking AC-12 numbers, with a high-degree node in scope (SR-12).** The hot-path + co-access compounding and fan-in precedent (#4463) make an unbacked budget a real regression risk.
4. **Enforce rank-and-limit-in-SQL as an invariant, not Rust-side slicing (SR-14).** LEFT JOIN with explicit NULL-confidence ordering so dangling targets are retained and deterministic (SR-11, D-02).
5. **Treat "which 3" as the acceptance surface (SR-13).** Under cap 3 every canonicalization/ranking/classification defect is 1/3 of the visible feature — elevate the relevant tests from smoke to discriminating.
