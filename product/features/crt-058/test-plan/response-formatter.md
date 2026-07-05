# Test Plan — response-formatter (`mcp/response/mutations.rs`)

**Unit under test:** `format_status_change` gains `edges_removed: Option<u64>` (BEFORE `format`); `format_deprecate_success` gains + forwards it; `format_quarantine_success` / `format_restore_success` pass `None`.

**Per-format contract (ADR-004):**
| Format | `Some(n)` | `Some(0)` | `None` |
|--------|-----------|-----------|--------|
| Summary | append ` \| {n} edges removed` | ` \| 0 edges removed` | line unchanged |
| Markdown | line `**Edges removed:** {n}` | `**Edges removed:** 0` | no line |
| Json | field `"edges_removed": n` | `"edges_removed": 0` | field absent |

**Placement:** extend `mcp/response/mod.rs` `mod tests` (`:209`, existing `format_deprecate_success` cases). Assertions parse/compare rendered values — never call-count or bare substring (R-05, #5427).

## Test Expectations

### AC-04(b) / AC-02 / FR-03 — per-format count matrix, `Some(n)` n>0 — **R-05**
- `test_format_deprecate_some_n_summary_renders_count_value` — `edges_removed = Some(3)`, Summary; assert the rendered line contains the value `3` in the advisory slot (assert the value, not merely that text exists).
- `test_format_deprecate_some_n_markdown_renders_count_line` — Markdown; assert a `**Edges removed:** 3` line with value `3`.
- `test_format_deprecate_some_n_json_field_parses_integer_3` — Json; `serde_json::from_str` the payload, assert `parsed["edges_removed"] == 3` as an INTEGER (not a substring match). This catches a format that drops the threaded argument and ships green.

### AC-05 / NFR-04 — zero case `Some(0)` renders literal `0` — **R-04 (RESOLVED)**
- `test_format_deprecate_some_zero_summary_renders_literal_0`
- `test_format_deprecate_some_zero_markdown_renders_literal_0`
- `test_format_deprecate_some_zero_json_field_is_integer_0` — `parsed["edges_removed"] == 0` (integer).
  - All three: advisory RENDERED (not omitted); other deprecation-success fields unchanged. `Some(0)` must be behaviorally DISTINGUISHABLE from `None` (this is the AC-05 vs AC-06 discriminator).

### AC-06 (formatter half) — `None` omits advisory
- `test_format_deprecate_none_summary_line_unchanged` — Summary line byte-identical to pre-feature output.
- `test_format_deprecate_none_markdown_no_edges_line` — no `**Edges removed:**` line present.
- `test_format_deprecate_none_json_field_absent` — `parsed.get("edges_removed").is_none()` (key ABSENT, not `null`).
  - Cross-check: `Some(0)` and `None` produce DIFFERENT output in every format (the two must be behaviorally distinguished — R-04 / AC-05 vs AC-06).

### Quarantine/restore backward-compat — **R-05 / delivery-time closure**
- `test_format_quarantine_success_output_byte_identical_after_signature_change`
- `test_format_restore_success_output_byte_identical_after_signature_change`
  - These call sites pass `None`; assert their Summary/Markdown/Json output is UNCHANGED vs the pre-feature baseline (they render no advisory). Guards the shared-formatter signature ripple — a wrong constant (`Some(0)` instead of `None`) or wrong-position argument at these sites compiles but mis-renders.
- `test_edges_removed_param_position_before_format` — signature/structural: `edges_removed` sits immediately before `format` in `format_status_change`; an `Option` placed after `format` would compile but mis-thread (ADR-004 integration risk).

## Notes for delivery
- The four call sites change in lockstep (Rust arity catches a missed site at compile time — good; but wrong-position/wrong-constant compiles — the byte-identity tests catch it).
- Do NOT assert via call-count or `text.contains("edges")` alone; parse the Json field and compare the rendered value for Summary/Markdown.
