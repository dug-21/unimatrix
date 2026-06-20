## ADR-004: Widen uni-docs Authorship Remit to All of docs/, Blast-Radius-Scoped

### Context

`docs/` has no author in the delivery workflow. `.claude/agents/uni/uni-docs.md` scopes the
agent to "README.md only" and forbids touching anything else, so `docs/client-setup.md`
drifted with nobody responsible (Goal 3, the #768 ownership half). This is the single
`.claude/` edit permitted in nan-020 (C-5). SR-05 warns it is a scope-creep magnet toward
Feature 2 (the `.claude/` automation-currency pattern); SR-07 warns "blast radius" is
under-specified, risking either a full-tree audit (gold-plating, C-4) or missed surfaces.
nan-005 established uni-docs and its delivery-protocol trigger; this extends that lineage
from README to all of `docs/`.

### Decision

**Widen uni-docs authorship to all of `docs/`, scoped by blast radius — authorship-remit
text only. No drift-checker, no gate, no Phase-4 trigger redesign (all Feature 2).**

Edits to `.claude/agents/uni/uni-docs.md`:
- Scope: "README.md only" → "README.md and all of `docs/`".
- Authorship is **blast-radius-scoped**: uni-docs updates the doc surfaces a delivered change
  *touches* — the files whose executable claims or narrative concern the behavior the
  feature changed — NOT a full-tree `docs/` audit every cycle. State the full-tree-audit
  non-goal explicitly in the definition (AC-07 / C-4).
- "Blast radius" operational definition: the set of doc files containing claims (executable
  or narrative) about the behavior a feature changed, determined from the feature's
  SCOPE/SPEC + the diff's touched surfaces — not by scanning all of `docs/` (SR-07).
- Relax the "no source code reading" rule NARROWLY: to write/verify an executable claim for
  `docs/`, uni-docs may read the CLI surface it documents, bounded to the touched surface —
  still not a general code-audit license.
- Authorship stays uni-docs's job; **detection of executable-claim drift stays the doc-test's
  job** — do not conflate (the doc-test is the gate, uni-docs is the author).

Explicitly OUT of scope for this edit (Feature 2, fences SR-05): any `.claude/` drift-checker,
any new CI gate, any redesign of the Phase-4 spawn trigger, any audit of `.claude/`
definitions.

### Consequences

- Easier: `docs/` finally has an owner; the #768 fix has an author; the pattern (author +
  gate, distinct) is established for Feature 2 to inherit without bundling it here.
- Harder: uni-docs's remit is broader, so the blast-radius scoping must be respected or the
  agent over-audits — mitigated by the explicit operational definition and full-tree-audit
  non-goal in the definition text.
- Bounded: exactly one `.claude/` file changes (C-5); no behavioral automation added.
  (Cross-ref ADR-003 for the executable/narrative split uni-docs applies; nan-005 ADRs
  #1254–#1257 for the lineage.)
