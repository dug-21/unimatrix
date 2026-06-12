# Test Plan — `uni-zero` SKILL + agent docs cleanup

> Component: doc updates landing **within** this feature (FR-12 / AC-10). Verification is
> **file-check + grep**, not runtime tests. **AC-10 + AC-11 are one acceptance unit** (SR-05):
> the `edges_carried` ack (AC-11, tested in context_correct_handler.md) is what makes the doc
> change non-load-bearing — neither ships without the other.
>
> Risk relevance: **R-10** (shed must target the new Active id; docs must not instruct shed
> against the Deprecated original). No code-execution risk in this component.

## Scope of the doc change

- `.claude/skills/uni-zero/` SKILL goal-curation guidance.
- Any agent docs carrying the **"re-declare edges on correction"** warning.

The change: (a) **remove** manual re-declaration guidance; (b) document carry-forward as the
**default**; (c) document `context_edge remove`/`redirect` against the **new** entry id as the
shed path; (d) note the **Deprecated original is frozen** and cannot be edited.

## Verification (AC-10) — file-check + grep checklist

### DC-01 — manual re-declaration guidance removed — REQUIRED
- **Grep**: the "re-declare edges on correction" (and equivalent "manually re-add edges")
  guidance no longer appears as an instruction in `uni-zero` SKILL or agent docs.
- **Assert**: zero remaining instructions telling agents to manually re-declare outgoing edges
  after `context_correct`. (Historical/changelog mentions framed as "no longer needed" are
  acceptable; live instructions are not.)

### DC-02 — carry-forward default documented — REQUIRED
- **Grep/file-check**: the docs state outgoing edges carry forward **by default** on
  `context_correct` (no `edges` param required), and reference the `edges_carried` ack as the
  awareness signal.

### DC-03 — shed path documented against the NEW entry id — REQUIRED (R-10)
- **Grep/file-check**: the shed/opt-out path is documented as `context_edge remove`/`redirect`
  with `source_id = <new entry id>`. The docs must **not** instruct shedding against the
  original/Deprecated id.

### DC-04 — Deprecated-original-frozen note present — REQUIRED (R-10 / SR-08)
- **File-check**: the docs explicitly note the Deprecated original cannot be edited
  (frozen-source rejection), so edge changes target the new Active entry only.

### DC-05 — AC-10/AC-11 coupling honored — REQUIRED (SR-05)
- **Cross-check**: confirm the `edges_carried` ack (AC-11) is implemented and tested
  (context_correct_handler.md) — the doc change is valid only alongside the working ack.
  Stage 3c notes both as one acceptance unit in RISK-COVERAGE-REPORT.md.

## Method
Stage 3c records each DC-0x as a grep/file-check result in RISK-COVERAGE-REPORT.md under the
AC-10 row. No `cargo`/`pytest` execution applies to this component — it is a documentation
gate verified by inspection.

## Out of scope
- The `edges_carried` ack behavior itself → context_correct_handler.md (AC-11).
- Shed runtime behavior (`context_edge` against B vs Deprecated A) → context_correct_handler.md
  (`test_shed_*`, R-10). This plan only verifies the **documentation** of the shed path.
