# Security Review: crt-040-security-reviewer

## Risk Level: low

## Summary

crt-040 adds a pure-cosine Supports edge detection path (Path C) inside the background
`run_graph_inference_tick`. The change introduces one new config field with proper range
validation, one new parameterized DB write helper using sqlx bindings, one string constant,
and removes a dead config field. All inputs to the write path are typed Rust values, not
user-supplied strings. No injection surface exists. No new dependencies. No secrets.

One behavioral asymmetry between `write_nli_edge` and `write_graph_edge` is noted as a
low-severity informational finding — it is pre-existing in `write_nli_edge` and does not
affect correctness in the new code. One metadata format construction point is flagged as
informational for future review. Neither finding is blocking.

---

## Findings

### F-01: write_nli_edge returns true on INSERT OR IGNORE regardless of rows_affected (informational, pre-existing)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection.rs:49-51`
- **Description**: `write_nli_edge` matches `Ok(_) => true`, meaning it returns `true`
  even when INSERT OR IGNORE silently discarded a duplicate row (`rows_affected = 0`). The
  new `write_graph_edge` correctly returns `query_result.rows_affected() > 0` — a stricter
  and more accurate signal. This asymmetry is pre-existing in `write_nli_edge` and is NOT
  introduced by this PR. The new code is actually better. Callers of `write_nli_edge` that
  treat `true` as "edge exists or was written" remain correct. No security impact; noted for
  future cleanup.
- **Recommendation**: Consider aligning `write_nli_edge` return semantics to match
  `write_graph_edge` in a follow-on PR. Not blocking this change.
- **Blocking**: no

### F-02: metadata JSON constructed via format! macro from typed f32 — not sanitized external input (informational)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:833`
  ```
  let metadata_json = format!(r#"{{"cosine":{cosine}}}"#);
  ```
- **Description**: The metadata string is constructed inline using Rust string interpolation
  of a typed `f32` value (`cosine`) that came from HNSW (internal, not user-supplied). This
  is NOT a user-controlled input and cannot be used for SQL injection because it is passed
  as a parameterized bind value (`?7`) to sqlx — not interpolated into the SQL string. The
  guard `!cosine.is_finite()` runs before this line, ensuring NaN/Inf are rejected before
  `format!` is called. A NaN `f32` would format to `NaN` (valid Rust output) but is blocked
  earlier. The only injection-adjacent risk would be if `cosine` somehow contained a
  non-finite value, which the guard prevents.
  
  The risk-test strategy's RISK-STRATEGY.md (Security section) already identified this path
  and correctly concluded it has no injection risk. This finding is informational — flagging
  the pattern for awareness, not as a defect.
- **Recommendation**: Consider using `serde_json::json!` for metadata construction to make
  the JSON validity guarantee explicit (as `format_nli_metadata` does for Path B). Not
  required for security correctness given the typed f32 input and parameterized bind, but
  would be consistent with existing pattern.
- **Blocking**: no

### F-03: supports_cosine_threshold at low values causes near-universal Supports edge flooding (known, validated)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs` — `validate()` method
- **Description**: An operator setting `supports_cosine_threshold = 0.001` would cause Path
  C to write Supports edges for nearly all candidate pairs each tick, potentially flooding
  the graph. The range validation `(0.0, 1.0)` exclusive prevents the absolute extremes but
  does not guard against operationally dangerous low values. This is an accepted
  misconfiguration risk identified in RISK-TEST-STRATEGY.md (Security section). Recovery is
  `context_status(maintain=true)`. No data leaves the system; no privilege escalation.
- **Recommendation**: Document the operational risk in the config field doc comment (already
  done — the doc comment mentions IR-02 and the operator invariant). No code change needed.
- **Blocking**: no

---

## OWASP Assessment

| OWASP Concern | Assessment |
|---------------|------------|
| **Injection (SQL)** | Not present. All DB writes use sqlx parameterized queries (`?1`...`?7`). No string interpolation into SQL. `source` parameter is bound, not concatenated. |
| **Injection (format string / command)** | Not present. `format!` at line 833 uses a typed `f32` internal value. No shell commands. |
| **Broken access control** | Not applicable. Path C is a background tick writing to internal DB tables. No MCP-level trust gate exists for tick internals (consistent with existing path architecture). |
| **Security misconfiguration** | Low risk via `supports_cosine_threshold`. Validated by `validate()`. Documented. |
| **Deserialization** | Not applicable to this change. No new deserialization of untrusted data. Config deserialization (TOML) is operator-controlled. The new field includes forward-compat serde test (AC-18) confirming `deny_unknown_fields` is not active. |
| **Vulnerable components** | No new crate dependencies introduced. |
| **Data integrity** | INSERT OR IGNORE with UNIQUE constraint is the dedup backstop. Budget counter increments only on `rows_affected = 1`. Intra-tick reversed pairs `(A,B)/(B,A)` are covered by the UNIQUE constraint (not the pre-filter). Tested by TC-17. |
| **Secrets / credentials** | No hardcoded secrets, tokens, or API keys in the diff. |
| **Path traversal** | Not applicable to this change. |

---

## Blast Radius Assessment

**If Path C has a subtle bug, the worst case is:**

1. **Graph flooding** — if the threshold comparison uses `>` instead of `>=`, boundary pairs
   at exactly 0.65 are silently missed. Consequence: slightly fewer Supports edges than
   expected. Not a security issue. Covered by TC-03 (boundary test added).

2. **Budget counter off-by-one** — if `false` returns (UNIQUE conflicts) incremented the
   budget counter, the tick would exhaust its budget prematurely. Consequence: fewer edges
   written per tick, feature degrades silently. Tested by TC-08.

3. **Wrong source tag** — if `EDGE_SOURCE_COSINE_SUPPORTS` were accidentally swapped for a
   different constant, GNN training signal labeling would be corrupted silently. Covered by
   direct assertion tests in TC-01/TC-02 (regression guard on `write_nli_edge` source).

4. **Tick infallibility broken** — if a `?` or `unwrap()` were introduced, a tick panic
   could kill the background worker. Grep confirms zero `unwrap()` calls in the new Path C
   code. All match branches return `()` or `continue`.

**Worst case in all scenarios**: degraded knowledge graph quality (fewer or mislabeled
edges) within the Unimatrix internal store. No data is exposed outside the system. No
privilege escalation. No denial of service to external callers. Recovery is graph
compaction via `context_status(maintain=true)`.

---

## Regression Risk

**Low.** The change is additive:

- `write_nli_edge` is NOT modified. Existing Path A and Path B callers are unaffected.
  Verified by TC-02 (regression guard asserting source='nli').
- `nli_post_store_k` removal is a dead-field cleanup. Consumer was deleted in crt-038.
  Forward-compat serde test (AC-18/TC-11) confirms existing configs with the old field
  continue to deserialize without error.
- The joint early-return (`if candidate_pairs.is_empty() && informs_metadata.is_empty()`)
  was removed to allow Path C's observability log to fire unconditionally. This is correct:
  Path A and Path C loop zero times on empty inputs — no write overhead.
- The `category_map` HashMap is built from `all_active` at line 440-443 before Path C is
  called. It was already being built for the Phase 5 sort; the reference is passed into
  `run_cosine_supports_path`. No redundant DB read introduced.

**One behavioral change** worth noting: the early-return removal means ticks that previously
returned immediately when both candidate lists were empty will now execute the full Path C
function body (loop over empty vec, emit one debug log) and then hit the `candidate_pairs.is_empty()`
check at Path B. This is a no-op write-wise but slightly increases tick execution cost for
empty-candidate ticks. Not a security concern; acceptable engineering tradeoff for
observability.

---

## Dependency Safety

No new crate dependencies. The diff adds no `Cargo.toml` changes. Existing sqlx, serde,
and tokio dependencies are unchanged.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff. Confirmed by inspection
of all added lines.

---

## Minimal Change Verification

The diff is minimal and scoped to the feature:

- `config.rs`: add field, add backing fn, add validation, add merge entry, remove dead field.
  All within the existing `InferenceConfig` pattern.
- `nli_detection.rs`: add `write_graph_edge` sibling function. `write_nli_edge` is unchanged.
- `nli_detection_tick.rs`: add `run_cosine_supports_path` private helper and constant.
  Import of `write_graph_edge` added. Remove joint early-return (required for AC-19).
- `read.rs` / `lib.rs`: add one constant and re-export.
- `product/features/crt-040/`: doc files only.

No changes unrelated to crt-040 scope detected.

---

## PR Comments

- Posted 1 comment on PR #490 (general findings summary, non-blocking).
- Blocking findings: no.

---

## Knowledge Stewardship

- Searched: `graph edge write security injection trust boundary sqlx parameterized`
- Searched: `config validation input range check attack surface`
- Stored: nothing novel to store — the parameterized sqlx write pattern and config range
  validation pattern are established in this codebase. The `write_nli_edge` return-value
  asymmetry (F-01) is pre-existing and feature-specific. The `format!` metadata pattern
  (F-02) is a single occurrence. Neither rises to a generalizable cross-feature anti-pattern
  requiring a lesson entry at this time.
