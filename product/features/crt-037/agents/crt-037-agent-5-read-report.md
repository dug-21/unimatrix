# Agent Report: crt-037-agent-5-read

**Component**: `query_existing_informs_pairs` — `crates/unimatrix-store/src/read.rs`
**Feature**: crt-037 (Informs Edge Type)
**Agent ID**: crt-037-agent-5-read

---

## Work Completed

Added `Store::query_existing_informs_pairs(&self) -> Result<HashSet<(u64, u64)>>` to `crates/unimatrix-store/src/read.rs`.

Implementation follows the `query_existing_supports_pairs` pattern exactly, with two intentional differences per ADR-003:
1. `relation_type = 'Informs'` (not `'Supports'`)
2. Returns `(source_id, target_id)` as-is — no `(a.min(b), a.max(b))` normalization

The directional contract is enforced: tuples are stored and returned exactly as written to the DB, relying on the Phase 8b temporal ordering guard to make the reverse pair detection-impossible.

---

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-store/src/read.rs`

---

## Tests

11 new unit tests added in `read::tests`, all passing:

| Test | Covers |
|------|--------|
| `test_query_existing_informs_pairs_empty_table_returns_empty_set` | ADR-003 empty case |
| `test_query_existing_informs_pairs_returns_directional_tuple` | R-09 scenario 1 |
| `test_query_existing_informs_pairs_does_not_normalize_reverse` | R-09 scenario 2 (critical) |
| `test_query_existing_informs_pairs_multiple_rows` | Multiple rows |
| `test_query_existing_informs_pairs_excludes_bootstrap_only_rows` | R-09 scenario 3 |
| `test_query_existing_informs_pairs_includes_non_bootstrap_excludes_bootstrap` | Mixed bootstrap |
| `test_query_existing_informs_pairs_excludes_other_relation_types` | Relation type isolation |
| `test_write_nli_edge_informs_row_is_retrievable` | R-01 write+readback |
| `test_graph_edges_informs_relation_type_stored_verbatim` | R-01 string verbatim |
| `test_query_existing_informs_pairs_dedup_prevents_duplicate_write` | R-17 / AC-23 |
| `test_query_existing_informs_pairs_directional_contract_reversed_storage` | ADR-003 directional |

**Test count**: 215 passed, 0 failed (was 204 before this change).

---

## Issues

None. All pre-delivery gates cleared per IMPLEMENTATION-BRIEF.md.

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced ADR-003 (#3940) confirming directional dedup rationale, and entry #3659 confirming contrast with symmetric Supports approach. Applied directly.
- **Stored**: nothing novel to store — the directional vs symmetric dedup distinction is already fully captured in ADR-003 (#3940). The implementation mirrors the pseudocode exactly with no surprises.
