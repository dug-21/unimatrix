# Pseudocode: C4 — USAGE-PROTOCOL.md Update

## File
`product/test/infra-001/USAGE-PROTOCOL.md`

## Changes Required

### 1. Update "When to Run" table

Current table (in "When to Run" section) covers:
- During Feature Development (Session 2)
- During Bug Fix Sessions
- Pre-Release / Pre-Merge

Add a new subsection after "Pre-Release / Pre-Merge":

```
### Pre-Release Gate

Before tagging any release, run the availability suite:

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m availability --timeout=150
```

**Expected outcome:** All non-xfail tests pass. Known-failing tests (marked `@pytest.mark.xfail`) are expected to fail and will show as `XFAIL` — this is not a blocking failure.

**Run time:** ~15-20 minutes.

**When xfails become passes:** When a bug fix (e.g., #275, #277) is merged, the corresponding `xfail` marker should be removed. At that point the test becomes a hard pass/fail.
```

### 2. Update the "When to Run" summary table

Add a row to the summary table showing the three tiers:

```
| Tier | When | Command | Time |
|------|------|---------|------|
| Smoke | Per-feature gate (Stage 3c), per-PR minimum | `pytest -m smoke` | <60s |
| Full suite | Pre-merge (all PRs touching server code) | `pytest suites/` | ~20 min |
| Availability | Pre-release only | `pytest -m availability` | ~15-20 min |
```

### 3. Update Suite Reference section

Add `availability` suite entry after existing suite descriptions:

```
### Availability Tests (`-m availability`)

~5 runnable tests (+ 1 deferred stub) covering time-extended reliability:
- Tick liveness (server responds after tick fires)
- Cold-start request race (no crash on immediate requests)
- Concurrent ops during tick (mutex pressure, currently xfail GH#277)
- Read ops not blocked by tick (currently xfail GH#277)
- Sustained multi-tick (3 cycles, currently xfail GH#275)

**Run time:** ~15-20 minutes. Pre-release gate only.
**Note:** xfail tests document known bugs. As bugs are fixed, remove corresponding markers.
```
