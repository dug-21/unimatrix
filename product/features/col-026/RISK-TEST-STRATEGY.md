# Risk-Based Test Strategy: col-026

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Inline `* 1000` timestamp conversion in PhaseStats bypasses `cycle_ts_to_obs_millis()`, silently misplacing phase boundaries | High | Med | Critical |
| R-02 | Phase window extraction produces wrong boundaries when `cycle_phase_end` events are absent, malformed, or share a timestamp with `cycle_start` | High | Med | Critical |
| R-03 | GateResult inference is broken by free-form outcome text — multi-keyword collisions, prefixed words, or empty strings yield wrong enum variant | High | High | Critical |
| R-04 | IN-clause batch query returns fewer rows than requested (quarantined/deleted entries) — cross-feature split arithmetic is silently wrong | High | Med | Critical |
| R-05 | `is_in_progress` derivation omits the `None` branch — pre-col-024 historical retros reported as `Some(false)` (confirmed-complete) instead of `None` | High | Med | Critical |
| R-06 | `What Went Well` metric direction table has a mis-classified entry — a higher-is-better metric classified as lower-is-better produces a false-positive favorable signal | High | Low | High |
| R-07 | Formatter section reorder touches every existing rendering test — undetected regression in sections not explicitly targeted (phase outliers, rework, baseline outliers) | Med | High | High |
| R-08 | Threshold language regex fails on composite claim strings — strip-and-replace leaves a malformed sentence or strips valid content | Med | High | High |
| R-09 | `attribution_path` assignment in the handler's three-path conditional chain is missing or wrong for the `Mixed` / multiple-session case | Med | Med | High |
| R-10 | `phase_stats` formatter populates `hotspot_ids` using earliest evidence timestamp — if a finding's evidence spans multiple phases, the wrong phase receives the annotation | Med | Med | High |
| R-11 | Tenth threshold language site added by a future detection rule goes undetected — the enumerated nine-file audit has no regression guard | Med | Low | Med |
| R-12 | `phase_stats = Some(vec![])` (empty but Some) vs `None` renders identically but may signal different conditions to JSON consumers | Low | Med | Med |
| R-13 | `FeatureKnowledgeReuse` construction sites outside `unimatrix-observe` not updated when new required fields are added — compile-time break | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Inline Timestamp Conversion in PhaseStats
**Severity**: High
**Likelihood**: Med
**Impact**: Observations are placed in the wrong phase window. Phase duration, record count, agent list, knowledge served/stored per phase are all wrong. No runtime error — silent data corruption.

**Test Scenarios**:
1. Construct a `PhaseStats` computation test where `cycle_phase_end.timestamp` is a known epoch-seconds value (e.g., `1700000100`) and at least one observation has `ts_millis` that is exactly `cycle_ts_to_obs_millis(1700000100)`. Assert the observation appears in the correct phase window and not the adjacent one.
2. Construct a boundary test where an observation's `ts_millis` is exactly at the window boundary (start inclusive, end exclusive). Assert correct inclusion/exclusion on both sides.
3. Grep `PhaseStats` computation code for `* 1000` — assert zero matches (static lint, enforced by ADR-002).

**Coverage Requirement**: A millisecond-boundary test that would fail if `ts_secs * 1000` were used instead of `cycle_ts_to_obs_millis(ts_secs)`. Must exercise the saturating_mul overflow guard path via the helper (pass `i64::MAX / 1000 + 1` as input).

---

### R-02: Phase Window Extraction Edge Cases
**Severity**: High
**Likelihood**: Med
**Impact**: Empty phase windows produce zero-record `PhaseStats` rows that exist in the output but contain no data. Zero-duration cycles (same `cycle_start` and `cycle_stop` timestamp) produce `duration_secs = 0` which may render as `0h 0m` or trigger division-by-zero in downstream ratios. Missing `phase` names on `cycle_phase_end` events produce empty strings in the `phase` column of the Phase Timeline.

**Test Scenarios**:
1. Provide `events` with zero `cycle_phase_end` rows (only `cycle_start` + `cycle_stop`). Assert `phase_stats` contains exactly one entry covering the full window duration.
2. Provide `events` where `cycle_start.timestamp == cycle_stop.timestamp`. Assert `PhaseStats.duration_secs == 0`, rendered as `0m`, and no panic/divide-by-zero occurs.
3. Provide a `cycle_phase_end` event with `phase = ""` (empty string). Assert the formatter renders `—` or `(unknown)` rather than an empty table cell.
4. Provide `events` with no observations in a phase window (empty slice after filter). Assert `record_count = 0`, `knowledge_served = 0`, `gate_result = Unknown`.

**Coverage Requirement**: All four edge case paths exercised. Zero-duration must not panic. Missing phase name must produce a non-empty rendered row.

---

### R-03: GateResult Inference from Free-Form Outcome Text
**Severity**: High
**Likelihood**: High
**Impact**: A `GateResult::Pass` reported for a failed gate misleads the phase timeline. A `Fail` reported for a successful gate undermines trust in the report. Multi-keyword strings (e.g., `"pass after rework"`) hit the wrong branch depending on evaluation order.

**Test Scenarios**:
1. Outcome `"PASS"` → `GateResult::Pass`. Outcome `"pass"` → `Pass` (case-insensitive).
2. Outcome `"failed: type errors"` → `GateResult::Fail`. Outcome `"error in gate 2b"` → `Fail`.
3. Outcome `"rework required"` → `GateResult::Rework`. Outcome `"REWORK"` → `Rework`.
4. Outcome `"approved on second pass"` — contains neither `pass` (from "approved"? — no) nor `fail` nor `rework` → assert `Unknown`. Note: "approved" contains no "pass" substring; verify the spec's exact keyword list ("pass", "complete", "approved" per spec domain model line 500).
5. Outcome `""` (empty string) → `GateResult::Unknown`.
6. Outcome `None` (`gate_outcome_text` absent on `cycle_phase_end`) → `GateResult::Unknown`.
7. Outcome `"pass after rework"` — contains both "pass" and "rework". Assert evaluation order matches spec (pass-count-based logic from spec line 500: `pass_count > 1` with final pass succeeding → `Rework` takes precedence over `Pass`).
8. Outcome `"compass"` — contains "pass" as substring. Assert this does NOT produce `GateResult::Pass` unless the spec intends substring matching for embedded words. This is a known fragility of naive `contains()` matching.

**Coverage Requirement**: All keyword variants (case-insensitive), empty/None input, and the multi-keyword collision case tested. Scenario 8 documents whether embedded-word matches are accepted or guarded against.

---

### R-04: Batch IN-Clause Returns Fewer Rows Than Requested
**Severity**: High
**Likelihood**: Med
**Impact**: Served entries with no `EntryMeta` result are silently excluded from `cross_feature_reuse` / `intra_cycle_reuse` counts. The split arithmetic `cross + intra` does not equal `delivery_count`, producing an inconsistent Knowledge Reuse section.

**Test Scenarios**:
1. Synthetic `entry_meta_lookup` closure returns metadata for only 3 of 5 served entry IDs. Assert: (a) no panic; (b) the 2 missing entries are counted as intra-cycle (or excluded — spec must confirm); (c) `cross_feature_reuse + intra_cycle_reuse <= delivery_count`.
2. All served entry IDs return no metadata (empty HashMap). Assert `cross_feature_reuse = 0`, `intra_cycle_reuse = 0`, no panic.
3. Entry ID set is empty (no entries served). Assert `entry_meta_lookup` closure is NOT called (per ADR-003: skip call when set is empty). Assert `delivery_count = 0` renders as "No knowledge entries served."

**Coverage Requirement**: Missing-metadata entries must not cause arithmetic that overcounts or panics. Empty entry set must skip the batch call.

---

### R-05: `is_in_progress` Three-State Derivation
**Severity**: High
**Likelihood**: Med
**Impact**: Pre-col-024 historical retro calls with no `cycle_events` rows are reported as `Some(false)` (confirmed-complete), misleading JSON consumers. Concretely: if the handler defaults to `is_in_progress = Some(false)` when `events.is_empty()`, every legacy retro shows `Status: COMPLETE`.

**Test Scenarios**:
1. `events = vec![]` → `is_in_progress = None`. Assert formatter omits Status line entirely.
2. `events` contains `cycle_start` but no `cycle_stop` → `is_in_progress = Some(true)`. Assert header includes `Status: IN PROGRESS`.
3. `events` contains both `cycle_start` and `cycle_stop` → `is_in_progress = Some(false)`. Assert header shows `Status: COMPLETE` (or omits Status line per spec FR-05: "When `Some(false)` or `None`, no Status line appears").
4. Serde roundtrip: serialize `RetrospectiveReport` with `is_in_progress = None` → assert key absent from JSON. Deserialize a JSON payload lacking `is_in_progress` key → assert field defaults to `None`, not `Some(false)`.

**Coverage Requirement**: All three states exercised in both derivation logic and formatter rendering. Serde roundtrip for the None case (key-absent, not null-valued).

---

### R-06: Metric Direction Table Mis-Classification
**Severity**: High
**Likelihood**: Low
**Impact**: A metric mis-classified as lower-is-better when it is higher-is-better (or vice versa) reports an unfavorable performance as a "What Went Well" signal, or suppresses a genuinely good signal. For example, if `parallel_call_rate` (higher-is-better) were marked lower-is-better, a cycle with high parallel call rate would never appear in What Went Well.

**Test Scenarios**:
1. For each metric in the direction table, construct a `BaselineComparison` where `current_value < mean` and assert the result matches the expected favorable classification per direction.
2. Specifically test `parallel_call_rate` (higher-is-better): `current = 0.8`, `mean = 0.5` → should be favorable. `current = 0.2`, `mean = 0.5` → should NOT be favorable.
3. Specifically test `knowledge_entries_stored` and `follow_up_issues_created` (both higher-is-better): verify both are in the "higher" direction.
4. A metric not in the direction table should be excluded from What Went Well regardless of its value.
5. All `is_outlier = true` metrics must be excluded even if direction is favorable.

**Coverage Requirement**: The complete direction table (16 entries) must be validated by tests — either a data-driven test iterating the table or individual assertions for the higher-is-better exceptions.

---

### R-07: Formatter Section Reorder Regression
**Severity**: Med
**Likelihood**: High
**Impact**: Moving Recommendations from position 9 to position 2 and inserting Phase Timeline and What Went Well sections changes the string offset of every subsequent section. Existing tests that assert section content by string position or check that one string does NOT appear before another may silently pass while the actual section order is wrong.

**Test Scenarios**:
1. Full-report formatter test with all sections populated: assert Recommendations section appears before (earlier string index than) Phase Timeline, which appears before What Went Well, which appears before Sessions, which appears before Findings.
2. Assert "## Recommendations" does not appear after "## Findings" in any rendered output.
3. Assert "Phase Timeline" appears before "Findings" when both sections are present.
4. All 12 section headers present in the correct order in the output string (verified by scanning the output string for each header in sequence, asserting each appears after the previous one).

**Coverage Requirement**: A single golden-order test verifying all 12 section headers appear in the specified sequence. This test catches any section inversion.

---

### R-08: Threshold Language Regex Strip-and-Replace
**Severity**: Med
**Likelihood**: High
**Impact**: A regex that is too broad strips valid content from the claim string. A regex that is too narrow misses a threshold pattern variant. A post-strip claim that begins with a comma or space produces malformed output.

**Test Scenarios**:
1. Claim `"43 compile cycles (threshold: 10) -- 4.3x typical"`: assert stripped output contains no "threshold" and appends correct baseline or ratio framing.
2. Claim with `stddev > 0` baseline available: assert output ends with `(baseline: {mean} ±{stddev}, +{z}σ)`.
3. Claim with no baseline (stddev = 0 or absent): assert output ends with `({ratio:.1}× typical)`.
4. Claim where threshold value is 0.0: assert ratio annotation is skipped entirely (no division by zero, no "inf× typical").
5. Claim with no threshold pattern: assert claim rendered unchanged (no content stripped).
6. All 9 enumerated detection files' claim formats: each must produce a claim string that after post-processing contains no "threshold: N" substring (data-driven test over the audit list in ARCHITECTURE.md §Component 5).
7. Formatter output for `compile_cycles` finding: assert rendered output does not contain "allowlist".

**Coverage Requirement**: The `format_claim_with_baseline` private function tested independently for all three path cases (baseline with stddev, ratio fallback, no threshold pattern). Each of the 9 enumerated claim formats tested with a synthetic detection output.

---

### R-09: Attribution Path Assignment in Multi-Session Fallback
**Severity**: Med
**Likelihood**: Med
**Impact**: When the primary path (`load_cycle_observations`) returns non-empty for some sessions but path 2 or 3 handles others, the `attribution_path` variable must correctly reflect the path that produced the dominant/non-empty result. A wrong label misleads agents about the data quality of the report.

**Test Scenarios**:
1. `load_cycle_observations` returns non-empty → `attribution_path = "cycle_events-first (primary)"`.
2. `load_cycle_observations` returns empty, `load_feature_observations` returns non-empty → `attribution_path = "sessions.feature_cycle (legacy)"`.
3. Both primary and legacy return empty, `load_unattributed_sessions` used → `attribution_path = "content-scan (fallback)"`.
4. All three paths return empty → `attribution_path = None` (or a defined "no data" sentinel — spec must confirm).
5. Assert the `attribution_path` value stored on `RetrospectiveReport` matches the label rendered in the header Attribution line exactly (no string transformation in formatter).

**Coverage Requirement**: All three path labels tested as handler integration tests. Path 4 (all empty) must be defined and covered.

---

### R-10: Hotspot Phase Annotation — Multi-Phase Finding
**Severity**: Med
**Likelihood**: Med
**Impact**: A finding whose evidence spans two phases (e.g., `compile_cycles` firing across both `implementation` and `review`) receives the annotation of the phase with the highest event count. If the count-comparison uses the wrong evidence slice, the annotation points to the wrong phase.

**Test Scenarios**:
1. Finding with evidence in phase A (3 events) and phase B (7 events): assert annotation is `— phase: B/1` (higher count wins).
2. Finding with evidence entirely in one phase: assert annotation matches that phase.
3. Finding with no phase mapping available (`phase_stats` is None): assert finding header rendered without `— phase:` annotation.
4. Finding whose earliest evidence timestamp falls before the first phase window start: assert no annotation rather than an out-of-bounds panic.

**Coverage Requirement**: Multi-phase tie-breaking logic tested. Out-of-bounds timestamp must not panic.

---

### R-11: Tenth Threshold Language Site — Future Regression
**Severity**: Med
**Likelihood**: Low
**Impact**: A new detection rule added post-col-026 that produces a claim string with threshold language bypasses the nine-site audit. The formatter's post-processing regex catches it only if the new claim format matches the regex. If the regex uses a whitelist approach (per-metric matching), the new site is invisible.

**Test Scenarios**:
1. After all formatter changes are complete, run a full-text search across `unimatrix-observe/src/detection/` and `report.rs` for the pattern `threshold` in claim-string contexts. Assert the count matches the audited 9 sites plus any explicitly added ones (snapshot test on the count).
2. The `format_claim_with_baseline` function must use a general regex (not a per-metric allowlist) so that future claim strings with threshold language are automatically post-processed.

**Coverage Requirement**: A count-based snapshot test on threshold-containing claim strings in detection code. Documents the baseline count; alerts on unexpected growth.

---

### R-12: `phase_stats = Some(vec![])` vs `None`
**Severity**: Low
**Likelihood**: Med
**Impact**: The formatter spec says "when `phase_stats` is None or empty, show `No phase information captured.`" — so the rendered output is identical for both states. However, JSON consumers who distinguish `None` (key absent) from `Some([])` (key present, empty array) will see different JSON shapes. This may cause subtle parsing bugs in downstream consumers.

**Test Scenarios**:
1. `phase_stats = None` → JSON output does not contain `"phase_stats"` key (key absent via `skip_serializing_if`).
2. `phase_stats = Some(vec![])` → JSON output contains `"phase_stats": []` (key present, empty array). Assert formatter renders "No phase information captured." for both.
3. The handler must never set `phase_stats = Some(vec![])` — on any error or empty result, it must set `None`. Assert the computation step returns `None` (not empty vec) when `events` is empty.

**Coverage Requirement**: Handler must canonicalize the empty-result case to `None` before setting the report field.

---

### R-13: `FeatureKnowledgeReuse` Construction Site Migration
**Severity**: Low
**Likelihood**: Low
**Impact**: New non-optional fields added to `FeatureKnowledgeReuse` break existing struct literal construction sites with a compile-time error. This is caught by the compiler, but only if implementation agents compile before committing.

**Test Scenarios**:
1. Compilation of the workspace after adding new fields to `FeatureKnowledgeReuse` — zero compile errors (all construction sites updated).
2. Assert `FeatureKnowledgeReuse` struct literal in `retrospective.rs` test fixtures includes the new `cross_feature_reuse`, `intra_cycle_reuse`, `total_stored`, `top_cross_feature_entries` fields.

**Coverage Requirement**: CI compilation gate. No additional test needed beyond `cargo build`.

---

## Integration Risks

### PhaseStats Computation — Handler Step Sequencing
Step 10h (PhaseStats) depends on `events` from step 10g and `attributed` from step 3. If either upstream step sets its result to an empty slice (best-effort degradation), PhaseStats computation must handle empty inputs without panicking. The error boundary wraps the entire step — but if `events` is unexpectedly `None` (not possible per current code, but possible after refactor), the handler should not crash.

Test: provide `attributed = vec![]` with non-empty `events`. Assert PhaseStats rows are produced with `record_count = 0`, `knowledge_served = 0`.

### `compute_knowledge_reuse` Extended Signature — Caller Migration
The addition of the `entry_meta_lookup` closure parameter to `compute_knowledge_reuse` changes the function signature. All call sites must be updated. The three known construction sites of `FeatureKnowledgeReuse` (types.rs tests, knowledge_reuse.rs, retrospective.rs tests) must all supply the new closure. A missed call site produces a compile error — but only if the agent compiles.

Test: the existing `compute_knowledge_reuse` unit tests must be updated to supply a synthetic `entry_meta_lookup` closure. The synthetic closure must be verified to be called exactly once per invocation.

### Phase Narrative vs. Phase Stats — Shared `events` Slice
Both `phase_narrative` (step 10g) and `PhaseStats` (step 10h) consume `events: Vec<CycleEventRecord>`. If step 10g takes ownership of `events` and step 10h cannot borrow it, there is a Rust ownership error. The architecture uses shared references (`&[CycleEventRecord]`); the implementation must not move `events` into step 10g.

Test: compile-time verification. No runtime test needed, but the implementation spec must state `events` is borrowed, not moved, by `build_phase_narrative`.

---

## Edge Cases

### Zero-Duration Phase Window
`cycle_start.timestamp == cycle_stop.timestamp` (or `cycle_phase_end.timestamp == cycle_start.timestamp`) produces a window of zero seconds. `duration_secs = 0` must render as `0m` without divide-by-zero in any throughput computation (knowledge per minute, records per minute).

### Phase Name Absent from `cycle_phase_end`
`cycle_phase_end` events where `phase` is null/empty produce a `PhaseStats.phase = ""`. The formatter must render a non-empty placeholder (`—` or `unknown`) rather than an empty table cell.

### Burst Notation with Single Evidence Record
A finding with exactly one evidence record must produce `Timeline: +0m(1▲)` with a `Peak:` line showing that record. The truncation logic (max 10 entries) must not misfire on single-record input.

### `session_started_at` Absent for Relative Time Origin
If all `SessionSummary.started_at` values are zero or missing, the relative time origin is undefined. Burst notation must fall back to showing raw time offsets from the first evidence record rather than from session start.

### Knowledge Reuse with Entry ID Exceeding 100 (Chunking)
When `delivery_count > 100`, the batch IN-clause chunking strategy (pattern #883, chunks of 100) must union results correctly. Entry IDs that span a chunk boundary must appear in exactly one chunk's result, not duplicated or dropped.

### `top_cross_feature_entries` Fewer Than 3 Results
The spec says "top 3–5". When only 1 or 2 cross-feature entries exist, the table must render with 1 or 2 rows (not error or show empty). Minimum is "available entries up to 5" — the "3–5" range is the target, not a hard minimum.

---

## Security Risks

### `goal` String in Header Output
The `goal` field is a `TEXT NULL` column written by agents via `context_cycle` MCP tool. It is agent-authored free-form text. Rendered directly into markdown output without escaping. Risk: an agent could store markdown-injection content (e.g., `goal = "## Injected Section\n..."`) that corrupts the report structure when rendered. Blast radius: report output only — no DB writes, no code execution, no file system access. The formatter is a read-only markdown renderer.

Test: store a goal containing markdown headers and assert the rendered output does not create spurious top-level sections. Consider stripping or escaping newlines in the `goal` field before rendering.

### `gate_outcome_text` in Rework Annotation
The `outcome` column on `cycle_phase_end` is agent-authored free-form text included verbatim in the rework annotation. Same injection risk as `goal`. Same blast radius (markdown output only).

Test: provide an `outcome` containing `\n## Injected` and assert it does not create a section header in the rendered output.

### `entry_title` in Top Cross-Feature Entries Table
Entry titles are fetched from the `entries` table and rendered in the Knowledge Reuse section. These are agent-authored strings. Markdown special characters in titles (backticks, pipes) could break the table cell rendering.

Test: entry title containing `|` character — assert table cell is escaped or the character does not break the table structure.

### Batch IN-Clause SQL Injection
The `entry_meta_lookup` closure executes a SQL query with entry IDs as bind parameters. Entry IDs are `u64` values from the DB's own index — they cannot contain SQL injection. No risk here, but the closure implementation must use parameterized queries (bind parameters, not string interpolation). This is enforced by rusqlite's API; no additional test needed beyond verifying the implementation uses `rusqlite::params![]`.

---

## Failure Modes

### PhaseStats Computation Error
Expected behavior: `phase_stats = None`, `warn!` log emitted, report continues normally. The Phase Timeline section shows "No phase information captured." All other sections render normally. The handler must not return an error response when PhaseStats computation fails.

### `get_cycle_start_goal` DB Error
Expected behavior: `goal = None`, `cycle_type = None` or `"Unknown"`, header omits Goal and Cycle type lines. `is_in_progress` derivation is unaffected (uses already-loaded `events`).

### Batch IN-Clause Query Error
Expected behavior: `cross_feature_reuse = 0`, `intra_cycle_reuse = 0`, `top_cross_feature_entries = vec![]`. The Knowledge Reuse section renders with zero cross-feature count rather than failing the entire tool invocation.

### All Attribution Paths Return Empty
Expected behavior: `attributed = vec![]`, report renders with all count-based sections showing zero records. The `attribution_path` field should indicate the path that was attempted (even if it returned empty) rather than `None`. Spec should define this behavior explicitly.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — `ts_millis` vs `timestamp` unit mismatch | R-01 | ADR-002: `cycle_ts_to_obs_millis()` is the only permitted conversion. ARCHITECTURE.md §Component 3 mandates call by name. Spec NFR-02 repeats the prohibition. |
| SR-02 — N+1 DB read in knowledge reuse split | R-04 | ADR-003: `entry_meta_lookup` closure takes `&[u64]` slice — structurally prevents N+1. Single IN-clause call. Chunking via pattern #883. |
| SR-03 — `is_in_progress: bool` semantic corruption | R-05 | ADR-001: `Option<bool>` with three-state semantics. NFR-04 explicitly prohibits `bool`. |
| SR-04 — Formatter overhaul blast radius / no golden-output test | R-07 | Spec AC-11, AC-17, NFR-05 mandate: section order enforced via numbered comments, existing tests must pass, new section-order test required. |
| SR-05 — Threshold language audit completeness | R-08, R-11 | ADR-004: nine-site enumeration in ARCHITECTURE.md §Component 5. Formatter-side post-processing via general regex (not per-metric allowlist). R-11 identifies the tenth-site future regression risk. |
| SR-06 — "No phase captured" note proliferation | R-12 | ARCHITECTURE.md §SR-06: simple per-cycle check (not cross-cycle). Formatter spec: note is a single line, no section header. |
| SR-07 — col-024/025 in-flight API dependency | — | ARCHITECTURE.md §Integration Points pins exact function signatures, field names, and types. Spec §API Surface Assumed from col-024/025 repeats the pin. col-026 branch cut from main only after both merge. Accepted risk — no architecture-level mitigation possible. |
| SR-08 — `FeatureKnowledgeReuse` public API change | R-13 | ARCHITECTURE.md §FeatureKnowledgeReuse Construction Sites enumerates all three sites. Adding fields is compile-time-enforced migration. `#[non_exhaustive]` not needed: all sites are known and updated. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01–R-05) | 22 scenarios |
| High | 5 (R-06–R-10) | 18 scenarios |
| Med | 2 (R-11–R-12) | 5 scenarios |
| Low | 1 (R-13) | 2 scenarios |
