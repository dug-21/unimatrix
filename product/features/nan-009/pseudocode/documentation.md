# Component: Documentation

File: `docs/testing/eval-harness.md`

## Purpose

Update the eval harness documentation to:
1. Document `context.phase` in the scenario format reference.
2. Document the phase vocabulary (snapshot, not allowlist), population requirement,
   null label `"(unset)"`, and governance model.
3. Document section 6 "Phase-Stratified Metrics" in the report reading guide.
4. Renumber the existing section 6 reference to section 7.
5. Update the section count reference in the "Reading the report" prose.

---

## Change 1: Scenario Output Format — Add `phase` to JSONL example

Current JSONL example (lines 141-157 in eval-harness.md):
```jsonc
{
  "id": "q-a1b2c3d4",
  "query": "what is the confidence scoring formula",
  "context": {
    "agent_id": "uni-rust-dev",
    "feature_cycle": "crt-022",
    "session_id": "sess-abc123",
    "retrieval_mode": "flexible"
  },
  "baseline": { ... },
  "source": "mcp",
  "expected": null
}
```

New JSONL example — add `phase` to the `context` object:
```jsonc
{
  "id": "q-a1b2c3d4",
  "query": "what is the confidence scoring formula",
  "context": {
    "agent_id": "uni-rust-dev",
    "feature_cycle": "crt-022",
    "session_id": "sess-abc123",
    "retrieval_mode": "flexible",
    "phase": "delivery"         // populated from query_log.phase; absent when null
  },
  "baseline": { ... },
  "source": "mcp",
  "expected": null
}
```

Add an explanatory paragraph after the JSONL example, under the existing "Note on schema"
comment:

```
**`context.phase`**: The workflow phase of the session that issued this query.
Populated from `query_log.phase`, which is set by `context_cycle` at session start.

Phase is present in the JSONL only when the source `query_log` row is non-null —
null-phase records omit the key entirely. Consumers should use
`#[serde(default)]` to tolerate absent keys.

**Known phase vocabulary (snapshot at nan-009, not a fixed allowlist):**
`"design"`, `"delivery"`, `"bugfix"`.

New session types produce new phase values in the per-phase report automatically,
without a harness code change. If historical rows need relabeling (e.g., a session
type was renamed), apply a `query_log` data migration:

    UPDATE query_log SET phase = 'new-name' WHERE phase = 'old-name';

**Phase population requirement:** Phase is populated only for MCP-sourced sessions
that called `context_cycle`. Rows inserted via the UDS transport, and all rows
predating col-028 (GH #403), have `phase = NULL`. An `eval scenarios` run against
a pre-col-028 snapshot or a UDS-only corpus will produce no `phase` keys in the
JSONL output, and `eval report` will omit section 6.
```

---

## Change 2: Result JSON Format — Add `phase` to example

Current result JSON example (lines 215-250 in eval-harness.md) does not show `phase`.

Add `phase` as a top-level field in the result JSON example:
```jsonc
{
  "scenario_id": "q-a1b2c3d4",
  "query": "what is the confidence scoring formula",
  "phase": "delivery",          // NEW — copied from context.phase; null when unset
  "profiles": {
    "baseline": { ... },
    "nli-candidate": { ... }
  },
  "comparison": { ... }
}
```

Add a brief note:
```
**`phase`**: The workflow phase of the originating session, copied from
`context.phase`. Always present as either `"delivery"` (or another phase string)
or `null` — the key is never absent from result JSON files produced by `eval run`.
```

---

## Change 3: "Reading the report" section — Update section count and add section 6

### Update prose header

Current (line ~359):
```
The report contains six sections in order:
```

New:
```
The report contains up to seven sections. Section 6 is present only when
at least one scenario result has a non-null phase value:
```

### Renumber existing section 6 to section 7

Current "### 6. Distribution Analysis" subsection heading — rename:
```
### 6. Distribution Analysis
```
to:
```
### 7. Distribution Analysis
```

Keep the body text of this subsection unchanged.

### Insert new section 6 before the (renumbered) section 7

Add the following subsection between the existing section 5 and the renamed section 7:

```markdown
### 6. Phase-Stratified Metrics

Aggregate metrics broken down by workflow phase. Each row represents all
scenarios that originated from sessions in that phase.

| Phase | Count | P@K | MRR | CC@k | ICD |
|-------|-------|-----|-----|------|-----|
| bugfix | 312 | 0.2813 | 0.3920 | 0.2244 | 0.4831 |
| delivery | 1,847 | 0.3102 | 0.4271 | 0.2688 | 0.5311 |
| design | 741 | 0.2944 | 0.4055 | 0.2501 | 0.4990 |
| (unset) | 407 | 0.3219 | 0.4418 | 0.2744 | 0.5102 |

_(The table above is a sample — your actual values will differ.)_

Phase values are sorted alphabetically. The `(unset)` bucket groups all
scenarios whose source `query_log` row had `phase = NULL` — this includes
UDS-sourced queries and all pre-col-028 sessions.

**This section is omitted entirely when no results have a non-null phase.**
If you run `eval report` against a corpus built entirely from UDS-only sessions
or pre-col-028 snapshots, section 6 will not appear and the numbering skips
directly from section 5 to section 7. This is expected behavior, not a bug.

Metrics in this section are computed from the **baseline profile only**.
Phase stratification answers "how does retrieval quality differ by workflow phase
on the baseline pipeline?" — not "which profile performs better per phase." A
per-phase × profile cross-product view is deferred to a future iteration.
```

---

## Change 4: Contents table of contents — Update section list

Current "Contents" list (lines 9-25) ends at `[Safety constraints]`. It does not enumerate
report sections. No change needed to the top-level Contents list.

The "Reading the report" anchor (`[Reading the report](#reading-the-report)`) remains valid.

---

## Change 5: "Full example walkthrough" — Update step 6 comment

Current (line ~681):
```bash
# 6. Review the report — focus on section 5 (Zero-Regression Check)
cat /tmp/eval/report.md
```

Update comment:
```bash
# 6. Review the report — focus on section 5 (Zero-Regression Check)
#    Section 6 (Phase-Stratified Metrics) is present when your corpus has phase data.
#    Section 7 (Distribution Analysis) shows CC@k and ICD distributions.
cat /tmp/eval/report.md
```

---

## Error Handling

Documentation has no error paths. If the section 6 is missing from a report, the
expected cause is documented: UDS-only corpus or pre-col-028 data. The reader is
directed to confirm via `query_log.phase` contents.

---

## Key Test Scenarios (Documentation Review, AC-07)

AC-07 specifies four items to verify by documentation review:

1. The `context.phase` field is documented in the scenario format reference, stating
   the field is populated from `query_log.phase`. — **Change 1 covers this.**

2. The known vocabulary (`design`, `delivery`, `bugfix`) is documented as a snapshot,
   not a fixed allowlist, and the migration-based governance model is described.
   — **Change 1 covers this.**

3. Section 6 "Phase-Stratified Metrics" is documented in the report output reference.
   — **Change 3 covers this.**

4. The note that phase requires MCP-sourced sessions with `context_cycle`, and that
   UDS-only/pre-col-028 corpora produce no phase section.
   — **Changes 1 and 3 both cover this.**

Documentation reviewer must confirm all four items are present after the update.

---

## Notes

- The existing text `"six sections"` appears in the "Reading the report" prose and in
  the full example walkthrough. Both sites are updated.
- The "Understanding the metrics" section (P@K, MRR, CC@k, ICD definitions) does not
  need updating — no new metrics are introduced, and phase is not a metric.
- The "Safety constraints" table at the end of the document does not need updating.
- The Platform baseline table (line ~479) showing current values does not need updating;
  it reflects overall corpus values, not per-phase values.
