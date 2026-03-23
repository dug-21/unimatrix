# crt-027 Retrospective — Architect Report

**Agent:** crt-027-retro-architect (uni-architect)
**Date:** 2026-03-23
**Feature:** crt-027 WA-4 Proactive Knowledge Delivery

---

## 1. Stewardship Quality Review

### #3253 — Non-Negotiable Test Name Verification Pattern for Rewritten Test Suites (pattern)

**Assessment: KEEP — adequate quality, minor noise tolerated.**

Structure review:
- **What**: Procedure for verifying test names via grep when a test suite is rewritten (not just added to).
- **Why**: `cargo test` passing does not catch deleted tests. Count-based verification misses name changes. Root cause anchored to lesson #2758.
- **Scope**: Applies when (a) function signature change forces test rewrite, (b) struct deleted with associated tests replaced, (c) batch invariant-equivalent replacement.

The entry includes the 11 specific crt-027 test names as a concrete illustration. These are feature-specific but serve as a worked example for future agents — acceptable as inline illustration, not prescriptive.

Quality verdict: the what/why/scope structure is present. The "why" references the actual failure mode (cargo test PASS does not catch deletions). Confidence 0.53 is low but the entry was stored 2026-03-23 (same day); it will rise with co-access. No deprecation warranted.

---

### #3296 — cfg(feature) gate on mcp/response module vs. WA-5 unconditional types: split mod declaration from re-export (pattern)

**Assessment: KEEP — good quality.**

Structure review:
- **What**: Remove `#[cfg(feature = "mcp-briefing")]` from `mod briefing;` declaration; keep the gate only on MCP tool registration code in tools.rs.
- **Why**: Feature flags should gate MCP handler registration, not the underlying type/formatter modules consumed by non-MCP paths (UDS CompactPayload in this case). Gating the mod declaration caused a compile error when services/index_briefing.rs imported IndexEntry.
- **Scope**: Applies generally to any unimatrix-server module that provides types used by both MCP and non-MCP (UDS) paths.

The pattern is generalizable beyond crt-027 — any future feature adding a type to mcp/response/ that UDS needs will hit this same issue. Clear actionable fix documented. Keep active.

---

### ADRs #3242–#3246, #3251

All six ADRs have the correct Context/Decision/Consequences structure. Gate 3b and 3c both confirmed implementation matches ADR specifications exactly. No implementation contradictions found. See Section 4 for full validation status.

---

## 2. Pattern Extraction

### New patterns stored

| ID | Title | Rationale |
|----|-------|-----------|
| #3324 | Hook-Side Stdout Format Dispatch by Source — Server-Agnostic Event Formatting | SubagentStart uses hookSpecificOutput JSON envelope; UserPromptSubmit uses plain text; both receive HookResponse::Entries from server unchanged. Pattern applies to any new hook type routed to an existing HookRequest variant with different stdout format requirements. Used in hook.rs run() dispatch. |
| #3325 | Three-Step Query Derivation Priority — Shared Free Function for Multi-Caller Services | `derive_briefing_query(task, session_state, topic)` extracted as a pure free function shared by MCP and UDS callers. Each caller resolves its own session_state before calling. Priority: explicit task > session signals > topic fallback. Used in 2 call sites. |
| #3326 | Three-Wave Service Replacement: Build-Migrate-Delete to Minimize Compile Cycles | Wave A: build replacement alongside old service. Wave B: migrate all callers (compile after all migrations, not after each). Wave C: delete old service. Addresses the crt-027 outlier of 70 mutated files and 159 compile cycles from BriefingService deletion. |

### Existing patterns consulted and not duplicated

| ID | Title | Decision |
|----|-------|----------|
| #1266 | Specialized Event-Type Handler Before Generic RecordEvent Dispatch | SubagentStart pattern is a variant of this — adds new match arm before `_` fallthrough. Already covered. No new entry needed. |
| #3230 | SubagentStart hook routing to ContextSearch — implementation pattern | Stored during design (pre-delivery). Describes what was built. Thin — does not include the session_id gotcha or the stdout format dispatch detail. Superseded in detail by #3297 (session_id gotcha) and #3324 (stdout format). Keep #3230 as context; it adds no duplication risk. |
| #3255 | serde(default) alone does not omit None on serialization | Good quality pattern, stored during delivery. Kept — the pair `#[serde(default, skip_serializing_if = "Option::is_none")]` is a genuinely reusable rule for wire protocol optional fields. |
| #3297 | SubagentStart hook routing: input.session_id vs ppid fallback | Good quality gotcha pattern, stored during delivery. Kept — specific enough to be actionable, the ppid fallback is a trap that will recur for any future hook event routed to ContextSearch. |
| #646 | Backward-Compatible Config Extension via serde(default) | Existing general serde(default) pattern. #3255 is more specific (wire enum variant + skip_serializing_if interaction). No conflict. |

### Patterns evaluated and skipped

| Candidate | Reason skipped |
|-----------|---------------|
| hookSpecificOutput envelope as standalone pattern | ADR-006 (#3251) already captures the decision with full context. #3324 covers the dispatch pattern. A third entry would be redundant. |
| IndexEntry typed contract as pattern | ADR-005 (#3246) captures the decision. Entry #1161 (Shared Typed Deserialization Structs for Cross-Module Format Contract) already covers the general principle. No new pattern entry needed. |

---

## 3. Procedure Review

### Existing procedure #2957 — Wave-Based Refactor: Scope cargo test to Affected Crate Per Wave

This procedure exists and is correct in principle. crt-027 provides a concrete application for service replacement specifically (vs. the col-023 type-change refactor that originated #2957). The new lesson #3328 records the crt-027-specific application — 26x compile cycle overage — to reinforce when and how to apply #2957.

No procedure update needed to #2957 itself. The crt-027 guidance (check-only during Wave A/B, test once after Wave B, workspace test only at final gate) is consistent with #2957 and is captured as a lesson (#3328) rather than a procedure modification.

### Hook routing procedure

No new procedure required. The SubagentStart routing pattern is now a pattern entry (#3324). The delivery protocol was updated as part of crt-027 (context_briefing calls at phase boundaries, max_tokens: 1000) — this is a protocol change, not a new procedure.

### Compile/test technique

The wave-based test scoping procedure (#2957) already covers the relevant technique. The lesson #3328 reinforces the specific application to unimatrix-server service replacement. No new procedure stored.

---

## 4. ADR Validation

All six ADRs validated by successful delivery. No supersession required.

| ADR | Entry | Status | Evidence |
|-----|-------|--------|----------|
| ADR-001: Optional source field on HookRequest::ContextSearch | #3242 | VALIDATED | Gate 3b: `#[serde(default, skip_serializing_if = "Option::is_none")]` on source field confirmed. 5 wire round-trip tests pass. Gate 3c: source field absent → None, source present → value, backward compat confirmed. |
| ADR-002: SubagentStart routing + MIN_QUERY_WORDS guard | #3243 | VALIDATED | Gate 3b: SubagentStart arm placed before `_` fallthrough at hook.rs line 401-426. MIN_QUERY_WORDS=5 at line 34. UserPromptSubmit uses `split_whitespace().count()` (functionally equivalent to `trim().split_whitespace()` per Rust semantics — documented in code). Gate 3c: boundary tests build_request_userpromptsub_four_words_record_event and build_request_userpromptsub_five_words_context_search both confirmed. |
| ADR-003: IndexBriefingService replaces BriefingService | #3244 | VALIDATED | Gate 3b: UNIMATRIX_BRIEFING_K not read, deprecation comment present, default_k=20 hardcoded, EffectivenessStateHandle required non-optional. Gate 3c: FR-13 confirmed via grep (parse_semantic_k only in comment); FR-18 confirmed via grep (no BriefingService struct/impl/use remains in non-comment code). |
| ADR-004: CompactPayload flat index migration | #3245 | VALIDATED | Gate 3b: CompactionCategories deleted, format_compaction_payload signature updated. 11 named tests all present and passing. Gate 3c: all 11 non-negotiable test names confirmed via `cargo test -- --list`. |
| ADR-005: IndexEntry typed WA-5 contract surface | #3246 | VALIDATED | Gate 3b: IndexEntry (5 fields), format_index_table, SNIPPET_CHARS=150 all match spec. Re-exported unconditionally (NFR-05). Gate 3c: `snippet_chars_constant_is_150` and `format_index_table_exact_column_layout` pass. |
| ADR-006: SubagentStart stdout hookSpecificOutput JSON envelope | #3251 | VALIDATED | Gate 3b: write_stdout_subagent_inject confirmed at hook.rs lines 667-708, JSON envelope matches ADR-006 exactly. Gate 3c: `write_stdout_subagent_inject_valid_json_envelope` and `write_stdout_plain_text_no_json_envelope` pass. |

One minor implementation divergence noted in Gate 3b (not ADR-level): the UserPromptSubmit word-count guard uses `query.split_whitespace().count()` rather than `query.trim().split_whitespace().count()`. This is functionally equivalent because Rust's `split_whitespace()` already skips leading/trailing whitespace. The code is documented with an inline comment. ADR-002 intent ("trim() semantics") is preserved. Not a contradiction — no supersession required.

---

## 5. Lessons

### Stored

| ID | Title | Source hotspot |
|----|-------|---------------|
| #3327 | Bash Permission Retries Persist Across Features — crt-027 Sets New Worst Case (51 retries) | permission_retries: 51 events (Outlier, 3.4x above mean 14.8). Sixth consecutive feature. Supersedes #2803. |
| #3328 | Service Replacement in unimatrix-server Generates 26x Compile Cycles Without Targeted Test Scoping | compile_cycles: 159 cycles (26x above threshold 6). Root cause: workspace-wide cargo test during BriefingService wave-based replacement. |

### Deprecations

| ID | Title | Reason |
|----|-------|--------|
| #2803 | Bash Permission Retries Persist Across Features — Cargo + Read + context_store Allowlist Fix Not Yet Applied | Superseded by #3327 (crt-027 retro). Updated recurrence count (6 features), new worst case (51 retries), upward trend analysis. |

### Existing lessons reinforced (not re-stored)

| ID | Title | crt-027 data point |
|----|-------|-------------------|
| #2478 / #1269 | High Compile Cycles Signal Need for Targeted Test Invocations | 159 cycles (26x) in crt-027 vs col-022/crt-021 data. Pattern holds. |
| #1271 | Context load and cold restart hotspots scale with component count — normalize before flagging | crt-027 cold_restart (2 instances, 57-min and 476-min gaps, 21 and 81 re-reads) is proportional to feature scope (8 components, 70 mutated files). Not a new lesson — normalizing by scope, the cold restart overhead is expected. |
| #324 | Session gaps without coordinator checkpointing cause expensive context re-reads | crt-027 confirmed: the 476-min gap produced 81 re-reads. Already captured. No new entry. |

### Hotspot assessments

**permission_retries (51 events, Outlier):** Actionable. Stored as #3327 (supersedes #2803). Human action required on settings.json. Trend: nan-002(6), col-022(28), crt-014(21), crt-023(38), crt-027(51) — monotonically increasing with feature size.

**compile_cycles (159 cycles, 26x threshold):** Actionable. Stored as #3328. Root cause: cargo test --workspace during each edit of the BriefingService replacement wave. Procedure #2957 and lesson #3328 together provide the mitigation. No rework resulted — all gates passed first time — so the overhead was recoverable.

**lifespan 489 minutes + mutation_spread 70 files:** Proportional to scope. BriefingService deletion required migrating 2 callers + deleting the file + rewriting 10 tests + updating ServiceLayer wiring = inherently broad. The three-wave pattern (#3326) addresses this for future features. Not flagged as a decomposition failure — the feature scope was correct, the execution pattern was suboptimal.

**cold_restart (2 instances):** crt-027 ran across multiple days (2 sessions, 51742s total). The gaps are explained by session boundaries, not agent error. The 81-re-read instance (476-min gap) is consistent with lesson #324 and #1271. No new lesson warranted; already covered.

**lifespan outlier (uni-rust-dev ran 489 minutes):** The BriefingService replacement is inherently a single-agent responsibility (one file being deleted, one being created, callers in the same crate). Splitting across multiple agents would introduce coordination overhead without benefit given the compile-dependency sequencing. The long lifespan reflects correct scope assignment, not poor decomposition. The compile_cycles reduction from targeted test scoping (lesson #3328) would have reduced this lifespan to ~300-350 minutes.

**search_via_bash (19.2% of bash calls):** Info-level. Use Grep/Glob tools instead of grep/find via Bash. Already documented in CLAUDE.md behavioral rules. No new lesson — agent behavior issue, not architectural.

---

## 6. Retrospective Findings Summary

### Actions taken

1. **Stored 3 new patterns** (#3324, #3325, #3326) — hook-side stdout format dispatch, three-step query derivation, three-wave service replacement.
2. **Stored 2 new lessons** (#3327, #3328) — permission retries new worst case, compile cycle root cause for service replacements.
3. **Deprecated #2803** — superseded by #3327 with updated recurrence data.
4. **Validated all 6 ADRs** — #3242-#3246, #3251 all confirmed by delivery. No supersession required.
5. **Assessed #3253 and #3296** — both kept active. #3253 adequate quality despite crt-027-specific illustration. #3296 good quality and generalizable.

### Recommendations for follow-on work

1. **settings.json allowlist** (human action required): Add cargo build/test/check/clippy and Read tool to allowlist. Entry #3327 documents the full recommended list. This is the highest-ROI single action for future delivery session efficiency.

2. **Adopt wave-based test scoping for next unimatrix-server feature**: When any future feature touches unimatrix-server with 5+ file mutations, apply procedure #2957 + lesson #3328 pattern: cargo check -p unimatrix-server during waves, cargo test -p unimatrix-server once after each wave, cargo test --workspace only at final gate.

3. **WA-5 dependency on crt-027 surface**: IndexEntry, format_index_table, and SNIPPET_CHARS are the compile-time-stable WA-5 contract. Any future feature modifying these types must coordinate with the WA-5 feature owner. ADR-005 (#3246) documents this constraint.

### No-action items

- **edit_bloat_ratio (0.58 vs mean 0.16)**: Outlier explained by BriefingService full replacement (entire file deleted, entire file created). Not an efficiency issue — this is correct behavior for a service replacement.
- **follow_up_issues_created (3)**: Security review found 3 findings during delivery. Normal for a feature touching hook stdout serialization (new JSON construction path). Issues filed and tracked. Not a process failure.
- **output_parsing_struggle (11 cargo commands piped through 3-8 filters)**: Info-level. Agents were searching for specific patterns in build output. Using Grep tool instead of Bash pipes would have been cleaner but did not affect correctness.
