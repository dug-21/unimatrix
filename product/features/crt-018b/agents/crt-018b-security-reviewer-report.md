# Security Review: crt-018b-security-reviewer

## Risk Level: low

## Summary

crt-018b introduces an in-memory effectiveness classification cache, search re-ranking utility deltas,
briefing sort tiebreakers, and a background-tick-driven auto-quarantine mechanism. The diff implements
all four components with explicit attention to the security risks identified in RISK-TEST-STRATEGY.md.
No blocking security findings were identified. Three low-severity observations are noted below.

---

## Findings

### Finding 1: AUTO_QUARANTINE_CYCLES validation — correctly implemented
- **Severity**: low (previously high potential if missed; mitigation is present)
- **Location**: `crates/unimatrix-server/src/background.rs:104–130`
- **Description**: The env var `UNIMATRIX_AUTO_QUARANTINE_CYCLES` is validated at startup via
  `parse_auto_quarantine_cycles_str()`. Values above 1000 are rejected with an error that propagates
  through `ServerError::ProjectInit`, failing startup cleanly. Negative values cannot parse to `u32`.
  Non-integer strings are rejected with an error message echoing the bad value (no format-string
  injection risk; the value is passed through `{:?}` debug formatting, not as a format string).
- **Assessment**: Constraint 14 (DoS mitigation) is fully satisfied. Unit tests cover: default=3,
  zero (disabled), boundary 1000 accepted, 1001 rejected, non-integer rejected, negative rejected.
- **Recommendation**: No action required. Verified correct.
- **Blocking**: no

### Finding 2: SYSTEM_AGENT_ID is a hardcoded constant, not user-controllable
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/background.rs:84`
- **Description**: The `agent_id = "system"` in auto-quarantine and tick-skipped audit events is the
  compile-time constant `SYSTEM_AGENT_ID`. It is not sourced from any MCP request parameter or
  external input. Security Risk 2 from RISK-TEST-STRATEGY is addressed. A unit test
  (`test_audit_constants_have_correct_values`) asserts the constant value.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 3: Lock poison recovery applied uniformly
- **Severity**: low (informational)
- **Location**: All `RwLock` and `Mutex` acquisitions in `services/effectiveness.rs`,
  `services/search.rs`, `services/briefing.rs`, and `background.rs`
- **Description**: Every lock acquisition uses `.unwrap_or_else(|e| e.into_inner())` with no
  `.unwrap()` or `.expect()` calls on these locks. This is consistent with the `CategoryAllowlist`
  convention established in prior features. Security Risk 3 (lock-poison cascades to all search calls)
  is mitigated. A dedicated poison-recovery test (`test_effectiveness_state_handle_poison_recovery`)
  verifies the recovery path does not panic and preserves pre-panic data.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4: Audit event `detail` field includes operator-controlled title/topic strings
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:524–541` (`emit_auto_quarantine_audit`)
- **Description**: The `detail` field of the auto-quarantine audit event includes `title` and `topic`
  strings sourced from `EntryEffectiveness` fields, which were originally stored by agents via
  `context_store`. These strings are not sanitized before inclusion in the audit event detail. The
  audit log is an internal observability artifact (SQLite column, not an externally served HTTP
  response), so the blast radius of a malformed or adversarial title is limited to audit log
  readability. There is no SQL injection risk because the strings are inserted as parameterized
  query values through the existing `AuditLog::log_event` infrastructure (not string-interpolated
  into SQL). Format strings use Rust's `{:?}` debug representation for title and entry_category, which
  escapes special characters. This is informational only.
- **Recommendation**: No immediate action required. If audit events are ever forwarded to an external
  logging system, the operator should apply output encoding at the export boundary.
- **Blocking**: no

### Finding 5: Step 11 `final_score` omits provenance boost and co-access boost (pre-existing)
- **Severity**: low (pre-existing behavior, not a regression)
- **Location**: `crates/unimatrix-server/src/services/search.rs` Step 11
- **Description**: The `ScoredEntry.final_score` field is constructed from
  `(rerank_score + delta) * penalty`, omitting `prov_boost` and `co_access_boost`. These boosts are
  included in the sort comparators (Steps 7 and 8) but not in the final stored score. This is a
  pre-existing behavior (the original code also omitted them from `final_score`). The crt-018b change
  correctly adds `utility_delta` to the `final_score` computation (Step 11), making it more consistent
  with the sort order. The omission of provenance and co-access from `final_score` is not introduced
  by this PR.
- **Recommendation**: Confirm the omission is intentional in a follow-up ticket if callers rely on
  `final_score` for display or downstream scoring. Out of scope for this review.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The most dangerous regression scenario is the auto-quarantine write path firing incorrectly. If
`process_auto_quarantine` calls `store.update_status(entry_id, Status::Quarantined)` for entries
that should not be quarantined (e.g., due to a logic error in the `consecutive_bad_cycles` scan),
knowledge base entries are silently removed from retrieval. Recovery requires manual operator action
via `context_quarantine` restore per entry and leaves a full audit trail. The blast radius is
bounded by:

1. Only entries classified Ineffective or Noisy for N consecutive ticks are candidates.
2. The category restriction check is applied in two places: inside the write lock (step 7) and again
   at the start of `process_auto_quarantine` (defense in depth). A bug in the category scan would need
   to bypass both.
3. `AUTO_QUARANTINE_CYCLES = 0` provides a hard off-switch.
4. Every quarantine action writes a rich audit event, enabling operator discovery and rollback.

If the search utility delta has a sign error (e.g., Effective entries receive -0.05 instead of +0.05),
search result ordering degrades silently — Effective entries rank lower than Ineffective. This is a
data quality regression, not a security incident. It does not cause data loss or corruption.

If the write lock is inadvertently held during the SQL call (R-13), the worst case is read-lock
starvation causing `search()` calls to block for the duration of the synchronous SQLite write
(potentially hundreds of milliseconds under bulk quarantine). The diff's scoped block pattern
explicitly drops the write guard at the closing `}` before `process_auto_quarantine` is called.
Verified in the diff.

---

## Regression Risk

**Existing functionality that could break:**

1. **Search re-ranking order**: The utility delta changes result ordering for entries with known
   effectiveness classifications. On cold start (empty `EffectivenessState`), the delta is 0.0 for
   all entries — behavior is identical to pre-crt-018b. After the first background tick, ordering
   changes are intentional. Existing tests that assert specific sort orderings without effectiveness
   data will continue to pass because the empty-state produces zero delta.

2. **BriefingService constructor**: The constructor now requires `EffectivenessStateHandle` as a
   non-optional parameter. Any callers that did not pass this parameter would fail to compile.
   The diff shows all call sites in `services/mod.rs` and test helpers updated. This is a compile-time
   guarantee, not a runtime risk.

3. **EffectivenessReport serialization**: Two new `#[serde(default)]` fields added to
   `EffectivenessReport`. The `#[serde(default)]` attribute ensures backward compatibility when
   deserializing existing serialized reports that lack these fields. No breakage for stored data.

4. **Existing background tick behavior**: `maintenance_tick()` now has additional parameters and
   conditionally executes the effectiveness write and auto-quarantine scan. The existing
   `run_maintenance()` call (confidence refresh, graph compaction, co-access cleanup) is unchanged
   and runs after the new code. No existing maintenance logic is removed or reordered.

---

## Dependency Safety

No new crate dependencies are introduced. The diff imports existing items from:
- `unimatrix_store::Status` (existing)
- `unimatrix_store::rusqlite` (existing)
- `unimatrix_engine::effectiveness::EffectivenessCategory` (existing crate, new import)
- Standard library (`std::collections::HashSet`, `std::sync::{Arc, Mutex}`)

No new third-party dependencies. No known CVE exposure.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in the diff. The only string constants added
are operational identifiers: `"system"`, `"auto_quarantine"`, `"tick_skipped"`.

---

## OWASP Coverage

| OWASP Concern | Assessment |
|---------------|-----------|
| Injection (SQL, command, path traversal) | No new SQL. Quarantine calls use parameterized ORM-style store API. No shell commands. No path operations. |
| Broken access control | Auto-quarantine runs as `agent_id = "system"` in the background tick — not triggered by MCP caller input. No access control change at the MCP tool layer. |
| Security misconfiguration | `AUTO_QUARANTINE_CYCLES` validated at startup: non-integer and values >1000 cause startup error. Zero correctly disables the feature. Default of 3 is conservative. |
| Input validation gaps | The `UNIMATRIX_AUTO_QUARANTINE_CYCLES` env var is the only new external input; it is validated before use. |
| Deserialization risks | Two new `#[serde(default)]` fields on `EffectivenessReport`. Default prevents deserialization failure on old data. |
| Data integrity failures | Auto-quarantine is irreversible without operator action, but is fully audited and requires N consecutive bad ticks. |
| Vulnerable components | No new dependencies. |
| Concurrency / race conditions | Lock ordering (R-01) verified: read guard from `effectiveness_state` is explicitly dropped (inner block scope) before `cached_snapshot.lock()` is acquired. Write lock (R-13) is dropped before SQL: the scoped block at lines 250–333 of `background.rs` returns `candidates` and the write guard falls out of scope before `process_auto_quarantine` is called. |

---

## Concurrency Safety

**R-01 (Lock ordering deadlock):** Verified. In both `search.rs` and `briefing.rs`, the pattern is:
```
{ let gen = effectiveness_state.read()...; guard.generation } // read guard dropped
// then:
let mut cache = cached_snapshot.lock()...;
```
The two locks are never held simultaneously. This is a scoped-block pattern, not a comment or
convention — the compiler enforces the drop at the closing `}`.

**R-13 (Write lock held during SQL):** Verified. In `maintenance_tick()`:
- The write lock is acquired inside a scoped block (`let to_quarantine: Vec<...> = { ... }`)
- `candidates` is moved out of the block and the write guard falls out of scope at the closing `}`
- `process_auto_quarantine(to_quarantine, ...)` is called after the block — write lock is NOT held

The comment at line 330 ("Write lock drops here — end of block scope. CRITICAL: No store calls
may be made inside this block") is accurate and confirmed by code structure.

**write lock re-acquisition in `process_auto_quarantine`:** After each successful quarantine SQL write,
`process_auto_quarantine` re-acquires the write lock briefly to remove the entry's counter from
`consecutive_bad_cycles`. This is a short critical section (single HashMap remove) with no SQL inside.
This is safe.

---

## PR Comments

- Posted 1 comment on PR #263.
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — all findings here (env-var DoS mitigation, lock ordering via
  scoped blocks, poison recovery pattern) are either already in the knowledge base or are specific
  to this feature's audit log detail surface. The audit-event-detail-includes-user-data observation
  is informational and not a recurring generalizable anti-pattern in this codebase.
