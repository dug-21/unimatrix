# Test Plan — get-edge-vocabulary (`GetEdge` / `EdgeTotals` / `EdgesView`)

The thin get-edge types in `response/edges.rs`. Owns the **shape contract** (R-04-payload /
guardrail), the **`authored` boolean** (R-09), and the **projection-matches-`EdgeRecord`**
fidelity (R-10/FR-15). Server unit tests.

## Unit Test Expectations

### Discovery-list shape (ADR-002 guardrail, FR-4/AC-02)

**`test_get_edge_exact_five_fields`**
Assert `GetEdge` carries **exactly** `{edge_type, direction, target_id, target_title, authored}`
and **nothing more** — no `source_id`, `depth`, `metadata`, raw `source` string, `weight`, or
`target_confidence`. A serialized `GetEdge` has exactly these 5 keys. Any added field is a
boundary violation.

**`test_edges_view_caps_at_limit`**
`EdgesView.edges.len() <= GET_EDGE_DISPLAY_LIMIT` (reference the constant, not 3).

### R-09 — `authored` boolean exactness (High)

**`test_authored_true_only_for_agent`** (AC-03)
`authored == (source == "agent")`: seed `source` values `agent`, `co_access`, `cosine`,
`behavioral`, `S8`; assert `authored` true **only** for `agent`, false for all inferred.

**`test_authored_exact_match_no_near_miss`** (the predicate-parity guard)
Near-miss strings `'Agent'`, `' agent'`, `'agent '` → `authored` stays **false** (exact match).
This predicate MUST be identical to the SQL `(source='agent')` rank term (store-ranked-query) —
a divergence corrupts both the trust split and authored-first ranking.

### R-10 / FR-15 — Projection fidelity to `EdgeRecord`

**`test_direction_strings_inbound_outbound_both`**
`direction` ∈ {`"inbound"`, `"outbound"`, `"both"`}; `"both"` is the get-only canonical-symmetric
value (renders `↔`). Assert the `"both"` variant exists and is distinct.

**`test_projection_matches_edgerecord_mapping`** (FR-15)
Cross-check the projected `edge_type`/`target_id`/`direction` against `context_graph`'s
`EdgeRecord` `relation_type`/`target_id`/(`incoming`/`outgoing`) mapping — same vocabulary; the
projection drops `source_id`/`depth`/`metadata`, adds `target_title`/`authored`, adds get-only
`↔`. The `↔` MUST NOT exist in the `EdgeRecord` (neighbors) vocabulary.

### EdgeTotals shape

**`test_edge_totals_inbound_outbound_both_object`** (OQ-01/ADR-005, 3-bucket contract)
`EdgeTotals { inbound, outbound, both }` — the nested object shape, **three keys**, uncapped (the
value comes from store-split-count; this asserts the type carries all three fields). Serialized,
assert `obj.len() == 3` and the `both` key is present. The digest-only `authored` aggregate is
**NOT** a field of `EdgeTotals` (it rides on `EdgesView`/assembly — see get-edge-assembly).

### R-20 — `target_confidence` never surfaced

**`test_target_confidence_not_in_get_edge`** (ADR-002/ADR-006)
The ranked row's `target_confidence` is used for the inferred tiebreak only — assert it is
**absent** from `GetEdge` (never projected/serialized).

## Integration Expectations (through MCP)
- `test_get_edge_shape_exact_fields` (tools) — AC-02 exact 5-field set in the JSON response.
- `test_get_authored_flag_agent_vs_inferred` (tools) — AC-03.

## Edge Cases
- `direction = "both"` carries no arrow (asserted in serializer-seam render).
- Near-miss `source` strings stay `authored=false`.
- `target_title: None` serializes as JSON `null`.

## Security
- No input surface (pure value types); the guardrail (no enrichment field) is the security-
  adjacent invariant — minimal disclosure.
