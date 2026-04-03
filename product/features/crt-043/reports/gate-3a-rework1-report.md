# Gate 3a Rework1 Report: crt-043

> Gate: 3a (Design Review — rework iteration 1)
> Date: 2026-04-02
> Result: REWORKABLE FAIL

## Scope of this report

This rework iteration checks only the three items that failed or were warned in the initial
gate-3a-report.md:

1. **Interface consistency** — test-plan/schema-migration.md assertion #6 visibility contradiction
2. **Knowledge stewardship** — pseudocode/OVERVIEW.md `Queried:` block
3. **Knowledge stewardship** — test-plan/OVERVIEW.md `Queried:` block

Checks that previously PASSED (Architecture Alignment, Specification Coverage, Risk Coverage)
are not re-evaluated.

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Interface consistency — assertion #6 vs. goal-embedding.md | FAIL | Test plan now says `pub`; OVERVIEW.md WARN-2 rework now says `pub(crate)`. Contradiction shifted, not resolved. |
| Knowledge stewardship — pseudocode/OVERVIEW.md | PASS | Documents attempt + disconnect + fallback. |
| Knowledge stewardship — test-plan/OVERVIEW.md | PASS | Documents attempt + disconnect + fallback + store rationale. |

---

## Detailed Findings

### 1. Interface Consistency — `encode_goal_embedding` / `decode_goal_embedding` visibility

**Status**: FAIL

The rework changed two artifacts in conflicting directions:

**test-plan/schema-migration.md assertion #6** (lines 281–282) now reads:

> "Both helpers are marked `pub` with re-export from `lib.rs`, per WARN-2 resolution in
> pseudocode/OVERVIEW.md: Group 6 accesses decoded embeddings via a future store query
> method (`get_cycle_start_embedding`), not by calling `decode_goal_embedding` directly
> from `unimatrix-server`. However, helpers are promoted to `pub` now to avoid a breaking
> change when Group 6 ships."

This correctly reflects the goal-embedding.md Option A decision.

**pseudocode/OVERVIEW.md WARN-2 section** (lines 15–23) now reads:

> "`decode_goal_embedding` is declared `pub(crate)` in `unimatrix-store`. This is correct
> and does not need to be promoted to `pub`. [...] If Group 6 designs a direct decode
> call-site in `unimatrix-server`, the visibility must be promoted to `pub` at that point
> via a separate PR."

The Shared Types section of pseudocode/OVERVIEW.md (lines 50–53) still shows:

```
pub(crate) fn encode_goal_embedding(vec: Vec<f32>) -> Result<Vec<u8>, bincode::error::EncodeError>
pub(crate) fn decode_goal_embedding(bytes: &[u8]) -> Result<Vec<f32>, bincode::error::DecodeError>
```

**goal-embedding.md** (unchanged, lines 141–167) explicitly states:

> "Decision for implementation agent: Use Option A. Promote both helpers to `pub` and
> re-export from `lib.rs`."

And the import path in the pseudocode uses `unimatrix_store::embedding::encode_goal_embedding`
(line 101), which requires a cross-crate-accessible function.

**Contradiction state after rework:**

| Document | Visibility decision |
|----------|---------------------|
| goal-embedding.md | `pub`, Option A (re-export from lib.rs) |
| test-plan/schema-migration.md assertion #6 | `pub`, re-export from lib.rs |
| pseudocode/OVERVIEW.md WARN-2 + Shared Types | `pub(crate)` — stays as-is |

The rework fixed the test plan (now agrees with goal-embedding.md) but simultaneously
reverted OVERVIEW.md to assert the opposite. A delivery agent reading OVERVIEW.md
will implement `pub(crate)`, producing a compilation error when `unimatrix-server`
calls `unimatrix_store::embedding::encode_goal_embedding`.

**Root cause:** The rework addressed the wrong document. The original gate report (#4, issue #1)
stated: "Update assertion #6 to reflect the WARN-2 resolution decision (helpers are `pub`)."
The test plan was correctly updated. But OVERVIEW.md's WARN-2 section was also rewritten —
in the opposite direction — reverting it to `pub(crate)`. The OVERVIEW.md WARN-2 section
should have been left alone (or updated to match goal-embedding.md's Option A decision).

**Fix required:**

Update pseudocode/OVERVIEW.md WARN-2 section to match goal-embedding.md's explicit Option A
decision. The section should state that both helpers are promoted to `pub` and re-exported
from `lib.rs`, because `encode_goal_embedding` must be callable from `unimatrix-server`.
The Shared Types section signatures must be updated from `pub(crate)` to `pub`.

---

### 2. Knowledge Stewardship — pseudocode/OVERVIEW.md

**Status**: PASS

Lines 136–138:

```
## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — attempted; Unimatrix MCP server was
  disconnected at agent spawn time. Fell back to reading ADR files directly from
  product/features/crt-043/architecture/. ADR entry IDs #4067, #4068, #4069 referenced
  by number from IMPLEMENTATION-BRIEF.md.
- Deviations from established patterns: none. The `pragma_table_info` pre-check pattern
  (entry #1264), the `enrich_topic_signal` pre-capture pattern (entry #3374), and the
  fire-and-forget spawn pattern (entry #735) are all followed as specified.
```

The `Queried:` entry documents a genuine attempt, the disconnection condition, and the
fallback path taken. Entry IDs referenced in the fallback provide evidence the agent
consumed the relevant knowledge. This satisfies the requirement.

---

### 3. Knowledge Stewardship — test-plan/OVERVIEW.md

**Status**: PASS

Lines 153–155:

```
## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — attempted; Unimatrix MCP server was
  disconnected at agent spawn time. Fell back to reading ADR files directly from
  product/features/crt-043/architecture/. ADR entry IDs #4067, #4068, #4069 referenced
  by number from IMPLEMENTATION-BRIEF.md.
- Stored: nothing novel to store — the migration fixture builder pattern is already
  established in `migration_v19_v20.rs` and the fire-and-forget test pattern is documented
  in existing entries (#735, #771).
```

Both `Queried:` and `Stored:` entries are present with documented reasoning. PASS.

---

## Rework Required

| # | Issue | Which Agent | What to Fix |
|---|-------|-------------|-------------|
| 1 | pseudocode/OVERVIEW.md WARN-2 section says helpers stay `pub(crate)` and the Shared Types signatures show `pub(crate)` — contradicting goal-embedding.md Option A decision and test-plan/schema-migration.md assertion #6 | pseudocode agent | Rewrite WARN-2 section to reflect the Option A decision from goal-embedding.md: helpers are promoted to `pub` and re-exported from `lib.rs` because `encode_goal_embedding` must be callable from `unimatrix-server`. Update Shared Types signatures from `pub(crate)` to `pub`. The `decode_goal_embedding` note about Group 6 using a store query method may remain as context, but the conclusion must be: both helpers are `pub` now. |

---

## Scope Concerns

None.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — not called (Unimatrix MCP not accessible in
  validator context).
- Stored: nothing novel to store — the pattern of a rework introducing a new contradiction
  in a different document while fixing the original is feature-specific and not yet
  recurring across features.
