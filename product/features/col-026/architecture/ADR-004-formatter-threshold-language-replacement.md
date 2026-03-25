## ADR-004: Threshold Language Replacement Is Formatter-Side Post-Processing

### Context

AC-13 requires that no finding or claim string in markdown output contains the word "threshold"
paired with a numeric value, and that all numeric comparisons use baseline framing or ratio
framing. The claim strings that contain threshold values are produced by detection rules in
`unimatrix-observe/src/detection/` — those rules will not change in col-026.

Two approaches were considered for removing threshold language from rendered output:

**Option A**: Modify each detection rule's `claim` string template to omit the threshold value
at source. Instead, have each rule produce a claim like `"{N} compile cycles detected"` without
mentioning the threshold. Problems: (1) the `threshold` field on `HotspotFinding` is used
internally by tests and the synthesis pipeline; removing it from claims requires updating ~10
rules and their tests; (2) the claim strings are the authoritative description of the finding
for both the markdown formatter and the JSON path — changing them affects JSON consumers too,
constituting a broader API change than col-026's scope.

**Option B**: Leave detection rules unchanged. Add post-processing in the markdown formatter
that strips threshold references from claim strings before rendering, then appends baseline
framing when a matching `BaselineComparison` entry exists, or ratio framing otherwise. The
`threshold` field on `HotspotFinding` stays for internal use. The JSON path continues to emit
the original unmodified claims. Accepted.

Option B is consistent with the existing responsibility split: detection produces raw signals
with raw numeric thresholds; the formatter is responsible for user-facing presentation. ADR-003
from vnc-011 (#952) established that all rendering logic stays in `response/retrospective.rs`.

### Decision

Threshold language replacement is implemented in `render_findings()` in
`response/retrospective.rs` as a pure string transformation step applied to each
`CollapsedFinding.claims[0]` before it is written to the output.

Algorithm:
1. Given `claim_text: &str`, check if it contains a substring matching
   `r"threshold[:\s]+[\d.]+"` (case-insensitive).
2. If match found, strip the matched substring from the claim.
3. Look up `baseline_comparison` for the metric name (match by `metric_name == rule_name`):
   - If found AND `stddev > 0.0`: append ` (baseline: {mean:.1} ±{stddev:.1}, +{zscore:.1}σ)`.
     `zscore = (measured - mean) / stddev`.
   - Else: append ` ({ratio:.1}× typical)` where `ratio = measured / threshold`.
     Use `finding.threshold` (still on the struct) for this ratio. If threshold is 0.0,
     skip the ratio annotation entirely.
4. If no threshold pattern found: emit claim unchanged.

This function is a private `fn format_claim_with_baseline(claim, rule_name, measured, threshold, baseline_comparison) -> String` in `retrospective.rs`.

Scope: the 9 detection files enumerated in ARCHITECTURE.md §Component 5 are the complete audit
scope. No other files produce claim strings that appear in markdown output.

### Consequences

Easier:
- Detection rules and their tests are untouched.
- JSON consumers continue to receive the original threshold-containing claim strings.
- Formatter change is isolated to `render_findings()` — no cross-cutting change.
- All threshold language in rendered output is eliminated by a single function.

Harder:
- The formatter now has a dependency on `baseline_comparison` data within `render_findings`,
  which previously only used `hotspots` and `narratives`.
- Test coverage must verify that each of the 9 enumerated claim formats is correctly stripped
  and replaced (both with and without baseline data present).
- `format_claim_with_baseline` function must handle edge cases: `threshold == 0.0`,
  `stddev == 0.0`, absent baseline entry.
