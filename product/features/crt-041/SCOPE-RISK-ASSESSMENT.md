# Scope Risk Assessment: crt-041

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | S2 dynamic SQL built from operator vocabulary — SQL injection if a vocabulary term contains `'`, `%`, or `\` characters | High | Med | Architect must use parameterized binding for each term (sqlx `bind()`), not string interpolation. The space-padded instr() pattern is safe only if the term is a bound parameter, not interpolated. |
| SR-02 | S1 self-join on `entry_tags` is O(N²) in the number of tag-bearing entries. At 10,000 entries with dense tags the un-LIMIT'd intermediate set may be millions of rows before HAVING filters | Med | Med | Architect must confirm the LIMIT cap is applied at the SQL level before GROUP BY materializes the full pair set, or restructure as a two-phase query (pre-filter by shared-tag candidate, then score). |
| SR-03 | S8 watermark is updated after edge writes (correct ordering); but if the process crashes between the last INSERT OR IGNORE and the counter UPDATE, the batch re-runs on the next tick with the same event_ids. INSERT OR IGNORE is safe, but re-processing may inflate per-tick latency unpredictably on restart | Low | Low | Document the at-least-once re-processing guarantee explicitly in spec ACs. No architecture change needed — idempotency covers it. See entry #4026. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | crt-040 prerequisite is stated as "assumed shipped" but SCOPE.md §Constraints flags it as an open question. If crt-040 ships without `write_graph_edge(source: &str, ...)`, crt-041 must add it — unscoped work that could widen delivery scope mid-sprint | High | Med | Spec must include an explicit gate: delivery agent verifies `write_graph_edge` exists before writing S1/S2/S8 call sites. Define the fallback scope (add the function in crt-041) in the spec so there is no ambiguity. |
| SR-05 | `cross_category_edge_count` and `isolated_entry_count` are new `GraphCohesionMetrics` fields. "Cross-category" is defined only in the eval gate goal (§Goals 7) — the exact field on the entry that defines "category" (the `category` column, or a tag?) is not spelled out | Med | Med | Spec must define the SQL for both new metrics precisely, including which column determines category membership and what constitutes "isolated" (degree = 0 across all relation types, or only Informs?). |
| SR-06 | S1/S2 edges are additive-only (§Design Decision 7). Tag or vocabulary changes do not remove stale edges. Over time, entries that shared tags but have since diverged remain connected in the PPR graph | Med | Low | Architect should specify whether orphaned-edge compaction (crt-039) covers S1/S2 source values, or whether a separate reconciliation pass is deferred. Scope creep risk if PPR quality degrades and compaction is insufficient. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Both crt-040 and crt-041 add fields to `InferenceConfig`. Entry #4014 documents the dual-maintenance trap: serde backing functions and `impl Default` diverge silently. A crt-040 merge that adds fields without updating `impl Default` corrupts the defaults seen by crt-041 tests | High | High | Both specs must mandate the dual-site check (entries #3817, #4014). The crt-041 spec must include a pre-delivery verification step: run `InferenceConfig::default()` and confirm all five new fields match their serde backing functions. |
| SR-08 | S8 reads `audit_log.target_ids` as a JSON array of u64. The `audit_log` schema stores `target_ids TEXT NOT NULL DEFAULT '[]'`. Malformed JSON (written by a bug in a prior tool call) causes the S8 batch to fail silently (warn! and continue per infallible design) — but also silently skips updating the watermark for that row, potentially re-processing it indefinitely | Med | Low | Spec must define per-row error handling: on JSON parse failure, log the `event_id` at warn! and advance the watermark past that row anyway (or skip it with a permanent marker). Do not leave the watermark stuck behind a single malformed row. |
| SR-09 | S1/S2/S8 all run after `run_graph_inference_tick`. S1 and S2 write Informs edges; `TypedGraphState::rebuild` has already run earlier in the tick. New edges are not visible to PPR until the next tick. This is accepted behavior — but the eval gate queries `context_status` immediately after delivery, meaning the first tick after delivery is the first moment edges appear in the rebuilt graph | Low | Low | Spec should note that the eval gate must be run after at least one full tick completes following delivery, not immediately after server start. |

## Assumptions

- **§Goals 1–3** assume `graph_edges.source` TEXT column is the canonical signal-origin field. This is confirmed by §Background Research. No assumption risk.
- **§Proposed Approach** assumes `write_pool_server()` is safe to call from background tick context without spawn_blocking. This follows the crt-034 co_access_promotion pattern — valid, but spec must confirm the pool handle is accessible from `graph_enrichment_tick.rs` without threading through a new parameter.
- **§Goals 7** assumes the ASS-039 behavioral scenarios at MRR 0.2875 remain the baseline. If crt-040 shifted MRR, the baseline may need refreshing before crt-041 eval.
- **§Non-Goals** states no schema migration. Confirmed by the `counters` table reuse for S8 watermark and `graph_edges` reuse for all writes. Valid assumption.

## Design Recommendations

- **SR-01 (Critical)**: Spec must explicitly require parameterized binding for S2 vocabulary terms. A code comment noting the injection risk and its mitigation (bound parameter) is required in the implementation AC.
- **SR-04 (High)**: Add a pre-flight AC to the spec: "Before implementing S1/S2/S8 call sites, verify `write_graph_edge(source: &str, ...)` exists in `unimatrix-store`. If absent, add it as the first implementation step."
- **SR-07 (High)**: Spec must enumerate both mutation sites for each new `InferenceConfig` field (the `default_*()` backing fn and the `impl Default` struct literal line) and require both are updated atomically. Reference entries #3817 and #4014.
- **SR-05 (Med)**: Spec must include the exact SQL for `cross_category_edge_count` (join graph_edges to entries on both endpoints, WHERE e1.category != e2.category) and `isolated_entry_count` (entries with no row in graph_edges as either source_id or target_id).
- **SR-02 (Med)**: Architect must verify the S1 SQL LIMIT fires before GROUP BY materialization, or add an explicit note that the query plan is acceptable up to 10,000 entries (test-verified).
