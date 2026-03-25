# Security Review: col-026-security-reviewer

## Risk Level: low

## Summary

col-026 adds new reporting fields and a formatter overhaul to `context_cycle_review`. The change is
read-only relative to the DB: no schema migration, no new writes, no new external-input pathways.
The primary security surface is markdown output rendered from agent-authored free-form strings
(goal, gate_outcome_text, entry titles). The formatter applies newline-stripping on goal and
outcome text and pipe-escaping on entry titles. SQL access is parameterized. No hardcoded secrets
or credentials found. No new dependencies introduced. No blocking findings.

---

## Findings

### Finding 1: Markdown Injection via `goal` and `gate_outcome_text` — Newlines Stripped, Headers Not
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs` lines 154, 333
- **Description**: The formatter applies `replace('\n', " ").replace('\r', " ")` to both `goal` and
  `gate_outcome_text` before rendering. This prevents raw newlines from creating spurious sections.
  However, other markdown-meaningful constructs — specifically `**`, `__`, `[]()`, `` ` `` — are
  not escaped. A goal stored as `**INJECTED BOLD** and [link](http://evil.com)` renders as bold
  text and a hyperlink in the output. The blast radius is the markdown report only — no code
  execution, no DB writes, no file system access. The risk is strictly report structure corruption
  and potential LLM-consumer confusion.
  The RISK-TEST-STRATEGY.md documents this concern and states the formatter is a read-only renderer
  (Security Risks section). The existing test at line 3833 asserts that `goal = "line1\nline2"`
  does not produce a section header, which confirms newline injection is guarded.
- **Recommendation**: The current protection is adequate for the blast radius (markdown output
  only). Escaping all markdown special characters would require sanitizing goal text stored by
  agents, which changes the tool's semantics. Document in the security notes that goal text is
  rendered verbatim except for newlines, and that this is intentional.
- **Blocking**: no

### Finding 2: `entry.category` and `entry.feature_cycle` Not Pipe-Escaped in Table
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/retrospective.rs` line 1003–1004
- **Description**: `entry.title` is correctly pipe-escaped (`replace('|', "\\|")`). However,
  `entry.category` and `entry.feature_cycle` are rendered verbatim. These values come from the
  `entries` table (category) and the `feature_entries` table (feature_cycle). Category values are
  constrained by the category allowlist at store time, making injection unlikely. Feature cycle
  values come from `feature_cycle` column which is set by the `context_cycle` tool — agent-authored
  but validated at storage time. The pipe character is the main concern for table cell corruption.
  In practice neither field is expected to contain `|`.
- **Recommendation**: For defense-in-depth, add `replace('|', "\\|")` to `entry.category` and
  `entry.feature_cycle` before rendering, matching the treatment of `entry.title`. Not blocking.
- **Blocking**: no

### Finding 3: `obs.ts as i64` Truncating Cast in Phase Window Filter
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` line 2633; `response/retrospective.rs`
  line 399
- **Description**: `ObservationRecord.ts` is `u64` epoch milliseconds. The cast `obs.ts as i64`
  wraps silently if `obs.ts > i64::MAX as u64` (i.e., timestamps after year 292,471,210 CE).
  The comment in the code acknowledges this: "If obs.ts > i64::MAX as u64, the cast wraps — still
  correct (saturates to MAX)." The comment is incorrect — a wrapping cast does not saturate; it
  wraps to a large negative number. In practice this is not exploitable (timestamps this large
  cannot be entered via the MCP tool), but the comment could mislead future maintainers into
  believing the code is safe when it is technically unsound.
- **Recommendation**: Replace `obs.ts as i64` with `i64::try_from(obs.ts).unwrap_or(i64::MAX)`
  or `obs.ts.min(i64::MAX as u64) as i64` for correctness. At minimum, correct the comment to
  remove the false claim that wrapping "saturates to MAX." Not blocking.
- **Blocking**: no

### Finding 4: SQL IN-Clause Parameterized Correctly — No Injection Risk
- **Severity**: info (no risk)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` lines 2186–2245
- **Description**: `build_batch_meta_query` generates a SQL template with `?` placeholders (not
  string interpolation of user input). Entry IDs are `u64` values from the DB's own index bound
  via `query.bind(id as i64)`. The `sqlx` parameterized query API prevents SQL injection. The
  status filter `AND status != 'quarantined'` is a hardcoded literal. No injection vector exists.
- **Recommendation**: None. Current implementation is correct.
- **Blocking**: no

### Finding 5: `infer_gate_result` Substring Matching — Known "compass" Edge Case
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` line 2394–2405
- **Description**: Gate result inference uses `.contains("pass")` which matches substrings. The
  word "compass" would match the `Pass` branch. The RISK-TEST-STRATEGY.md (R-03, scenario 8)
  explicitly documents this as a "known fragility of naive `contains()` matching" and says the
  test should document whether embedded-word matches are accepted or guarded against. The
  implementation accepts this fragility. The blast radius is incorrect gate classification in the
  Phase Timeline table — report output only, no DB writes, no execution.
- **Recommendation**: Document explicitly in code that word-boundary checking was considered and
  rejected for simplicity (the comment in `infer_gate_result` partially does this). Not blocking.
- **Blocking**: no

### Finding 6: No Hardcoded Secrets
- **Severity**: info (no risk)
- **Location**: all changed files
- **Description**: No hardcoded API keys, tokens, passwords, or credentials found in the diff.
- **Blocking**: no

### Finding 7: No New Dependencies
- **Severity**: info (no risk)
- **Location**: Cargo.toml files (not changed by this PR)
- **Description**: The diff introduces no new crate dependencies. All code uses existing
  `rusqlite`/`sqlx`, `serde`, `tracing`, and `rmcp` dependencies.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

- `compute_phase_stats` error: the entire step is wrapped in a best-effort error boundary.
  On failure, `report.phase_stats = None` and a `tracing::warn!` is emitted. The Phase Timeline
  section displays "No phase information captured." All other sections render normally. The handler
  does not return an error response. Blast radius: degraded report, no data loss, no crash.

- `batch_entry_meta_lookup` failure: all chunks log warn and skip. `cross_feature_reuse = 0`,
  `intra_cycle_reuse = 0`, `top_cross_feature_entries = vec![]`. Knowledge Reuse section renders
  with zero cross-feature count. Handler continues. Blast radius: degraded report section only.

- `get_cycle_start_goal` failure: logged as warn, `goal = None`, header omits Goal line. Blast
  radius: missing header field only.

- `format_retrospective_markdown` panic: if any rendering function panics (e.g., integer
  overflow in duration math), the entire tool call returns an error to the MCP caller. No state
  is written. Blast radius: tool invocation fails; no other tools or sessions are affected.

**Data corruption**: no new DB write paths are introduced. All new code is read-only with respect
to the database. Silent data corruption is not possible — the worst case is incorrect report
rendering (wrong gate classification, wrong phase window assignment), not corrupted stored data.

**Denial of service**: the batch IN-clause query is chunked at 100 IDs. An adversary with write
access to the knowledge store could theoretically trigger a large batch query by storing many
entries and then running `context_cycle_review`. This is within the existing threat model (trusted
agents only).

---

## Regression Risk

**Affected functionality and regression potential:**

1. **`context_cycle_review` output format change**: The report header changes from
   `# Retrospective: {cycle}` to `# Unimatrix Cycle Review — {cycle}`. Any downstream consumer
   that pattern-matches the old header string will break. This is a documented breaking change
   (AC-17). Tests have been updated to expect the new format.

2. **Section order change**: Recommendations moved from position 9 to position 2. Tests that
   assert section ordering by string position or `starts_with` are updated. The existing test
   `test_all_none_optional_fields_valid_markdown` was updated. No automated regression guard exists
   for the full 12-section order — this was flagged as R-07 in the risk register.

3. **`compute_knowledge_reuse` signature change**: the function gained two new parameters
   (`current_feature_cycle: &str` and `entry_meta_lookup: G`). All call sites are updated.
   The compiler enforces this at compile time — no silent regression possible.

4. **`FeatureKnowledgeReuse` struct gains new fields**: all construction sites updated. Existing
   JSON consumers deserializing old payloads will see new fields defaulting to 0/empty via
   `#[serde(default)]`. Backward-compatible.

5. **`RetrospectiveReport` gains five new fields**: all existing construction sites updated with
   `None` defaults. Old JSON payloads deserialize correctly (backward-compatible via
   `#[serde(default, skip_serializing_if)]`).

6. **Phase narrative rendering unchanged**: `phase_narrative` section remains at position 12 (end).
   Existing behavior preserved.

**Low regression risk overall.** The compiler enforces struct literal migration. Serde defaults
provide backward compatibility for JSON consumers. The main regression exposure is the report
format change (header, section order) which is intentional and tested.

---

## PR Comments

- Posted 1 comment on PR #377
- Blocking findings: no

---

## Knowledge Stewardship

Nothing novel to store -- the markdown-injection-in-formatter pattern is documented in the risk
strategy and is specific to this PR. The accepted fragility (title escaping but not category/
feature_cycle) is a minor gap worth noting in a future PR but does not rise to a generalizable
anti-pattern lesson.
