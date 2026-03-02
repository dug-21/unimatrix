## ADR-002: Rework Threshold — Edit-Fail-Edit × 3, Server-Side Evaluation

### Context

Rework detection determines whether a session receives `Flagged` signals instead of `Helpful` signals. The threshold must be:
1. **Conservative**: false positives flag entries incorrectly; developers iterating normally should never trigger rework.
2. **Server-side**: the hook process (stateless, ephemeral per-invocation) cannot accumulate state across PostToolUse events. Only the server-side `SessionState` persists across hook invocations within a session.
3. **Deterministic**: the same event sequence must always produce the same outcome.

SR-06 (Scope Risk Assessment) noted that rapid multi-edits to the same file are normal (updating multiple sections of a module) and must NOT be flagged. The SCOPE.md Resolved Design Decision #2 states: rework fires only with an intervening failure.

Three candidate threshold definitions were considered:
- **Option A**: Any Bash failure after an Edit — too sensitive; routine compilation errors after minor edits trigger it.
- **Option B**: Edit → Bash failure → Edit-same-file, once — moderate; still catches normal "edit, compile, small fix" cycles.
- **Option C**: Edit → Bash failure → Edit-same-file, three or more times for the same file — conservative; requires genuine repeated failure cycles on the same path.

### Decision

Rework is defined as: **the same file path appears in 3 or more separate Edit/Write/MultiEdit tool calls, each separated by at least one failed Bash call (non-zero exit_code or interrupted=true), within a single session.**

Formally, the rework threshold is crossed when `ReworkContext::check_threshold()` returns true, which requires:
```
exists file_path in rework_events such that:
  count(edit events for file_path with a failed Bash event between consecutive edits) >= 3
```

Evaluation logic lives in `SessionState` as `fn has_crossed_rework_threshold(&self) -> bool`. The hook sets `outcome = "success"` unconditionally for Stop events. `SessionRegistry::generate_signals()` evaluates the threshold and overrides to `"rework"` if crossed.

**Rework-eligible tools**: `Edit`, `Write`, `MultiEdit` (file-mutating). `Bash` provides failure signals. `Read`, `Glob`, `Grep`, `WebFetch` are not rework-eligible.

**Bash failure definition**: `exit_code` field is non-zero integer, OR `interrupted` field is true.

**File path deduplication**: For MultiEdit, each `{path}` in the `edits` array is tracked independently.

**Named constants**:
```rust
const REWORK_EDIT_CYCLE_THRESHOLD: usize = 3;  // 3 edit-fail-edit cycles = rework
```

### Consequences

- A developer who edits, compiles (fails), edits, compiles (fails), edits, compiles (fails), edits on the same file crosses the threshold on the 4th edit to that file (3 failure-separated edit pairs). This is conservative: most normal iteration cycles are 1-2 compile-fix loops.
- Easier: false positives from normal development iteration are avoided.
- Harder: genuine rework from slow, methodical developers who don't repeat failures may be missed (classified as success). This is acceptable — the asymmetric design means missed rework is informational (no false unhelpful vote), not harmful.
- Harder: the rework_events vector grows proportionally to PostToolUse calls. For a long session with many file edits, it may consume megabytes of memory. Mitigated by only storing rework-eligible tool events.
