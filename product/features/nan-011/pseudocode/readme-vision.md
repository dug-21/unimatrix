# Component 1 — README + PRODUCT-VISION.md Repair

## Purpose

Bring `README.md` and `product/PRODUCT-VISION.md` into alignment with the current
implementation after multiple shipping cycles. Key changes: approved vision statement,
removal of NLI capability claims, addition of three new sections (Graph-Enhanced
Retrieval, Behavioral Signal Delivery, Domain-Agnostic Observation Pipeline), binary
name fix (`unimatrix-server` → `unimatrix`), and two targeted status fixes in
PRODUCT-VISION.md.

---

## Pre-Work: Read Before Editing

Read the following before touching any file:
- `/workspaces/unimatrix/product/features/nan-011/SCOPE.md` — approved vision statement verbatim text
- `/workspaces/unimatrix/README.md` — current state, identify existing sections
- `/workspaces/unimatrix/product/PRODUCT-VISION.md` — current state, find W1-5 row and HookType row

Do NOT rely on memory for the vision statement. Copy it character-for-character.

---

## README.md Operations

### Operation R-1: Vision Statement Replacement

LOCATE: The current opening paragraph of README.md (the "what is Unimatrix" text).
Current text begins with "Unimatrix is a self-learning..." or similar.

REPLACE with the four-paragraph approved vision statement (verbatim from SPEC FR-1.1):

```
Unimatrix is a workflow-aware, self-learning knowledge engine built for agentic
software delivery. It captures the knowledge that emerges from doing work —
decisions, patterns, lessons, conventions — and makes it trustworthy, retrievable,
and continuously improving. As agents move through delivery cycles, Unimatrix learns
what matters at each phase and delivers the right knowledge dynamically, before
agents need to ask for it. Knowledge retention becomes a first-class citizen of the
delivery process, not a side effect.

Unimatrix is not an orchestration engine. It does not coordinate agents, schedule
work, or manage workflows. It is a knowledge engine that understands workflow context
— your current phase, what your team has been doing, what comes next — and uses that
understanding to surface relevant knowledge at exactly the right moment.

The key mental model: workflow definitions, agent definitions, and skill definitions
are static — they live in your tooling and change infrequently. Architecture
decisions, patterns, and lessons-learned are dynamic — they evolve with every
feature, every delivery, every failure. Unimatrix was designed to manage the dynamic
layer. Every architectural pivot, every hard-won lesson, every reusable pattern is
captured, attributed, and made available to every future agent that needs it.

Built for agentic software delivery. Configurable for any workflow-centric domain.
```

IMMEDIATELY AFTER the vision block, add the FR-1.2 qualifier sentence as a new
paragraph (no heading):

```
This workflow-phase-conditioned delivery means knowledge is surfaced at phase
transitions based on what the engine has learned about each phase — it is not
unconditional injection into every prompt.
```

VERIFY: The combined vision block + qualifier matches the approved text. Do not
add headings between the four paragraphs. Do not reorder sentences.

### Operation R-2: Section Removal (NLI Sections)

LOCATE and DELETE entirely:
1. The section titled "Semantic Search with NLI Re-ranking" (or equivalent) — the
   entire section from its heading through the last paragraph before the next heading.
2. The section titled "Contradiction Detection and NLI Edge Classification" — same.

These two sections are REMOVED, not edited. Their content is replaced by R-3 and R-4.

### Operation R-3: New Section — Graph-Enhanced Retrieval

INSERT in the Capabilities block after "Knowledge Lifecycle", before "Adaptive
Embeddings". This replaces the removed NLI Re-ranking section.

Section heading: `### Graph-Enhanced Retrieval`

Content must cover ALL of the following (one to two paragraphs):
- HNSW vector similarity locates initial candidate entries
- PPR (Personalized PageRank) co-access traversal expands the pool to surface
  cross-category entries that pure vector search misses
- Phase-conditioned category affinity stratifies results by workflow phase
- Co-access ranking promotes entries historically retrieved together
- The three layers compose: semantic similarity → graph expansion → phase/co-access ranking
- PPR expansion contributes a confirmed +0.0122 MRR improvement

Do not claim NLI is used in this pipeline. MicroLoRA is a separate section —
retain it unchanged.

### Operation R-4: Updated Section — Contradiction Detection

LOCATE: The (now-removed) "Contradiction Detection and NLI Edge Classification"
section has been deleted in R-2. INSERT a replacement section in the Capabilities
block titled `### Contradiction Detection` (no NLI qualifier in the heading).

Content must accurately describe (one paragraph):
- Cosine Supports detection at threshold >= 0.65
- Contradiction density as a Lambda dimension using the periodic scan
- Manual contradiction management via `context_correct`

Do NOT claim NLI is used for contradiction detection. The cosine model is the
only active mechanism.

### Operation R-5: New Sections — Behavioral Signal Delivery and Domain-Agnostic Pipeline

INSERT two new sections in the Capabilities block (per ADR-001 canonical order):

Section 1: `### Behavioral Signal Delivery`
Content (one paragraph minimum):
- Cycle outcomes (from `context_cycle`) feed as graph edges, reinforcing co-access
  signals between entries retrieved during successful delivery phases
- Goal-conditioned briefing: `context_briefing` uses the current phase and cycle
  history to prioritize knowledge relevant to the agent's declared phase
- Reference: crt-046, Group 6

Section 2: `### Domain-Agnostic Observation Pipeline`
Content (one paragraph minimum):
- `source_domain` guard on all detection rules; each rule fires only for its declared
  domain
- Domain packs registered via `[[observation.domain_packs]]` in config.toml
- Built-in "claude-code" domain pack is always active, requires no configuration
- Any domain's event stream connects without code changes
- Reference: W1-5, col-023

### Operation R-6: Binary Name Fix Throughout README

FIND ALL occurrences of:
- `unimatrix-server` — replace with `unimatrix`
- `target/release/unimatrix-server` — replace with `target/release/unimatrix`

Use Grep to search before editing to ensure complete coverage:
```bash
grep -n "unimatrix-server" README.md
```

Every match must be corrected. Fenced code blocks are included — a binary name
in a shell example is as much a violation as prose.

VERIFY after editing:
```bash
grep "unimatrix-server" README.md
# Must return zero matches
```

### Operation R-7: Section Order Enforcement

After all insertions and deletions, verify the Capabilities sub-section order matches
ADR-001:
1. Knowledge Lifecycle
2. Graph-Enhanced Retrieval  (new, replaces NLI Re-ranking)
3. Adaptive Embeddings / MicroLoRA  (retained, no changes)
4. Behavioral Signal Delivery  (new)
5. Contradiction Detection  (updated, no NLI claim)
6. Domain-Agnostic Observation Pipeline  (new)

If sections are out of order after editing, reorder them.

---

## PRODUCT-VISION.md Operations

### Operation V-1: Vision Statement Replacement

LOCATE: The opening Vision section paragraph (currently begins "Unimatrix is a
self-learning knowledge integrity engine...").

REPLACE with the same four-paragraph approved vision statement used in R-1.
Use the verbatim text — character-for-character. The qualifier sentence (FR-1.2)
is README-only; do not add it to PRODUCT-VISION.md unless FR-1.3 explicitly
requires it (it does not — FR-1.3 only requires the vision statement itself).

VERIFY: diff the vision block against the approved text. Zero character differences.

### Operation V-2: W1-5 Status Fix

LOCATE: The W1-5 row in the roadmap/milestone table. The current status is
"IN PROGRESS" or equivalent incomplete marker.

CHANGE the W1-5 row status to: `COMPLETE — col-023, PR #332, GH #331`

Exact format may adapt to the existing table structure, but must include all
three references: `col-023`, `PR #332`, `GH #331`.

VERIFY: Read the W1-5 section back and confirm "COMPLETE", "col-023", "PR #332",
and "GH #331" are all present in the same row/block.

### Operation V-3: HookType Domain Coupling Row Fix

LOCATE: The Domain Coupling table. Find the row for "HookType enum tied to Claude
Code events".

Current Status column: "In progress" or "Open" or equivalent incomplete marker.

CHANGE the Status column to: `Fixed — col-023 / W1-5 (PR #332)`

VERIFY: The row contains "Fixed", "col-023", "W1-5", and "PR #332".

---

## Error Handling

If the current README structure differs significantly from expectations (e.g., sections
use different headings than anticipated), do not guess at intent:
- Read the full README first with Read tool
- Map existing headings to the canonical ADR-001 order
- Apply each operation to the correct existing heading
- Flag any structural mismatch in the agent report

If PRODUCT-VISION.md's W1-5 row cannot be uniquely located, read surrounding context
(10 lines before/after) to confirm you have the correct row.

---

## Key Test Scenarios

1. Vision statement verbatim check: diff README.md vision block against SPEC FR-1.1 text —
   zero character differences.
2. FR-1.2 qualifier present: search for "workflow-phase-conditioned" in README.md —
   must return at least one match, immediately after the vision block.
3. NLI section removal: `grep -i "nli re-rank\|nli cross-encoder\|nli contradiction\|nli re-ranker\|nli sort" README.md` — zero matches.
4. Binary fix: `grep "unimatrix-server" README.md` — zero matches.
5. Graph-Enhanced Retrieval present: grep for "Graph-Enhanced Retrieval" in README.md —
   must match; paragraph must mention "PPR".
6. PRODUCT-VISION.md W1-5: grep for "col-023" and "COMPLETE" near W1-5 — must both match.
7. PRODUCT-VISION.md HookType: grep for "Fixed" near "HookType" in Domain Coupling table.
