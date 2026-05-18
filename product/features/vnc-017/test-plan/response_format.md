# Test Plan: response_format

**Component**: Response text appended in `context_correct` handler (`tools.rs`) or via `format_correct_success` (`response/entries.rs`)
**FR**: FR-10 — 4-variant conditional append table
**AC**: AC-11, AC-12, AC-13, AC-17
**Integration ACs**: AC-12 (verified end-to-end), R-11 (response text in actual `CallToolResult`)

---

## Authoritative Format Table (FR-10)

| Condition | Text Appended |
|-----------|---------------|
| `found == 0` | *(no append — response text unchanged)* |
| `found > 0`, `truncated == false`, `skipped == 0` | `"Redirected N incoming edges (M failed, see logs)"` |
| `found > 0`, `truncated == false`, `skipped > 0` | `"Redirected N incoming edges (K skipped — invalid source, M failed, see logs)"` |
| `truncated == true` | `"Redirected N incoming edges (truncated from M, see logs)"` |

Where: N = `redirected`, M = `failed` (non-truncated) or `total_raw` (truncation variant), K = `skipped`.

**Note on AC-17**: When `found > 0` and all edges are skipped (all sources Quarantined/Deprecated), `redirected == 0`, `skipped == N`. This matches Variant 3 (skipped > 0, truncated == false). The text reads: `"Redirected 0 incoming edges (3 skipped — invalid source, 0 failed, see logs)"`.

---

## Unit Test Expectations

All unit tests verify the text produced by the `format_correct_success` function call or the post-call text append, using stubbed `RedirectSummary` values. Tests do not go through the MCP call stack.

---

### AC-11: found == 0 — no append

**Arrange**: `RedirectSummary { found: 0, skipped: 0, redirected: 0, failed: 0, truncated: false, total_raw: 0 }`

**Assert**: The `context_correct` response text does NOT contain the substring "Redirected".

Existing `format_correct_success` output is unchanged. The test also asserts that no `tracing::info!` summary log is emitted.

**Test name**: `test_response_format_no_append_when_found_zero`

---

### AC-12: found > 0, skipped == 0, truncated == false (all succeed)

**Arrange**: `RedirectSummary { found: 2, skipped: 0, redirected: 2, failed: 0, truncated: false, total_raw: 2 }`

**Assert**: Response text contains exactly `"Redirected 2 incoming edges (0 failed, see logs)"` as a substring.

**Assert also**: Text does NOT contain "skipped" or "truncated".

**Test name**: `test_response_format_all_success_variant`

---

### AC-13: found > 0, some failed, skipped == 0, truncated == false

**Arrange**: `RedirectSummary { found: 3, skipped: 0, redirected: 1, failed: 2, truncated: false, total_raw: 3 }`

**Assert**: Response text contains `"Redirected 1 incoming edges (2 failed, see logs)"`.

**Assert also**: `failed == 2` is accurately reflected (not 0, not 3).

**Test name**: `test_response_format_partial_failure_variant`

---

### AC-17: all-skipped case (all sources Quarantined/Deprecated)

**Arrange**: `RedirectSummary { found: 3, skipped: 3, redirected: 0, failed: 0, truncated: false, total_raw: 3 }`

**Assert**: Response text contains `"Redirected 0 incoming edges (3 skipped — invalid source, 0 failed, see logs)"`.

**Assert also**:
- Text contains "skipped — invalid source".
- `failed == 0` is reflected as "0 failed".
- Does NOT read as "no redirect occurred" — the skipped-count variant communicates that edges were found but not processable.

**Test name**: `test_response_format_all_skipped_variant`

---

### Skipped > 0, partial success (mixed Variant 3)

**Arrange**: `RedirectSummary { found: 4, skipped: 1, redirected: 2, failed: 1, truncated: false, total_raw: 4 }`

**Assert**: Response text contains `"Redirected 2 incoming edges (1 skipped — invalid source, 1 failed, see logs)"`.

**Test name**: `test_response_format_mixed_skipped_and_failed_variant`

---

### Truncation variant (R-05)

**Arrange**: `RedirectSummary { found: 50, skipped: 0, redirected: 50, failed: 0, truncated: true, total_raw: 55 }`

**Assert**: Response text contains `"Redirected 50 incoming edges (truncated from 55, see logs)"`.

**Assert also**: Text does NOT contain "(0 failed" or "skipped" — the truncation variant is mutually exclusive with the failure/skip details per FR-10.

**Test name**: `test_response_format_truncated_variant`

---

### Existing context_correct response fields unchanged (FR-11)

**Arrange**: Build the `context_correct` response with a non-zero `RedirectSummary` and a non-trivial correction entry.

**Assert**:
- `deprecated_original` field is present and correct.
- `corrected_entry` field is present and correct.
- The redirect summary is appended as text only — no new JSON fields added to the response schema.
- Existing field values are not modified by the redirect summary append.

**Test name**: `test_response_format_does_not_alter_existing_fields`

---

## Integration Test Expectations

### AC-12 / R-11: Response text verified in real CallToolResult

**Suite**: `test_lifecycle.py`
**Test**: `test_correct_response_text_contains_redirect_summary`
**Fixture**: `server` (fresh DB)

**Arrange**:
1. Store entry A.
2. Store entry C.
3. Add edge `C → A` (Prerequisite) via `context_edge`.
4. Store entry D (second source).
5. Add edge `D → A` (Prerequisite) via `context_edge`.

**Act**: Call `context_correct(original_id=A_id, ...)` via MCP.

**Assert**:
- `result["result"]["content"][0]["text"]` (or equivalent `CallToolResult` text path) contains the substring `"Redirected 2 incoming edges (0 failed, see logs)"`.
- The assertion is an exact substring match, not a contains-words check.

**Why this matters (R-11)**: The unit tests verify the format function logic. The integration test verifies that the text is actually reachable through the real MCP call stack (i.e., the text is not produced by a dead code path that is bypassed in production).

---

## Format String Consistency Contract

The exact format strings below must be implemented verbatim. No synonyms or rephrasing:

| Variant | Exact String Template |
|---------|-----------------------|
| All success | `"Redirected {N} incoming edges ({M} failed, see logs)"` |
| With skipped | `"Redirected {N} incoming edges ({K} skipped — invalid source, {M} failed, see logs)"` |
| Truncated | `"Redirected {N} incoming edges (truncated from {M}, see logs)"` |

The em-dash in "skipped — invalid source" is a plain ASCII double-dash sequence `--` or actual em-dash `—`; the test must match whatever the implementation produces. The test plan specifies the intent; the exact byte sequence must be confirmed against the pseudocode/implementation.

---

## Edge Cases

- **N == 1**: format produces `"Redirected 1 incoming edges"` (grammatically incorrect "edges" for singular; this is intentional per FR-10 — no special singular handling specified). Test must assert the plural form is used for N=1 unless FR-10 is updated.
- **All failed, redirected == 0**: Response text reads `"Redirected 0 incoming edges (N failed, see logs)"` — this is Variant 2 (skipped == 0, truncated == false) with N=0. AC-13 covers partial failure; a zero-redirected full-failure case should also be tested:

  **Arrange**: `RedirectSummary { found: 2, skipped: 0, redirected: 0, failed: 2, truncated: false, total_raw: 2 }`
  **Assert**: `"Redirected 0 incoming edges (2 failed, see logs)"` is the appended text.
  **Test name**: `test_response_format_all_failed_variant`

---

## Code Review Gate

The implementation must gate on `found > 0` (not `redirected > 0`) to decide whether to append. If the gate is `redirected > 0`, the all-skipped variant (AC-17) would silently produce no append despite edges being found — this contradicts FR-10's behavior specification. The gate condition is a required code review assertion.
