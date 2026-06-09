# Test Plan — Band-1/2 docs (Wave 2)

**Component**: `docs/testing/eval-harness.md` (modify) + Band-2 guides (authoring guide, migration runbook, two-corpus model, config-knob reference).
**Wave**: 2 — deferrable, **zero code coupling** to Wave-1 (NFR-04). Does NOT gate the AC-14 sweep.

## Verification method: file-check + manual doc-review (no runtime tests)

### AC-10 — capability doc
- File-check: `docs/testing/eval-harness.md` diff covers all new capabilities (tunability levers, trust metric class, token-weighted cost, fixture corpus, two-corpus model, drift guard).
- Unimatrix ADR entries exist (#4893–#4898 per the brief's corrected IDs).

### AC-11 — Band-2 sufficiency (doc-review checklist)
- Authoring guide, migration runbook, two-corpus doc, config-knob reference all exist under `docs/testing/`.
- **Doc-review checklist** confirms a dev (human or agent) could author a scenario / migrate the corpus / run a sweep **from the docs alone**, without reverse-engineering code.
- The config-knob reference states each lever's meaning, valid range, default, effect (FR-05/FR-25).
- **NFR-08 cost-proxy caveat**: the config-knob reference documents the token-proxy's definition + stated error bars, labeled explicitly as a **proxy** (the doc half of the OVERVIEW §5 out-of-band obligation; the ADR-003 statement ships in Wave-1).
- Corpus authoring guide carries the ADR-004 §5 depth obligation (deprecated-connected crossover findable in a bracketed range).

## Wave independence
These artifacts must be **absent** during the Wave-1-alone test (`corpus-loader.md` R-14). A Wave-1 code path depending on any of these files is a defect.
