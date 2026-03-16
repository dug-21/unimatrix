# Security Review: bugfix-280-security-reviewer

## Risk Level: low

## Summary

The change is a performance fix that decouples the O(N) ONNX contradiction scan from the background maintenance tick and from the `context_status` MCP tool path. It introduces one new module (`contradiction_cache.rs`) following an established `Arc<RwLock<Option<T>>>` pattern, adds a lightweight `load_maintenance_snapshot()` method, and adds a `Default` impl for `StatusReport`. No new external inputs, no new deserialization of untrusted data, no new file system operations, no new dependencies. Security surface is unchanged. No blocking findings.

## Findings

### Finding 1: Thin-shell `StatusReport` default coherence score is 1.0
- **Severity**: low
- **Location**: `mcp/response/status.rs:125–130` (Default impl); `background.rs:608–611` (thin-shell construction)
- **Description**: The thin-shell `StatusReport` built in `maintenance_tick()` uses `Default`, which sets `coherence: 1.0`, `confidence_freshness_score: 1.0`, etc. These are sentinel values, not computed values. The struct is passed only to `run_maintenance()`, which reads only `graph_stale_ratio`. The defaulted fields are never written to any audit log, metric, or MCP response. The thin shell is a local variable that is dropped at the end of `maintenance_tick()`.
- **Recommendation**: No action required. The values are not observable to callers. The `Default` impl is correct semantics for a zero-initialized placeholder used exclusively by the tick's internal graph-compaction trigger. The risk would materialize only if `run_maintenance()` or any future caller reads a coherence field from this struct — which would require a code change to introduce.
- **Blocking**: no

### Finding 2: Contradiction scan runs at tick 0 (server startup)
- **Severity**: low
- **Location**: `background.rs:480`, `contradiction_cache.rs:27`
- **Description**: `CONTRADICTION_SCAN_INTERVAL_TICKS = 4` and `current_tick % 4 == 0` fires on tick 0, meaning the O(N) ONNX scan runs on the first tick after startup. This is intentional (documented in module-level comments) but means a degraded-performance window exists at startup in addition to every 60 minutes. The scan is wrapped in `TICK_TIMEOUT` (120s) and failures retain the previous cached result gracefully.
- **Recommendation**: Acceptable. The behaviour is documented, the timeout guards against hangs, and the previous-cache-retained fallback on failure/timeout is correct. Not a security concern; noted as a startup latency observation.
- **Blocking**: no

### Finding 3: RwLock poison recovery via `unwrap_or_else(|e| e.into_inner())` — consistent with codebase convention
- **Severity**: low (informational)
- **Location**: `background.rs:131`, `services/status.rs:596`, `contradiction_cache.rs` tests (lines 58, 78, 83)
- **Description**: All `RwLock` acquisitions in the new code use `.unwrap_or_else(|e| e.into_inner())` for poison recovery, consistent with `EffectivenessState` and `CategoryAllowlist` conventions documented in the module header. The write path (background tick) is the sole writer; if it panics mid-write, the `into_inner()` path recovers the last-written value, which may be a partially updated `ContradictionScanResult`. Given `ContradictionScanResult` is assigned atomically as a whole (`*guard = Some(result)` — a single pointer store under the write lock), there is no torn-write risk.
- **Recommendation**: No action required. Poison recovery is correct and consistent.
- **Blocking**: no

### Finding 4: Contradiction scan result vector may be large (unbounded clone)
- **Severity**: low
- **Location**: `services/status.rs:599`
- **Description**: `report.contradictions = result.pairs.clone()` clones the full contradiction pairs vector on every `compute_report()` call (every `context_status` invocation). The vector size is bounded by the number of active entries, which is bounded by the database. At current scale (53 active entries), this is negligible. At pathological scale (thousands of entries), a large clone here is a momentary memory spike. This is a pre-existing concern on the write path (`scan_contradictions` builds the vector); the read path clone adds no new risk surface.
- **Recommendation**: No action required in this PR. If entry counts grow significantly, a `take` or reference pattern could be considered — but that is a future optimization, not a security issue.
- **Blocking**: no

### Finding 5: Research documents (ass-020, ass-021) included in diff
- **Severity**: informational
- **Location**: `product/research/ass-020/`, `product/research/ass-021/`
- **Description**: The diff includes new research document files that are not part of the code fix. These are documentation artifacts (findings reports, design notes). They contain no code, no secrets, no hardcoded credentials. Inclusion in this PR does not introduce security risk, but is noted as an out-of-scope change relative to the stated fix.
- **Recommendation**: The research artifacts are safe. If this is intended bundling (the research informed the fix), it is acceptable. If unintentional, they can be split into a separate commit. This is a scope observation, not a security concern.
- **Blocking**: no

## Blast Radius Assessment

**Scenario: subtle bug in tick counter logic causes contradiction scan to never fire after wrap.**

The `wrapping_add` path is tested (`test_tick_counter_u32_max_wraps_without_panic`), so this is low probability. If it were to fail silently, the `contradiction_cache` would remain in its pre-wrap state indefinitely. The worst case is stale contradiction data in `context_status` responses — the same data that was already cached. No data corruption, no availability loss, no privilege escalation.

**Scenario: contradiction scan panic inside `spawn_blocking` after a successful write guard acquisition.**

The write path is `*guard = Some(result)` — the store is only written after `scan_contradictions` returns `Ok`. A panic inside `scan_contradictions` (before the write) would be caught by the `Ok(Err(e))` arm, retaining the previous cached value. A panic after the `Ok(Ok(pairs))` match but before the write guard is acquired is not possible (the guard acquisition is the next expression with no code between them). Safe.

**Scenario: `StatusReport::default()` coherence scores are read by a monitoring consumer that expects real values.**

The thin-shell `StatusReport` is a local variable in `maintenance_tick()`. It is not serialized, not returned via MCP, not written to the audit log. It passes through `run_maintenance()` where only `graph_stale_ratio` is read (confirmed by inspection of `run_maintenance()` call sites in the existing code). Blast radius is zero.

**Scenario: contradiction scan writes to cache concurrently with `compute_report()` reading from it.**

The `Arc<RwLock<>>` correctly serializes these accesses. Multiple concurrent `context_status` calls take read locks simultaneously; the background tick takes an exclusive write lock after `scan_contradictions` completes. No torn read is possible. Worst case: a reader acquires the read lock just before the writer updates — it reads the previous scan result, which is the intended stale-but-non-empty cache semantics.

## Regression Risk

**Low.** The change is strictly additive:
- `compute_report()` is untouched; it now reads from cache instead of running ONNX — this is a functional change but the contract is the same (contradiction data or empty on cold start).
- `maintenance_tick()` now calls `load_maintenance_snapshot()` instead of `compute_report()`. The three data items it consumed from the old call (`active_entries`, `graph_stale_ratio`, `effectiveness`) are all present in the new snapshot.
- The `Default` impl for `StatusReport` is new but does not change any existing callers — none existed before this change.
- `tick_counter` is a new field on `TickMetadata` (public, `pub`). Any consumer reading `TickMetadata` will now also see this field. No existing consumers parse this struct except the tick loop itself.

The one behavioral regression risk: `context_status` will now report `contradiction_scan_performed: false` until the first background scan tick completes (~15 minutes after startup). Previously it always ran the scan inline. This is the intended fix — but operators should be aware that cold-start status calls will show no contradiction data until the first scan tick fires. This is documented in the module header and is not a security concern.

## PR Comments

- Posted 1 comment on PR #294 (non-blocking findings summary).
- Blocking findings: no.

## Knowledge Stewardship

- Stored: nothing novel to store -- the `Arc<RwLock<Option<T>>>` shared cache pattern is already established in this codebase (ConfidenceStateHandle, SupersessionStateHandle) and the reviewer process confirmed no new anti-pattern appeared. The research files bundled with a code fix is a scope observation already documented in the investigator/developer reports from prior sessions.
