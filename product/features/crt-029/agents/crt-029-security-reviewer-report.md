# Security Review: crt-029-security-reviewer

## Risk Level: low

## Summary

The crt-029 change adds a recurring background graph inference tick that writes `Supports` edges
via HNSW expansion and rayon-dispatched NLI scoring. The implementation correctly enforces all
critical security constraints identified in the risk-test strategy: the rayon closure is
synchronous CPU-bound only (no tokio runtime access), the tick never writes `Contradicts` edges,
config thresholds are validated with the correct strict inequality, and all SQL uses parameterised
queries with no string interpolation. No secrets, injection vectors, or access control regressions
were found. One low-severity correctness finding is noted (edge counter over-count on stale
pre-filter).

---

## Findings

### Finding 1 — R-09 Rayon/Tokio Boundary (PASS)

- **Severity**: n/a (constraint verified satisfied)
- **Location**: `nli_detection_tick.rs:233-241` (the rayon closure)
- **Description**: The closure passed to `rayon_pool.spawn(move || {...})` contains only
  `nli_pairs.iter().map(...)` (owned data collection) and `provider_clone.score_batch(&pairs_ref)`
  (sync CPU call). There is no `.await` inside the closure body, no `Handle::current()`, and no
  call to any async function. The `.await` at line 242 is on the rayon pool's join future, which
  executes on the tokio thread — not inside the closure. Comments document the boundary explicitly.
  `grep -n 'Handle::current' nli_detection_tick.rs` returns only documentation comments. The
  pre-merge gate requirement (R-09 / C-14) is satisfied.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 2 — Contradicts Write Absence (PASS)

- **Severity**: n/a (constraint verified satisfied)
- **Location**: `nli_detection_tick.rs` (entire file); `write_inferred_edges_with_cap` signature
- **Description**: `grep -n 'Contradicts' nli_detection_tick.rs` returns only documentation
  comments and one test assertion that confirms *absence* of Contradicts edges. The string literal
  `"Supports"` is hardcoded at line 367. `write_inferred_edges_with_cap` has no
  `contradiction_threshold` parameter. The contradiction score from `NliScores` is read by
  `format_nli_metadata` only for metadata JSON serialisation — it is not evaluated for edge
  writing. `spawn_blocking` is absent from the file entirely. The R-01 / C-13 / AC-10a / AC-19
  gate requirements are satisfied.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 3 — Threshold Validation Ordering and Correctness (PASS)

- **Severity**: n/a (constraint verified satisfied)
- **Location**: `config.rs:665-713`
- **Description**: Validation applies in this order: (1) `supports_candidate_threshold` range
  check (0.0, 1.0) exclusive, (2) `supports_edge_threshold` range check (0.0, 1.0) exclusive,
  (3) cross-field `supports_candidate_threshold >= supports_edge_threshold` → reject (strict `>=`
  per SR-03 / AC-02). The cross-field check uses `>=`, meaning equal values (both 0.7) are
  rejected. This is the correct predicate — the architecture specifies the invariant as "must be
  strictly less than". Tests cover: equal values, candidate > edge, candidate < edge (pass),
  zero and one for each field, and within-range values. All boundary combinations are present.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 4 — `write_nli_edge` Boolean Return Over-counts on INSERT OR IGNORE No-Op (low)

- **Severity**: low
- **Location**: `nli_detection.rs:556-557`, `nli_detection_tick.rs:373`
- **Description**: `write_nli_edge` returns `Ok(_) => true` for any `Ok` result from sqlx,
  including `INSERT OR IGNORE` no-ops where `rows_affected() == 0` (duplicate row, not written).
  The tick's `edges_written` counter increments when `written == true`. In the R-13 scenario
  (stale pre-filter HashSet due to a concurrent post-store NLI write), a pair that was already
  in the DB proceeds to NLI scoring, the `INSERT OR IGNORE` succeeds (no-op), `write_nli_edge`
  returns `true`, and `edges_written` increments as if a new edge was written. This causes the
  cap to be exhausted prematurely — the tick may stop processing earlier than intended when the
  pre-filter is stale.
- **Recommendation**: This is a correctness/metrics issue, not a security vulnerability. The
  `INSERT OR IGNORE` prevents any actual duplicate row — data integrity is maintained. The impact
  is limited to slightly under-utilising the NLI budget in the R-13 (rare concurrent write)
  scenario. This behaviour is pre-existing in `write_nli_edge` (shared with the post-store path)
  and is not introduced by crt-029. Acceptable to carry as a known limitation; suggest a
  follow-up issue to track rows_affected() distinction if accurate cap semantics are needed.
- **Blocking**: no

---

### Finding 5 — SQL Injection Assessment (PASS)

- **Severity**: n/a (no vulnerability found)
- **Location**: `nli_detection.rs:541-554` (`write_nli_edge`), `read.rs:1382-1401`
  (`query_entries_without_edges`), `read.rs:1417-1436` (`query_existing_supports_pairs`)
- **Description**: All three SQL statements use sqlx parameterised binds (`?1`, `?2`, etc.).
  No string interpolation or format!/concat! macros appear in SQL strings. The `relation_type`
  parameter in `write_nli_edge` accepts `&str`, but the tick exclusively passes the literal
  `"Supports"` — this value never originates from external input. `format_nli_metadata` uses
  `serde_json::json!{}` macro (documented inline) to produce the metadata JSON string, preventing
  injection via float serialisation. The two new Store queries are static SQL with no user-
  controlled parameters. No injection surface introduced.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 6 — Input Validation at System Boundary (PASS)

- **Severity**: n/a (no vulnerability found)
- **Location**: `nli_detection_tick.rs:45-51` (function signature), `config.rs:565-713`
- **Description**: `run_graph_inference_tick` accepts no external inputs — all data originates
  from the internal `Store` and `VectorIndex`. Entry content fetched via
  `get_content_via_write_pool` is passed to `score_batch` as text strings; the NLI model treats
  these as opaque inputs. No new MCP tool parameters, file paths, or user-supplied data enter
  through this path. Config fields are validated at startup via `InferenceConfig::validate()`.
  No new trust boundaries are introduced.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 7 — Hardcoded Secrets / Credentials (PASS)

- **Severity**: n/a (no finding)
- **Location**: all changed files
- **Description**: No API keys, passwords, tokens, or credentials appear in any changed file.
  Test fixtures use literal placeholder strings ("test", "hash", "nli") which are not credentials.
- **Recommendation**: No action required.
- **Blocking**: no

---

### Finding 8 — Dependency Safety (PASS)

- **Severity**: n/a (no new dependencies)
- **Description**: No new `[dependencies]` entries appear in any `Cargo.toml` in the diff. The
  change imports only existing crate-internal symbols (`write_nli_edge`, `format_nli_metadata`,
  `current_timestamp_secs` promoted to `pub(crate)`) and existing crate-external types
  (`NliScores`, `EntryRecord`, `Status`, `Store`, `VectorIndex`). No new transitive dependency
  surface introduced.
- **Recommendation**: No action required.
- **Blocking**: no

---

## Blast Radius Assessment

The worst-case failure mode if the fix contains a subtle bug:

1. **Cap miscounting (Finding 4 scenario)**: If the pre-filter HashSet is consistently stale,
   `edges_written` would reach `max_graph_inference_per_tick` faster than actual new edges are
   written, causing the tick to stop early each cycle. Result: slower graph densification, but
   no data corruption, no security impact, and no crash.

2. **Rayon closure R-09 violation (hypothetical)**: If a future edit introduces `.await` inside
   the `rayon_pool.spawn` closure, the server panics with "no current Tokio runtime". The tick
   would crash on every cycle while NLI is enabled. The server would not crash entirely — the
   panic is contained within the rayon join handle, which the tick matches as `Err(e)` and logs
   at `warn`, returning without writing edges. Safe failure mode: no edges written, no data
   corruption. This risk is currently zero per Finding 1.

3. **Threshold validation bypass**: If `validate()` were skipped (not possible via normal config
   load), a config with `supports_candidate_threshold >= supports_edge_threshold` would cause
   every HNSW candidate to immediately pass the edge threshold, writing Supports edges for all
   HNSW neighbours up to the cap. This would over-densify the graph with low-confidence edges,
   degrading search re-ranking quality. No security impact; no data loss. Blocked by Finding 3
   which confirms validation is correct.

---

## Regression Risk

**Existing functionality at risk**: low.

- `nli_detection.rs` changes are three visibility promotions only (`fn` → `pub(crate) fn`).
  These are additive; all existing callers within `nli_detection.rs` continue to work. No logic
  was changed.
- `background.rs` adds one conditional call after `maybe_run_bootstrap_promotion`. The condition
  `if inference_config.nli_enabled` mirrors the existing bootstrap promotion guard. If
  `nli_enabled = false`, the new call is unreachable. No existing tick behaviour changes.
- `config.rs` additions are: four new struct fields with defaults, four validation checks (all
  additive — they only reject previously-invalid-but-unvalidated configs), four default functions,
  four merge_configs arms, and one new ConfigError variant. The struct's `Default` impl is updated
  with explicit values for all four fields. No existing config validation paths changed.
- `read.rs` adds two new public async methods. No existing methods were modified.
- `CLAUDE.md` removes a single bullet (`/uni-query-patterns`) and adds a `context_get` usage
  block. This is documentation-only with no code effect.
- The `InferenceConfig` struct literal trap (R-07, 52 occurrences) was flagged as high likelihood
  pre-implementation. Post-implementation, the Default impl is updated and `..InferenceConfig::default()`
  tail is used in all test constructions visible in the diff. A `cargo check` pass is required to
  confirm all 52 occurrences were updated — this is a compile-time catch, not a security risk.

---

## PR Comments

- Posted 1 comment on PR #420 (see below).
- Blocking findings: no.

---

## Knowledge Stewardship

- Nothing novel to store — the rayon/tokio boundary anti-pattern is already captured in entries
  #3339, #3353 (lesson-learned) and #3660 (grep-gate pattern). The `INSERT OR IGNORE` return
  semantics over-count issue (Finding 4) is a pre-existing shared helper behaviour, not a new
  crt-029 pattern. No generalizable new anti-pattern emerged from this review.
