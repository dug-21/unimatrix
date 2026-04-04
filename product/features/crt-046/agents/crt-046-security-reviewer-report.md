# Security Review: crt-046-security-reviewer

## Risk Level: low

## Summary

The diff introduces behavioral signal emission, goal-cluster persistence, and
goal-conditioned briefing blending. No injection vulnerabilities or access-control
bypasses were found. All SQL writes use positional parameterized queries. The only
finding that warrants a comment (non-blocking) is that three new `InferenceConfig`
fields are not validated in `InferenceConfig::validate()`, diverging from the
established pattern for all other threshold/weight fields. A second non-blocking
finding is an unbounded per-request `store.get()` loop whose blast radius is
latency/DoS, not data corruption. No hardcoded secrets, no unsafe blocks, no new
dependencies.

---

## Findings

### Finding 1 — New `InferenceConfig` fields missing `validate()` range checks

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, `InferenceConfig::validate()` (ends at line 1377)
- **Description**: Three new fields added by crt-046 have documented valid ranges but are
  not validated at startup:
  - `goal_cluster_similarity_threshold`: documented range `(0.0, 1.0]`. A value of `0.0`
    would cause `query_goal_clusters_by_embedding` to pass `threshold = 0.0`, causing
    every cluster row to match regardless of goal relevance — poisoning briefing output
    with unrelated historical clusters. A value greater than `1.0` would suppress all
    matches, silently disabling the feature.
  - `w_goal_cluster_conf`: no range check. A negative value or value `> 1.0` would produce
    negative or extreme `cluster_score` values, distorting the blend ranking. A NaN or
    +Inf config value (possible if TOML is hand-edited with `inf`) would propagate through
    the `cluster_score` formula producing NaN scores, which `partial_cmp` treats as
    `Ordering::Equal` — silently randomising sort order.
  - `w_goal_boost`: same exposure as `w_goal_cluster_conf`.
  - Every other threshold and weight field in `InferenceConfig` — `nli_entailment_threshold`,
    `supports_cosine_threshold`, `ppr_alpha`, `ppr_blend_weight`, etc. — has an explicit
    range check in `validate()`. The new fields are an inconsistency, not a design intent.
- **Recommendation**: Add range checks to `InferenceConfig::validate()` consistent with
  the documented ranges:
  - `goal_cluster_similarity_threshold`: `<= 0.0 || > 1.0` rejects (exclusive lower, inclusive upper).
  - `w_goal_cluster_conf`: `< 0.0 || > 1.0` rejects.
  - `w_goal_boost`: `< 0.0 || > 1.0` rejects.
  - Optionally add an `f32::is_nan() || f32::is_infinite()` guard for all three.
- **Blocking**: no (defaults are safe; internal deployment; startup validation is defense-in-depth).

---

### Finding 2 — Unbounded `store.get()` loop in `context_briefing` hot path

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs`, lines 1150–1201 (cluster entry ID
  collection and fetch loop)
- **Description**: At briefing time, `cluster_entry_ids_raw` is the deduplicated union of
  entry IDs from up to 5 matching cluster rows. Each `goal_clusters` row stores all entry
  IDs accessed across all sessions for a cycle (`all_entry_ids` in `run_step_8b`, step 9),
  which is bounded only by observation count — no cap is enforced before writing to
  `entry_ids_json`. A cycle with many `context_get` calls can produce a cluster row with
  hundreds of IDs. Five such rows at briefing time would emit hundreds of individual
  `store.get(id).await` calls sequentially in the request path. This is a latency concern,
  not a data-integrity concern: the Active-status filter prevents inactive entries from
  appearing; the `blend_cluster_entries` dedup step prevents ID duplication in output.
  The worst case is high-latency briefings for agents with large cycles, not incorrect
  results.
- **Recommendation**: Cap `cluster_entry_ids_raw` after dedup (e.g. to 100 IDs) or cap
  `all_entry_ids` before writing to `entry_ids_json` in `run_step_8b`. This converts the
  latency cliff into a bounded O(1) cost. A follow-up issue is acceptable given the
  observed low risk profile; this is not a security blocker.
- **Blocking**: no.

---

### Finding 3 — `get_latest_cycle_phase` uses `write_pool_server()` for a read query

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/behavioral_signals.rs`, lines 576–598
- **Description**: `get_latest_cycle_phase` issues a `SELECT` query using `write_pool_server()`
  because `read_pool()` is `pub(crate)` and therefore not accessible from `unimatrix-server`.
  This is a pre-existing architectural constraint, not a new vulnerability. The consequence
  is that this read query competes with structural writes for the write pool's limited
  connections (max 2). Under high throughput it could add latency to write operations. This
  pattern is documented in the code comment. No security risk; noted for completeness.
- **Recommendation**: Low priority. Future work could expose a `pub(crate)`-to-server read
  helper, or the method could be moved to `unimatrix-store` as an `impl SqlxStore` method
  using `read_pool()` directly.
- **Blocking**: no.

---

## OWASP Checks

| Check | Finding |
|-------|---------|
| SQL Injection | **CLEAR.** All queries in `behavioral_signals.rs`, `goal_clusters.rs`, and `db.rs` use sqlx positional parameters (`?1`, `?2`, etc.). No string interpolation into SQL. `entry_ids_json` is serialized by `serde_json::to_string` (not interpolated) before binding as a parameter. |
| Broken access control | **CLEAR.** No new trust-boundary crossings. Behavioral edges and goal-clusters are written at `context_cycle_review` time, which is already authenticated/attributed. Briefing blending is gated on `session_state.feature` — no cross-session data leakage. `store.get_by_ids()` Active filter prevents deprecated/quarantined entries from surfacing. |
| Security misconfiguration | **LOW (Finding 1).** New config fields unvalidated; safe defaults make this low risk. |
| Deserialization | **CLEAR.** `decode_goal_embedding` (bincode) is called on data written by the system itself, not from external input. Malformed BLOBs are caught and treated as `Ok(None)` — no panic path. `serde_json::from_str::<Vec<u64>>` for `entry_ids_json` parses to a bounded integer type; parse errors are logged and skipped, not propagated. |
| Input validation | **CLEAR (production paths).** `collect_coaccess_entry_ids` parses `ObservationRow.input` JSON and extracts a `u64` — the integer type prevents injection into downstream SQL. Parse failures are counted, logged, and non-fatal. The `current_goal` emptiness check and `session_state.feature` guard prevent triggering cluster work for unauthenticated or unattributed sessions. |
| Hardcoded secrets | **CLEAR.** No secrets, API keys, tokens, or credentials in the diff. |
| Unsafe Rust | **CLEAR.** No `unsafe` blocks in any new or modified production code. |
| New dependencies | **CLEAR.** No new crate dependencies introduced. All usages are of existing `serde_json`, `sqlx`, `bincode`, and standard library primitives. |
| Error handling / panic paths | **CLEAR.** No `.unwrap()` or `.expect()` in non-test production code. All error paths in step 8b and briefing blending fall through to cold-start or log-and-continue. The one `.expect()` at tools.rs line 2365 (`full_report.expect(...)`) is a logic invariant guard for a developer error — acceptable. |

---

## Blast Radius Assessment

**If Finding 1 (missing validation) manifests via misconfiguration:**
- `goal_cluster_similarity_threshold = 0.0`: every past cycle matches current goal;
  briefing results are flooded with historically-accessed entries regardless of relevance.
  Degraded knowledge quality, not data corruption. Recoverable via config fix.
- `w_goal_cluster_conf = NaN` or `w_goal_boost = NaN`: `cluster_score` becomes NaN for all
  cluster entries; `partial_cmp` returns `Ordering::Equal` for all NaN comparisons;
  sort order is indeterminate. Output is `k=20` entries with undefined ordering but valid
  content. No crash, no data corruption.
- Scope: `context_briefing` responses only. `context_cycle_review`, `context_store`, and
  all other tools are unaffected.

**If Finding 2 (unbounded loop) degrades performance:**
- Each `store.get()` acquires from the read pool (max 6–8 connections). With 500 IDs across
  5 clusters, this is 500 sequential awaits inside a single MCP handler invocation.
  Worst case: briefing takes seconds instead of milliseconds. No panic, no incorrect data.
  Other in-flight requests compete for the read pool but are not blocked — only slowed.

**Worst-case overall**: latency degradation on `context_briefing` for agents with large
historical cycles; no data corruption, no privilege escalation, no information disclosure
beyond what briefing already returns.

---

## Regression Risk

**Low.** The cold-start fallback path (no feature attribution, NULL embedding, empty
`goal_clusters` table, below-threshold similarity) falls through to `briefing.index()`
unchanged. The architecture document and code comments confirm this is the intended
path for all pre-v22 sessions and fresh deployments. The two-level guard structure
(level 1: feature absent; level 2: embedding absent) is correctly implemented.

Existing tests that do not set `session_state.feature` will not trigger any new code
paths, preventing false regressions. The memoisation-gate deferral (memo_hit is now
populated but the early return is deferred until after step 8b) is a refactor of the
existing `context_cycle_review` flow. The logic invariant is that `full_report.expect()`
at line 2365 will panic only on a developer error (memo_hit = None AND full_report = None),
not in any valid runtime path.

Migration v21 → v22 is additive: only a new table and index are added. No existing
columns are modified. Rollback to v21 code on a v22 database would encounter "table
not found" errors only for the new `insert_goal_cluster` call — which is non-fatal per
the error handling design.

---

## PR Comments

- Posted 1 comment on PR #512 (see below).
- Blocking findings: no.

---

## Knowledge Stewardship

Nothing novel to store — the missing config validation finding is specific to this PR's
fields and the recommendation is the same pattern applied to every prior threshold field
in `InferenceConfig`. No generalizable anti-pattern beyond what the existing code already
demonstrates by example.
