# crt-028 Retrospective — Architect Report

> Agent: crt-028-retro-architect (uni-architect)
> Feature: crt-028 — WA-5: PreCompact Transcript Restoration
> Mode: retrospective
> Date: 2026-03-23

---

## 1. Patterns

### New Entries

| ID | Title | Category | Stored |
|----|-------|----------|--------|
| #3345 | Fail-Open Closure Pattern: Inner Closure Returning Option for Infallible Public Functions | pattern | yes |
| #3346 | Sole-Write-Gate Allowlist: Exhaustive Match with Wildcard Fallback and Single Write Site | pattern | yes |
| #3348 | Tail-Bytes Read Strategy for Append-Only Files: seek_back = window.min(file_len) | pattern | yes |

**#3345 — Fail-Open Closure Pattern**

Extracted from `extract_transcript_block` in `hook.rs`. The idiom `let inner = || -> Option<String> { ... }; inner()` wraps multi-step fallible I/O so all `?` operators map to `None` without converting each error manually. The outer function signature remains `-> Option<String>`, making it structurally impossible for any I/O error to escape. This is the Rust-idiomatic implementation of the ADR-003 degradation contract and recurs any time a hook function must be infallible.

**#3346 — Sole-Write-Gate Allowlist**

Extracted from `sanitize_observation_source` in `listener.rs` (GH #354). The pattern has three properties that work together: (1) exhaustive match over all known-valid values, (2) wildcard fallback that catches everything else including None and adversarially long strings without a length cap, (3) a doc comment explicitly naming this as the sole write site. The sole-write-gate doc comment is the distinguishing feature from generic allowlist validation — it is a warning to future developers against adding a second unvalidated write site for the same column.

**#3348 — Tail-Bytes Read Strategy**

Extracted from `extract_transcript_block` in `hook.rs` (ADR-001). The critical detail is the clamp: `seek_back = window.min(file_len)`. Without the clamp, `SeekFrom::End` with a negative offset larger than the file produces an OS error. The first post-seek line is always discarded via the fail-open parser (it landed mid-record). This pattern is applicable to any hook reading append-only log or JSONL files with a latency budget.

### Updated Entries

None.

### Skipped

| Pattern | Reason |
|---------|--------|
| Adjacent-record pairing (`build_exchange_pairs`) | The JSONL tool_use/tool_result pairing is JSONL-format-specific and Claude Code transcript-specific. It lives in ADR-002 (#3334). Not general enough to stand alone as a cross-feature pattern. |
| `index_briefing.rs` quarantine regression test | One-off regression test per briefing: not a reusable pattern. Already noted as skip in task brief. |
| PreCompact hook read-before-request pattern | Already stored as #3331 (prior feature knowledge check confirmed this). No update needed. |
| hook.rs unit test patterns (tempfile, no-tokio) | Already stored as #3338 (confirmed via search). No update needed. |

---

## 2. Procedures

### New Entries

None. The parallel agent delivery pattern (Wave 1 with 3 agents) is already addressed by the lesson (#3347) rather than a procedure. The existing procedure #553 covers worktree isolation validation. The branch discipline issue (agents committing to bugfix/358 instead of feature/crt-028) was not confirmed as a recurring pattern — it occurred twice in this feature but is not documented as recurring across prior features. No procedure stored; the lesson covers the verification gap.

### Updated Entries

None. Searched for existing branch discipline procedures — no dedicated entry found (#553 covers worktree isolation validation at design time, not delivery-phase branch targeting by agents). A procedure update was considered but the root cause is spawn-prompt precision (agents were not told the correct branch explicitly), which is a protocol gap rather than a procedure gap. Not stored as a procedure.

---

## 3. ADR Validation

All 4 crt-028 ADRs were validated against the gate reports and risk coverage report.

| ADR | Unimatrix ID | Title | Validation Status |
|-----|-------------|-------|-------------------|
| ADR-001 | #3333 | Tail-Bytes Read Strategy for Transcript I/O | VALIDATED |
| ADR-002 | #3334 | Tool-Use/Tool-Result Pairing via Adjacent-Record Scan | VALIDATED |
| ADR-003 | #3335 | Graceful Degradation Contract — Transcript Block Only, BriefingContent Always Emits | VALIDATED |
| ADR-004 | #3336 | Source Field Allowlist for GH #354 — Allowlist Match, Default Fallback, No Length Cap | VALIDATED |

**ADR-001 (#3333)**: Implemented as `seek_back = window.min(file_len)` with `SeekFrom::End`. Zero-byte file handled (seek_back == 0 → skip seek). Boundary cases `file_len = window`, `file_len = window - 1`, `file_len = window + 1` all tested or code-reviewed correct. Gate 3c confirmed. VALIDATED.

**ADR-002 (#3334)**: Adjacent-record look-ahead implemented in `build_exchange_pairs`. Non-adjacent result (no tool_result in next record) produces empty snippet rather than drop. Unmatched tool_result silently skipped. All edge cases pass in `build_exchange_pairs_*` test suite. Gate 3c confirmed. VALIDATED.

**ADR-003 (#3335)**: `extract_transcript_block` returns `Option<String>` with inner closure. All failure classes in the architecture table confirmed: None path, missing file, seek error, all-malformed, no pairs. Gate 3a security reviewer confirmed R-01 structurally correct. 3,217 tests pass with 0 failures. VALIDATED.

**ADR-004 (#3336)**: `sanitize_observation_source` present at listener.rs line 101, call site at line 837. All 6 allowlist cases tested and passing. No length cap applied (correct — allowlist match is structural). GH #354 confirmed closeable in Gate 3c. VALIDATED.

No supersessions needed. These four ADRs are all new decisions for crt-028 with no prior overlapping ADRs.

---

## 4. Lessons

### New Entries

| ID | Title | Category | Stored |
|----|-------|----------|--------|
| #3347 | Background Agent Silent Write Failure: Verify File System State Before Advancing to Gate 3b | lesson-learned | yes |

**#3347 — Background Agent Silent Write Failure**

Source: Gate 3b REWORKABLE FAIL. A `uni-rust-dev` agent for listener.rs reported "all three changes correct" but no file system writes occurred. Gate 3b validator found listener.rs line 813 unchanged. SM re-implemented `sanitize_observation_source`, the call site, and all 6 unit tests manually.

Root cause: background agents without worktree isolation have no mandatory write-verification step. The agent may have made changes to a stale copy or had a write failure not surfaced.

Lesson captured: before advancing to Gate 3b, the SM or gate validator MUST verify agent-reported writes with a grep or cargo check — not trust agent self-reports alone. Verification pattern: `grep -n "{symbol}" {file}` expecting at least a definition and a call site. If the symbol is absent, treat agent as having failed silently and re-spawn or implement manually.

### Considered but Not Stored

| Candidate | Disposition |
|-----------|-------------|
| compile_cycles outlier (60 vs threshold 6) | Circumstantial — came from index_briefing.rs debugging cycles. Not a process failure; no lesson extracted. |
| follow_up_issues outlier (4 vs mean 1.26) | Positive signal — security review issues #354/#355 bundled from crt-027; rayon bug #358 was concurrent unrelated work. No lesson extracted. |
| cold_restart / 12h session gap | Normal for async workflow. Not a lesson. |
| Branch targeting (agents committed to wrong branch twice) | Spawn-prompt precision issue; not a recurring cross-feature pattern confirmed in prior features. No lesson stored. |

---

## 5. Retrospective Findings Summary

**Feature health**: Clean delivery overall. Gate 3a and 3c passed without rework. Gate 3b had one REWORKABLE FAIL (listener.rs silent write failure) that was resolved in Wave 1 before Gate 3c. Final state: 3,217 tests, 0 failures, all 15 ACs IMPLEMENTED, all 13 risks covered.

**Key hotspots with architectural relevance**:

- **edit_bloat_ratio outlier (0.529 vs 0.171 mean)**: hook.rs is ~32K tokens — a large file requiring many piecemeal reads/writes. This is inherent to the file's accumulated scope (all hook event handlers, constants, and tests co-located). Not an architecture problem but worth noting if future features add to hook.rs: consider whether a module split is warranted at some threshold.

- **Gate 3b listener.rs failure**: The sole meaningful rework event. Architectural implication: when spawning multiple agents in parallel without worktree isolation, the SM must treat agent-reported completions as claims requiring file system verification, not facts. Lesson stored as #3347.

- **scope_hotspot_count outlier (3 vs 1.05 mean)**: 36 design artifacts + 4 ADRs + 1 post-delivery issue. This reflects genuine design complexity (3 separate components, security fixes from a prior feature, an open spec question resolved during design). Not a process failure.

**Architecture quality**: The three-component decomposition (hook.rs / listener.rs / index_briefing.rs) was clean and well-scoped. Integration surface table in ARCHITECTURE.md prevented any interface invention by implementation agents. All 4 ADRs captured the "why" accurately and were confirmed correct in implementation. The ADR-003 degradation contract (Option<String> return, inner closure, briefing always emits) was structurally enforced rather than just documented — this was validated explicitly by the security reviewer.

**Knowledge stored this session**:
- #3345 — Fail-Open Closure Pattern (new pattern, generalizable)
- #3346 — Sole-Write-Gate Allowlist (new pattern, security-relevant)
- #3347 — Background Agent Silent Write Failure (new lesson, process-relevant)
- #3348 — Tail-Bytes Read Strategy (new pattern, I/O-relevant)
