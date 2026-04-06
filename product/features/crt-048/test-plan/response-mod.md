# Test Plan: `mcp/response/mod.rs` — Test Fixtures

## Component Scope

Contains test helper functions and inline test fixtures that construct
`StatusReport` literals. This is the highest-likelihood compile-failure site:
removing `confidence_freshness_score` and `stale_confidence_count` from the
`StatusReport` struct immediately breaks all 8 fixture sites.

This component has no production logic — it is test infrastructure only
(inside `#[cfg(test)]`). Its test plan is about correct fixture removal, not
behavioral coverage.

---

## Risks Owned by This Component

| Risk | Coverage Requirement |
|------|---------------------|
| R-02 (Critical/High) | All 8 fixture sites updated; `make_coherence_status_report()` non-default values explicitly verified |

---

## The 8 Fixture Sites (Authoritative Checklist)

Each site must have both field assignments removed. The build fails if any
single assignment remains.

| Site | Line (freshness_score) | Line (stale_count) | Values | Risk |
|------|----------------------|-------------------|--------|------|
| `make_status_report()` helper | 614 | 618 | default (1.0 / 0) | Normal |
| Inline fixture 1 | 710 | 714 | default | Normal |
| Inline fixture 2 | 973 | 977 | default | Normal |
| Inline fixture 3 | 1054 | 1058 | default | Normal |
| Inline fixture 4 | 1137 | 1141 | default | Normal |
| Inline fixture 5 | 1212 | 1216 | default | Normal |
| Inline fixture 6 | 1291 | 1295 | default | Normal |
| `make_coherence_status_report()` | 1434 | 1438 | **0.8200 / 15** | HIGH RISK |

**Additional lines beyond the 8 sites:**
- Line 1731: `report2.stale_confidence_count = 0` — standalone field assignment; must be removed
- Lines 1794/1798: default assertions on removed fields — must be removed or rewritten

---

## Test Expectations for This Component

### `make_coherence_status_report()` — explicit removal verification (R-02)

**This is the single highest-risk missed site in the entire feature.**

The helper at line 1434 sets `confidence_freshness_score: 0.8200` and
`stale_confidence_count: 15`. These are non-default, non-zero values.
A search-and-replace targeting `1.0` (default for freshness) or `0`
(default for stale_count) will NOT find this site.

**Verification in Stage 3c:**
1. `cargo build --workspace` succeeds — if this site is missed, it is a compile error.
2. Grep check:
   ```bash
   grep -n "0.8200\|0\.82\b" crates/unimatrix-server/src/mcp/response/mod.rs
   ```
   Must return zero matches related to `confidence_freshness_score` after delivery.
   (If 0.82 appears legitimately in another context, inspect the surrounding code.)
3. Grep check:
   ```bash
   grep -n "stale_confidence_count" crates/unimatrix-server/src/mcp/response/mod.rs
   ```
   Must return zero matches.

---

### Build gate — primary test mechanism (R-02, AC-14)

All 16 field reference removals across 8 sites are detected atomically by:
```bash
cargo build --workspace
```

A compile error citing `no field 'confidence_freshness_score'` or
`no field 'stale_confidence_count'` indicates exactly which site was missed.
The error message includes the file and line number.

**This is not optional.** A partial removal that compiles without error is
impossible because the fields are removed from the struct definition in
`mcp/response/status.rs` (Component C). Any remaining struct literal with
those fields is a compile error.

---

### Deleted tests — must not exist (R-02 cleanup)

Four tests must be entirely removed. Their presence after delivery indicates
incomplete cleanup:

**`test_coherence_json_all_fields`** (lines 1474–1533)
- Asserted `confidence_freshness_score` and `stale_confidence_count` are present
  in JSON output — the inverse of the correct post-crt-048 assertion.
- Must be deleted, not modified to assert absence.

**`test_coherence_json_f64_precision`**
- References `report.confidence_freshness_score` value (or its JSON rendering).
- Must be deleted.

**`test_coherence_stale_count_rendering`**
- Tests that `stale_confidence_count` appears in text and markdown output.
- Must be deleted.

**`test_coherence_default_values`**
- Asserts `report.confidence_freshness_score == 1.0` and `report.stale_confidence_count == 0`.
- Must be deleted.

**Verification in Stage 3c:**
```bash
cargo test --workspace -- --list 2>&1 | grep -E \
    "test_coherence_json_all_fields|test_coherence_json_f64_precision|\
test_coherence_stale_count_rendering|test_coherence_default_values"
```
Must return zero matches.

---

### Remaining tests in `mod.rs` must still pass (AC-10, NFR-05)

After the 4 deleted tests and 8 fixture site updates, all remaining tests
in `mcp/response/mod.rs` must pass. No tests should be disabled or commented out
that were passing before crt-048.

**Expected passing tests include (non-exhaustive):**
- Tests using `make_status_report()` helper — now without freshness fields
- Tests using the 6 inline fixtures — now without freshness fields
- Tests using `make_coherence_status_report()` — now without freshness fields
- Tests for text format coherence line (minus freshness values)
- Tests for markdown format coherence section (minus freshness bullet)

The exact count of surviving tests is deterministic from the deletion list.
In Stage 3c, record the before/after test count for `mcp/response/mod.rs`
to confirm only the expected 4 tests were removed (plus any tests that
depended solely on the deleted fixtures).

---

### Line 1731 — `report2.stale_confidence_count = 0` (standalone assignment)

This line is a standalone field assignment, not part of a struct literal.
It must be removed along with whatever `report2` test context it appears in.

**Verification:** Included in the global grep:
```bash
grep -n "stale_confidence_count" crates/unimatrix-server/src/mcp/response/mod.rs
```
Must return zero matches.

---

### Lines 1794/1798 — default assertions

These lines assert default values for removed fields. They must be removed or
rewritten without the freshness field references.

If the surrounding test (`test_coherence_default_values` or similar) is being
deleted entirely, lines 1794/1798 are deleted as part of that test. If they appear
in a surviving test, they must be removed from that test's assertion block.

---

## Static Analysis Checklist for Stage 3c

Run all of these before marking this component PASS:

```bash
# Zero freshness field references in mod.rs
grep -n "confidence_freshness_score\|stale_confidence_count" \
    crates/unimatrix-server/src/mcp/response/mod.rs

# Zero non-default freshness values (0.82 sentinel)
grep -n "0\.8200\|0\.82[^0-9]" \
    crates/unimatrix-server/src/mcp/response/mod.rs

# Deleted tests absent
cargo test --workspace -- --list 2>&1 | grep -E \
    "test_coherence_json_all_fields|test_coherence_json_f64_precision|\
test_coherence_stale_count_rendering|test_coherence_default_values"
```

All three commands must return zero output.

---

## Edge Cases

| Scenario | Expected |
|----------|---------|
| All 8 sites updated but line 1731 missed | Compile error: `no field stale_confidence_count on StatusReport` |
| `make_coherence_status_report()` at 1434 missed | Compile error on that specific line |
| Default fixture sites found by search-and-replace but non-default site (0.8200/15) missed | Compile error on line 1434 |
| A deleted test re-introduced | Test would reference removed fields → compile error |
