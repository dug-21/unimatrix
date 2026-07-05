# Agent Report — vnc-044-agent-5-projection-seam

## Scope
Component A (`graph_read_projection.rs`, new) + Component B (`graph_read.rs`, modify) — the
lean summary projection and its sole consumer (the serialization seam). Implemented A then B.

## Files Modified
- `crates/unimatrix-server/src/mcp/graph_read_projection.rs` (new) — 430 lines
- `crates/unimatrix-server/src/mcp/graph_read.rs` (modify) — 469 lines
- `crates/unimatrix-server/src/mcp/graph_read_tests_vnc044.rs` (new test module) — 258 lines
- `crates/unimatrix-server/src/mcp/graph_read_tests.rs` (declare the vnc044 test child module — one `#[path] mod` block)

All under the 500-line budget. `graph_read.rs` landed at 469 (est. was ~460), so the C-7
resolver relocation to `graph_read_validation.rs` was NOT triggered.

## What was built
- `NodeSummary` (8-field, `#[derive(Debug, Serialize)]`), `node_summary()`, `edge_summary()`
  (4-field `serde_json::Value`, drops `direction`/`metadata`), and `GraphSummaryProjection`
  trait impls for all five node-bearing envelopes, each preserving its own metadata
  (`truncated`/`seed_ids`/`depth_reached`/`total_returned`/`Truncated`; `current` = single node).
  No `skip_serializing_if` added to shared `EntryRecord`/`EdgeRecord` (C-2/C-3).
- `GraphParams.detail: Option<String>` appended at struct end (`#[serde(default)]`), additive —
  no existing field moved/retyped/reordered (C-1). `format` doc re-scoped to serialization-only.
- `GraphSerialization` enum (`Json` only, `#[derive(Debug)]`) + `resolve_graph_output` — legacy
  alias FIRST (with `format=summary` + explicit `detail` conflict), then serialization
  (markdown rejected loudly, no silent fallback), then `parse_detail`. Runs pre-dispatch in
  `handle_graph`, so all seven modes reject uniformly.
- Serialization seam via a generic `serialize_detail<T: Serialize + GraphSummaryProjection>`
  helper: `Detail::Full` → `serde_json::to_string(&result)` (byte-identical, does NOT route
  through the projection); `Detail::Summary` → `to_string(&result.to_summary_json())`. Threaded
  into the five node-bearing arms. `neighbors`/`path` keep `serde_json::to_string(&result)`
  unchanged (accept-and-ignore).

## Tests
- `cargo test -p unimatrix-server --lib graph_read`: **235 passed, 0 failed**.
- Projection unit tests (17): R-07 present-AND-absent key sets (node 8-field, edge 4-field);
  status lifecycle string; R-10 empty/tags/confidence fidelity; R-03 per-envelope metadata
  preservation incl. the `current` single-node-not-array trap.
- Seam/resolver unit tests (vnc044 module, 21): full `resolve_graph_output` decision table
  (R-03/R-05/R-08 incl. the summary+summary conflict order pin); full-arm byte-identical +
  not-projected guard (R-04 side); summary-arm-projects; additive `GraphParams.detail`
  round-trip (R-06/AC-09); `detail` universal on neighbors/path (R-09).
- Integration/through-wire tests (Python harness `test_lifecycle.py`/`test_tools.py`, AC-04
  golden, AC-06 size win, markdown-all-7-modes end-to-end) are the tester's Stage 3c scope —
  not built here; the unit layer proves the resolver + seam contract.
- `cargo build -p unimatrix-server`: clean. `cargo clippy -p unimatrix-server --lib`: zero
  warnings. No bare `256` literal in the graph path (R-12) — routed through
  `CONTENT_PREVIEW_BYTES`.

## Issues / Flags
- **`graph_read_validation.rs` NOT touched.** My spawn guardrail restricts that file to
  relocation-only, and no relocation was needed. The IMPLEMENTATION-BRIEF component map lists a
  *comment-only* doc note there ("`detail` is universal, no per-mode rejection arm"). I deferred
  it to respect the guardrail. Behavior is correct regardless — `detail` is never listed in the
  validator, and the vnc044 tests pin that `detail` passes validation on neighbors/path. If the
  leader wants that documentation comment, it is a one-line addition to route to another agent.
- No out-of-scope breakage discovered. `graph_read_subgraph.rs` (742-line debt), `schema.rs`,
  and `response/mod.rs` untouched.
- Tooling gotcha (stored below): bare `rustfmt <file>` fails in this crate; must use
  `rustfmt --edition 2024 --config skip_children=true <file>`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-002 (#5510), ADR-001 (#5509), and
  the graph-module line-limit split patterns (#4518/#4509); confirmed the resolver/projection
  approach against the ratified ADRs. No conflicting guidance.
- Stored: entry #5520 "rustfmt single-file needs --edition 2024 --config skip_children=true in
  this workspace" via context_store (pattern, topic `unimatrix-server`) — a real tooling trap
  that makes the protocol's "rustfmt <yourfile>" instruction fail on the graph_read module
  family (edition-2024 syntax + `#[path] mod` child descent).
