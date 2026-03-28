# Security Review: bugfix-439-security-reviewer

## Risk Level: low

## Summary

PR #441 adds a single `tracing::debug!` call that logs NLI score distribution statistics (max, mean, p75 of entailment scores, threshold value, and pair count) after the rayon NLI scoring barrier in `run_graph_inference_tick`. A private helper function `nli_score_stats` computes these stats. The change is additive-only: no inputs change, no access control changes, no new deserialization paths, no new dependencies, and no secrets. The fix is minimal (one file, 79 lines added) and correctly positioned. Two informational findings are noted; neither is blocking.

---

## Findings

### Finding 1: Debug log emits aggregate statistics, not raw entry content — no information disclosure risk
- **Severity**: informational / non-finding
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:278-285`
- **Description**: The `tracing::debug!` call emits five scalar fields: `nli_score_max`, `nli_score_mean`, `nli_score_p75` (all f32 aggregate statistics computed from the NliScores entailment field), `threshold` (a config value), and `pairs` (a count). None of these fields are entry IDs, entry content, titles, or user-controlled strings. The log message is a fixed string literal. There is no path by which untrusted input reaches the log call. This is assessed as a non-finding.
- **Recommendation**: None. The logging pattern is correct.
- **Blocking**: no

### Finding 2: NaN propagation in nli_score_stats — theoretical, bounded failure mode
- **Severity**: low / informational
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:373-385`
- **Description**: `nli_score_stats` sorts using `partial_cmp(b).unwrap_or(Ordering::Equal)` to handle NaN. If NaN values appear in `NliScores::entailment` (e.g., from a malformed ONNX model output), the sort is stable but the computed max, mean, and p75 would contain NaN. NaN propagates into the debug log fields harmlessly (the tracing crate handles NaN f32 values). However, if NaN values were to propagate through to `write_inferred_edges_with_cap`, the `>` threshold comparison (`entailment > supports_edge_threshold`) would evaluate to `false` for NaN, meaning no edges would be written — a safe silent no-op, not a crash or data corruption. This failure mode pre-exists the fix; `nli_score_stats` does not introduce it. The fix neither adds nor removes NaN handling at the write boundary.
- **Recommendation**: Track as a low-priority enhancement to add a NaN guard at the ONNX output boundary in `cross_encoder.rs` (the `score_batch` implementation). This is not introduced by this PR.
- **Blocking**: no

### Finding 3: 500-line file limit exceeded — pre-existing, not introduced
- **Severity**: informational / out-of-scope
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs` (948 lines)
- **Description**: The file exceeds the project's 500-line limit. This is a pre-existing condition (869 lines before this PR, per the gate report). The fix adds 79 lines (10% growth). The oversized file is not a security concern, but it increases the maintenance burden of future security review. This is correctly flagged in the gate report as out-of-scope for this PR.
- **Recommendation**: Open a follow-up issue to split the file at a natural module boundary (e.g., extract `select_source_candidates` and stat helpers into a sub-module). Not blocking for this minimal observability fix.
- **Blocking**: no

---

## OWASP Evaluation

| Check | Verdict |
|-------|---------|
| Injection (SQL, command, path) | Not applicable — no format strings with user data; log message is a string literal with scalar config/stats fields |
| Broken access control | Not applicable — no permission checks changed; `run_graph_inference_tick` is a background service function |
| Security misconfiguration | Not applicable — no config defaults changed; `supports_edge_threshold` is read, not written |
| Vulnerable components | Not applicable — no new crate dependencies introduced |
| Data integrity failures | Not applicable — the added code is read-only observation; no write path modified |
| Deserialization risks | Not applicable — no new deserialization introduced |
| Input validation gaps | Not applicable — no new inputs from external sources; NliScores is produced internally by ONNX inference |
| Secrets / credentials | None present in diff or log fields |
| Unsafe code | None — confirmed via gate report and independent diff review |
| Information disclosure via logs | Non-finding — log fields are aggregate f32 stats + config value + count; no entry content, IDs, or user strings |

---

## Blast Radius Assessment

The change is purely observational. The worst realistic failure mode is:

1. **NaN in stats fields** — if the ONNX model returns NaN for entailment scores, `nli_score_stats` produces NaN stats that appear in the debug log. The log consumer (tracing subscriber) handles this without panic. No data is written incorrectly because `write_inferred_edges_with_cap` already has a threshold comparison that evaluates to false for NaN. Failure mode: silent no-op with a potentially confusing debug log line.

2. **Index out of bounds** — `nli_score_stats` has an explicit `is_empty()` guard returning `(0.0, 0.0, 0.0)` before any indexing. The `vals[n-1]` and `vals[p75_idx]` accesses are both protected by the non-empty guard and the `.min(n - 1)` clamp. No panic path exists in this code.

3. **Performance** — `nli_score_stats` allocates a Vec<f32> of the same length as the NLI results batch and sorts it. The batch is already capped by `config.max_graph_inference_per_tick`, so this is O(k log k) where k is bounded. No performance blast radius.

The debug log call runs only when the tracing subscriber is at DEBUG level, meaning in production (typically INFO or WARN) the call is elided at compile time by the tracing macro. No runtime cost in production.

**Worst-case blast radius**: None beyond a potentially confusing NaN in a debug log line that only appears when DEBUG logging is enabled.

---

## Regression Risk

Minimal. The fix is purely additive:

- No existing function signatures changed.
- `run_graph_inference_tick` returns `()` — the added lines between the length-mismatch guard and `write_pairs` construction do not touch any data that flows to the write path.
- `nli_score_stats` is a private function (`fn`, not `pub`). It cannot be called from outside this module.
- The three new unit tests cover the only boundary conditions for the helper (empty, single, multi-element).

Existing tests: 2273 unit tests and 13/13 contradiction suite integration tests pass (per gate and verify reports). The contradiction suite exercises the NLI inference tick end-to-end, confirming no regression in the write path.

---

## Dependency Safety

No new crate dependencies introduced. The `tracing` crate is already a transitive dependency throughout the workspace (it is used on every other line in this file). `NliScores` and `f32` arithmetic are std. No known CVEs introduced.

---

## Minimal Change Verification

One file changed (`nli_detection_tick.rs`). All 79 added lines are:
- The 11-line debug log block (the fix itself)
- The 15-line `nli_score_stats` helper with doc comment
- The 53-line test module addition (3 tests)

No unrelated changes in the fix commit (222a276). The branch also carries commits from bugfix-436, but those are separate and were reviewed under PR #440.

---

## PR Comments

- Posting 1 non-blocking comment on PR #441
- Blocking findings: no

---

## Knowledge Stewardship

- Searched: Unimatrix for tracing/logging/NLI security patterns — no established patterns exist for this concern in this codebase; results were about fire-and-forget recording and audit log ADRs (not applicable).
- Stored: nothing novel to store — the finding that "debug logs of aggregate f32 stats carry no information disclosure risk" is too narrow to generalize. The NaN propagation observation (Finding 2) is a pre-existing concern at the ONNX boundary, not novel to this PR.
