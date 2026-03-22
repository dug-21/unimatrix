# Risk-Based Test Strategy: crt-026 — WA-2 Session Context Enrichment

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Test must assert numerical floor on score delta — "ranks higher" alone is not sufficient | Med | Low | Medium |
| R-02 | Cold-start regression — empty histogram path produces different scores than pre-crt-026 | High | Med | High |
| R-03 | Duplicate store increments histogram — non-duplicate guard bypassed or placed incorrectly | High | Med | High |
| R-04 | Unregistered session causes panic or side effect in `record_category_store` | Med | Low | Medium |
| R-05 | UDS search path omits histogram pre-resolution — boost silently absent on hook-driven searches | High | Med | High |
| R-06 | `FusionWeights::effective()` NLI-absent re-normalization denominator includes `w_phase_histogram` — dilutes existing weights | High | Med | High |
| R-07 | `phase_explicit_norm` hardcoded `0.0` removed as "dead code" in future — placeholder contract broken | Med | Med | Medium |
| R-08 | Status penalty applied before histogram boost — deprecated entries escape penalty via histogram lift | Med | Low | Medium |
| R-09 | `p(category)` total-count division by zero when histogram is non-empty but total sums to zero | Med | Low | Medium |
| R-10 | Histogram summary emitted when histogram is empty — spurious `Recent session activity:` block in CompactPayload | Med | Med | Medium |
| R-11 | `w_phase_histogram` or `w_phase_explicit` range validation missing — out-of-range config accepted silently | Med | Low | Medium |
| R-12 | `ServiceSearchParams` struct literal construction sites not updated — compile failure or silent field default | Med | High | High |
| R-13 | Pre-resolution placed after an `await` point — race with concurrent session mutation | Med | Low | Medium |
| R-14 | WA-2 extension stubs not removed — stub comment persists and confuses W3-1 integration | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Test must assert numerical floor on score delta
**Severity**: Med
**Likelihood**: Low
**Impact**: AC-12 passes vacuously or uses imprecise assertions. Regression in boost computation goes undetected. W3-1 receives an unvalidated cold-start seed.

Note: at `w_phase_histogram=0.02` (ADR-004), the boost is clearly detectable at realistic
histogram concentrations. A 50% concentration produces a boost of `0.01`, a p=1.0 test
produces exactly `0.02`. Extreme concentration is NOT required for detectability.

**Test Scenarios**:
1. Manufacture a session histogram where one category (e.g., `"decision"`) is the only stored category: histogram = `{"decision": 5}`, total = 5, `p("decision") = 1.0`. Construct two `ScoredEntry` values with all fused inputs equal except category. Assert `score(decision) - score(other) == 0.02` (exactly `w_phase_histogram * 1.0`).
2. Verify that a histogram with 60% concentration: `{"decision": 3, "pattern": 2}` (total = 5) produces `phase_histogram_norm("decision") = 0.6` and a score delta of exactly `0.02 * 0.6 = 0.012`.
3. Verify that a category absent from the histogram produces `phase_histogram_norm = 0.0` and zero boost (AC-13).

**Coverage Requirement**: AC-12 must assert a score delta of `≥ 0.02` with `p=1.0` concentration. No "ranks higher" assertion without a numerical floor.

---

### R-02: Cold-start regression — empty histogram changes existing ranking behavior
**Severity**: High
**Likelihood**: Med
**Impact**: All sessions without prior stores receive different ranking output than pre-crt-026. Silent breakage affecting every agent that does not use session context.

**Test Scenarios**:
1. Search with `category_histogram = None` (no session); assert per-candidate `phase_histogram_norm = 0.0` for all entries.
2. Search with `category_histogram = Some(HashMap::new())` (empty map) — handler maps this to `None` before `ServiceSearchParams` construction; assert same result as scenario 1.
3. Compute `compute_fused_score` with all six existing inputs at known values and `phase_histogram_norm = 0.0, phase_explicit_norm = 0.0`; assert output equals the pre-crt-026 formula result (bit-for-bit, no floating-point drift from the zero terms).
4. End-to-end: run a full search against a populated store with no `session_id`; assert result order is unchanged from the pre-crt-026 baseline.

**Coverage Requirement**: NFR-02 (cold-start safety) is a hard regression boundary. Must have at least one bit-exact comparison test and one end-to-end ordering test with no session context.

---

### R-03: Duplicate store increments histogram
**Severity**: High
**Likelihood**: Med
**Impact**: Histogram becomes inflated relative to actual unique knowledge stored. Category probabilities are skewed. `p(category)` overestimates session focus. W3-1 training signal is corrupted.

Historical context: Entry #1611 documents the pattern where real-time and background paths for implicit vote accumulation must have explicit disjointness — the same principle applies here to the duplicate guard.

**Test Scenarios**:
1. Register a session. Call `context_store` with a new entry (category `"decision"`) — assert histogram = `{"decision": 1}`.
2. Call `context_store` again with the identical entry (same content hash, triggers `duplicate_of.is_some()`) — assert histogram remains `{"decision": 1}`, not `{"decision": 2}`.
3. Confirm the duplicate guard check (`insert_result.duplicate_of.is_some()`) precedes the `record_category_store` call — not after it.

**Coverage Requirement**: AC-02. The duplicate-guard ordering must be verified by test, not just code review. A test that stores the same entry twice and asserts count = 1 after the second store.

---

### R-04: Unregistered session causes panic or side effect in `record_category_store`
**Severity**: Med
**Likelihood**: Low
**Impact**: Any `context_store` call with an unrecognized `session_id` panics or corrupts state. Affects agents that use ad-hoc session IDs or reconnect after server restart.

**Test Scenarios**:
1. Call `record_category_store("nonexistent-session", "decision")` on a fresh `SessionRegistry` — assert no panic, histogram map unchanged, method returns normally.
2. Call `get_category_histogram("nonexistent-session")` — assert returns an empty `HashMap`, not an error or panic.

**Coverage Requirement**: AC-03. Silent no-op contract must be unit-tested explicitly.

---

### R-05: UDS search path omits histogram pre-resolution
**Severity**: High
**Likelihood**: Med
**Impact**: Hook-driven searches receive no histogram boost even when the session has accumulated category data. OQ-04 / OQ-B resolution is untested. Agents using hook-based search get worse ranking than MCP-based search in the same session.

**Test Scenarios**:
1. Simulate a UDS `HookRequest::ContextSearch` with a `session_id` that has a populated histogram. Verify `ServiceSearchParams.category_histogram` is populated (non-None) at the point `SearchService::search` is called.
2. Verify that `sanitize_session_id` is called on the UDS `session_id` BEFORE `get_category_histogram` — the ordering established in `listener.rs` lines 796-803 must not be inverted by the new pre-resolution block.
3. Confirm that a UDS search with a populated histogram produces a different (boosted) result order than the same search with no `session_id`.

**Coverage Requirement**: FR-07. UDS path requires at least one integration test. The `session_id` source is the `HookRequest::ContextSearch` payload field, not `audit_ctx` — test must use the UDS-specific construction path.

---

### R-06: `FusionWeights::effective()` NLI-absent path includes `w_phase_histogram` in re-normalization denominator
**Severity**: High
**Likelihood**: Med
**Impact**: When NLI is absent, the existing five weights (`w_sim, w_conf, w_coac, w_util, w_prov`) are diluted by inclusion of `w_phase_histogram` in the denominator. Rankings in NLI-absent mode are silently wrong. Regression for all environments without NLI.

Historical context: Entry #2964 documents the risk of sequential sort passes causing NLI override; the `effective()` NLI-absent re-normalization is the same class of pipeline ordering bug.

**Test Scenarios**:
1. Construct a `FusionWeights` with `w_phase_histogram = 0.02` and call `effective(false)` (NLI absent). Assert returned `w_phase_histogram` equals `0.02` unchanged (pass-through, not re-normalized).
2. Assert the re-normalization denominator used in `effective(false)` equals `w_sim + w_conf + w_coac + w_util + w_prov` (five terms, not seven).
3. Verify the existing test `test_fusion_weights_effective_nli_active_headroom_weight_preserved` still passes — it constructs a manual FusionWeights at 0.90 and does not assert the struct defaults.

**Coverage Requirement**: ADR-004 NLI-absent invariant. Must test `effective(false)` explicitly with new fields. Existing tests for `effective(true)` must also be confirmed green.

---

### R-07: `phase_explicit_norm = 0.0` placeholder removed as dead code
**Severity**: Med
**Likelihood**: Med
**Impact**: W3-1 integration contract broken. When W3-1 attempts to populate `phase_explicit_norm`, the field no longer exists and W3-1 requires a struct change, potentially causing a breaking API change mid-GNN development.

**Test Scenarios**:
1. Assert `FusedScoreInputs` has a `phase_explicit_norm: f64` field (compilation test, struct layout).
2. Assert `FusionWeights` has a `w_phase_explicit: f64` field (compilation test).
3. Assert `InferenceConfig::default()` returns `w_phase_explicit = 0.0` and `w_phase_histogram = 0.02`.
4. Assert `compute_fused_score` with `phase_explicit_norm = 0.0` and any `w_phase_explicit` value produces identical output to a call with the field absent (the zero term contributes nothing).

**Coverage Requirement**: AC-09, ADR-003. The placeholder fields must have tests that confirm their presence and zero-contribution, with a comment in the test citing ADR-003 to prevent future removal.

---

### R-08: Status penalty applied before histogram boost — deprecated entries escape penalty
**Severity**: Med
**Likelihood**: Low
**Impact**: A deprecated entry matching the session histogram receives boost AFTER penalty, resulting in `(fused * penalty) + boost` rather than `(fused + boost) * penalty`. Deprecated entries are insufficiently penalized relative to their boost.

**Test Scenarios**:
1. Construct a candidate with `status_penalty = 0.5` and a matching histogram category. Assert `final_score = compute_fused_score(&inputs_with_histogram, &weights) * 0.5` — the boost is inside the fused score, not added outside the penalty multiply.
2. Compute the same candidate's score with and without a histogram match; assert the ratio of scores matches `(base + boost) * penalty / (base * penalty)` not `base * penalty + boost`.

**Coverage Requirement**: C-06, AC-10. The application order invariant must be verified by a test that constructs a penalized candidate with a histogram match and asserts the exact score formula.

---

### R-09: Division by zero in `p(category)` computation when total is zero
**Severity**: Med
**Likelihood**: Low
**Impact**: Panic or NaN injected into `phase_histogram_norm` for every candidate. Search call crashes or returns NaN scores. Entire search result is corrupted.

**Test Scenarios**:
1. Call the scoring loop with `category_histogram = Some(HashMap::new())` — this should be mapped to `None` by the handler; verify the scoring loop never sees an empty `Some` map (the `None` check in the handler is the guard).
2. If any code path can deliver a `Some(empty_map)` to `SearchService`, assert `phase_histogram_norm = 0.0` (not a division, not NaN) when `total = 0`.
3. Unit test `p(category)` computation inline: `total = 0` → `phase_histogram_norm = 0.0`.

**Coverage Requirement**: The handler's empty-map-to-None mapping is the primary guard. Both the guard and the in-function defensive check (if present) must be tested.

---

### R-10: Histogram summary emitted when histogram is empty in CompactPayload
**Severity**: Med
**Likelihood**: Med
**Impact**: CompactPayload output contains a spurious `Recent session activity:` block with no content, or a header with empty body. Confuses receiving agents; may consume `MAX_INJECTION_BYTES` budget unnecessarily.

**Test Scenarios**:
1. Call `format_compaction_payload` with an empty `category_counts` map — assert the returned string does NOT contain `"Recent session activity"`.
2. Call with a non-empty histogram `{"decision": 3, "pattern": 2}` — assert the returned string DOES contain `"Recent session activity: decision × 3, pattern × 2"`.
3. Call with a histogram of 7 categories — assert only the top 5 by count appear, and the block does not exceed 100 bytes.

**Coverage Requirement**: AC-11, FR-12. Both the empty-omit and non-empty-emit paths must be explicitly tested. The top-5 cap must be tested at the boundary (exactly 5 and exactly 6 categories).

---

### R-11: `w_phase_histogram` or `w_phase_explicit` out-of-range config accepted silently
**Severity**: Med
**Likelihood**: Low
**Impact**: A misconfigured `w_phase_histogram = 2.0` inflates scores unpredictably; weak-similarity entries override high-NLI entries. The `<= 1.0` per-field range check does not fire.

**Test Scenarios**:
1. Construct an `InferenceConfig` with `w_phase_histogram = 1.5` — assert `validate()` returns an error or panics at startup.
2. Construct with `w_phase_explicit = -0.1` — assert `validate()` rejects it (value below 0.0).
3. Confirm `w_phase_histogram = 0.02` and `w_phase_explicit = 0.0` pass `validate()` cleanly.

**Coverage Requirement**: ADR-004 per-field range checks. `InferenceConfig::validate()` must have dedicated test cases for both new fields at boundary values (0.0, 1.0, just above 1.0, below 0.0).

---

### R-12: `ServiceSearchParams` construction sites not updated
**Severity**: Med
**Likelihood**: High
**Impact**: Compile failure (if fields are non-optional) at all existing `ServiceSearchParams { ... }` construction sites in tests and handlers. Or silent default if fields are `Option` — handler inadvertently omits histogram threading on some paths.

Historical context: ADR-001 "Harder" consequence explicitly identifies this: "All existing struct literal constructions of these types must be updated."

**Test Scenarios**:
1. Compile the codebase — all `ServiceSearchParams { ... }`, `FusedScoreInputs { ... }`, `FusionWeights { ... }`, and `InferenceConfig { ... }` literal constructions must compile without warning or error.
2. Grep for all `ServiceSearchParams {` construction sites and confirm `session_id` and `category_histogram` fields are explicitly set (not omitted).
3. Confirm the UDS `handle_context_search` construction block populates both new fields.

**Coverage Requirement**: Compilation is the primary gate. A post-implementation audit of all struct literal sites is required.

---

### R-13: Pre-resolution placed after an `await` point in the handler
**Severity**: Med
**Likelihood**: Low
**Impact**: A concurrent `context_store` call in the same session can mutate `category_counts` between the `await` and the histogram read. The pre-resolved histogram does not reflect the session state at search time. Race condition violates the crt-025 SR-07 snapshot invariant.

**Test Scenarios**:
1. Code review: verify the `get_category_histogram` call in `context_search` MCP handler occurs before the first `await` point in the function.
2. Code review: verify the same ordering in `handle_context_search` UDS handler.
3. (Stress test, optional): spawn concurrent `context_store` and `context_search` calls in the same session; assert no panic and no NaN score.

**Coverage Requirement**: The no-await-before-snapshot invariant is a code review check (not automatable in unit tests), but the concurrent stress test provides runtime evidence.

---

### R-14: WA-2 extension stubs not removed from `search.rs`
**Severity**: Low
**Likelihood**: Low
**Impact**: Stub comments `// WA-2 extension:` persist at lines 55, 89, 179. Future W3-1 contributor assumes the extension is still unimplemented. Confusion and duplicate implementation risk.

**Test Scenarios**:
1. Assert no string matching `"WA-2 extension"` exists in `services/search.rs` after implementation.

**Coverage Requirement**: AC-14. A simple grep assertion in CI or a code review checklist item.

---

## Integration Risks

**Component boundary: `context_store` handler → `SessionRegistry`**
The `record_category_store` call is synchronous and lock-held. If the handler is ever refactored to move steps 7-8 into a spawned task (as `record_usage` is fire-and-forget), the duplicate guard check and histogram recording could become separated from the insert result — creating a window where a duplicate store is recorded. The ordering must be preserved: duplicate guard → histogram → confidence seeding → usage recording. This ordering is part of the handler contract, not just an implementation choice.

**Component boundary: `context_search` handler → `SearchService`**
`ServiceSearchParams` gains two new fields. `SearchService` treats `category_histogram = None` as the cold-start case and must never attempt to read a `None` histogram. If any internal path constructs `ServiceSearchParams` without setting `category_histogram` (e.g., a test helper that uses `..Default::default()`), the scoring loop silently operates in cold-start mode even when a session is active. All construction sites must be audited.

**Component boundary: `FusionWeights::effective()` → `compute_fused_score`**
The `effective()` method must pass `w_phase_histogram` and `w_phase_explicit` through unchanged in BOTH NLI-active and NLI-absent paths. If the NLI-absent re-normalization loop iterates over all fields generically (e.g., a slice of weight values), adding the new fields to that slice would silently include them in the denominator. The implementation must explicitly enumerate the six core terms in the denominator.

**Component boundary: UDS `handle_context_search` → `SessionRegistry`**
The UDS path has no `audit_ctx`. The `session_id` originates from `HookRequest::ContextSearch.session_id`. `sanitize_session_id` is already applied before any registry access (confirmed in ARCHITECTURE.md, OQ-B). The pre-resolution must be inserted after the sanitize check — not before it. Any future refactoring of the sanitize block must preserve this ordering.

---

## Edge Cases

**EC-01: Session with exactly one store** — `p(category) = 1.0` for that category. Max boost of `0.02` applied to all entries in that category. Must not panic (total = 1 is valid).

**EC-02: Session with 1000+ stores** — large histogram is cloned by `get_category_histogram`. Clone is O(categories), not O(stores), since counts are aggregated. For the typical category vocabulary (< 20 categories), this is negligible.

**EC-03: Category string is empty or whitespace** — `category_validate` in `context_store` handler should reject empty category before `record_category_store` is called. Verify that `category_validate` is called before the histogram recording step. An empty string key in the histogram would produce zero probability for any real category lookup.

**EC-04: Two categories with equal counts** — sort order in `format_compaction_payload` is by count descending. Equal-count categories may be ordered non-deterministically. The spec does not require a stable tie-breaking order; test should accept either ordering for equal-count categories.

**EC-05: `w_phase_histogram` set to `0.0` via config** — boost is zero for all entries regardless of histogram contents. Behavior identical to cold-start. Must not produce division by zero or NaN. Valid config state for operators disabling the feature.

**EC-06: Session registered, then server restarts** — histogram is in-memory; after restart, histogram is empty. Cold-start path kicks in. NFR-03 covers this. No session persistence across server restarts.

**EC-07: Category histogram top-5 cap** — histogram block shows only 5 categories even if 10 are present. The omitted categories are silently excluded; no ellipsis or count is required by the spec, but the block must not exceed `MAX_INJECTION_BYTES`.

---

## Security Risks

**UDS `session_id` input (handle_context_search)**
The `session_id` value in `HookRequest::ContextSearch` originates from the hook payload, which is agent-controlled. `sanitize_session_id` is applied before any registry access (confirmed ARCHITECTURE.md OQ-B). The histogram lookup is a pure registry read on the sanitized value — no SQL, no file I/O, no deserialization. Blast radius of a malformed `session_id`: the session registry returns an empty HashMap (silent no-op for unregistered sessions); no state mutation occurs.

**Histogram key injection via category string**
`category` values in `record_category_store` are already validated by `category_validate` in the `context_store` handler before this method is reached. The category string is used as a HashMap key — no SQL, no path traversal. Blast radius: an unexpected category string produces a histogram entry under that key; it does not affect other sessions or the persistent store.

**CompactPayload output injection**
The histogram summary block is formatted as a string and appended to the `format_compaction_payload` output. The category names and counts originate from internal session state (not directly from the raw hook payload at this point). Category names were validated at store time. No shell execution or HTML rendering path exists for this output. Blast radius: a long category name could expand the block beyond 100 bytes, but the top-5 cap and `MAX_INJECTION_BYTES` budget limit this.

**No new external input surfaces introduced** — `ServiceSearchParams.category_histogram` is internal data derived from session state, not directly from any external input. No new deserialization boundary is introduced.

---

## Failure Modes

**FM-01: Histogram lock contention** — the `sessions` Mutex is held during both read and write. High-frequency concurrent stores in the same session could cause microsecond contention. This is the same contract as `record_injection`. If contention becomes a problem in practice, the mitigation is sharding by session (future work, not crt-026). Expected behavior under lock contention: sequential execution with no deadlock.

**FM-02: `get_category_histogram` returns empty for a valid session** — occurs if the session was registered but no stores have succeeded. This is correct cold-start behavior (NFR-02); the handler maps the empty map to `None` and search proceeds without boost.

**FM-03: `format_compaction_payload` called with very large category string** — the top-5 cap limits the output block size. Even with long category names, the block is bounded. No truncation within the block is required; the `MAX_INJECTION_BYTES` budget applies to the entire payload, not just this block.

**FM-04: `InferenceConfig` deserialization from TOML with missing new fields** — both new fields use `#[serde(default)]`, so existing `config.toml` files without these fields will deserialize with defaults (`w_phase_explicit = 0.0`, `w_phase_histogram = 0.02`). No startup failure for operators not yet upgrading their config.

**FM-05: `compute_fused_score` called with `phase_histogram_norm = NaN`** — if division-by-zero guard is absent and a `Some(empty_map)` reaches the scoring loop, `total = 0` → `NaN` in `phase_histogram_norm` → `NaN` propagates to `final_score` → all entries score NaN → sort produces undefined order → top-k is garbage. This is the failure mode R-09 guards against. The handler's `is_empty() → None` mapping is the primary defense.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `w_phase_histogram` signal strength and test detectability | R-01 | ADR-004 raises default to `0.02` (ASS-028 calibrated value, full session signal budget). Signal is detectable at realistic concentrations. AC-12 specifies score delta floor of `≥ 0.02`. Risk mitigated. |
| SR-02: `InferenceConfig::validate()` sum-invariant with 0.97 | — | Resolved at architecture level (ADR-004, OQ-A). The six-weight sum check does not include new phase fields; 0.97 total passes the `<= 1.0` guard. No architecture risk; confirmed by code-level analysis. |
| SR-03: `w_phase_histogram=0.02` as W3-1 cold-start initialization | — | 0.02 is the ASS-028 calibrated value — a meaningful cold-start seed that provides useful gradient signal. Risk substantially reduced vs. 0.005. W3-1 refines from this seed. |
| SR-04: AC-07 ambiguity — explicit phase term in or out of scope | — | Resolved in specification: AC-07 explicitly dropped. `w_phase_explicit = 0.0` is a placeholder. R-07 tests that the placeholder fields are present and contribute zero. Fully resolved. |
| SR-05: `context_briefing` scope boundary — WA-4b coupling | — | Non-goal confirmed in specification. No architecture risk in crt-026. WA-4b is a separate feature. Accepted. |
| SR-06: Concurrent duplicate-store race on histogram | R-03 | Architecture confirms the lock hold is synchronous and fire-and-commit; the window is effectively zero. R-03 tests the guard placement by asserting count = 1 after two stores of the same entry. |
| SR-07: WA-4a forward-compatibility with pre-resolution pattern | — | Flagged as forward-compatibility note in ADR-002 and ARCHITECTURE.md (OQ-C). No code change required in crt-026. WA-4a must supersede ADR-002. Accepted for this feature. |
| SR-08: UDS `session_id` source is `HookRequest::ContextSearch` payload field | R-05 | Architecture confirms `sanitize_session_id` is already applied at lines 796-803. OQ-B resolved. R-05 requires UDS path integration test to validate histogram threading. Mitigated. |
| SR-09: `status_penalty` applied before histogram boost | R-08 | Architecture confirms OQ-D resolution: `final_score = compute_fused_score(...) * status_penalty`. Boost is inside `compute_fused_score` before penalty. R-08 tests the application order with a penalized candidate and a histogram match. Mitigated. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios (p=1.0 delta floor, 60% concentration, absent-category zero) |
| High | 4 (R-02, R-03, R-05, R-06) | 4 + 3 + 3 + 3 = 13 scenarios |
| Medium | 8 (R-04, R-07, R-08, R-09, R-10, R-11, R-12, R-13) | 2+4+2+3+3+3+3+3 = 23 scenarios |
| Low | 1 (R-14) | 1 scenario (grep assertion) |
| **Total** | **14** | **≥ 40 scenarios** |

**Non-negotiable tests** (gate blockers):
- AC-12: score delta ≥ 0.02 with p=1.0 concentration (R-01)
- AC-02: duplicate store does not increment histogram (R-03)
- AC-08: cold-start parity — empty histogram produces identical scores (R-02)
- AC-03: unregistered session is silent no-op (R-04)
- AC-11: CompactPayload block present/absent (R-10)
- AC-13: absent-category boost = 0.0 (R-01, R-13)
- `FusionWeights::effective(false)` excludes `w_phase_histogram` from denominator (R-06)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` "lesson-learned failures gate rejection" — found #2758 (gate-3c non-negotiable test name validation), #2800 (circuit-breaker cap logic testability as extracted unit function). Both informed R-01 (boost must be an extractable unit assertion, not an end-to-end observable).
- Queried: `/uni-knowledge-search` "risk pattern session registry scoring boost additive" — found #3156 (WA-2 affinity boost architecture decision), #2964 (NLI override by additive boosts pattern). Entry #2964 directly informs R-06 (effective() denominator risk) and confirms ADR-001's choice.
- Queried: `/uni-knowledge-search` "FusedScoreInputs FusionWeights compute_fused_score NLI boost override" — found #2964 and #3161 (ADR-001 crt-026), reinforcing R-06 and confirming R-08 application order risk is already addressed by architecture.
- Queried: `/uni-knowledge-search` "outcome rework session state concurrent mutation race condition" — found #1274 (force-set race in session-registry), confirming R-13 (pre-resolution before await) is a real class of bug in this codebase.
- Queried: `/uni-knowledge-search` "duplicate store histogram double count guard mutation" — found #1611 (implicit vote path disjointness). No crt-026-specific entry; R-03 is novel to this feature.
- Stored: nothing novel — R-01 (synthetic histogram concentration floor for small-weight tests) is a crt-026-specific instantiation of a pattern. Will store after confirming it recurs in W3-1 or WA-3 work.
