## ADR-005: Per-Profile Section 5 Rendering with Independent Gating

### Context

When multiple candidate profiles are present in a single `eval run`, the current Section 5
("Zero-Regression Check") renders a single block covering all regressions across all
candidates. nan-010 introduces a second gating mode (Distribution Gate) that applies to
profiles declaring `distribution_change = true`.

In a multi-profile run, it is possible for one candidate to declare `distribution_change = true`
and another to not. Three rendering options were considered:

**Option A — Single merged Section 5**: Flatten all profiles into a single Section 5 block.
For distribution-change profiles, show the Distribution Gate; for others, show Zero-Regression
rows. Problem: the merged view has no clear structure when gate types differ, and a
distribution-change profile that passes its gate appears in the same block as a standard
profile with regressions.

**Option B — Gate mode selected by majority**: If any profile declares `distribution_change =
true`, switch all of Section 5 to Distribution Gate mode. Profiles that do not declare
distribution change get Distribution Gate rendering applied to them. Problem: this renders an
incorrect gate for standard profiles and contradicts the per-profile independence stated in
SCOPE.md §Design Decisions #1.

**Option C — Per-profile Section 5**: Each non-baseline profile gets its own Section 5
sub-block, independently gated based on its own `distribution_change` flag. This is SCOPE.md
§Design Decisions #1: "A global Section 5 that flattens both modes hides whether the
distribution-changing profile passed — per-profile is the honest rendering."

The scope risk assessment (SR-05) flagged this as needing explicit resolution before
implementation because it affects the render loop structure.

### Decision

Section 5 is rendered once per non-baseline profile, independently gated.

For a single candidate profile (the common case), the header is:
- `## 5. Distribution Gate` (if `distribution_change = true`)
- `## 5. Zero-Regression Check` (existing behavior, unchanged)

For multiple candidate profiles, each gets a numbered sub-block:
- `### 5.1 Distribution Gate — {profile_name}` (if `distribution_change = true`)
- `### 5.2 Zero-Regression Check — {profile_name}` (if `distribution_change = false`)

The render loop in `render_report` iterates `stats` (non-baseline profiles only), and for
each calls either `render_distribution_gate_section` or the existing zero-regression block
based on `profile_meta.get(profile_name).distribution_change`.

`find_regressions` output is still computed for all profiles. For distribution-change profiles,
the regressions data is ignored in Section 5 rendering (not passed to
`render_distribution_gate_section`). It is not harmful to compute it — the existing logic runs
unchanged. This avoids a conditional in the aggregation path.

### Consequences

Easier:
- Each profile's Section 5 is self-contained and unambiguous.
- Adding a third candidate profile (two candidates + baseline) extends naturally: a third
  sub-block is added.
- Testing per-profile independence is straightforward: construct a run with two candidates
  (one with `distribution_change = true`, one without) and verify distinct section headers.

Harder:
- The single-candidate case (header `## 5.`) and multi-candidate case (header `### 5.N`) use
  different Markdown heading levels. The render loop must handle both cases. The
  implementation must count non-baseline profiles before the loop to choose heading level.
- Existing tests that assert on exact Section 5 Markdown output need to be confirmed against
  the single-profile case — the `## 5.` heading is unchanged for single-profile runs without
  `distribution_change`.
