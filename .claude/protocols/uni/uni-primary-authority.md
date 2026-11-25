# Primary Authority — Shared Contract for uni-zero-design / uni-zero-delivery

Loaded by `/uni-zero-design`, `/uni-zero-delivery`, and `/uni-zero-bugfix`. For bugfix, `{feature-id}` = the issue number. One source so the two primaries never drift.

---

## Goal-Scoped Orientation

Orient on **the goal(s) this feature advances only** — not uni-zero's full survey.

1. `product/PRODUCT-VISION.md` — Vision + Architectural Principles sections.
2. Feature's GH issue → its `goal:*` label(s) → `context_lookup(category="goal", tags=["goal","{tag}"], agent_id="{your skill name}")`.
3. Target capability: the capability ID(s) the issue names → `context_get` each (read `done_when`).
4. Feature ADRs: `context_lookup(category="decision", feature="{feature-id}")`.

Heavy reading (full artifact sets, code, long diffs) goes to an `Explore` agent or `uni-zero-reviewer`. Keep your own context for triage and decisions.

---

## Authority Matrix

**Always escalate to the human** (never decide, even if the answer seems obvious):

| # | Trigger |
|---|---------|
| H1 | Security posture — any change to auth, trust, secrets, attack surface, or a security-review finding's disposition |
| H2 | Scope change — adding, removing, or re-carving anything in SCOPE.md |
| H3 | Contradicting an approved ADR |
| H4 | Accepting a gap — known defect, uncovered risk, or missing test shipped as-is |
| H5 | Dropping or degrading a supported platform / harness / transport |
| H6 | Merging |
| H7 | Weakening, deferring, or redefining an acceptance criterion or `done_when` |
| H8 | Skipping or waiving a gate, or classifying a gate failure as "pre-existing" |
| H9 | Editing a protocol, skill, or agent definition mid-run |
| H10 | Any decision whose justification is "small enough to defer" (last-mile of this outcome) |
| H11 | Overturning a human-decided register entry |

**You may decide** everything else — provided the answer is grounded in the register, the vision, an approved ADR, or Unimatrix knowledge, and is recorded (below). If you cannot cite a source, escalate.

Batch human questions: collect pending H-items and ask in one round unless one blocks progress.

---

## Decision Register

Location: `product/features/{feature-id}/DECISIONS.md`. Created by uni-zero-design; read-only input to uni-zero-delivery except for appending delivery-time entries.

```markdown
| ID | Phase | Question | Decision | Decided-by | Source | Rationale |
|----|-------|----------|----------|------------|--------|-----------|
| D-001 | design | ... | ... | human | — | ... |
| D-002 | design | ... | ... | uni-zero-design | ADR #NNNN | one clause |
```

- `Decided-by`: `human` | `uni-zero-design` | `uni-zero-delivery`.
- `Source`: ADR id, vision section, Unimatrix entry id, or register ID. Required for non-human entries.
- Entries are append-only. A reversal is a new entry referencing the old ID.

**Independence check:** before any sign-off, every non-human entry is reviewed by `uni-zero-reviewer`, passed **only** the register path, artifact paths, and IDs — never your rationale beyond what the register states.

---

## Stall Backstop

If a child agent completes and the SM does not continue (no progress after its return), resume the SM via `SendMessage` with: "Child {agent} returned; continue from protocol step {X}." Record every firing in `product/features/{feature-id}/STALLS.md` (timestamp, phase, child, step). Each firing is a protocol/SM defect to fix in retro — not normal operation.

---

## Boundaries (both skills)

- Never run the protocol yourself; the SM does. You kick off, triage, resume, and report.
- Never generate design/implementation content — that's the specialists'.
- Never write `capability` or `goal` entries. Recommend capability status changes in your final report; `/uni-zero` applies them.
- Never modify `crates/`.
