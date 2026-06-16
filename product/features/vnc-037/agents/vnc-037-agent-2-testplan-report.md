# Agent Report — vnc-037-agent-2-testplan

**Phase:** Stage 3a test-plan targeted update (post-Gate-3a OQ-02 lock to THREE buckets).
**Trigger:** ADR-005 AMENDED (TOTALS BUCKET CONTRACT, locked 2026-06-16) — `↔` gets its own
`both` bucket, no longer folded into `inbound`.

## Files Updated

- `product/features/vnc-037/test-plan/store-split-count.md`
- `product/features/vnc-037/test-plan/get-edge-vocabulary.md`
- `product/features/vnc-037/test-plan/serializer-seam.md`
- `product/features/vnc-037/test-plan/get-edge-assembly.md`
- `product/features/vnc-037/test-plan/OVERVIEW.md`

## Confirmation — 3-bucket + locked-digest assertions in place

- **store-split-count.md:** intro + invariant block restated to 3 buckets
  `EdgeCountSplit { inbound, outbound, both }` + digest-only `authored` aggregate.
  `test_count_uncapped_exact` now asserts `inbound + outbound + both`. Symmetric-counted-once
  retargeted to `both`. `count_nested_shape_three_buckets`, `count_returns_scalars_not_materialized`,
  Supersedes-excluded-from-all-three, edge cases all updated.
- **#744 inbound-integrity test ADDED:** `test_count_symmetric_increments_both_never_inbound`
  replaces the retired "↔ folds into inbound" assertion — asserts `↔` ⇒ `both += 1`, `inbound`
  unchanged, `both` distinct from `inbound` (a node with N `CoAccess` + 0 true inbound reads
  `inbound:0, both:N`).
- **authored aggregate test ADDED:** `test_count_authored_aggregate_over_full_set` —
  `SUM(source='agent')` over the full canonicalized `deduped` set, digest-only (not JSON/markdown).
- **get-edge-vocabulary.md:** `EdgeTotals` test now `{inbound, outbound, both}`, asserts
  `obj.len() == 3` + `both` key; notes `authored` is not an `EdgeTotals` field.
- **serializer-seam.md:** `test_summary_digest_locked_byte_form` asserts the EXACT locked bytes
  `" | edges: {outbound}↑ {inbound}↓ ↔{both} ({K} authored)"`, fixed arity, all-zero ⇒
  `" | edges: none"`, `{K}` over the FULL uncapped set. JSON branch asserts 3-key `edge_totals`.
  `…N more` arithmetic from `inbound + outbound + both`. Zero-edge json now 3 explicit buckets.
- **get-edge-assembly.md:** intro + new tests for `EdgeCountSplit → EdgeTotals{in,out,both}`
  projection (`test_totals_projection_three_buckets`) and `authored_total` threading from the full
  set into `EdgesView` (`test_authored_total_threaded_from_full_set`).
- **OVERVIEW.md:** R-01, R-03, R-17 risk→test rows and integration test #5 updated to the 3-bucket
  shape + locked digest + #744 guard.

## Unchanged (as instructed)

Discriminating ranking test (#3886 proof-outside-cap), canonicalization-on-display-and-totals
split, the fail-loud RED test (#4876), and byte-identity-via-real-producer (#1268) — all left
intact.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — NOT RUN (Unimatrix MCP disconnected per spawn
  prompt; ADR read directly from `architecture/ADR-005-...md`). Non-blocking per protocol.
- Stored: nothing novel to store — this was a targeted contract-propagation edit driven entirely by
  the locked ADR-005 amendment; no new reusable test infrastructure pattern emerged. (The
  three-bucket digest contract itself belongs in the ADR, not as a generic testing pattern.) MCP
  was disconnected regardless, so storage was not possible this session.
