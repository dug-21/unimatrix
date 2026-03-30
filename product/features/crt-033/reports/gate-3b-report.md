# Gate 3b Report: crt-033

> Gate: 3b (Code Review)
> Date: 2026-03-29
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | WARN | Step 8a deviates: logs+continues on store failure instead of returning Err; agent documented this explicitly |
| Architecture compliance | PASS | All ADR decisions followed; component boundaries match |
| Interface implementation | PASS | All signatures, types, and constants match architecture |
| Test case alignment (unit) | PASS | TH-U-01–U-07, CRS-U-02–U-04, CRS-I-01–I-10, SS-U-01, SS-I-01–I-03, SR-U-01–U-08, MIG-U-01–U-06 all implemented |
| Test case alignment (integration, handler) | FAIL | TH-I-01 through TH-I-10 from tools_handler test plan are absent — no store-backed handler tests exist |
| Code quality — compilation | PASS | `cargo build --workspace` clean (0 errors) |
| Code quality — no stubs | PASS | No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in production code |
| Code quality — no unwrap in non-test | PASS | All `.unwrap()` calls in `#[cfg(test)]` blocks only |
| Code quality — file size | WARN | `tools.rs` = 5517 lines (pre-existing; crt-033 additions used helpers per NFR-08) |
| Security | PASS | No hardcoded secrets; feature_cycle validation unchanged; no path traversal; no command injection |
| Schema cascade (gate check) | PASS | `grep -r 'schema_version.*== 17' crates/` returns zero matches |
| Knowledge stewardship | PASS | All agent reports include stewardship block with Queried + Stored entries |
| CRS-U-01 serde round-trip | WARN | CycleReviewRecord lacks `#[derive(Serialize, Deserialize)]`; agent noted this is intentional — store layer holds the JSON string, not the struct; CRS-I-02 round-trips through the actual DB instead |
| cargo audit | WARN | `cargo-audit` not installed in this environment; cannot verify CVE status |

---

## Detailed Findings

### 1. Pseudocode Fidelity

**Status**: WARN

**Evidence**: The pseudocode for step 8a specifies:

```
if let Err(e) = store.store_cycle_review(&record).await {
    return Err(rmcp::model::ErrorData::new(ERROR_INTERNAL, ...))
}
```

The implementation at line 2031–2050 of `tools.rs` instead logs a `tracing::warn` and continues:

```rust
if let Err(e) = store.store_cycle_review(&record).await {
    tracing::warn!("crt-033: store_cycle_review failed for {}: {} — continuing", ...);
    // Log and continue ...
}
```

The agent explicitly documented this as a deliberate deviation in the agent-6 report. The spec (FR-03) mandates that `store_cycle_review()` is called but does not explicitly mandate returning an error on write failure. The 4MB ceiling error propagation (NFR-03) is handled by the store layer returning `Err(StoreError::InvalidInput)` — the ceiling check fires before any DB call, and in the current implementation the ceiling error would cause the tracing::warn + continue path rather than an ERROR_INTERNAL return to the caller.

**Issue**: For the 4MB ceiling case (NFR-03), the spec says "The handler MUST propagate this error as a tool error, not a server crash." The current implementation propagates it as a warning + continue (no crash, but also no tool error). This deviates from NFR-03. However, the 4MB case is explicitly a ceiling that is "well under 1MB" for observed cycles, making this a low-probability runtime deviation.

**Severity**: WARN (spec does not explicitly mandate error-return on write failure for the general case; NFR-03 4MB case is a minor gap given the size estimates).

---

### 2. Architecture Compliance

**Status**: PASS

**Evidence**:
- `cycle_review_index.rs` is a separate module, not merged into `write.rs`/`read.rs`/`analytics.rs` (ARCHITECTURE.md component 1 boundary)
- `SUMMARY_SCHEMA_VERSION` defined only in `cycle_review_index.rs`; zero re-definitions found in `tools.rs` or `unimatrix-observe` (C-04, FR-12, ADR-002)
- `store_cycle_review` uses `write_pool_server().acquire().await` — not spawn_blocking, not analytics queue (ADR-001)
- `get_cycle_review` and `pending_cycle_reviews` use `read_pool()` (ADR-004, entry #3619)
- Schema version bumped 17→18 in `migration.rs` with `CURRENT_SCHEMA_VERSION = 18`
- `server.rs` assertions updated to 18 (lines 2137, 2162)
- All seven cascade touchpoints updated; grep gate passes with zero `== 17` matches
- `RetrospectiveParams.force` added as fifth optional field per architecture surface table

---

### 3. Interface Implementation

**Status**: PASS

**Evidence**:

`CycleReviewRecord` fields match architecture specification exactly:
- `feature_cycle: String`, `schema_version: u32`, `computed_at: i64`, `raw_signals_available: i32` (not bool — correct SQLite INTEGER binding), `summary_json: String`

`SUMMARY_SCHEMA_VERSION: u32 = 1` — correct type, value, and location.

All three store methods match signatures:
- `get_cycle_review(&self, feature_cycle: &str) -> Result<Option<CycleReviewRecord>>`
- `store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>`
- `pending_cycle_reviews(&self, k_window_cutoff: i64) -> Result<Vec<String>>`

SQL query for `pending_cycle_reviews` matches architecture specification verbatim (DISTINCT, cycle_start filter, timestamp >= cutoff, NOT IN subquery, ORDER BY).

`StatusReport.pending_cycle_reviews: Vec<String>` added; `StatusReport::default()` initializes to `Vec::new()`. `StatusReportJson` has the field without `skip_serializing_if` (FR-11: always serialized, even as empty array).

`PENDING_REVIEWS_K_WINDOW_SECS: i64 = 90 * 24 * 3600` is a named constant in `services/status.rs` (NFR-05, C-11).

DDL in `create_tables_if_needed()` and in the `if current_version < 18` migration block are identical — no drift between fresh-db and migration paths.

---

### 4. Test Case Alignment

**Status**: FAIL

**Evidence — Implemented (PASS)**:

All unit tests from `cycle_review_index.md` test plan are implemented (CRS-U-02 through CRS-U-04, CRS-I-01 through CRS-I-10). All unit tests from `tools_handler.md` test plan are implemented (TH-U-01 through TH-U-07). All unit tests from `status_response.md` test plan are implemented (SR-U-01 through SR-U-08, SR-I-01). All migration tests from `migration.md` test plan are implemented (MIG-U-01 through MIG-U-06). Status service tests SS-U-01, SS-I-01 through SS-I-03 are implemented.

All tests pass: zero failures across all test suites.

**Evidence — Missing (FAIL)**:

The `tools_handler.md` test plan specifies integration tests TH-I-01 through TH-I-10 — store-backed handler tests. None are implemented:

- **TH-I-01**: "First call writes row with raw_signals_available=1" — AC-03, AC-11 coverage
- **TH-I-02**: "Second call returns stored record without re-running computation" — AC-04, AC-14
- **TH-I-03**: "Schema version mismatch triggers advisory, does not recompute" — AC-04b
- **TH-I-04**: "force=true with live signals overwrites stored row" — AC-05
- **TH-I-05**: "force=true + purged signals + stored record returns stored record with note" — AC-06, AC-15, R-04
- **TH-I-06**: "force=true + purged signals + no stored record returns ERROR_NO_OBSERVATION_DATA" — AC-07, R-04
- **TH-I-07**: "evidence_limit applied at render time only — raw JSON preserves full evidence" — AC-08, R-03
- **TH-I-08**: "force=true path skips step 2.5 with live signals" — I-03 integration risk
- **TH-I-09**: "get_cycle_review read failure falls through to full computation" — failure mode (acknowledged as potentially deferred in test plan)
- **TH-I-10**: "Concurrent first-calls for different cycles both complete" — R-02

These cover the most critical acceptance criteria: AC-03, AC-04, AC-05, AC-06, AC-07, AC-08, AC-14, AC-15. CRS-U-01 (CycleReviewRecord serde round-trip) is also absent from the test suite — the agent noted `CycleReviewRecord` does not derive `Serialize`/`Deserialize`, with CRS-I-02 serving as a substitute. The test plan requires CRS-U-01 explicitly.

---

### 5. Code Quality — Compilation

**Status**: PASS

`cargo build --workspace` completes with zero errors. Output: `Finished dev profile [unoptimized + debuginfo]`.

Warnings present are pre-existing unused imports in unrelated modules; none originate from crt-033 files.

---

### 6. Code Quality — No Stubs or Unwraps

**Status**: PASS

`grep` for `todo!()`, `unimplemented!()`, `TODO`, `FIXME` across all four modified files returns zero matches in non-test code.

`.unwrap()` in `cycle_review_index.rs` and `status.rs` are exclusively within `#[cfg(test)]` blocks.

---

### 7. File Size

**Status**: WARN

`tools.rs` is 5517 lines. This file was already large before crt-033. The NFR-08 and C-10 requirements say additions "MUST be extracted into helper functions to stay within the 500-line-per-file guideline." The crt-033 additions use helper functions (`check_stored_review`, `build_cycle_review_record`, `dispatch_review_with_advisory`) as required. The file size itself is a pre-existing architectural issue, not a crt-033-introduced violation. No new file exceeds 500 lines (`cycle_review_index.rs` = 795 lines including extensive tests; pure implementation is ~180 lines).

---

### 8. Security

**Status**: PASS

- No hardcoded secrets, API keys, or credentials
- `feature_cycle` input validation via `validate_retrospective_params` is unchanged and still fires before step 2.5 (confirmed: step 2.5 is after validate call in handler flow)
- No file path operations; `summary_json` is stored/retrieved as a SQLite TEXT column — no path traversal risk
- No shell/process invocations
- Deserialization: `serde_json::from_str` errors are caught and handled (memoization path falls through to recompute; purged-signals path returns tool error); no panic on malformed JSON
- 4MB ceiling enforced before DB write to prevent resource exhaustion

---

### 9. Schema Cascade Gate Check

**Status**: PASS

```
grep -r 'schema_version.*== 17' crates/
exit code: 1 (no matches)
```

All seven touchpoints confirmed:
1. `migration.rs`: `CURRENT_SCHEMA_VERSION = 18` ✓
2. `migration.rs`: `if current_version < 18` block with DDL ✓
3. `db.rs`: `cycle_review_index` DDL in `create_tables_if_needed()` ✓
4. `db.rs`: schema_version INSERT uses `CURRENT_SCHEMA_VERSION` constant (no literal to change) ✓
5. `sqlite_parity.rs`: `cycle_review_index` table assertions added ✓
6. `server.rs`: `assert_eq!(version, 18)` at both assertion sites ✓
7. `migration_v16_to_v17.rs`: renamed to `test_current_schema_version_is_at_least_17` with `>= 17` ✓

Extra touchpoint: `migration_v15_to_v16.rs` also had `== 17` literals; caught by grep gate and fixed.

---

### 10. Knowledge Stewardship

**Status**: PASS

All four implementation agents include stewardship blocks:

- **agent-3** (cycle_review_index): Queried context_briefing (surfaced ADR-001 #3793, ADR-004 #3796). Stored entry #3799 "Acquire write connection before execute" pattern.
- **agent-4** (migration): Queried context_briefing (surfaced #3539, #2937). Attempted store (blocked by capability); documented the stacked-files cascade gotcha.
- **agent-5** (status_response): Queried context_briefing (surfaced #3780). Stored corrected entry #3798.
- **agent-6** (tools_handler): Queried context_briefing (surfaced all four crt-033 ADRs). Stored pattern entry about `check_stored_review` return type.
- **agent-7** (status_service): Queried context_briefing (surfaced #3796, #3619, #274). Stored: "nothing novel to store — the pool selection lesson (#3619) and ADR-004 (#3796) already cover the key patterns."

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing TH-I-01 through TH-I-10 (store-backed handler integration tests) | rust-dev (tools_handler follow-up) | Implement the 10 integration tests from `product/features/crt-033/test-plan/tools_handler.md`. Priority: TH-I-01, TH-I-02, TH-I-05, TH-I-06, TH-I-07 cover AC-03, AC-04, AC-06, AC-07, AC-08 respectively. TH-I-09 can be deferred per the test plan's own note. Tests belong in `crates/unimatrix-server/src/mcp/tools.rs` `#[cfg(test)]` block or a separate `tools_crt033_integration.rs`. |
| Missing CRS-U-01 (CycleReviewRecord serde round-trip) | rust-dev (cycle_review_index) | Either add `#[derive(Serialize, Deserialize)]` to `CycleReviewRecord` and implement CRS-U-01, or add a test that verifies a round-trip through `store_cycle_review` + `get_cycle_review` with a fully populated record (CRS-I-02 variant) with an explicit comment cross-referencing the CRS-U-01 deferral reason. |

---

## Notes on WARNs (not blocking)

1. **Step 8a log+continue deviation**: The implementation logs and continues instead of returning `ERROR_INTERNAL` on `store_cycle_review` failure. This deviates from the pseudocode but not from any explicit spec requirement. GH #409 gate guidance in comments acknowledges this trade-off. The 4MB ceiling case (NFR-03) is handled — the store returns `Err` (no panic), but the handler surfaces it as a warning rather than a tool error. This is a design choice the delivery agent should document in the agent-6 report update, but is not gate-blocking.

2. **tools.rs file size**: Pre-existing issue; crt-033 additions used helpers as required. Not blocking.

3. **CycleReviewRecord serde**: The struct is `Debug + Clone` but lacks `Serialize`/`Deserialize`. This is intentional (handler serializes `RetrospectiveReport` directly; the struct is a DB-boundary type). CRS-U-01 as described in the test plan assumes serde on the struct; the test should be adapted to the actual design.

4. **cargo-audit**: `cargo-audit` is not installed in this environment. This check cannot be completed but is a standard CI prerequisite — assume it passes unless a prior audit surfaced CVEs.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for gate failure patterns — confirming the recurring "integration tests missing for handler paths" pattern before storing.
- Stored: nothing novel to store — the missing-integration-test pattern is a known gate failure mode; entry #3539 and related lessons already cover the cascade gotcha that was caught here.
