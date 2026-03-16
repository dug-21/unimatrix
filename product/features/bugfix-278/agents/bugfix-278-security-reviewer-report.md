# Security Review: bugfix-278-security-reviewer

## Risk Level: low

## Summary

This change moves contradiction scanning from on-demand ONNX inference (per-request and
per-tick) to a cached background-tick result. The change introduces no new external
trust boundaries, no new deserialization of untrusted input, no injection surfaces, and
no new dependencies. The concurrency model follows established patterns in the codebase
(`Arc<RwLock<_>>` with poison recovery). One low-severity observation is noted regarding
stale-cache information disclosure, but it does not require blocking changes.

---

## Findings

### Finding 1 — Stale cache exposes removed/corrected entries to status report consumers
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs:447-457`
- **Description**: The contradiction cache is only refreshed every 4 ticks (~60 minutes).
  If an entry is quarantined, deprecated, or corrected between scans, the stale cached
  `ContradictionPair` referencing it will continue to appear in `context_status` responses
  for up to 60 minutes. The `scan_contradictions()` function itself filters non-active
  entries at scan time, but the cached result is not re-filtered on read. This means a
  consumer could observe an entry ID in `contradictions` that no longer exists as an
  active entry — a minor information integrity issue, not a data leak of protected data.
- **Recommendation**: Document this known staleness window in the module-level doc comment.
  Optionally, filter the cached `pairs` list at read time against active entry IDs, but
  this would require an additional store read on every `compute_report()` call and may
  not be worth the tradeoff. At minimum, callers should be aware that
  `contradiction_scan_performed: true` does not mean the result is current.
- **Blocking**: no

### Finding 2 — Double-bounded tick timeout: contradiction scan shares TICK_TIMEOUT with maintenance tick
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:489-528`
- **Description**: The contradiction scan runs after the maintenance tick and supersession
  rebuild within `run_single_tick()`. Each of these three operations is independently
  bounded by `TICK_TIMEOUT` (120 seconds). In the worst case a single tick can consume
  up to 3 x 120 = 360 seconds of wall time before the extraction tick runs. The tick loop
  itself has no outer timeout; only each sub-operation is bounded. This is not a new risk
  introduced by this PR (the pattern matches supersession rebuild added in GH#264), but
  the PR does add a third long-running operation in this slot. At very large N (1000+
  entries) the scan itself could approach 120 seconds, pushing total tick time past 6
  minutes. No denial-of-service surface is exposed to external callers since the tick is
  internal.
- **Recommendation**: Note in a follow-up issue that the cumulative tick budget is now
  effectively 3 x TICK_TIMEOUT per interval for infrequent ticks. Consider a shared
  budget envelope in a future refactor.
- **Blocking**: no

### Finding 3 — No injection, path traversal, deserialization, or access-control concerns
- **Severity**: informational
- **Location**: all changed files
- **Description**: All data flowing into the cache originates from ONNX/HNSW internal
  computation over store contents — no external user input is deserialized or written
  to the cache. The `ContradictionScanResult` is a plain Rust struct with no serde
  deserialization path from external sources. Hardcoded constants (`SYSTEM_AGENT_ID`,
  `CONTRADICTION_SCAN_INTERVAL_TICKS`) are compile-time values, not derived from
  environment or caller input. No file paths, shell commands, or SQL strings are
  constructed with user data in the changed code.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4 — No hardcoded secrets or credentials
- **Severity**: informational
- **Location**: all changed files
- **Description**: Full diff scanned. No API keys, tokens, passwords, or credentials
  present.
- **Recommendation**: No action required.
- **Blocking**: no

---

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

1. **Cache write bug** (e.g., `*guard = Some(...)` overwrites with wrong data): every
   subsequent `context_status` call would return incorrect contradiction counts for up
   to 60 minutes, until the next scan tick. No persistent data is written to the store;
   the cache is in-memory only. A server restart clears the cache and restores correct
   behavior on the next tick 0.

2. **RwLock deadlock**: If the write lock is somehow held when a read is attempted, the
   read blocks. Given the single-writer (background tick) / multiple-reader (status calls)
   pattern and that the write lock is held only for a pointer swap (not for the scan
   itself), deadlock risk is negligible. Poison recovery via `.unwrap_or_else(|e|
   e.into_inner())` is consistent with other state handles in the codebase.

3. **Tick counter wrap bug**: A `u32` wrapping from `u32::MAX` to `0` triggers a scan on
   the next tick. This is correctly handled by `wrapping_add` and is explicitly tested.
   The scan running one tick early (or late) after 4,294,967,295 ticks is inconsequential.

The failure mode is always observable (wrong count in status report, up to 60 minutes)
and self-correcting (next tick scan resets the cache). No silent data corruption to
persistent storage is possible.

---

## Regression Risk

- **StatusService.compute_report()** previously ran ONNX inference synchronously.
  The new path reads from cache; on cold-start (before tick 0 completes) the response
  returns `contradiction_scan_performed: false`. Any caller checking that flag will see
  an initial period of `false` before the first scan completes (~0-15 minutes after
  server start, depending on tick timing). This is documented behavior (same as before
  the server has an embedding model loaded) and is acceptable regression.

- **Embedding consistency check** (Phase 3) is correctly preserved unchanged — it remains
  on the MCP request path and is not gated behind the new cache. The diff confirms the
  `if let Ok(adapter) = self.embed_service.get_adapter().await` block is retained.

- **Background tick correctness**: maintenance tick, supersession rebuild, and extraction
  tick are all preserved in order. The contradiction scan is inserted between supersession
  rebuild and extraction tick — this ordering is correct since the scan reads from the
  store (not from supersession state) and writes only to the cache.

- **Test coverage**: 5 new unit tests in `contradiction_cache.rs` directly cover the new
  types and constants. No existing tests were removed. The gate report confirms all 2538
  unit and 61 integration tests pass.

---

## PR Comments

- Posted 1 comment on PR #292 (see below)
- Blocking findings: no

---

## Knowledge Stewardship
- nothing novel to store -- the general pattern of "cache expensive background computations in Arc<RwLock<_>>" is already captured in Unimatrix (entry #1762 per gate report). The stale-cache information staleness observation is feature-specific and belongs in the PR comment, not as a generalizable lesson.
