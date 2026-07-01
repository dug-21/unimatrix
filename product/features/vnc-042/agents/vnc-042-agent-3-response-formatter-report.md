# Agent Report — vnc-042 Response Formatter (agent-3)

## Scope
Implement the `response-formatter` component: `ResolutionNote` enum + `format_single_entry_with_note`
in `crates/unimatrix-server/src/mcp/response/entries.rs`. `format_single_entry` left byte-identical.

## Files modified
- `crates/unimatrix-server/src/mcp/response/entries.rs` — added `ResolutionNote` enum, private
  `render_note_text` / `render_note_json` helpers, `format_single_entry_with_note`, and the
  `#[cfg(test)] mod tests` (13 unit tests).
- `crates/unimatrix-server/src/mcp/response/mod.rs` — re-export line only: added `ResolutionNote`
  and `format_single_entry_with_note` to the `pub use entries::{...}` block so the handler agent
  (tools.rs) can import them (mirrors how `format_single_entry` is imported). No other change.

## Implementation notes (per ADR-003 / brief)
- Note NEVER injected inside `format_single_entry` (C-7). Clean passthrough carries no note and
  routes to the untouched base formatter → byte-identity preserved.
- JSON `resolution` object present ONLY on non-clean paths (function only reached on non-clean),
  inserted via the SAME `as_object_mut()` map after edges/edge_totals → clean json byte-identical.
- `superseded_by: Option<u64>` matched, never unwrapped: `Some(z)` → `superseded by #{z} …`;
  `None` → `deprecated; no recorded successor.` and json `superseded_by: null`. No panic, no `#{}`,
  no `#null` (AC-08, R-08, C-4).
- Text placement: Followed/DeadEnd prepend a line; AsStoredDeprecated appends a footer
  (markdown as `> blockquote`, matching `format_store_success_with_note`).
- No `.unwrap()` in non-test code (`serde_json::to_string_pretty(...).unwrap_or_default()` as the
  existing formatters do).

## Tests
- New component tests (`mcp::response::entries::tests`): 13 passed, 0 failed.
  Covers all four ADR-003 cases × formats, strip-and-compare additivity, body-equivalence,
  null-successor well-formed footer (no panic/`#`), and edges-on-note-path.
- Regression guards UNTOUCHED and green: full `mcp::response` lib suite = 305 passed, 0 failed,
  including the byte-identity canary `test_none_json_byte_identical_to_base_object` and the
  `format_single_entry` shape tests. ZERO edits to any guard test.
- clippy `-p unimatrix-server --lib`: clean on entries.rs. fmt: entries.rs + mod.rs formatted clean.

## Issues / adjacent breakage flagged (NOT fixed — outside my component)
1. **`graph_read.rs:61` unused-import warning** — `pub(crate) use graph_read_neighbors::follow_to_current;`
   is currently unused. Expected transient state: the `follow-to-current` re-export agent added it;
   it resolves once the handler agent (tools.rs) calls it. Benign, not my file.
2. **Integration-test link OOM** — `cargo test -p unimatrix-server <filter>` (without `--lib`) tries
   to link ALL integration test binaries (cert_provisioner, project_routing_integration,
   import_integration) and the linker was OOM-killed (`ld: signal 9`) in this env. This is an
   environment/link constraint, NOT a code failure; component tests run cleanly under `--lib`.
   Flagging so the tester budgets memory (or serial link) for the pre-PR `--workspace` gate.
3. **Pre-existing fmt diffs in non-component files** — `src/uds/listener.rs` and
   `src/http_provision/slug_config_tests.rs` show `cargo fmt --check` diffs. NOT my files; left
   untouched. Leader may want to revert/format as churn control.
4. **File length** — `entries.rs` is now ~925 lines (≈526 production + tests). Over the 500-line
   guideline, but consistent with this module's convention (formatter tests live in the 1982-line
   `mod.rs`); the `cargo test … response::entries` filter and the brief both pin tests to
   `entries.rs`. Flagging for the leader to decide if a test-submodule split is wanted (would not
   change the `response::entries` test path).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced formatter/format-selectable
  precedents (ADR-004 vnc-002 #87, patterns #3459/#298, format_store_success_with_note precedent);
  applied the `_with_note` split + structured-json-key convention. No blocking gotchas missed.
- Stored: FAILED — attempted `/uni-store-pattern` (byte-identity additivity test technique +
  `--lib` integration-link OOM note) but the store was rejected: "Agent 'anonymous' lacks Write
  capability." Non-blocking; content captured here for the leader/retro to store under
  `unimatrix-server` if desired.
