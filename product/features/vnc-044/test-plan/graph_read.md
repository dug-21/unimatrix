# Test Plan — `graph_read.rs` (resolver + seam threading)

> Component: `GraphParams.detail: Option<String>` (additive), `GraphSerialization` enum, `resolve_graph_output`, per-arm serialization seam that fixes the `:251` parse-and-drop.
> Owns R-05 (markdown reject, resolver side), R-08 (legacy alias + conflict), R-04 (full byte-equality — threading side), R-03 (axis threading), R-09 (accept-and-ignore dispatch), R-06 (shared-type guard), R-13 (substring discipline).
> Pseudocode: pseudocode/graph_read.md · AC-02, AC-04, AC-07, AC-08, AC-09 · R-03, R-04, R-05, R-08, R-09.

## Unit Test Expectations — `resolve_graph_output(&GraphParams) -> Result<(Detail, GraphSerialization), ErrorData>`

Table-driven over `(format, detail)` inputs. This decision table is the resolver's whole contract; test every cell. Error assertions use `ERROR_INVALID_PARAMS` code + **substring**, never verbatim (R-13).

| `format` | `detail` | Expected | Risk |
|----------|----------|----------|------|
| `None` | `None` | `(Summary, Json)` — defaults | R-03 |
| `None` | `Some("full")` | `(Full, Json)` | R-03 |
| `Some("json")` | `Some("summary")` | `(Summary, Json)` | R-03 |
| `Some("json")` | `None` | `(Summary, Json)` | AC-05 |
| `Some("markdown")` | any | `Err` INVALID_PARAMS, substring `"markdown"` + `"format=json"` | **R-05** |
| `Some("summary")` (legacy) | `None` | `(Summary, Json)` — alias | **R-08** |
| `Some("summary")` | `Some("full")` | `Err` INVALID_PARAMS (conflict, do-not-combine) | **R-08** |
| `Some("summary")` | `Some("summary")` | `Err` INVALID_PARAMS (still a conflict — resolver order pinned) | **R-08** |
| `Some("bogus")` | any | `Err` INVALID_PARAMS | R-08 |
| any valid format | `Some("bogus")` | `Err` INVALID_PARAMS (from `parse_detail`) | R-09 |

- `test_resolve_default_summary_json`, `test_resolve_markdown_rejected_substring`, `test_resolve_legacy_summary_alias`, `test_resolve_summary_plus_explicit_detail_conflict`, `test_resolve_detail_bogus_rejected`.
- **Pin resolver order** (R-08 row 3): the legacy-alias-conflict branch must fire *before* verbosity parse, so `format=summary`+`detail=summary` is a conflict, not a silent agreement. Assert this explicitly — it is the ADR-002 §2 decision.
- `GraphSerialization` has only `Json` today; `markdown` is rejected *before* a `GraphSerialization` value is produced. Assert no code path yields a markdown serialization value (C-5).

## Unit Test Expectations — seam threading

The `:251` parse-and-drop fix: the resolved `(Detail, _)` must reach each mode arm.

- **Full arm does NOT route through the projection (R-04 side):** `Detail::Full` → `serde_json::to_string(&result)` on the *original* envelope. A unit/inspection test confirms the full arm serializes the raw typed result, not `to_summary_json()`. This is the structural guard behind the golden integration test. `test_full_arm_serializes_raw_result`.
- **Summary arm routes through projection:** `Detail::Summary` → `to_summary_json()`. `test_summary_arm_uses_projection`.
- **neighbors/path always full (R-09):** both edge-only arms call `serde_json::to_string(&result)` regardless of `detail` — no projection, no per-value branch. `test_neighbors_ignores_detail`, `test_path_ignores_detail`.

## Unit Test Expectations — validation (`graph_read_validation.rs`, comment-only change)

- `detail` is **universal**: `validate_no_unsupported_params` adds **no** rejection arm for `detail` on any mode (R-09). Assert `detail=summary` and `detail=full` pass validation on neighbors/path (not rejected as unsupported). `test_detail_not_rejected_on_neighbors`, `test_detail_not_rejected_on_path`.

## Unit Test Expectations — GraphParams layout (R-06 / AC-09)

- Existing `GraphParams` layout/serde test still passes with the additive `detail: Option<String>` field — no existing field removed/retyped/reordered (ADR-003, C-1). If a field-order or round-trip snapshot test exists, it must stay green with `detail` appended last. `test_graph_params_detail_additive`.

## Integration Test Expectations (infra-001 — the Critical through-wire proofs)

These are the tests only the compiled binary can prove. Add to `test_tools.py` (axis behavior) and `test_lifecycle.py` (per-mode projection + golden). Requires the `harness/client.py` `detail=` extension (OVERVIEW harness plan).

### AC-02 / R-03 — axis threading (proves `:251` is fixed)
- Same subgraph query, `detail=summary` vs `detail=full` → structurally different payloads (summary nodes lack `content`; full nodes have it). `test_graph_detail_axis_threaded`.

### AC-05 / R-03 — default-summary + explicit-summary per node-bearing mode (5 modes, Critical)
One default (no `detail`) + one explicit `detail=summary` test **per mode** — `subgraph`, `chain`, `current`, `inverse`, `filter`. Each asserts (a) nodes are lean 8-field summaries, (b) default output equals explicit-summary output, (c) the mode's own envelope metadata is present:
- subgraph → `truncated`, `seed_ids`, `depth_reached`
- chain → `truncated`/`Truncated`
- current → single projected node (not array)
- inverse → `total_returned`
- filter → `total_returned`

`test_graph_{mode}_default_is_summary`, `test_graph_{mode}_summary_preserves_metadata`. **No mode covered by subgraph alone** (R-03 mandate / architect OQ-3).

### AC-04 / R-04 — `detail=full` golden byte-for-byte (Critical)
- Golden fixture: capture `detail=full` output for a fixed multi-node `subgraph` query against the **pre-vnc-044** binary; assert byte-identical under vnc-044 `detail=full`. Key order + field presence both asserted. Repeat for ≥1 other node-bearing mode (`chain` or `inverse`).
- If a pre-change capture is impractical in the harness, fall back: assert full-arm output parses to the complete `EntryRecord` key set (all counts/hashes/timestamps/`metadata`/`direction` present) and is byte-stable across two identical runs. Document which method was used and why in RISK-COVERAGE-REPORT.md.
- `test_graph_full_golden_subgraph`, `test_graph_full_golden_{other}`.

### AC-08 / R-05 — `format=markdown` rejected on ALL SEVEN modes
- `format=markdown` on each of subgraph/chain/current/inverse/filter/neighbors/path → `ERROR_INVALID_PARAMS`, no JSON body, reason substring (`"markdown"`, `"format=json"`). Parametrize over modes. This proves resolution is **pre-dispatch** — neighbors/path (which never touch projection) must reject too. `test_graph_markdown_rejected_all_modes`.
- `format=json` and `format` absent → accepted on all modes (regression). `test_graph_json_accepted_all_modes`.

### AC-07 / R-08 — legacy alias + conflict
- `format=summary` (no `detail`) → byte-identical to `detail=summary` output, no error. `test_graph_legacy_summary_alias_equivalent`.
- `format=summary` + `detail=full` → `ERROR_INVALID_PARAMS` (conflict). `test_graph_legacy_summary_conflict_rejected`.

### AC-08 / R-09 — accept-and-ignore on neighbors/path
- `neighbors` and `path` with `detail=summary`, `detail=full`, and `detail` absent → all three **identical, non-erroring** output. `test_graph_neighbors_detail_ignored`, `test_graph_path_detail_ignored`.
- `detail=bogus` on neighbors/path → still `ERROR_INVALID_PARAMS` (universal parse runs). `test_graph_detail_bogus_rejected_on_edge_modes`.

### AC-06 — #913 size win
- Subgraph fixture large enough to be representative; default (summary) payload byte-size well below the `detail=full` baseline for the same query, and valid parseable JSON. Assert the ratio/threshold, not an absolute KB (fixture-dependent). `test_graph_summary_shrinks_payload`.

## Static / Review Gates (R-06, R-12, R-13, R-14)

- **R-06 shared-type guard:** `cargo test --workspace --no-run` compiles with **no** new exhaustive-match arms on `ResponseFormat`; grep confirms no `skip_serializing_if` added to `EntryRecord`/`EdgeRecord`; non-graph tool regression suite (`context_get`/`search`/`lookup`/`status`/mutations/`briefing`) green with full output unchanged. Code-review gate, not just tests.
- **R-12 single-source `256`:** grep the graph path (`graph_read*.rs`, `graph_read_projection.rs`) for a bare `256` literal → none; all references go through `CONTENT_PREVIEW_BYTES`.
- **R-13 substring discipline:** no verbatim-sentence assertion on the markdown-rejection copy anywhere; only code + substring.
- **R-14 line budget:** `graph_read.rs` stays under 500 after the resolver + arm branching; if it crosses, `resolve_graph_output` relocates to `graph_read_validation.rs` or the projection module (non-blocking, watch at Gate 3b).

## Edge Cases Owned Here

- `format=summary` + explicit `detail` conflict; resolver order (`summary`+`summary` still conflict).
- markdown on edge-only modes (proves pre-dispatch resolution).
- `detail` present on neighbors/path (accept-and-ignore).
- `detail=bogus` rejected uniformly.
- Additive `detail` field preserving GraphParams layout.
