# Gate Bug Fix Validation Report: GH #501 / #502

> Gate: Bug Fix Validation
> Date: 2026-04-03
> Feature: crt-043 (eval harness bugs)
> Issues: GH #501 (cross-snapshot MRR invalidity), GH #502 (scenario ID collision)
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Fix addresses root cause (#502 — ID collision) | PASS | Query content hash applied at both search (line 118) and briefing (line 173) ID construction |
| Fix addresses root cause (#501 — snapshot mismatch) | PASS | Three-way sidecar check implemented: absent→WARNING, match→silent, mismatch→exit(1) with override flag |
| No TODO/FIXME/unimplemented in changed Python files | PASS | grep confirmed clean |
| All 8 new tests pass | PASS | Confirmed by tester agent report |
| No new clippy warnings | PASS | Confirmed by tester agent (1 pre-existing warning in unrelated auth.rs) |
| No unsafe code introduced | PASS | Python-only fix |
| Fix is minimal — no unrelated changes | PASS | Both Python files contain only the targeted fixes |
| New tests would have caught original bug | PASS | `test_no_id_collision_same_session_same_ms` constructs exact collision scenario |
| scenarios.jsonl uniqueness | PASS | 1761 IDs, 1761 unique, 0 collisions (independently verified) |
| scenarios_meta.json present with required fields | PASS | source_db_hash, generated_at, scenario_count all present |
| BASELINE_MRR updated to re-measured value | PASS | Updated from 0.2875 to 0.2651 (conf-boost-c, 1761 scenarios, 2026-04-03) |
| log.jsonl new entry with snapshot_hash + scenarios_date | PASS | Entry present with snapshot_hash:"138f898d382f" and scenarios_date:"2026-04-03T15:40:59Z" |
| docs/testing/eval-harness.md documents paired-snapshot requirement | PASS | Explicit "Paired-snapshot requirement" section added to Step 2 |
| No xfail markers needed | PASS | Python-only fix; 2 pre-existing Rust failures unrelated and pre-existing |
| Knowledge stewardship — tester agent | PASS | 501-502-agent-2-verify-report.md has Queried + Stored entries |
| Knowledge stewardship — investigator agent report | WARN | No 501-502-agent-1-investigator-report.md found in agents/ directory |

## Detailed Findings

### Fix addresses root cause (#502 — Scenario ID Collision)

**Status**: PASS

**Evidence**: `build_scenarios.py` line 117-118:
```python
query_hash = hashlib.md5(query.encode()).hexdigest()[:6]
scenario_id = f"obs-{sid[:8]}-{ts}-{query_hash}"
```
And line 172-173 (briefing path):
```python
query_hash = hashlib.md5(query_str.encode()).hexdigest()[:6]
scenario_id = f"obs-briefing-{sid[:8]}-{ts}-{query_hash}"
```
Both ID construction paths now incorporate a 6-character MD5 hash of the query content. The old format `obs-{sid[:8]}-{ts}` would produce identical IDs for two different queries fired within the same session at the same millisecond. The new format makes the ID unique per (session, timestamp, query). The duplicate detection assertion at line 193 is also present as a hard guard.

### Fix addresses root cause (#501 — Cross-Snapshot MRR Invalidity)

**Status**: PASS

**Evidence**: `run_eval.py` `check_snapshot_pairing()` (lines 82–136) implements the full three-way behavior:
- Absent sidecar: prints `WARNING: ... backward compat mode` and returns (no exit)
- Hash match: returns silently (line 113)
- Hash mismatch + `allow_mismatch=False`: prints ERROR with explanation and calls `sys.exit(1)` (line 136)
- Hash mismatch + `allow_mismatch=True`: prints WARNING but does not exit
The function is called after taking the snapshot (line 259), before running eval. The `--allow-snapshot-mismatch` flag is wired via argparse (lines 216-219) and threaded through as `args.allow_snapshot_mismatch`.

### No TODO/FIXME/placeholder in changed Python files

**Status**: PASS

**Evidence**: Grep of `build_scenarios.py` and `run_eval.py` for TODO, FIXME, unimplemented, todo! returned no matches. Comments present are informational (e.g., `# Short hash for disambiguation only — not cryptographic`).

### New tests catch the original bug

**Status**: PASS

**Evidence**: `test_no_id_collision_same_session_same_ms` in `test_build_scenarios.py` (lines 85-109) constructs exactly the collision scenario from the bug report: same session, same timestamp (ts = 1712345678000), two different queries inserted. Asserts `len(ids) == len(set(ids))` — this test would have failed against the pre-fix code where both scenarios would receive ID `obs-aabbccdd-1712345678000`.

`test_uniqueness_assertion_fires_on_collision` additionally patches `hashlib.md5` to produce a constant digest, verifying the assertion at line 193 triggers when a collision would occur.

### scenarios.jsonl uniqueness

**Status**: PASS

**Evidence**: Independent Python verification confirms 1761 lines, 1761 unique IDs, 0 collisions.

### scenarios_meta.json sidecar

**Status**: PASS

**Evidence**: `/workspaces/unimatrix/product/research/ass-039/harness/scenarios_meta.json` contains all three required fields:
- `source_db_hash`: 64-char SHA-256 hex string (97bd647c...)
- `generated_at`: ISO 8601 UTC timestamp (2026-04-03T15:40:59Z)
- `scenario_count`: 1761 (matches scenarios.jsonl line count)

### BASELINE_MRR re-measured

**Status**: PASS

**Evidence**: `run_eval.py` line 45 shows `BASELINE_MRR = 0.2651`. The docstring header (lines 19, 181) explicitly documents the re-measurement context: "conf-boost-c, 1761 scenarios, 2026-04-03, GH #501/#502". The prior value was 0.2875 (recorded in the spawn prompt as the old baseline). The new value reflects the full 1761 scenario set, not the collision-truncated 1443-equivalent set. The log.jsonl entry notes "post-#502 fix: 1761 unique scenarios (was 1443, 142+ collisions resolved)".

### log.jsonl new entry

**Status**: PASS

**Evidence**: `product/test/eval-baselines/log.jsonl` line 9 contains:
```json
{"date":"2026-04-03","scenarios":1761,...,"feature_cycle":"bugfix-501-502","snapshot_hash":"138f898d382f","scenarios_date":"2026-04-03T15:40:59Z","note":"post-#502 fix..."}
```
Both `snapshot_hash` and `scenarios_date` fields are present, consistent with the new README field spec.

### docs/testing/eval-harness.md paired-snapshot documentation

**Status**: PASS

**Evidence**: The "Paired-snapshot requirement" paragraph was added under Step 2 (lines 138-146 of the doc) and explicitly calls out: the sidecar hash, the exit-non-zero behavior on mismatch, and the root cause of the spurious GH #500 comparison. The full-example walkthrough at the bottom also updated the Step 7 echo template to include `snapshot_hash` and `scenarios_date` fields (line 848-849).

### Knowledge Stewardship — Tester Agent

**Status**: PASS

**Evidence**: `product/features/crt-043/agents/501-502-agent-2-verify-report.md` contains `## Knowledge Stewardship` block with:
- `Queried:` entry referencing `context_briefing` and entries #4084, #4085, #4086
- `Stored:` entry with explicit reason ("patterns already captured during implementation phase")

### Knowledge Stewardship — Investigator/Implementer Agent Report

**Status**: WARN

**Evidence**: Only one bugfix agent report is present (`501-502-agent-2-verify-report.md`). No `501-502-agent-1-*` (investigator) or implementer report was found in `product/features/crt-043/agents/`. The spawn prompt specifies a tester and presumably an investigator + implementer. The verify agent's stewardship block references entries #4084–#4086 as "already stored during implementation phase" — confirming an implementer ran — but their report file is absent from the agents directory.

This is a WARN (not FAIL) because: the knowledge was demonstrably stored (referenced by ID from the verify agent), and the implementation correctness can be validated directly from the code. The missing file is a tracking gap, not a quality gap.

## Rework Required

None. All checks PASS or WARN. No FAIL findings.

## Warnings Summary

1. **Missing investigator/implementer agent report** — The `501-502-agent-1` report file is absent. The verify agent's stewardship block confirms knowledge was stored (#4084–#4086), so this is a recordkeeping gap only. If the bugfix protocol requires all agent reports to be filed, the implementer agent should retroactively file their report.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` — searched for "eval harness scenario id collision" and "snapshot pairing validation" before analysis.
- Stored: nothing novel to store -- this is a straightforward bugfix validation with no recurring gate failure patterns. The underlying lessons (#4084 scenario ID collision, #4085 snapshot mismatch) were already stored during the implementation phase of this bugfix.
