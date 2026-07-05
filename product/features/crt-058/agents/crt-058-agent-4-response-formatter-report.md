# Agent Report — crt-058-agent-4-response-formatter

**Component:** response-formatter (Wave A)
**Feature:** crt-058 — Eager Agent-Authored Edge Cleanup at `context_deprecate`

## Scope Delivered

Added the `edges_removed: Option<u64>` advisory to the shared status formatter per ADR-004,
rendered in all three formats for `Some(n)` (incl. `Some(0)`), omitted for `None`.

### Files modified
- `crates/unimatrix-server/src/mcp/response/mutations.rs`
  - `format_status_change`: new `edges_removed: Option<u64>` param positioned BEFORE `format`.
    - Summary: appends ` | {n} edges removed` for `Some(n)`; line byte-identical for `None`.
    - Markdown: adds `**Edges removed:** {n}\n` line for `Some(n)`; no line for `None`.
    - Json: `obj` kept as a mutable `serde_json::Value`; key inserted ONLY in the `Some` branch
      so `None` OMITS the key (not `null`) — the `reason` field still serializes directly.
  - `format_deprecate_success`: gained `edges_removed: Option<u64>`, forwards it.
  - `format_quarantine_success` / `format_restore_success`: pass `None` (delete no edges); public
    arity unchanged.
- `crates/unimatrix-server/src/mcp/response/mod.rs`
  - Updated the 6 existing `format_deprecate_success` / 3 existing `format_status_change` call sites
    (the intended arity-break tripwires) to the new signature.
  - Added 13 crt-058 tests (see below).

## Tests
- `cargo test -p unimatrix-server --lib response`: **359 passed, 0 failed** (includes the 13 new tests).
- New tests (all passing):
  - Some(n) n>0 per-format matrix: `..._some_n_summary_renders_count_value`,
    `..._some_n_markdown_renders_count_line`, `..._some_n_json_field_parses_integer_3`
    (Json PARSES the integer field, not a substring).
  - AC-05 Some(0) literal `0`: `..._some_zero_summary/markdown/json_field_is_integer_0`,
    plus `..._some_zero_distinct_from_none_all_formats` (AC-05 vs AC-06 discriminator).
  - AC-06 None omits: `..._none_summary_line_unchanged`, `..._none_markdown_no_edges_line`,
    `..._none_json_field_absent` (`get("edges_removed").is_none()`).
  - R-05 byte-identity: `test_format_quarantine_success_output_byte_identical_after_signature_change`,
    `test_format_restore_success_output_byte_identical_after_signature_change`.
  - Structural: `test_edges_removed_param_position_before_format`.
- Component files pass `cargo fmt --check` (edition 2024) and are <500 lines.

## Issues / Flags
- **Adjacent breakage (EXPECTED, out of scope — Wave B deprecate-handler owns it):**
  `tools.rs:1443` (step-5 idempotent early return → pass `None`) and `tools.rs:1478` (step-8 →
  pass `Some(count)`/`None` from the eager-delete helper) still call the OLD 3-arg
  `format_deprecate_success` and will not compile until the handler agent threads the new slot.
  These are the intended compile-time tripwires per OVERVIEW §75-82. I did NOT fix them; `tools.rs`
  is left byte-identical to its original.
- **Test-execution note:** because `tools.rs` (Wave B) does not yet compile, I temporarily added
  placeholder `None` args at the two `tools.rs` deprecate call sites ONLY to unblock my component
  test run, then reverted `tools.rs` to byte-identical original. Safe under wave ordering (Wave A
  precedes Wave B — no concurrent owner). The handler agent must still thread the REAL
  `Some(count)`/`None` there.
- **Concurrent-agent fmt (not mine):** `mcp/edge_write_delete_agent_tests.rs` shows `cargo fmt`
  diffs — it belongs to the concurrent eager-delete-helper agent. I did NOT run crate-wide
  `cargo fmt` (would churn that file); I formatted only my two files via `rustfmt --edition 2024`.
- **Pre-existing test:** `test_none_json_byte_identical_to_base_object` in mod.rs is not authored
  by me (pre-existing); left untouched.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-004 (#5461, my binding contract),
  vnc-037 ADR-003 (#5011: "None => key absent byte-identity invariant" — the exact
  insert-into-mutable-Value pattern I applied), rust-dev formatter convention (#307), and the
  Option-omission lesson (#3449). Applied all.
- Stored: nothing novel — the load-bearing gotcha (render `None` as OMITTED key via conditional
  insert on a mutable `serde_json::Value`, never serialize `Option` directly → `null`) is already
  captured by #5461 (ADR-004) and #5011 (vnc-037). Storing again would duplicate/poison recall.
