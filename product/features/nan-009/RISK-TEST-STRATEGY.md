# Risk-Based Test Strategy: nan-009

## ~~BLOCKER: "(none)" vs "(unset)" Null-Phase Label Conflict~~ — RESOLVED

**Human decision (design session):** `"(unset)"` is canonical. All artifacts updated.

ADR-003 reasoning adopted: `"(unset)"` unambiguously signals field-not-populated and
cannot collide with a real phase value (real values never use parentheses). `"(none)"`
was ambiguous with a `query_log.phase` row whose value is the string `"none"`.

SPECIFICATION.md Constraint 5, FR-07, AC-05, AC-07, Domain Models, Phase Vocabulary, and
SCOPE.md RD-03 all updated to use `"(unset)"` uniformly. R-01 is closed.

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | "(none)" vs "(unset)" null-label conflict between ADR-003 and SPECIFICATION.md — delivery agent implements one, test asserts the other, test fails | High | High | Critical |
| R-02 | Section renumbering: Distribution Analysis (currently §6) shifts to §7; any test asserting `"## 6. Distribution Analysis"` silently passes a stale assertion if the old heading string is not removed — pattern #3426 | High | High | Critical |
| R-03 | Dual-type partial update: `ScenarioResult` field added to `runner/output.rs` but omitted from `report/mod.rs` — compiles, silently defaults `phase` to `None` in report — #3526, #3550 | High | Med | High |
| R-04 | `insert_query_log_row` test helper not updated to bind `phase` — test inserts `phase = NULL` for all rows, making AC-10 and AC-11 untestable — lesson #3543 (col-028 precedent) | High | High | Critical |
| R-05 | Serde annotation applied to wrong copy: `skip_serializing_if = "Option::is_none"` placed on `report/mod.rs` copy instead of `types.rs`/`runner/output.rs` — would silently suppress `phase` in result JSON, breaking AC-03 — lesson #885 | Med | Med | High |
| R-06 | Phase injected into `ServiceSearchParams` or `AuditContext` during replay — violates measurement purity constraint (SCOPE.md Constraint 3); no compile-time guard | High | Low | High |
| R-07 | `compute_phase_stats` returns non-empty vec when all phases are `None` — section 6 renders a single `"(unset)"` / `"(none)"` row, violating AC-04 omission condition | Med | Med | Med |
| R-08 | `"(unset)"` / `"(none)"` label collides with alphabetical sort — the `(` character sorts before `a`-`z` in ASCII; if the sort is purely lexicographic, the null bucket appears first, not last — ADR-003 notes this explicitly | Med | Med | Med |
| R-09 | `render_phase_section` called when `phase_stats` is empty — produces an empty Markdown table heading instead of being omitted, breaking AC-04 | Med | Med | Med |
| R-10 | UDS-only corpus produces no phase data; eval operator misreads absence of section 6 as a bug rather than expected behaviour — SR-06 | Low | Med | Low |
| R-11 | `aggregate.rs` grows past 500 lines after adding `compute_phase_stats`; file size constraint (Constraint 7) violated | Low | Low | Low |
| R-12 | Section 2 (Notable Ranking Changes) phase label rendered for null-phase scenarios — should be omitted; a missing null-guard produces spurious `"(none)"` labels in section 2 | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: "(none)" vs "(unset)" Null-Phase Label Conflict
**Severity**: High
**Likelihood**: High
**Impact**: Delivery agent implements one literal; the round-trip test (AC-11, ADR-002) asserts the other; gate-3b failure. Alternatively, a delivery agent notices the conflict and makes an arbitrary choice — both artifacts remain inconsistent, creating a maintenance debt.

**Test Scenarios**:
1. `test_compute_phase_stats_null_bucket_label` — call `compute_phase_stats` with a result where `phase = None`, assert the returned `PhaseAggregateStats.phase_label` equals the one canonical string. This test is the ground truth: whichever literal it asserts is the authoritative choice.
2. `test_report_round_trip_phase_section_null_label` — run the full round-trip with a null-phase result, assert the rendered section 6 contains exactly one row whose phase column contains the canonical null label.

**Coverage Requirement**: The null-bucket label literal must appear in exactly one const or function in the implementation. Both test scenarios must assert against the same string. If the two tests use different strings, the conflict is unresolved.

---

### R-02: Section Renumbering Regression
**Severity**: High
**Likelihood**: High
**Impact**: A test that previously asserted `"## 6. Distribution Analysis"` continues to pass after renumbering if the *old* string is not explicitly negated in the updated test. The report silently ships with both `"## 6. Phase-Stratified Metrics"` and `"## 6. Distribution Analysis"` until someone reads the output — pattern #3426, documented as a recurring formatter regression.

**Test Scenarios**:
1. `test_report_round_trip_phase_section_7_distribution` (ADR-002) — asserts `## 6. Phase-Stratified Metrics` present, `## 7. Distribution Analysis` present, `## 6. Distribution Analysis` absent, and section order `pos("## 6.") < pos("## 7.")`.
2. `test_report_contains_all_sections` — updated to enumerate all seven section headings, not five. A section-count regression (e.g., missing `## 7.`) fails immediately.

**Coverage Requirement**: At least one test must assert `!content.contains("## 6. Distribution Analysis")` after renumbering. The section-order assertion (`pos` comparison) must be present. Both are explicit in ADR-002 but must be confirmed present in the implementation.

---

### R-03: Dual-Type Partial Update (Silent Phase Loss)
**Severity**: High
**Likelihood**: Med
**Impact**: `phase` is written correctly into result JSON by `runner/output.rs` but silently defaulted to `None` when `report/mod.rs` deserialises it. Section 6 is never rendered. No compile error, no obvious runtime error — #3526, #3550.

**Test Scenarios**:
1. `test_report_round_trip_phase_section_7_distribution` (ADR-002, step 4) — asserts `content.contains("delivery")`. A partial update causes this to fail because the report-side `phase` is `None` and `"delivery"` does not appear in section 6.
2. `test_scenario_result_phase_round_trip_serde` — serialize `ScenarioResult { phase: Some("design"), ... }` to JSON and deserialize it back using the report module's local type; assert `phase == Some("design")`. This catches a type mismatch without requiring a full report run.

**Coverage Requirement**: The round-trip test must use a non-trivial, non-default phase value (not `None`, not `"design"` if the default is `"design"`). Using `None` allows a partial update to pass silently.

---

### R-04: Test Helper `insert_query_log_row` Not Updated
**Severity**: High
**Likelihood**: High
**Impact**: Integration tests for AC-10 and AC-11 insert rows via the helper. If the helper's `phase` parameter is not added, tests can only exercise the `phase = NULL` path. The `phase = "delivery"` extraction path (AC-10) is never tested. Lesson #3543 documents the exact same failure pattern from col-028.

**Test Scenarios**:
1. `test_scenarios_extract_phase_non_null` — uses the updated `insert_query_log_row(conn, ..., phase: Some("delivery"))` and asserts `context.phase == Some("delivery".to_string())` in the extracted JSONL. If the helper is not updated, this test cannot be written as specified.
2. `test_scenarios_extract_phase_null` — uses `phase: None` in the helper, asserts the `phase` key is absent from the JSONL (AC-02).

**Coverage Requirement**: The `insert_query_log_row` helper must accept `phase: Option<&str>` as a parameter. Tests must include at least one call with `Some(...)` and one with `None`. A helper that silently binds `NULL` for `phase` regardless of the parameter value must cause the non-null test to fail.

---

### R-05: Serde Annotation Applied to Wrong Struct Copy
**Severity**: Med
**Likelihood**: Med
**Impact**: If `skip_serializing_if = "Option::is_none"` is placed on the `report/mod.rs` copy of `ScenarioResult` instead of (or in addition to) the `runner/output.rs` copy, the report module would suppress `phase` from any future re-serialization. The runner copy must always emit `phase` (even as `null`) to satisfy AC-03. Lesson #885 documents gate failures from under-tested serde attributes on complex types.

**Test Scenarios**:
1. `test_scenario_result_phase_null_serialized_as_null` — serialize a `ScenarioResult` (runner-side type) with `phase = None` and assert the output JSON contains `"phase":null` (not a missing key). This verifies the runner copy does NOT have `skip_serializing_if`.
2. `test_scenario_context_phase_null_absent_from_jsonl` — serialize a `ScenarioContext` with `phase = None` and assert NO `"phase"` key is present. This verifies `types.rs` DOES have `skip_serializing_if`.

**Coverage Requirement**: Both tests must exist — they test opposite behaviors on two different types. A single test that only checks one direction leaves the other copy unguarded.

---

### R-06: Phase Injected into Retrieval During Replay
**Severity**: High
**Likelihood**: Low
**Impact**: If `replay_scenario` passes `phase` into `ServiceSearchParams` or `AuditContext`, the eval run measures a phase-augmented retrieval path rather than the baseline pipeline. Results are invalidated. This is the most consequential correctness risk in the feature.

**Test Scenarios**:
1. `test_replay_scenario_phase_not_in_search_params` — construct a `ScenarioRecord` with `context.phase = Some("design")`, call `replay_scenario` (or inspect the `ServiceSearchParams` constructed inside it), assert that `ServiceSearchParams` contains no phase field or that any phase-related weight remains at its default (zero for `w_phase_explicit`).
2. Code review checkpoint: `replay_scenario` in `eval/runner/replay.rs` must be explicitly reviewed to confirm `phase` is only assigned to `ScenarioResult`, never forwarded to search parameters.

**Coverage Requirement**: The test must inspect `ServiceSearchParams` directly (not just `ScenarioResult`) to confirm phase is not present in the search invocation.

---

### R-07: Compute Phase Stats Returns Non-Empty Vec for All-Null Phases
**Severity**: Med
**Likelihood**: Med
**Impact**: If `compute_phase_stats` groups `None` values into a single `"(unset)"` / `"(none)"` bucket even when ALL results are null, it returns a non-empty vec. The renderer then renders section 6 as a single-row table — violating AC-04's omission condition. Operators see the section and assume phase data exists.

**Test Scenarios**:
1. `test_compute_phase_stats_all_null_returns_empty` — call `compute_phase_stats` with a vec where all `phase` fields are `None`. Assert the returned vec is empty.
2. `test_render_phase_section_absent_when_stats_empty` — call `render_report` with an empty `phase_stats` slice. Assert rendered output does not contain `"## 6. Phase-Stratified Metrics"`.

**Coverage Requirement**: The function contract must explicitly state `None`-only input returns empty. The renderer guard must be tested independently from the aggregation logic — both layers must enforce the omission condition.

---

### R-08: Null-Bucket Sort Order
**Severity**: Med
**Likelihood**: Med
**Impact**: A purely lexicographic sort places `"(unset)"` or `"(none)"` before `"bugfix"` because `(` (ASCII 40) precedes `b` (ASCII 98). The spec and ADR-003 both require the null bucket to sort last. A naive `sort_by_key(|s| s.phase_label.clone())` violates this.

**Test Scenarios**:
1. `test_compute_phase_stats_null_bucket_sorts_last` — call `compute_phase_stats` with results for phases `"delivery"`, `"design"`, `"bugfix"`, and `None`. Assert the last element in the returned vec has the null-bucket label, and the preceding elements are in alphabetical order.

**Coverage Requirement**: The sort test must include at least three named phases plus the null bucket to confirm the sort is stable and the null bucket position is last regardless of alphabetical position.

---

### R-09: `render_phase_section` Called with Empty Stats
**Severity**: Med
**Likelihood**: Med
**Impact**: If the renderer calls `render_phase_section` without first checking whether `phase_stats` is empty, it may produce a section heading with no rows. This violates AC-04 (section must be omitted entirely, not rendered as empty).

**Test Scenarios**:
1. `test_render_phase_section_empty_input_returns_empty_string` — call `render_phase_section` with an empty `&[]`. Assert the return value is an empty string (or that the caller in `render_report` skips the call when stats are empty).
2. `test_report_round_trip_null_phase_only_no_section_6` — full report run where all phase values are `None`; assert section 6 heading is completely absent.

**Coverage Requirement**: The omission guard must be exercised at both the section-render level (R-09) and the aggregation level (R-07) — they are independent code paths.

---

## Integration Risks

**IR-01: SQL column name mismatch** — `row.try_get::<Option<String>, _>("phase")` uses a string literal column name. If the SELECT clause spells the column differently (e.g., aliased as `ql_phase`), the `try_get` call returns an error at runtime, not compile time. Test: AC-10 integration test catches this by exercising the full SQL → struct path.

**IR-02: `replay.rs` phase passthrough location** — Phase must be set on `ScenarioResult` after the search invocation returns, not before. Setting it before does not introduce a correctness bug (it's metadata), but placing it inside the search-parameter construction block increases the risk of R-06 (phase accidentally forwarded). Test: explicit review + R-06 scenario 1.

**IR-03: `run_report` pipeline wiring** — `compute_phase_stats` must be called before `render_report` and its result passed as `phase_stats`. A wiring error (passing an empty slice regardless of results, or not calling the function at all) is compile-safe but functionally silent. Test: AC-11 round-trip test.

**IR-04: `render_report` function signature change** — Adding `phase_stats: &[PhaseAggregateStats]` changes the `render_report` signature. All call sites must be updated. Compiler enforces this. Risk is low but the parameter must carry a non-trivial value in the round-trip test to catch silent empty-slice passing.

---

## Edge Cases

**EC-01: Empty scenario corpus** — `eval scenarios` run against a database with no `query_log` rows. `compute_phase_stats` receives an empty vec; must return empty (not panic). `render_phase_section` is never called.

**EC-02: Single phase, single result** — Exactly one result with `phase = Some("delivery")`. Section 6 renders a table with one named row and no null-bucket row. Table must be syntactically valid Markdown.

**EC-03: Phase string contains special Markdown characters** — A future session type named `"design/research"` would produce a table cell with a forward slash. The renderer must not break Markdown table syntax. No sanitization is currently specified; this is an accepted risk for the current vocabulary.

**EC-04: Extremely long phase string** — `query_log.phase` is a free-form string. A very long value (e.g., 200 chars) produces an unreadable table row. Current scope accepts this; the harness is not a UI. No truncation specified.

**EC-05: Mixed pre- and post-col-028 result files** — `eval report` processes a directory containing both legacy files (no `phase` key) and new files (with `phase`). Deserialization must succeed for both via `#[serde(default)]`. The null-phase results contribute to the null bucket in `compute_phase_stats`; if all non-legacy results have `phase = None` too, section 6 is still omitted (R-07).

**EC-06: Phase key present as explicit `null` in JSON** — Result files written by `eval run` always emit `"phase":null` when phase is `None` (runner copy has no `skip_serializing_if`). The report module's `#[serde(default)]` handles this. Explicit `null` and absent key must both deserialize to `None`. Test: AC-06.

---

## Security Risks

**SEC-01: Phase value as free-form string in Markdown output** — The phase label is written verbatim into a Markdown table cell in the rendered report. The report is consumed by humans reading Markdown, not by a parser. A crafted phase string such as `"x | injected_col | y"` would break the table structure. The risk is low because `query_log.phase` is set by `context_cycle` which reads from a controlled protocol definition — it is not user-facing input. The blast radius is limited to report readability, not data loss. No sanitization is required in the current scope; this is a documentation-only risk.

**SEC-02: SQL injection via phase filter** — The `eval scenarios` SQL query SELECTs `phase` as a read-only column. There is no user-supplied phase filter added to the query in this feature (Non-Goal: no `--phase` CLI flag). No injection surface is introduced. Risk: None.

**SEC-03: Deserialization of untrusted JSONL** — Scenario JSONL files are read from a directory specified by the operator. A malicious JSONL file with a crafted `phase` field (e.g., deeply nested, very long string) would be deserialized. `serde_json` with `Option<String>` does not execute arbitrary code and is not susceptible to deserialization attacks. The `phase` field is a scalar string; no gadget chain is possible. Risk: Low.

---

## Failure Modes

**FM-01: Section 6 absent when expected** — Operator runs `eval report` expecting phase output but section 6 is missing. Expected causes: all scenarios have `phase = None` (UDS corpus or pre-col-028 data). Expected behavior: no error or warning; report renders normally without section 6. Mitigation documented in FR-11 / eval-harness.md (SR-06).

**FM-02: Unknown phase value appears in section 6** — A typo in `context_cycle` produces an unexpected phase string (e.g., `"delivvery"`). Expected behavior: the typo appears as its own row in section 6. No validation, no warning. Operator must detect the typo visually. This is explicitly accepted by ADR-003.

**FM-03: `compute_phase_stats` panics on empty results** — If the aggregation function indexes into an empty slice. Expected behavior: return empty vec. Test: EC-01.

**FM-04: `eval report` fails to open result JSON files** — Pre-existing failure mode, unchanged by nan-009. Expected behavior: `run_report` emits `WARN: no result JSON files found` and exits. Phase logic is never reached.

**FM-05: `eval run` produces result JSON without `phase` key** — Occurs when replay runs against scenario files that have no `phase` in context (pre-col-028 extraction). Expected behavior: `ScenarioResult.phase` is `None`; result JSON includes `"phase":null` (runner always emits); report reads `None` via `#[serde(default)]`. Section 6 omitted. No error.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Specification Mitigation | Resolution |
|-----------|------------------|--------------------------|------------|
| SR-01: `serde(default)` emits explicit `null` — wire shape change | — | NFR-02, FR-02, Constraint 1 | Resolved by ADR-001: `skip_serializing_if = "Option::is_none"` on `ScenarioContext.phase` in `types.rs`; runner copy always emits (AC-03 requires it); report copy uses `serde(default)` only |
| SR-02: Section renumbering breaks tests asserting exact headings | R-02 | AC-12, AC-11, AC-09(5) | Mitigated by ADR-002 round-trip test — must assert `!content.contains("## 6. Distribution Analysis")` and section order. Still a critical delivery risk if test is weak. |
| SR-03: Dual-type partial update compiles silently, drops phase from report | R-03 | NFR-05, AC-11, Constraint 2 | Mitigated by ADR-002 round-trip test. Risk remains if delivery agent uses a zero/null phase value in the test — non-trivial value is mandatory. |
| SR-04: `"(none)"` vs `"(unset)"` label inconsistency in SCOPE.md | R-01 | All artifacts updated to `"(unset)"` — human-confirmed in design session | **RESOLVED.** `"(unset)"` is canonical throughout. |
| SR-05: All-null corpus produces degenerate single-bucket table | R-07, R-09 | AC-04, Constraint 5 (conditional omission) | Resolved in spec: section omitted when `compute_phase_stats` returns empty. AC-04 requires both the omission condition and an explicit test. |
| SR-06: UDS-only corpus produces no phase section — may look like a bug | R-10 | FR-11, AC-07 (documentation note) | Mitigated by doc update. Low risk; no test required beyond documentation review. |
| SR-07: `compute_phase_stats` must be synchronous — risk of async leakage | — | NFR-03, Constraint 4 | Resolved architecturally: report module has no async runtime. Compiler enforces this if the function signature is `fn` (not `async fn`). No test needed beyond compilation. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-04) | R-01: 2 scenarios; R-02: 2 scenarios; R-04: 2 scenarios — 6 total |
| High | 4 (R-03, R-05, R-06, R-07) | R-03: 2; R-05: 2; R-06: 2; R-07: 2 — 8 total |
| Med | 5 (R-08, R-09, R-12 + EC/FM coverage) | R-08: 1; R-09: 2; R-12: 1 — 4 minimum |
| Low | 2 (R-10, R-11) | Documentation review (R-10); file-size check (R-11) |

Minimum required test count: 18 scenario-level tests plus 1 code review checkpoint (R-06).

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search "lesson-learned failures gate rejection" — found #3543 (nullable column compile-silent test helper, col-028), #885 (serde gate failure col-020), #3548 (test exists but omits assertion, nan-008)
- Queried: /uni-knowledge-search "risk pattern eval harness section order golden output" — found #3426 (formatter section-order regression, golden-output required), #3526 (round-trip dual-type pattern, nan-008), #3522 (ADR-003 nan-008 precedent)
- Queried: /uni-knowledge-search "serde dual-type ScenarioResult round-trip" — found #885, #3526, #3522
- Queried: /uni-knowledge-search "eval harness SQLite query_log column extraction rework" — found #3543, #3555 (eval harness phase gap)
- Stored: nothing novel to store — all patterns already captured in #3426, #3526, #3543, #3550; the "(none)" vs "(unset)" conflict is feature-specific, not a recurring pattern
