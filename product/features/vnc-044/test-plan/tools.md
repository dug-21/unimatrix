# Test Plan — `tools.rs` (`context_graph` tool description)

> Component: `context_graph` tool-description edit — document both axes, `summary` default + per-tool divergence, `format=markdown` rejection, and the lifecycle-vs-delivery status caveat.
> Owns R-11 (lifecycle-vs-delivery status — **documentation/expectation gate, NOT a code defect**) and the description-drift guard (R-13, vnc-043 precedent #5449).
> Pseudocode: pseudocode/tools.md · AC-09, AC-06 (doc side) · FR-12.

## Critical Context — twin-literal byte-equality guard (from #5449 / #869)

The `context_graph` contract lives in **two `&str` literals that must agree byte-for-byte**:
1. mirror const `CONTEXT_GRAPH_DESCRIPTION` (tools.rs:~76, substring-tested)
2. the live `#[tool(description = "...")]` attribute literal on the handler (tools.rs:~3945-3996, what MCP clients receive)

rmcp 1.7.0 cannot consume a const inside the attribute, so a single-source collapse is impossible; the existing guard `test_graph_tool_attr_description_matches_const` (tools.rs:~6263, #869) asserts they are byte-identical. **vnc-044's description edit MUST touch BOTH literals identically** or this guard red-bars. This is the single highest-probability CI break for the tools.rs change — call it out to the implementer.

## Unit / Static Test Expectations

### Description-content substring assertions (extend existing at tools.rs:~6198+)

The description must document the new contract. Extend the existing substring-presence assertions (they assert presence, so additions don't disturb existing ones). Assert the description contains substrings covering:

| Requirement | Substring to assert (illustrative — pick the running phrase) | AC / FR |
|-------------|--------------------------------------------------------------|---------|
| `detail` axis exists | `"detail"` + `"summary"` + `"full"` | FR-12, AC-09 |
| default is summary | `"default"` near `"summary"` | FR-3, AC-05 |
| per-tool default divergence (migration window) | phrase noting graph defaults summary during suite migration | SR-04, FR-12 |
| `format` = serialization only | `"json"` / `"markdown"` framed as serialization | FR-1 |
| `format=markdown` rejected | phrase naming markdown-not-supported + `"format=json"` | SR-05, AC-08 |
| **lifecycle-vs-delivery status caveat** | `"lifecycle"` + status is NOT capability delivery status; point to `context_get`/follow-up | **SR-09, R-11, AC-09** |

- `test_graph_description_documents_detail_axis`, `test_graph_description_states_lifecycle_status_caveat`, `test_graph_description_states_markdown_rejection`.
- **R-13 discipline:** assert **substrings**, not the whole description block. Match the running phrase, not the ADR/spec wording (which diverges — pattern #3337).
- **Byte-equality guard stays green:** confirm `test_graph_tool_attr_description_matches_const` passes after both literals are edited (regression — do not modify the guard).

### R-11 — lifecycle-vs-delivery status (documentation/expectation gate)

This is the single most important nuance of vnc-044 and it is **NOT a functional pass/fail**. The verification is by **review**, not assertion:

1. Tool-description review: the description states projected `status` is **lifecycle** (`active|deprecated|proposed|quarantined`), **not** capability delivery status (`missing|partial|proven|claimed`), and points delivery-status needs at `context_get` / named follow-up #3. (Substring test above backs this.)
2. AC-06 doc-review: the #913 reproduction result is documented as carrying lifecycle status only — a capability subgraph shows `active` for every node.
3. **Optional behavioral illustration (evidence, not a defect):** an integration test that pulls a capability-node subgraph and asserts `status:"active"` for every node — this *demonstrates the gap is real*, it does **NOT** treat delivery-status absence as a bug. If written, name it `test_graph_summary_status_is_lifecycle_illustration` and comment it clearly as illustrative. **Do NOT** write any test that fails because delivery status is absent (R-11 mandate).

## Integration Test Expectations

- Tool discovery unaffected: `test_protocol.py:37` still asserts exactly 14 `context_*` tools; the description edit does not add/remove a tool. Regression only.
- The description is delivered to clients via `tools/list`; a smoke assertion that `context_graph`'s description mentions `detail` confirms the live attribute (not just the mirror const) carries the new contract. `test_graph_tool_description_advertises_detail` (through-wire, guards against editing only the mirror const).

## Edge Cases Owned Here

- Both literals edited in lockstep (guard green).
- Substring assertions match running strings, not spec wording (R-13).
- Delivery-status absence is documented, never tested-as-defect (R-11).
