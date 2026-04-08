# Test Plan: README + PRODUCT-VISION.md Repair

## Component Scope

Files under test:
- `README.md` (repo root)
- `product/PRODUCT-VISION.md`

Acceptance criteria covered: AC-01, AC-02, AC-03, AC-04, AC-05
Risks covered: R-10, R-11, R-13

---

## AC-01: Vision Statement Verbatim Check

**Risk**: R-10 (Med/Med) — any character-level deviation from the approved text fails this AC.

### Step 1 — Extract approved text from SCOPE.md

The approved vision statement lives in `product/features/nan-011/specification/SPECIFICATION.md`
under FR-1.1. It is a four-paragraph block beginning "Unimatrix is a workflow-aware..." and
ending "Configurable for any workflow-centric domain." The FR-1.2 qualifier sentence
("This workflow-phase-conditioned delivery...") appears immediately after in README.md.

### Step 2 — Verify README.md opening block

```bash
# Confirm vision statement paragraph 1 is present verbatim
grep -c "Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic" README.md
# Expected: 1

grep -c "Configurable for any workflow-centric domain\." README.md
# Expected: 1

# Confirm FR-1.2 qualifier sentence is present
grep -c "This workflow-phase-conditioned delivery means knowledge is surfaced at phase" README.md
# Expected: 1
```

### Step 3 — Character-level diff (manual)

Read the README.md opening block. Read the FR-1.1 approved text from SPECIFICATION.md.
Assert: zero character differences (no word substitutions, no punctuation changes,
no reordered sentences, no extra line breaks, no trailing spaces).

Assert: the FR-1.2 qualifier sentence appears IMMEDIATELY after the vision block, not
one or more sections below it.

### Step 4 — Verify PRODUCT-VISION.md opening block

```bash
grep -c "Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic" product/PRODUCT-VISION.md
# Expected: 1
```

Perform the same manual diff as Step 3. PRODUCT-VISION.md uses the vision statement
(FR-1.1 block) but does NOT require the FR-1.2 qualifier sentence.

**Pass criteria**: Both grep counts return 1. Both manual diffs show zero character differences.

---

## AC-02: Zero NLI Re-ranking References in README

**Risk**: R-11 (Med/Med)

```bash
grep -i "nli re-rank\|nli cross-encoder\|nli contradiction\|nli re-ranker\|nli sort" README.md
# Expected: zero matches (empty output)
```

**Pass criteria**: Command produces no output.

---

## AC-03: New Capability Sections Present

**Risk**: none — direct spec compliance check

### Step 1 — Graph-Enhanced Retrieval section

```bash
grep -n "Graph-Enhanced Retrieval\|Graph.Enhanced Retrieval" README.md
# Expected: at least one match (section heading)
```

Then read the surrounding paragraph. Assert:
- PPR (Personalized PageRank) expansion is mentioned
- Phase-conditioned category affinity is mentioned
- Co-access ranking is mentioned
- The composition model (similarity → graph expansion → phase/co-access) is described

### Step 2 — Behavioral signal delivery paragraph

```bash
grep -n -i "behavioral signal\|Behavioral Signal" README.md
# Expected: at least one match
```

### Step 3 — Domain-agnostic observation pipeline paragraph

```bash
grep -n -i "domain.agnostic observation\|Domain.Agnostic Observation" README.md
# Expected: at least one match
```

### Step 4 — Confirm removed sections are absent

```bash
grep -in "Semantic Search with NLI\|NLI Re-ranking" README.md
# Expected: zero matches

grep -in "NLI Edge Classification\|Contradiction Detection and NLI" README.md
# Expected: zero matches
```

**Pass criteria**: Steps 1-3 show at least one match each. Step 4 shows zero matches.

---

## AC-04: Binary Name Correction

**Risk**: R-11 (binary name subcategory)

```bash
grep -n "unimatrix-server" README.md
# Expected: zero matches (including inside fenced code blocks and inline code)
```

```bash
# Confirm correct binary path is present
grep -n "target/release/unimatrix" README.md
# Expected: at least one match
```

**Edge case**: `unimatrix-server` may appear inside a code block showing the old name for
historical comparison. Any such occurrence fails AC-04 — the README must not document the
old name in any context that could confuse an operator.

**Pass criteria**: First command produces zero output. Second produces at least one match.

---

## AC-05: PRODUCT-VISION.md Status Rows

**Risk**: R-13 (Low/Low) — wrong-row edits are visually subtle in PR diffs

### Step 1 — W1-5 row completeness

```bash
grep -n "W1-5\|col-023\|PR #332\|GH #331" product/PRODUCT-VISION.md
# Expected: multiple matches; all four strings must appear
```

Manual verification: read the W1-5 row. Confirm "COMPLETE", "col-023", "PR #332", and
"GH #331" all appear in the SAME row or immediately adjacent block (not scattered to
unrelated rows).

### Step 2 — HookType domain coupling row

```bash
grep -n "HookType" product/PRODUCT-VISION.md
# Expected: at least one match in the Domain Coupling table
```

Manual verification: find the HookType row. Confirm Status column shows "Fixed" (or
equivalent) with a reference to col-023/W1-5/PR #332. Confirm the row is in the Domain
Coupling table, not a different section.

**Pass criteria**: Both grep commands return matches. Manual review confirms correct rows
were edited, not adjacent rows.
