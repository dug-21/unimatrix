# Test Plan: Documentation (`docs/testing/eval-harness.md`)

Component: documentation update only — no Rust code changes.

---

## Risk Coverage

| Risk | Verification |
|------|-------------|
| R-10 (Low) | Documentation review — all five AC-07 items present |

---

## Verification Method

Manual review of `docs/testing/eval-harness.md` after delivery (Stage 3c).

The RISK-COVERAGE-REPORT.md must record:
- `AC-07: PASS/FAIL` with evidence (line numbers or section names confirming presence).

---

## Required Content Checklist (AC-07)

All five items must be present in `docs/testing/eval-harness.md`:

### Item 1 — `context.phase` field documented

The scenario format reference section must include `context.phase` as a field, stating
it is populated from `query_log.phase` (col-028, GH #403).

Example expected text (not normative — delivery agent may word differently):
> `context.phase` (optional) — the workflow phase from the originating session.
> Populated from `query_log.phase` when the scenario was created from an MCP session
> that called `context_cycle`.

---

### Item 2 — Known vocabulary documented as snapshot, not allowlist

The documentation must state the known values as of nan-009:
- `"design"`, `"delivery"`, `"bugfix"`.
- Must note these are a current snapshot, not a fixed allowlist.
- Must note that new values appear in the report automatically.
- Must note that retroactive relabeling uses a `query_log` data migration.

This is the ADR-003 free-form vocabulary governance model.

---

### Item 3 — Section 6 "Phase-Stratified Metrics" documented in report reference

The eval report output reference must include section 6 with:
- Section heading: `## 6. Phase-Stratified Metrics`.
- Description of the Markdown table: one row per phase, columns Phase / Count / P@K /
  MRR / CC@k / ICD.
- Note that `"(unset)"` row appears last.
- Note that the section is omitted when all scenarios have null phase.
- Note that Distribution Analysis is now section 7.

---

### Item 4 — Phase population requirement documented

The documentation must note:
> Phase is populated only for MCP-sourced sessions that called `context_cycle`.
> UDS-only corpora and pre-col-028 databases produce no phase section.

This mitigates R-10 — operators who see no section 6 should understand this is
expected, not a bug.

---

### Item 5 — Migration-based governance model documented

The documentation must note the governance model for phase vocabulary evolution:
- New phase values appear in the report automatically (no code change needed).
- Retroactive relabeling of existing rows requires a `query_log` data migration.
- No schema change is needed (phase is free-form TEXT).

---

## File Size Check (R-11)

After delivery, verify that `aggregate.rs` does not exceed 500 lines:

```bash
wc -l crates/unimatrix-server/src/eval/report/aggregate.rs
```

If it approaches or exceeds 500 lines, the delivery agent must have extracted
`compute_phase_stats` to `aggregate_phase.rs`. This is a design constraint
(Constraint 7 / NFR-04), not a test.

The RISK-COVERAGE-REPORT.md must record the actual line count for R-11.
