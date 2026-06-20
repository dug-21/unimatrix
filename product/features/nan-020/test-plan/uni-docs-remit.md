# Test Plan — `.claude/agents/uni/uni-docs.md` remit widen

> Component: widen uni-docs authorship remit README-only → all of `docs/`, blast-radius-scoped, with
> the full-tree-audit non-goal stated; narrow source-read relaxation; retain prompt-injection defense.
> The ONE `.claude/` edit (C-5). Risks: R-13 (Med), R-14 (Low, human-owned), R-16 (scope statement).
> ACs: AC-07, AC-08. **Inspection-based — explicitly NOT machine-checked (C-3; no gold-plating).**

## Test Vehicle

Reviewer/human inspection of the `uni-docs.md` diff, plus a few grep tripwires for the forbidden
Feature-2 machinery. There is NO automated gate for the remit text itself — building one would be
the Feature-2 scope creep R-13 warns against.

## R-13 / AC-07 — scope-creep fence + blast-radius definition (inspection + grep tripwire)

- `test_remit_authorship_text_only` (inspection): the diff widens the SCOPE line README→all of
  `docs/` and adds the blast-radius rule — and NOTHING ELSE. No drift-checker, no CI gate, no
  Phase-4 trigger redesign (all Feature 2, forbidden C-5/SR-05).
- `test_remit_no_feature2_machinery` (grep tripwire): grep the diff for `drift`, `gate`, `Phase 4`/
  `Phase-4 trigger`, `CI` additions → none introduced as new mechanism. (A tripwire, not a contract.)
- `test_remit_blast_radius_defined` (inspection): the definition states authorship is
  **blast-radius-scoped** (the doc surfaces a change touches) AND states the **full-tree-audit
  non-goal explicitly** ("does NOT audit all of docs/ every cycle" — FR-18/AC-07/SR-07).
- `test_remit_relaxation_bounded` (inspection): the "no source code reading" relaxation is narrow —
  read the CLI surface a touched doc documents, NOT a general code-audit license. Prompt-injection
  defense + "document only what is shipped" rules RETAINED (FR-20).

## R-14 / AC-08 — N5 framing (human-owned inspection; NOT machine-checked)

- `test_n5_framing` (inspection): the artifact referencing N5 reads
  "deployable-as-released → usable-as-documented", names the doc-test as the docs-layer guard, binds
  the claim to the `--bundle` chain (AG-1 / R-16), N5 status unchanged, NO new NFR/capability id minted.
- `test_remit_internally_consistent` (inspection): scope, constraints, and self-check in `uni-docs.md`
  all admit `docs/` consistently (AC-07).

**R-14 is a process risk — explicitly flagged HUMAN-OWNED. No automated coverage exists or should be
built (gold-plating, C-3/ADR-004). Verified at the human gate.**

## R-16 (accepted residual) / AC-08 — scope statement

- `test_n5_bound_to_bundle_only` (inspection): the N5 "usable-as-documented" framing names the
  `--bundle` chain as THE doc-tested path; `--remote` is documented-only by design, not a silent
  omission. Confirms "usable-as-documented" is not mis-read as covering both modes.

## Self-Check

- [x] Diff is authorship-text only; no drift-checker/gate/Phase-4 trigger (R-13).
- [x] Blast radius defined + full-tree-audit non-goal stated (AC-07).
- [x] Source-read relaxation bounded; prompt-injection defense retained (R-13).
- [x] N5 framing inspected; bound to `--bundle` chain; no new NFR (AC-08, R-16).
- [x] R-14 explicitly human-owned, NOT machine-checked (C-3).
