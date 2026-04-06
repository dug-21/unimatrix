# Agent Report: 528-agent-2-verify

**Agent ID:** 528-agent-2-verify
**Phase:** Test Execution (Bug Fix Verification)
**Bug Fix Area:** co_access promotion tick — denylist → allowlist for deprecated endpoint filtering

---

## Unit Tests

### Bug-Specific Test

`test_deprecated_endpoint_pair_not_promoted` (co_access_promotion_tick_tests.rs):

- Result: PASS
- Validates both assertions:
  - SECONDARY: A↔D edges absent from graph_edges (deprecated endpoint excluded)
  - PRIMARY: A↔B weight = 1.0, not 0.5 — subquery max_count not inflated by deprecated pairs

### Full Workspace

```
cargo test --workspace
```

All suites passed with no failures:

| Crate / Suite | Tests | Result |
|---------------|-------|--------|
| unimatrix-store | 423 | PASS |
| unimatrix-server | 2769 | PASS |
| unimatrix-core | 368 | PASS |
| unimatrix-embed | 128 | PASS |
| unimatrix-vector | 47 | PASS |
| migration integration | 16+16 | PASS |
| pipeline e2e | 7 | PASS |
| (other suites) | various | PASS |

Total: all passed, 0 failed.

---

## Clippy

```
cargo clippy --workspace -- -D warnings
```

**Result:** Pre-existing errors only — none in changed files.

Errors found:
- `crates/unimatrix-engine/src/auth.rs` — `collapsible_if` (2 instances)
- Other `collapsible_if` errors in `unimatrix-engine` crate

**Verdict:** All clippy errors are pre-existing in `unimatrix-engine`, which was last touched in crt-014. The changed files (`co_access_promotion_tick.rs`, `co_access_promotion_tick_tests.rs`, `typed_graph.rs`) produced zero clippy warnings.

No GH Issue filed for pre-existing clippy errors — they pre-date this bugfix and are outside its scope.

---

## Integration Tests

### Smoke Gate (MANDATORY — PASS)

```
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60
```

**Result: 22 passed, 0 failed** in 191s. All smoke tests green.

### Lifecycle Suite (co_access/graph area)

```
python -m pytest suites/test_lifecycle.py -v --timeout=60
```

**Result: 44 passed, 5 xfailed, 2 xpassed** in 525s.

Pre-existing xfail/xpass statuses (none caused by this fix):

| Test | Status | Reason |
|------|--------|--------|
| `test_auto_quarantine_after_consecutive_bad_ticks` | XFAIL | Requires UNIMATRIX_TICK_INTERVAL_SECONDS env var — tick-timing test |
| `test_context_status_supports_edge_count_increases_after_tick` | XFAIL | MCP-visible tick validation needs fast_tick_server fixture |
| `test_s1_edges_visible_in_status_after_tick` | XFAIL | Tick timeout — needs short tick interval in CI |
| `test_inferred_edge_count_unchanged_by_s1_s2_s8` | XFAIL | Tick timeout — AC-30/R-13 backward compat |
| `test_search_multihop_injects_terminal_active` | XPASS | Pre-existing — search injection stops at first hop (not caused by col-028) |
| `test_inferred_edge_count_unchanged_by_cosine_supports` | XPASS | Pre-existing — AC-15/NFR-06 after Path C writes |

All 5 xfails have existing markers with documented reasons. Both xpasses are pre-existing. No new failures, no GH Issues filed.

---

## Summary

| Check | Result |
|-------|--------|
| Bug-specific test (`test_deprecated_endpoint_pair_not_promoted`) | PASS |
| Full workspace unit tests | ALL PASS |
| Clippy (changed files) | CLEAN |
| Clippy (pre-existing in unimatrix-engine) | Pre-existing — not caused by this fix |
| Integration smoke gate | 22/22 PASS |
| Integration lifecycle suite | 44 passed, 5 xfailed (pre-existing), 2 xpassed (pre-existing) |

**Verdict: PASS. The bug fix is verified. No regressions introduced.**

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #3882, #4162 (directly relevant), #3979, #3822
- Checked: entry #4162 already captures the dual-assertion test design insight (seed deprecated pair with higher count to make subquery filter failure detectable via weight < 1.0). No new lesson to store — the developer agent wrote a comprehensive lesson before this verification pass.
- Stored: nothing novel to store — entry #4162 is complete and covers the full pattern including all four test cases required per entry #4156.
