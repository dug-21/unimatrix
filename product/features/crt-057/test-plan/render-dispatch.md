# Test Plan — Render Dispatch (`"summary"` arm dropped ×4)

**File:** `unimatrix-server/src/mcp/tools.rs` render loci `:2532`, `:3359`, `:4268`, `:4324`
**Risks:** R-12 (Med) · **ACs:** AC-11

> `"summary"` is DROPPED (not folded) → `ERROR_INVALID_PARAMS` at ALL FOUR render loci. Breaking for any live
> `format:"summary"` caller — the R-12 consumer sweep below is mandatory before ship.

---

## R-12 — render divergence / `"summary"` drop (AC-11)
- `test_markdown_json_content_equivalence` — the same cycle rendered `markdown` vs `json` → **semantic
  content equality** (serialization differs only); buffer intact after both; NO candidates on either (both
  are non-`transcript` paths). (R-12 sc.1.)
- `test_summary_returns_error_invalid_params_exact_message` — `format:"summary"` → `ERROR_INVALID_PARAMS`
  with the exact message `Unknown format '…'. Valid values: "markdown", "json".` (R-12 sc.2.)
- `test_summary_arm_removed_at_all_four_loci` — assert no `"summary"` arm survives at any of the four
  dispatch loci (source-assertion over the four render sites); no third divergent render path. (R-12 sc.2.)
- `test_unknown_format_returns_error_invalid_params` — any unknown value (not just `"summary"`) →
  `ERROR_INVALID_PARAMS`.

## R-12 — consumer sweep (grep guard; ties `consumer-reconciliation.md`)
- `test_no_reconciled_consumer_emits_format_summary` — sweep the reconciled consumers (`uni-retro/SKILL.md`,
  tool description) for a live `format:"summary"` caller; flag any survivor to the delivery leader. If one
  surfaces, reconsider fold-to-markdown (the non-breaking delegated option). (R-12 sc.3.)

## Integration anchor
`suites/test_tools.py::test_cycle_review_format_summary_invalid_params` (OVERVIEW §6c) — the MCP-visible
error path with the exact message.
