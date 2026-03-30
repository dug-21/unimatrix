# crt-033-docs Agent Report

## Agent
crt-033-docs (Documentation Agent)

## Task
Update README.md for feature crt-033 (#453).

## Artifacts Read
- `product/features/crt-033/SCOPE.md`
- `product/features/crt-033/specification/SPECIFICATION.md`
- `README.md`

## Sections Modified

### 1. MCP Tool Reference — `context_cycle_review` row
- Added memoization behavior to the Purpose column: first call computes and stores the full report; subsequent calls return the stored record without recomputation.
- Added version advisory note: when stored record schema version differs from current, an advisory is included.
- Added `force` parameter to key params: `force` (bool, default false — when true, forces recomputation even if a stored record exists).
- Source: SCOPE Goals 2–5, SPEC FR-01/FR-04, AC-12.

### 2. MCP Tool Reference — `context_status` row
- Added `pending_cycle_reviews` to the Purpose column description (cycle IDs started within the retention window with no stored cycle review, always computed).
- Extended "When to Use" to include identifying cycles awaiting retrospective review before signals can be purged.
- Source: SCOPE Goal 6, SPEC FR-09/FR-11, AC-09.

### 3. Architecture Overview — Storage section
- Updated schema version 16 → 18 (crt-033 delivers v18 via v17→v18 migration).
- Updated table count 20 → 21 (new `cycle_review_index` table).
- Source: SCOPE AC-01, SPEC AC-02b.

### 4. Architecture Overview — Data Layout block
- Updated `schema v16` → `schema v18` in the `unimatrix.db` comment.
- Source: same as above.

## Commit
`d88e079` — `docs: update README for crt-033 (#453)` on branch `feature/crt-033`.

## No Source Code Read
All understanding derived from SCOPE.md and SPECIFICATION.md only.

## Sections Not Affected
- Core Capabilities (no new capability category added)
- Skills Reference (no skill changes)
- Knowledge Categories (no category changes)
- CLI Reference (no CLI changes)
- Tips for Maximum Value (no new operational constraint)
- Security Model (no security model change)
