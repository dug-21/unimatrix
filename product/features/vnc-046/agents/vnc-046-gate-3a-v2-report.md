# Agent Report: vnc-046-gate-3a-v2 (Gate 3a re-validation, iteration 1)

**Result: PASS** (was REWORKABLE FAIL). Report: `reports/gate-3a-report.md`.

## What I checked
The single gate-blocking item from iteration 0 — OQ-2 (per-slug signature scanner omitted) — plus
the spawn-prompt's four verification questions. Per the validation-iteration cap I checked only the
previously-failed item and its blast radius, not the full check set.

## Findings
- **OQ-2 closed** across ADR-002 (per-slug decision, fallible, triple), project-provisioner.md (P1
  builds scanner before the tick clone, `map_err`→`ServerError::Config`+`?`), boot-assertion.md
  (probe carries `signal_class_names` + `has_hold`; P3 class-names sentinel), OVERVIEW (Shared Types
  + OQ-2 resolved). Empty-scanner defect is netted behaviorally by the authoritative INV-C1 "must not
  false-pass on declared-but-zero #930 symptom" (`test-plan/project-provisioner.md` l.49-51).
- **OQ-1 / OQ-3 consistent** across ADR-002 and ADR-003.
- **3 WARN residuals** (non-blocking, Stage-3b/3c must-confirm): (1) scanner block references
  out-of-scope `r` inside `build_project_server` — thread the scanner/patterns as a param at the call
  site per ADR-002's params-at-end discipline; (2) authoritative AC-07 parity test not strengthened
  to signal-bearing/count>0 (guard lives only in the pseudocode hint); (3) OVERVIEW OQ-1/OQ-3 notes
  stale vs the now-ratified ADRs.

## Knowledge Stewardship
- Queried: reviewed prior gate report + ADR-002/003, project-provisioner/isolation-suite/boot-assertion
  pseudocode, authoritative test-plan, and `http_provision.rs`/`main.rs` source to verify the fix.
- Stored: nothing novel to store -- both candidate patterns (rework strengthening the pseudocode hint
  but not the authoritative test plan; compiling a per-slug value from ambient `r` inside a
  params-at-end function where `r` is out of scope) are single-feature observations captured in the
  gate report's WARN table, not yet cross-feature recurrences that warrant a validation lesson/pattern.
