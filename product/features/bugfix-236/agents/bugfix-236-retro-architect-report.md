# Agent Report: bugfix-236-retro-architect

## Task
Retrospective knowledge extraction for bugfix-236 (server reliability: ghost process, tick contention, handler timeouts).

## 1. Patterns

### Updated
| Entry | Title | Change | Reason |
|-------|-------|--------|--------|
| #732 -> #1366 | Tick Loop Error Recovery: Extract-and-Catch Pattern | Added timeout wrapping (TICK_TIMEOUT=120s) around sub-ticks | bugfix-236 extended the pattern with tokio::time::timeout to prevent long ticks from blocking MCP handlers |
| #318 -> #1369 | MCP Tool 6-Step Handler Pipeline | Step 4 now requires spawn_blocking_with_timeout for business logic | bugfix-236 established that bare spawn_blocking in handler business logic causes indefinite hangs |

### New
| Entry | Title | Reason |
|-------|-------|--------|
| #1367 | spawn_blocking_with_timeout for MCP Handler Mutex Acquisition | New reusable utility (infra/timeout.rs) wrapping spawn_blocking with tokio::time::timeout. Generic, applies to all future MCP handlers. |

### Skipped
| Component | Reason |
|-----------|--------|
| Cancellation token shutdown (main.rs) | One-time structural fix specific to rmcp. Correctly declined by rust-dev agent. |
| SIGKILL escalation (pidfile.rs) | Addressed via procedure update (#668 -> #1368), not a standalone pattern. |

## 2. Procedures

### Updated
| Entry | Title | Change |
|-------|-------|--------|
| #668 -> #1368 | How to open database with retry after stale process termination | Added SIGKILL escalation step after SIGTERM timeout. Added PID validation context (is_unimatrix_process). |

### New
None. No new build/test/migration procedures introduced.

## 3. ADR Status

No ADRs were created during bugfix-236 (the rust-dev agent correctly declined -- implementation-level decisions, not architectural).

### Prior ADR validation
| ADR | Status | Notes |
|-----|--------|-------|
| #189 (vnc-004: fs2 file locking) | Validated | SIGKILL escalation does not change the locking mechanism. fs2 flock remains correct. |

No ADRs flagged for supersession.

## 4. Lessons

### From gate failures and rework
None. Gate passed 10/10 on first attempt. No rework sessions detected.

### From hotspots and recommendations

| Entry | Title | Source Hotspot | Severity |
|-------|-------|----------------|----------|
| #1370 | Batch structural changes before compiling to reduce compile cycles | compile_cycles: 34 cycles (Warning) | Actionable |
| #1371 | Agents default to Bash for search instead of Grep/Glob tools | bash_for_search: 26.8% / 159 count (Outlier) | Recurring |
| #1372 | Bugfix spawn prompts should include distilled library API signatures | context_load: 649 KB (Warning + Outlier) | Actionable |

### Hotspots assessed but not stored (existing coverage or not actionable)

| Hotspot | Assessment |
|---------|------------|
| permission_retries (Bash:7, Read:6) | Recurring across col-022, col-020, others. Recommendation (add to settings.json allowlist) is a platform config change, not a knowledge entry. Already noted in 3+ retrospectives. |
| cold_restart (46 min, 24 re-reads) | Covered by #1271 (context load scales with component count). 24 re-reads for 7 files after a 46-min gap is within the per-component normalization from that lesson. |
| lifespan (rust-dev: 45m, tester: 53m) | Borderline. Multi-root-cause bugfix with investigation naturally runs longer. Not actionable without restructuring the bugfix protocol. |
| file_breadth (45 files) | Driven by rmcp registry source reads (investigation). Addressed indirectly by #1372 (spawn prompt should include API signatures). |
| mutation_spread (12 files) | 7 source + 5 design artifacts. Source mutations are minimal for a 3-root-cause fix. Not actionable. |
| sleep_workarounds (2 instances) | Info severity. Recommendation to use run_in_background + TaskOutput is valid but already documented in tool descriptions. Not recurring enough to store. |

### Baseline outlier summary

| Metric | Value | Mean | Assessment |
|--------|-------|------|------------|
| edit_bloat_ratio | 0.269 | 0.06 | Driven by investigation-heavy read phase. Addressed by #1372. |
| bash_for_search_count | 159 | 34.4 | Addressed by #1371. |
| context_load_before_first_write_kb | 649 | 41.4 | Addressed by #1372. Existing lesson #1163 also covers. |
| agent_hotspot_count | 7 | 2.66 | Inherent to 3-root-cause bugfix complexity. Not independently actionable. |

## 5. Summary of Unimatrix changes

| Action | Old ID | New ID | Type |
|--------|--------|--------|------|
| Corrected | #732 | #1366 | pattern (tick loop + timeout) |
| Stored | -- | #1367 | pattern (spawn_blocking_with_timeout) |
| Corrected | #668 | #1368 | procedure (stale process termination + SIGKILL) |
| Corrected | #318 | #1369 | convention (6-step pipeline + timeout requirement) |
| Stored | -- | #1370 | lesson-learned (compile cycle batching) |
| Stored | -- | #1371 | lesson-learned (bash-for-search tool preference) |
| Stored | -- | #1372 | lesson-learned (bugfix spawn prompt API signatures) |

## Knowledge Stewardship
- Queried: #732, #731, #667, #733, #668, #318, #735, #763, #189, #1365, #1262, #1163, #1271, #886
- Corrected: #732 -> #1366, #668 -> #1368, #318 -> #1369
- Stored: #1367, #1370, #1371, #1372
- Declined: Cancellation token pattern (one-off), SIGKILL as standalone pattern (covered in procedure)
