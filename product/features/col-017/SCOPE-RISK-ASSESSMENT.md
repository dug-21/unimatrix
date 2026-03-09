# col-017: Scope Risk Assessment

## Summary

Hook-Side Topic Attribution is a **medium-complexity, low-risk** feature. The core extraction logic already exists and is tested. The main work is plumbing: adding a field to the wire protocol, accumulating signals in session state, and resolving on close. Schema migration is straightforward (additive column). Backward compatibility is well-handled via `serde(default)`.

**Overall Risk: LOW-MEDIUM**

---

## Risk Register

### R1: Cross-Crate Visibility Change — Attribution Functions (MEDIUM)

**What**: `extract_from_path`, `extract_feature_id_pattern`, and `extract_from_git_checkout` in `unimatrix-observe/src/attribution.rs:26-56` are currently **private** (`fn`, not `pub fn`). Scope requires making them public for hook-side use.

**Why it matters**: These functions are called from `unimatrix-server` hook code (`uds/hook.rs`). Making them `pub` crosses a crate boundary (`unimatrix-observe` → `unimatrix-server`). This creates a stable API surface — any future signature change breaks the server.

**Likelihood**: HIGH (confirmed: functions are private today)
**Impact**: LOW (additive change, no breakage)
**Mitigation**: Make `pub` with `#[inline]` since they're small string scanners. Document that these are the canonical extraction functions. Consider a `pub fn extract_topic_signal(text: &str) -> Option<String>` facade that encapsulates the priority chain (path > pattern > git) so callers don't need to know the order.

**Architect attention**: Decide whether to expose individual functions or a single facade. Facade is cleaner but less flexible.

---

### R2: SessionState Accumulation — Memory Growth in Long Sessions (LOW-MEDIUM)

**What**: `SessionState` (session.rs:83) will accumulate `Vec<String>` of topic signals for the session lifetime. Sessions can run for hours (4h stale threshold).

**Why it matters**: At 3,200 events/day across all sessions, a single long session could accumulate hundreds of topic signal strings. Each is a short feature ID (e.g., "col-017"), so memory is bounded (~10-20 bytes × event count), but the Vec grows unbounded until SessionClose.

**Likelihood**: LOW (typical sessions are shorter; signals are small strings)
**Impact**: LOW (even 1000 × 20 bytes = 20KB — negligible)
**Mitigation**: Scope correctly identifies this as low risk. Could use a `HashMap<String, u32>` (topic → count) instead of `Vec<String>` to bound memory to O(unique topics) rather than O(events). This also makes majority vote O(1) lookup instead of O(n) counting.

**Architect attention**: `HashMap<String, u32>` vs `Vec<String>` is a design choice with testability implications. HashMap is strictly better for production but Vec is simpler to reason about in tests.

---

### R3: Migration Coordination with col-018/col-019 (MEDIUM)

**What**: Scope states schema migration v9→v10 may be shared with col-018 and col-019 if they land in the same release. All three features are Wave 1 parallel.

**Why it matters**: Three features touching the same migration function creates merge conflict risk. Migration code in `migration.rs` is sequential (each version bump is a distinct function). If features merge independently, the second and third must rebase their migration onto whatever landed first. The `CURRENT_SCHEMA_VERSION` constant (migration.rs:18) is a single-point conflict.

**Likelihood**: MEDIUM (three parallel features, all adding to same migration)
**Impact**: MEDIUM (merge conflicts in migration code delay delivery; incorrect migration ordering can corrupt data)
**Mitigation**:
1. **Assign one feature as migration owner** — col-017 adds the v9→v10 migration shell; col-018 and col-019 add their DDL into the same function.
2. **Or**: Each feature increments independently (v9→v10, v10→v11, v11→v12) and merge order determines final version. Simpler but wastes version numbers.
3. **Gate**: Integration test `test_schema_version_is_N` (sqlite_parity.rs:698) will catch version mismatches.

**Architect attention**: Decide migration strategy before implementation begins. Single shared migration vs independent bumps.

---

### R4: False-Positive Topic Attribution from `extract_feature_id_pattern` (LOW-MEDIUM)

**What**: `extract_feature_id_pattern` matches any `alpha-digits` pattern (e.g., "col-002", "api-100"). This can false-match on non-feature identifiers in tool inputs (e.g., "utf-8", "x86-64", "arm-v7", "sha-256").

**Why it matters**: False positives pollute the signal accumulator. Majority vote mitigates single false positives, but in sessions with few real signals, a false positive could dominate.

**Likelihood**: MEDIUM (pattern is intentionally broad per col-014)
**Impact**: LOW (majority vote + priority ordering mitigates; file path signals dominate when present)
**Mitigation**: Scope correctly identifies this. The `is_valid_feature_id` filter (if it exists) rejects common false positives. The priority chain (path > pattern > git) means high-confidence signals take precedence. Consider adding a minimum signal count threshold (e.g., ≥2 occurrences) before trusting pattern-only attribution.

---

### R5: Wire Protocol Backward Compatibility Gap (LOW)

**What**: Adding `topic_signal: Option<String>` to `ImplantEvent` with `serde(default)`. Old hook binary → new server works (field absent, deserialized as None). New hook binary → old server — the extra field is silently ignored by serde.

**Why it matters**: During rolling updates, hook binary and server binary may be different versions. The `serde(default)` approach handles this correctly in both directions. This is a validated pattern used previously.

**Likelihood**: LOW
**Impact**: LOW (graceful degradation in both directions)
**Mitigation**: Already handled by design. Add integration test verifying deserialization with and without the field.

---

### R6: SessionClose Race — Signals Arriving After Close (LOW)

**What**: Hook events are fire-and-forget over UDS. A RecordEvent with a topic signal could arrive after SessionClose has already resolved and persisted the topic.

**Why it matters**: Late-arriving signals are lost. The session's feature_cycle is already written.

**Likelihood**: LOW (SessionClose is the last event in the lifecycle; UDS is local so ordering is preserved)
**Impact**: LOW (one missed signal doesn't change majority vote outcome)
**Mitigation**: UDS preserves ordering for a single connection. Subagent events on separate connections could theoretically arrive late, but the 4h stale threshold provides a buffer. No action needed.

---

### R7: Content-Based Fallback Path Performance (LOW)

**What**: When no hook-side signals exist, SessionClose falls back to loading all observations from DB and running `attribute_sessions()`.

**Why it matters**: This loads potentially hundreds of observation rows and scans their content. It's the existing retrospective path, just triggered earlier (at SessionClose rather than at retrospective time).

**Likelihood**: LOW (most sessions will have hook-side signals after this feature)
**Impact**: LOW (observation scan is cheap; ~100ms for typical session)
**Mitigation**: This is a fallback for edge cases (sessions with no tool use, or sessions started before the new hook binary is deployed). Performance is acceptable. Could add a metric to track fallback frequency.

---

## Risk Matrix

| Risk | Likelihood | Impact | Priority | Needs Architect |
|------|-----------|--------|----------|-----------------|
| R1: Cross-crate API surface | HIGH | LOW | P2 | YES — facade vs individual fns |
| R2: SessionState memory | LOW | LOW | P3 | YES — HashMap vs Vec |
| R3: Migration coordination | MEDIUM | MEDIUM | P1 | YES — migration ownership |
| R4: False-positive attribution | MEDIUM | LOW | P3 | No |
| R5: Wire protocol compat | LOW | LOW | P4 | No |
| R6: SessionClose race | LOW | LOW | P4 | No |
| R7: Fallback performance | LOW | LOW | P4 | No |

## Top 3 Risks for Architect Attention

1. **R3: Migration coordination** — Three parallel features (col-017/018/019) touching v9→v10. Decide: single shared migration or independent version bumps. Must be resolved before any feature begins implementation.

2. **R1: Cross-crate API surface** — Attribution extraction functions must become public across crate boundary. Decide: expose 3 individual functions or provide a single `extract_topic_signal()` facade that encapsulates priority ordering.

3. **R2: SessionState accumulation structure** — `HashMap<String, u32>` (count per topic) vs `Vec<String>` (raw signals). HashMap is O(unique topics) memory and O(1) vote resolution; Vec is simpler but O(events). Both work; architect should pick the canonical representation.

## Scope Fitness

- **Scope is well-bounded**: Clear in/out scope. No feature creep risk.
- **Dependencies are minimal**: All extraction logic exists. No new crate dependencies.
- **Testing strategy is sound**: Existing 20+ attribution tests cover extraction. New tests needed for accumulation, majority vote, and SessionClose persistence.
- **Backward compatibility is solid**: `serde(default)` is a proven pattern in this codebase.
- **No vision misalignment**: Feature directly enables the Activity Intelligence milestone's core goal (topic attribution for retrospective pipeline).
