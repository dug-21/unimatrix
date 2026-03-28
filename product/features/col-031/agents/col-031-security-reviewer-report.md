# Security Review: col-031-security-reviewer

## Risk Level: low

## Summary

The col-031 changes introduce a phase-conditioned frequency table built from `query_log`
access history and wire it into the fused scoring pipeline. The implementation is
security-clean: SQL uses exclusively parameterized binding, no unsafe blocks exist (enforced
by `#![forbid(unsafe_code)]` at both crate roots), all RwLock acquisitions use
`.unwrap_or_else(|e| e.into_inner())`, and the SQL time-window query is bounded by a
validated config field. One minor informational finding is documented below; no blocking
findings exist.

---

## Findings

### Finding 1: `current_phase` in MCP path is always `None` (informational)

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:357`
- **Description**: The MCP `context_search` handler explicitly sets
  `current_phase: None` with a comment "phase not yet threaded from tool params".
  This means the `w_phase_explicit = 0.05` term is silently inactive on the MCP
  path post-merge. Only the UDS path and the eval runner (AC-16 fix) supply
  `current_phase`. This is intentional per the implementation brief (col-031
  activates the term; full MCP wiring is a follow-up), but a reviewer reading
  the scoring weight change in isolation might expect the feature to be live on
  all paths. No security risk — the fallback sets `phase_explicit_norm = 0.0`,
  which is score-identical to pre-col-031 behavior (NFR-04).
- **Recommendation**: Add a TODO comment at the `None` site referencing the
  follow-up ticket for MCP phase threading, to prevent the omission from being
  mistaken for a bug in a future review.
- **Blocking**: no

### Finding 2: `i64 as u64` cast for `entry_id` (informational)

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/query_log.rs:264`
- **Description**: `entry_id` is read from SQLite as `i64` (correct for sqlx 0.8)
  and cast to `u64` via `as u64`. SQLite INTEGER PRIMARY KEY AUTOINCREMENT starts
  at 1 and SQLite guarantees it is non-negative for well-formed rows. The JOIN
  `ON CAST(je.value AS INTEGER) = e.id` filters to only rows that match a live
  entry, so a hypothetical negative value in `result_entry_ids` JSON would produce
  no match and be excluded. The cast is therefore safe by schema invariant, and
  is consistent with the same pattern used in `row_to_query_log` (line 278). No
  security concern; documented for traceability.
- **Recommendation**: None required. The existing doc comment on
  `row_to_phase_freq_row` explains the rationale clearly.
- **Blocking**: no

### Finding 3: Dynamic SQL in `scan_query_log_by_sessions` (pre-existing, informational)

- **Severity**: low (informational, pre-existing)
- **Location**: `crates/unimatrix-store/src/query_log.rs:130-148`
- **Description**: The pre-existing `scan_query_log_by_sessions` method constructs
  SQL with a dynamically-sized `IN (?, ?, ...)` placeholder string. The placeholder
  indices (`?1`, `?2`, ...) are generated from the loop index `i+1` — not from
  user input — and each session ID value is then bound via sqlx parameterized
  binding. This is not a SQL injection vector. However, this pattern was not
  introduced by col-031 (it pre-dates this PR). Noted here for completeness since
  this is the file where col-031 adds new code.
- **Recommendation**: No action needed for col-031. A future cleanup could switch
  this to sqlx's `query_builder` to avoid manual placeholder construction, but
  the current implementation is safe.
- **Blocking**: no

---

## Focused Security Checks

### SQL Injection — PASS

`query_phase_freq_table` uses a static SQL string with a single `?1` placeholder.
`lookback_days` (a `u32` from a validated config field) is cast to `i64` and bound
via sqlx parameterized API. There is no string interpolation of any user-controlled
value into the SQL. Phase strings from `query_log.phase` are read from the database
and used only as HashMap keys in Rust memory — never interpolated back into SQL.

### Integer Overflow — PASS

The rank formula `1.0_f32 - ((rank - 1) as f32 / n as f32)`:
- `rank` is `idx + 1` where `idx` comes from `.enumerate()` on a `Vec` — bounded
  by `Vec::len()` which is bounded by the number of rows returned from a database
  query within a time window.
- `n` is `bucket_rows.len()` — same bound.
- `rank - 1` where `rank = idx + 1` and `idx >= 0` means `rank >= 1`, so
  `rank - 1 >= 0` — no underflow.
- Both values cast to `f32`. Maximum `usize` representable in `f32` without
  precision loss is 2^24 (~16.7M). A bucket with more than 16.7M distinct entries
  would lose precision in the rank score but would not overflow — the result remains
  a valid `f32` in `[0.0, 1.0]`. In practice, the number of entries per
  (phase, category) bucket is bounded by the size of the knowledge base; no
  overflow risk.
- `lookback_days as i64`: `u32::MAX` = 4,294,967,295 which fits in `i64`; no
  overflow. Config validation enforces `[1, 3650]` at startup before this cast
  is ever reached at runtime, making this a belt-and-suspenders safe operation.

### RwLock Poison Safety — PASS

Every `RwLock` acquisition in the new and modified code uses
`.unwrap_or_else(|e| e.into_inner())`:
- `phase_freq_table.rs`: lines 270, 278, 286, 294, 303, 308 (test code);
  `new_handle()` wraps in `Arc::new(RwLock::new(...))` — no acquisition.
- `background.rs`: line 607 — write lock on rebuild success path.
- `search.rs`: line 836 — read lock pre-loop.
- No bare `.unwrap()` or `.expect()` on any `RwLock` acquisition site in
  col-031 changes.

The poison recovery pattern is consistent with `TypedGraphState`,
`EffectivenessState`, and `CategoryAllowlist` conventions across the codebase.

### Denial of Service — PASS (bounded)

The `query_phase_freq_table` SQL scans `query_log` within a time window:
`WHERE q.ts > strftime('%s','now') - ?1 * 86400`. The bound is `lookback_days`,
validated to `[1, 3650]` by `InferenceConfig::validate()` at server startup.
A misconfigured value of 0 or >3650 causes startup to abort — it never reaches
the query. The maximum window is 10 years (3,650 days), which is a large but
finite scan. The query runs in a `tokio::spawn` with `TICK_TIMEOUT` wrapping
it; a slow query produces a timeout error and existing state is retained
(retain-on-error semantics). No unbounded result set is possible.

The `json_each` expansion is bounded by the size of `result_entry_ids` per row,
which is limited to the top-k results stored at query time. The JOIN with
`entries` further constrains results to live entries. No DoS vector.

### Secrets / Hardcoded Keys — PASS

No secrets, API keys, tokens, or credentials appear in any diff hunk. No
hardcoded connection strings. Config is loaded from TOML files via the existing
validated config pipeline.

### Unsafe Blocks — PASS

`#![forbid(unsafe_code)]` is present in both `crates/unimatrix-server/src/lib.rs`
and `crates/unimatrix-store/src/lib.rs`. Grep of all col-031 changed files
confirms zero `unsafe` keywords in new code. The references to `unsafe` in
`background.rs` are comments explaining why certain patterns are avoided, not
actual `unsafe` blocks.

### Access Control / Trust Boundaries — PASS

`PhaseFreqTable` is internal state only (no MCP tool, no external API). The
`phase_affinity_score` method accepts `entry_id`, `entry_category`, and `phase`
from Rust-typed sources only — the scoring loop in `search.rs` supplies these
from already-fetched database records, not from user-provided strings directly.
Phase strings from `ServiceSearchParams.current_phase` are used only as HashMap
lookup keys — they are never executed, interpolated, or logged in a way that
could leak internal state.

### Deserialization — PASS

`PhaseFreqRow` is deserialized from trusted SQLite row data via sqlx's
`try_get::<T, _>()` API with explicit type annotations. No external
deserialization of untrusted data is introduced. The `serde` attributes on
`InferenceConfig` handle TOML deserialization from operator-controlled config
files (already trusted by the existing config pipeline).

### Error Handling — PASS

Errors from `PhaseFreqTable::rebuild` are handled without leaking internal state:
the error is logged via `tracing::error!` with the error value (using the `Display`
impl, not `Debug`), and existing state is retained. The error path in
`run_single_tick` does not panic. No `process::exit` calls in new code.

---

## Blast Radius Assessment

Worst case if `PhaseFreqTable::rebuild` has a subtle bug:

1. **Silent suppression** (most likely): Incorrect rank scores in `[0.0, 1.0]`
   would shift search result ordering. With `w_phase_explicit = 0.05`, the maximum
   impact on any single entry's fused score is 0.05 (5 percentage points). The
   pre-existing similarity, confidence, co-access, and effectiveness terms
   collectively contribute 0.95+, bounding the distortion to a minor reranking
   effect. No data corruption.

2. **Complete rebuild failure**: Returns `Err(e)`; existing state retained;
   `use_fallback = true` (or whatever the last good state was). The search path
   sees `phase_explicit_norm = 0.0` for all candidates — identical to pre-col-031
   behavior. Safe failure mode.

3. **Lock contention**: The read lock in the search hot path is held only for a
   snapshot extraction (cloning a subset of a HashMap). The write lock in the
   background tick is held only for a pointer swap (`*guard = new_table`). Neither
   is held across an await point. Deadlock between the two is structurally
   impossible: the read lock is not held when the write lock is acquired (they run
   in different async tasks). No concern.

4. **Panic in rebuild task**: Caught by `Ok(Err(join_err))` match arm;
   `tracing::error!` emitted; existing state retained. No propagation to the
   search path.

---

## Regression Risk

**Low.** The cold-start guard (`use_fallback = true`) produces `phase_explicit_norm
= 0.0` for all candidates, which is mathematically identical to the pre-col-031
state. Callers that do not supply `current_phase` (e.g., the MCP path as of this
PR) get `phase_snapshot = None` → `phase_explicit_norm = 0.0` — also identical to
pre-col-031. The regression gate (AC-12 with AC-16 fix) exercises the non-vacuous
path by forwarding phase from eval records.

The `w_phase_explicit` default change from `0.0` to `0.05` only takes effect when
`current_phase` is `Some(_)` AND `use_fallback = false`. Tests that set
`phase_explicit_norm: 0.0` explicitly in `FusedScoreInputs` are unaffected — they
bypass the scoring weight entirely.

---

## PR Comments

Posted 1 comment on PR #423 with the full security assessment.

---

## Knowledge Stewardship

Nothing novel to store — the security patterns here (parameterized SQL, `#![forbid(unsafe_code)]`, `unwrap_or_else` poison recovery) are already established conventions in this codebase and well-documented in existing patterns and ADRs.
