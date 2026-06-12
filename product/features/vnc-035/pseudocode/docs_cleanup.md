# Component: docs_cleanup (uni-zero SKILL + agent docs)

**This is a doc-edit PLAN, not code.** Scope: AC-10 + FR-12. Coupled with AC-11 (the
`edges_carried` ack) — neither ships without the other (SR-05). Land **within this feature**.

## Purpose

Remove guidance that instructs agents to manually re-declare outgoing edges on `context_correct`,
and document the new defaults: (a) eligible outgoing edges **carry forward by default**; (b) the
supported opt-out / shed is `context_edge remove`/`redirect` against the **new** (Active) entry id;
(c) the Deprecated original is frozen and cannot be edited.

## Discovery (run before editing — the doc surface is small but verify)

The feature names "the `uni-zero` SKILL goal-curation guidance and any agent docs carrying the
're-declare edges on correction' warning." Confirmed surface from grep:

- **`.claude/skills/uni-zero/SKILL.md`** — the goal-curation sections that drive `context_correct`:
  - "Adding a new goal" (~line 242-248): step 6 "Enrich via `context_correct` as the strategy
    matures — each correction preserves the evolution."
  - "Updating a goal" (~line 250-254): steps 1-4 ending in "Apply via `context_correct`."
  - Note at ~line 51: "IDs change on every `context_correct`" (still true — leave as-is; it is
    about ID volatility, not edge re-declaration).

  **Current state:** the SKILL does NOT contain an explicit "re-declare the `Advances → vision_root`
  edge after correcting a goal" instruction. The cleanup is therefore primarily **additive
  reassurance** — state that the goal's edges (e.g. `Advances → vision_root`) now carry forward
  automatically, so a goal correction no longer silently orphans the edge (the AC-03 regression).
  If, during implementation, a "re-declare edges" line is found (in the SKILL or elsewhere), it is
  REMOVED per FR-12.

- **Agent docs grep:** re-run a content search for the literal guidance before editing —
  `re-declare`, `re-declaration`, `re-pass edges`, `manually re-declare`, "edges on correction" —
  across `.claude/skills/` and `.claude/agents/uni/`. Edit ONLY docs that actually instruct manual
  re-declaration on correction. Do NOT edit docs that merely mention `context_correct` for unrelated
  reasons (e.g. ID volatility, correction chains). The grep at design time surfaced
  `uni-research-sm.md`, `uni-vision-guardian.md`, `uni-spike-researcher.md`, `uni-js-dev.md` for the
  word "carry" and several for "context_correct" — inspect each; most are false positives (generic
  "carry forward context" prose), but the implementer MUST confirm none carries the re-declaration
  instruction. Flag any genuine hit and edit it.

## Edit plan — `.claude/skills/uni-zero/SKILL.md`

In the **"Updating a goal"** section (~line 250-254), add an explicit note after the steps:

```
**Updating a goal** — use `context_correct` to preserve the correction chain:
1. Propose the change in conversation. Quote what is changing and why.
2. Confirm with the human before writing.
3. Apply via `context_correct`.
4. Update `PRODUCT-VISION.md` if the goals table needs to reflect the change.

> Edges carry forward automatically (vnc-035). `context_correct` copies the original entry's
> eligible outgoing edges — including a goal's `Advances → {vision_root}` link — onto the new
> entry by default. You do NOT need to re-pass them in `edges`; the response reports
> `edges_carried` so you can confirm. To DROP an edge that no longer holds, use `context_edge
> remove`/`redirect` against the **new** entry id (the only Active source after correction) —
> never against the Deprecated original, which is frozen and rejects edits.
```

In the **"Adding a new goal"** section, step 6 ("Enrich via `context_correct` ... each correction
preserves the evolution") is already correct and need not change — but the carry-forward note above
reinforces that the `Advances → {vision_root_id}` edge created in step 3 survives every subsequent
correction. Optionally cross-reference the note; not required.

## Content requirements (what every edited doc must state — AC-10)

1. **Carry-forward is the default** — eligible outgoing edges copy onto the new entry automatically;
   no manual `edges` re-declaration on correction.
2. **Shed path targets the NEW entry id** — `context_edge remove`/`redirect` with `source_id =
   <new entry id>` (Active). This replaces the former "re-declare on correction" practice.
3. **Deprecated original is frozen** — editing edges against the original (Deprecated post-correct)
   is rejected (frozen-source). Stated explicitly so a reader does not target the old id (SR-08, R-10).
4. **`edges_carried` awareness** — the response reports how many edges carried (the awareness
   channel; there is no DB provenance marker — OQ-03). Couples the doc to AC-11.

## What NOT to change

- The "IDs change on every `context_correct`" note (SKILL ~line 51) — still accurate; unrelated to
  edge re-declaration.
- Correction-chain / supersession prose — unaffected by carry-forward.
- Any doc that mentions `context_correct` without instructing manual edge re-declaration.
- PRODUCT-VISION.md — goals-table sync is a separate, human-driven step, not part of this cleanup.

## Coupling / sequencing

- **AC-10 ⟺ AC-11 (SR-05):** the doc change is non-load-bearing ONLY because the `edges_carried`
  ack exists. Land both together; Gate verifies both present.
- No code dependency — the doc edits can be made in parallel with the Rust components, but must be
  in the same delivery as AC-11.

## Key verification (hints — doc review, not automated tests)

- The `uni-zero` SKILL "Updating a goal" section documents carry-forward default + the shed path
  against the **new** entry id + the frozen-original note.
- No remaining doc instructs manual edge re-declaration on `context_correct` (grep clean for the
  re-declaration phrasing after edits).
- The shed-path documentation names the **new** Active id, not the Deprecated original (R-10 guard).

## Open questions

- **Genuine vs false-positive doc hits:** the grep surfaced several agent docs for "carry" /
  "context_correct" that are likely generic prose. The implementer must inspect each and confirm
  whether any carries the actual re-declaration instruction. Flag any genuine hit; do not
  over-edit unrelated mentions. (Non-blocking — resolved by inspection at implementation time.)
