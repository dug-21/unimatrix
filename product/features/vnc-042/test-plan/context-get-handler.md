# Test Plan — `context_get` handler (resolution branch, `GetParams`, tool-desc)

**Component:** `crates/unimatrix-server/src/mcp/tools.rs` — `GetParams` (`:246-274`), `context_get`
handler resolution branch (`:950-1052`), tool-description strings (`:947-948`).
**Owns risks:** R-02 (Critical), R-03 (High), R-04 (High), R-09 (High-accept). Route selection for
R-07 (formatter renders; handler must not route clean⇒note wrapper).

Tests live in the `#[cfg(test)]` module of `tools.rs`; async paths use `#[tokio::test]`.

---

## Unit test expectations

### `GetParams` serde three-state (R-02, NFR-02, NFR-06)
- `test_get_params_follow_supersessions_absent_deserializes_none` — JSON omitting the field ⇒
  `params.follow_supersessions == None`. (Field-value check; the *behavioral* proof is TS-09 below.)
- `test_get_params_follow_supersessions_true_deserializes_some_true` — `"follow_supersessions": true`
  ⇒ `Some(true)`.
- `test_get_params_follow_supersessions_false_deserializes_some_false` — `false` ⇒ `Some(false)`.
- `test_get_params_follow_supersessions_no_quoted_scalar_coercion` — `"follow_supersessions": "true"`
  (quoted) MUST NOT silently coerce to `Some(true)` (assert deserialize error or reject); guards
  against a `deserialize_*_or_string` field (#3728). Plain `Option<bool>` only.
- **`test_get_params_no_existing_field_removed_or_retyped` (NFR-06 / TS-03) — MUST stay green,
  edit = FLAG event.** New field is purely additive.

### Resolution-branch effective-id selection (handler logic)
Arrange a store with a correction chain A→B (B active terminal); Act via the handler; Assert on the
selected `effective_id` and `ResolutionNote`.

- `test_get_handler_default_deprecated_resolves_to_terminal` (**TS-04, AC-01/AC-06**) — `context_get(A)`
  param omitted ⇒ returned entry id == B, body == B's stored content, note == `Followed{from:A,to:B}`.
- `test_get_handler_active_terminal_clean_passthrough` (**TS-05, AC-02 no-hop**) — `context_get(B)` ⇒
  `effective_id == B`, **no note**, routed through `format_single_entry` (not the note wrapper).
- `test_get_handler_follow_false_returns_as_stored` (**TS-06, AC-03**) — `context_get(A, false)` ⇒
  `effective_id == A`, note == `AsStoredDeprecated{requested:A, superseded_by:Some(B)}`.
- `test_get_handler_follow_false_active_no_footer` (AC-03) — `context_get(B, false)` on an active
  entry ⇒ clean passthrough, **no note** (footer only for deprecated).

### Behavioral default-on — highest-value test (R-02, TS-09, AC-06, NFR-01)
- `test_get_handler_field_absent_resolves_to_terminal` — **behavioral**, authoritative. A JSON tool
  call with `follow_supersessions` ABSENT ⇒ `context_get(A_deprecated)` returns terminal B, NOT
  as-stored A. This fails if delivery implements a bare `#[serde(default)] bool` (defaults OFF) even
  though a serde field-value round-trip would pass. This is the single highest-value test — assert
  the *resolved behavior*, never merely the deserialized field value.

### `id → effective_id` threading (R-03 — highest-probability integration defect)
- `test_get_handler_resolved_edges_keyed_on_terminal` (**TS-03/TS-08, AC-07**) — `context_get(A,
  include_edges=true)` hopping to B ⇒ `build_edges_view` called with `effective_id == B`; returned
  edge list is **B's** edges, returned entry id == B. Guards the partial-swap defect (terminal
  content + requested-id edges).
- `test_get_handler_include_edges_false_skips_assembly` — `context_get(A, include_edges=false)` ⇒ no
  edge assembly, no `edges` key, resolution still occurs (id == B).
- `test_get_handler_deadend_edges_keyed_on_requested` — dead-end / as-stored ⇒ `effective_id ==
  requested id`, edges keyed on requested id (ADR-002/ADR-003).
- Assert no `as`/`try_into` cast introduced on the `validated_id → u64 → follow_to_current` path
  (type flow clean, RISK §Integration Risks).

### Dead-end fail-loud (R-04, TS-07, AC-04) — every `None` sub-case ⇒ non-empty loud flag
- `test_get_handler_deadend_orphaned_terminal_returns_requested_id_flag` — chain dead-ends on an
  orphaned deprecated (`superseded_by IS NULL`, `status != Active`) ⇒ non-empty, returned id ==
  originally-requested id, note == `DeadEnd{requested:id}`.
- `test_get_handler_deadend_quarantined_terminal_flag` — quarantined terminal ⇒ same loud flag.
- `test_get_handler_deadend_over_50_hops_flag` — chain > 50 hops **exercised through the handler**
  (not only `graph_queries_tests.rs`) ⇒ cap trips ⇒ `None` ⇒ dead-end flag. Cap NOT weakened (C-3).
- `test_get_handler_deadend_cycle_self_supersedes_flag` — `superseded_by` self-loop/cycle ⇒ cap
  trips, no infinite loop ⇒ dead-end flag.
- `test_get_handler_follow_to_current_store_error_fails_loud` — store error collapsed to `None`
  inside `follow_to_current` ⇒ dead-end **flag** (loud), never silent success (#4876: verify error
  propagation empirically, not assumed).
- **Assert in every case:** result is non-empty, is NOT an `Err`/empty payload, returned id ==
  requested id, flag present.

### Fail-loud on post-primary-read failures (C-4, FR-14)
- `test_get_handler_edge_assembly_failure_fails_loud` — `build_edges_view` error ⇒ mapped
  `ServerError::Core` returned, resolution does not soften (existing `tools.rs:984-987` behavior).
- `test_get_handler_terminal_fetch_failure_fails_loud` — `follow_to_current` returns `Some(B)`, then
  `entry_store.get(B)` fails (deleted between reads) ⇒ FAIL-LOUD `ServerError::Core`, NOT a dead-end
  flag (RISK §Failure Modes terminal-fetch race). Low likelihood; note-level coverage.
- Grep/review: **no `.unwrap()`** in the non-test resolution path (C-4).

### Tool description + canonical call-site (R-09/R-05 — handler-side asserts)
- `test_get_tool_description_documents_follow_supersessions` (**BLD-04, AC-05 proxy, FR-13**) — the
  `context_get` description string (`tools.rs:947-948`) contains `follow_supersessions`, states the
  new default (follow), and mentions the escape hatch (`=false` / as-stored). A lying description is
  a known hazard (#4303).
- **BLD-02 (grep, AC-05):** handler invokes `crate::mcp::graph_read::follow_to_current` (canonical,
  Pattern #4436), NOT the `graph_read_supersession.rs:122` duplicate, and adds no new recursive CTE
  or in-memory walk in `tools.rs`.

---

## Integration expectations (through MCP — see OVERVIEW §4)
- Default-resolves, clean-passthrough, escape-hatch, dead-end, and orthogonality scenarios are the
  end-to-end proofs of the handler's effective-id selection and route choice. Dead-end quarantine
  scenario needs `admin_server`. Requires the additive `follow_supersessions` kwarg on the harness
  `context_get` client helper (tracked in OVERVIEW §4.3).

## Edge cases (from RISK §Edge Cases)
- Requested id **is** the active terminal ⇒ clean passthrough, no notice.
- Requested id is non-deprecated, non-superseded active ⇒ clean passthrough.
- Requested id itself quarantined (not deprecated), no successor ⇒ `None` ⇒ dead-end flag on
  requested id.
- Exactly **50 vs 51** hops — boundary: 50 resolves, 51 ⇒ dead-end.
- Non-existent id ⇒ primary fetch error, FAIL-LOUD (unchanged behavior).

## Accepted / flagged (not gated here)
- **R-09:** non-code durable-id consumers (memory files, edges, prior sessions) are outside any
  harness — behavioral coverage **impossible by design**. Testable proxy = tool-desc assert above.
  **Flag for human.**
- **R-10:** graph `resolve_supersessions=false` vs get `follow_supersessions=true` — documented in
  ADR-001; review-time awareness, no test.
