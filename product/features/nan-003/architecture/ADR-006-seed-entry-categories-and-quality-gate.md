## ADR-006: Seed Entry Categories Restricted to convention/pattern/procedure + What/Why/Scope Gate

### Context

The uni-init prototype generated 67 entries, all subsequently deprecated. Root cause analysis (SCOPE.md background research): fully automated extraction with no human validation produced low-signal entries. A second risk: seeding with the wrong categories (e.g., outcome entries or ADR entries) pollutes the knowledge base with entries that will be superseded immediately when real feature work begins.

`/unimatrix-seed` explores arbitrary repo structure and proposes knowledge entries. Without a category scope restriction and a quality gate, it can generate any type of entry — including ADRs, outcomes, and lessons that are meaningless without feature context.

### Decision

**Category restriction**: `/unimatrix-seed` may only propose entries in three categories:
- `convention` — project-level standards (naming, file layout, process)
- `pattern` — reusable architectural/implementation approaches
- `procedure` — step-by-step workflows specific to this repo

The following categories are **excluded from seeding**:
- `decision` (ADR) — ADRs emerge from architectural decisions made during feature work; they cannot be seeded from repo exploration alone
- `outcome` — outcomes require completed feature cycles; they cannot be seeded
- `lesson-learned` — lessons require failures; they cannot be seeded
- `duties` — duties are role-specific; they don't emerge from arbitrary repo exploration

**Quality gate**: Every entry proposed by `/unimatrix-seed` must pass the What/Why/Scope test before being presented to the human for approval:

| Field | Rule | Reject if |
|-------|------|-----------|
| `what` | One sentence ≤ 200 chars describing the convention/pattern/procedure | Exceeds 200 chars; missing; API doc rather than a reusable insight |
| `why` | ≥ 10 chars explaining consequence or motivation | Under 10 chars; "it works"; tautological |
| `scope` | Where it applies: module name, file type, workflow, or team context | Missing |

Entries that fail the quality gate are discarded before presentation — not presented to the human for rejection. The skill instruction must not surface low-quality entries to the human (they consume approval bandwidth and set a low bar).

**Approval mechanics**:
- Level 0: batch approval — present all 2-4 entries together; human approves or rejects the batch
- Level 1+: per-entry approval — present each entry individually; human approves or rejects each

**Dedup rule**: Before storing any approved entry, the skill notes that server-side dedup (0.92 cosine) will catch exact duplicates. Near-duplicates are prevented by the EXISTING_CHECK state at skill entry (warn if ≥ 3 active entries already exist in seeding categories).

### Consequences

- The category restriction prevents the most common seed quality failures (wrong-category entries that age poorly)
- The What/Why/Scope gate, applied before human presentation, keeps the approval conversation focused on genuinely useful entries
- Batch approval at Level 0 is correct because 2-4 high-level repo entries are low-risk; individual approval at Level 1+ is correct because deeper entries are higher-stakes
- The excluded categories (decision, outcome, lesson-learned) are not blocked from the knowledge base permanently — they populate naturally through feature work and retrospectives using the appropriate skills (`/store-adr`, `/record-outcome`, `/store-lesson`)
- The threshold for the "already seeded" warning (≥ 3 active entries in seeding categories) is a concrete operational definition left to the spec writer to validate
