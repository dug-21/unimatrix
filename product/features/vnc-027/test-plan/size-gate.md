# Test Plan — size-gate (`test/check-hook-client-size.js` rewrite)

Component 8 / ADR-005 / FR-1..FR-4 / **AC-09 (Critical, merges FIRST)** / Risks R-02 (Critical), R-03 (Med).
The stripper lives in `test/` and does NOT count against the client budget. Self-test runs on every invocation
BEFORE measuring; self-test failure fails the gate closed (never measures with a broken stripper).

## Unit / self-test expectations (embedded corpus — runs on every gate invocation)

Stripper is a character-level state machine: code, single-quote string, double-quote string, template literal
(`${}` nesting back to code), line comment, block comment, regex literal (with char-class sub-state). Removal-only.

- `test_selftest_runs_before_measure_and_fails_gate_on_strip_bug` — assert self-test executes first; a deliberately broken stripper input makes the gate exit non-zero with a self-test failure message, not a size table.
- **String-literal safety (FR-2, R-03):**
  - `test_strip_preserves_double_quote_string_with_slashslash` — `const s = "// not a comment";` → the `//` survives, nothing after it on the line is removed.
  - `test_strip_preserves_template_literal_with_dollar_brace_and_backtick` — `` `a ${x} /* still code */ b` `` → braces/`/*` inside the template are NOT treated as comment; `${}` returns to code state and a real comment after the closing backtick IS stripped.
  - `test_strip_preserves_regex_with_slashslash_and_slashstar` — `/foo\/\/bar/` and `/a\/\*b/` are regex bodies, not comments.
  - `test_strip_regex_char_class_slash` — `/[/]/` and `/[\/]/`: `/` inside `[...]` does not close the regex.
- **Regex-vs-division heuristic (ADR-005 §3, R-03):** prev-significant-token rule. Cover at least:
  - `test_regex_open_after_paren_eq_return` — `(`, `=`, `return` prev-token → `/` opens a regex.
  - `test_division_after_identifier` — `a / b / c` after an identifier → division, the `//`-style trap does not fire.
- **Escape sequences (FR-2):** `test_strip_escapes_in_string_template_regex` — `\"` in strings, `` \` `` in templates, `\/` in regex do not exit their state.
- `test_strip_is_removal_only_byte_subsequence` — for the real client tree, stripped output is a byte-subsequence of input (no rewriting); `0 < stripped < raw`.

## Gate-behavior expectations (FR-1, FR-3)

- `test_gate_passes_within_both_limits` — synthetic tree stripped ≤ 100,000 and raw ≤ 160,000 → exit 0.
- `test_gate_fails_on_stripped_over_100000` — synthetic file stripped > 100,000 B → exit non-zero, per-file table, BOTH totals printed.
- `test_gate_fails_on_raw_over_160000` — comment-heavy tree raw > 160,000 B but stripped ≤ 100,000 → exit non-zero (backstop independently triggerable).
- `test_both_limits_independently_triggerable` — the two limits fail on disjoint fixtures (R-02 coverage requirement).
- `test_header_documents_human_decision_rule` — gate file header text contains the C-04 dual-limit definition and the "cap changes are human decisions on the feature issue" rule (FR-3).
- Decimal interpretation retained (100,000 / 160,000, not KiB).

## Merge-order audit (R-02, process check at Gate 3c)

- `git log --oneline -- packages/unimatrix/test/check-hook-client-size.js packages/unimatrix/lib/hook-client/` shows the gate rewrite as the first vnc-027 commit touching the client tree, before any `lib/hook-client/` byte growth. Auditable, not a runtime test.
- vnc-030 depends on this redefinition (cross-feature contract, #680) — the redefinition must be merged and stable.

## Edge cases
- Empty `lib/hook-client/` tree → gate handles gracefully (no divide-by-zero, no false pass on missing dir — exit 1 if dir absent, per existing behavior).
- A file at exactly 100,000 B stripped → boundary is `≤` (passes at exactly the limit).
- Stripper bug cannot admit unbounded growth: the raw ≤ 160,000 backstop caps a hypothetical miscount (R-03 mitigation, assert backstop still fires under a forced strip-everything bug).
