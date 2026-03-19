# Integration Test Harness — Usage Protocol

## Purpose

This document defines how the infra-001 integration test harness is used during feature development, bug fixes, and release validation. It establishes when tests are run, how failures are triaged, and the boundary between "fix it now" and "file a GH Issue."

---

## What the Harness Does

The harness exercises the compiled `unimatrix-server` binary through the MCP JSON-RPC protocol over stdio — the exact interface agents use. It validates system-level behavior that unit tests cannot: protocol compliance, multi-step lifecycle flows, security defenses, confidence math, contradiction detection, scale behavior, and edge cases.

**205 tests across 9 suites:**

| Suite | Tests | Focus |
|-------|-------|-------|
| `protocol` | 13 | MCP handshake, JSON-RPC compliance, tool discovery, graceful shutdown |
| `tools` | 73 | All 12 tools — every parameter, valid/invalid inputs, all response formats |
| `lifecycle` | 25 | Multi-step flows: store→search, correction chains, confidence evolution, restart persistence |
| `volume` | 11 | Scale to hundreds of entries, large payloads, contradiction scan at scale |
| `security` | 17 | Content scanning, capability enforcement, input validation boundaries |
| `confidence` | 14 | 6-factor composite formula, Wilson score, re-ranking, base scores per status |
| `contradiction` | 12 | Negation detection, incompatible directives, false positive resistance |
| `edge_cases` | 24 | Unicode, boundary values, empty DB operations, concurrent ops |
| `adaptation` | 10 | Category allowlist, boosted categories, format negotiation |

---

## Running the Harness

### Prerequisites

**Local (no Docker):**
- Python 3.12+
- `pip install pytest pytest-timeout pytest-json-report`
- Built binary at `target/release/unimatrix-server` (or set `UNIMATRIX_BINARY`)
- ONNX Runtime shared library available (`ORT_DYLIB_PATH` or in `LD_LIBRARY_PATH`)

**Docker:**
- Docker + Docker Compose
- No other dependencies — everything is in the image

### Commands

```bash
# -- Docker (recommended) --

# Run all suites
docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Run a specific suite
TEST_SUITE=security docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Run multiple suites
TEST_SUITE=protocol,tools,security docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Smoke tests only (~30s)
PYTEST_ARGS="-m smoke" docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit

# Teardown
docker compose -f product/test/infra-001/docker-compose.yml down -v


# -- Local (no Docker) --

# Build the binary first
cargo build --release

# Run from the harness directory
cd product/test/infra-001

# All suites
python -m pytest suites/ -v --timeout=60

# Single suite
python -m pytest suites/test_security.py -v

# Smoke only
python -m pytest suites/ -v -m smoke

# Specific test
python -m pytest suites/test_tools.py::test_store_roundtrip -v
```

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `TEST_SUITE` | `all` | Suite selection: `all`, `protocol`, `tools`, `lifecycle`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`, or comma-separated |
| `TEST_WORKERS` | `1` | Parallel workers (use 1 — tests are not designed for parallel execution) |
| `PYTEST_ARGS` | _(empty)_ | Extra pytest arguments (e.g., `-m smoke`, `-k test_store`, `--tb=short`) |
| `UNIMATRIX_BINARY` | _(auto-detected)_ | Path to `unimatrix-server` binary |
| `RUST_LOG` | `info` | Server log level (visible in captured stderr) |
| `RESULTS_DIR` | `/results` | Output directory for reports (Docker only) |

### Output

```
/results/
├── junit.xml          # CI integration (JUnit XML)
├── report.json        # Detailed pytest JSON report
├── summary.txt        # Human-readable table
└── logs/              # Server stderr per suite
```

---

## When to Run

### During Feature Development (Session 2)

**Stage 3c (Testing & Risk Validation):**

After code implementation (Stage 3b) and before Gate 3c, run the integration harness alongside `cargo test`. The `uni-tester` agent (execution mode) should:

1. Run `cargo test --lib` (unit tests — existing gate requirement)
2. Run the integration harness: `cd product/test/infra-001 && python -m pytest suites/ -v --timeout=60`
3. Include integration test results in the RISK-COVERAGE-REPORT.md

**Which suites to run depends on the feature:**

| Feature touches... | Run these suites |
|--------------------|------------------|
| Any server tool logic | `tools`, `protocol` |
| Store/retrieval behavior | `tools`, `lifecycle`, `edge_cases` |
| Confidence system | `confidence`, `lifecycle` |
| Contradiction detection | `contradiction` |
| Security (scanning, caps, validation) | `security` |
| Schema or storage changes | `lifecycle` (restart persistence), `volume` |
| Any change at all | `smoke` (minimum gate) |

**Minimum gate requirement:** `pytest -m smoke` must pass before Gate 3c. Full suite run is recommended but not blocking if specific suites are irrelevant to the feature.

### During Bug Fix Sessions

**Phase 3 (Verification):**

After the fix is implemented, the `uni-tester` agent should:

1. Run `cargo test --lib` (unit tests)
2. Run the relevant integration suite(s) based on what the bug affected
3. Run `pytest -m smoke` as a regression baseline

### Pre-Release / Pre-Merge

Before merging any PR that touches server code, run the full suite:

```bash
docker compose -f product/test/infra-001/docker-compose.yml up --build --abort-on-container-exit
```

All 205 tests must pass.

### Pre-Release Gate

Before tagging any release, run the availability suite:

```bash
cd product/test/infra-001
python -m pytest suites/ -v -m availability --timeout=150
```

**Expected outcome:** All non-xfail tests pass. Known-failing tests (marked
`@pytest.mark.xfail`) show as `XFAIL` — this is expected and not a blocking
failure. Tests marked `@pytest.mark.skip` show as `SKIPPED`.

**Run time:** ~15-20 minutes.

**When xfails become passes:** When a bug fix (e.g., GH#275, GH#277) is merged,
remove the corresponding `xfail` marker. The test then becomes a hard pass/fail
gate for future releases.

---

## When to Run — Summary

| Tier | When | Command | Expected Time |
|------|------|---------|---------------|
| Smoke | Per-feature gate (Stage 3c), minimum per-PR check | `pytest -m smoke` | <60s |
| Full suite | Pre-merge for any PR touching server code | `pytest suites/` | ~20 min |
| Availability | Pre-release only | `pytest -m availability` | ~15-20 min |

---

## Failure Triage Protocol

When an integration test fails, the critical question is: **is this failure caused by the feature under development, or does it reveal a pre-existing issue?**

### Decision Tree

```
Integration test fails
  │
  ├─ Is the failure in code YOU changed in this feature?
  │   │
  │   YES → Fix it now. This is your bug.
  │   │     - Fix the code, re-run the test, continue.
  │   │     - Document the fix in the gate report.
  │   │
  │   NO → Is this a pre-existing issue exposed by the test?
  │         │
  │         YES → File a GH Issue. Do NOT fix it in this PR.
  │         │     - Label: bug, {phase prefix}
  │         │     - Title: "[infra-001] {test name}: {brief description}"
  │         │     - Body: test name, expected vs actual, server logs
  │         │     - Mark the test with @pytest.mark.xfail(reason="GH#NNN")
  │         │     - Continue with the feature — the test is now expected to fail.
  │         │
  │         NO → Is the test itself wrong? (Bad assertion, wrong expectation)
  │               │
  │               YES → Fix the test in this PR.
  │               │     - The harness is code; test bugs are legitimate fixes.
  │               │     - Document in the gate report: "Test X had incorrect expectation."
  │               │
  │               NO → Investigate further. Ask the human if unclear.
```

### Why This Matters

Fixing unrelated issues inside a feature PR creates several problems:

1. **Scope creep** — The PR grows beyond its intended change set, making review harder.
2. **Blame diffusion** — If the "fix" introduces a regression, it's tangled with the feature work.
3. **No audit trail** — The issue was never tracked, so there's no record it existed or how it was resolved.
4. **Skipped risk assessment** — Pre-existing bugs deserve their own SCOPE, risk analysis, and targeted testing through the proper lifecycle (bugfix protocol).

Filing a GH Issue and marking the test `xfail` preserves the signal ("we know this is broken") while keeping the feature PR clean.

### GH Issue Template for Test-Discovered Bugs

```bash
gh issue create \
  --title "[infra-001] test_<name>: <brief description>" \
  --label "bug" \
  --body "$(cat <<'EOF'
## Discovered By

Integration test: `suites/test_<suite>.py::test_<name>`

## Expected Behavior

<what the test expected>

## Actual Behavior

<what actually happened>

## Server Logs

```
<relevant stderr from the test run>
```

## Reproduction

```bash
cd product/test/infra-001
python -m pytest suites/test_<suite>.py::test_<name> -v
```

## Notes

Discovered during infra-001 integration testing. Not caused by the current feature under development.
EOF
)"
```

### Marking Tests as Expected Failures

When a GH Issue is filed for a pre-existing bug, mark the test so it doesn't block other work:

```python
@pytest.mark.xfail(reason="Pre-existing: GH#42 — deprecated entries not excluded from search")
def test_deprecated_excluded_from_search(server):
    ...
```

When the bug is fixed in a later PR, remove the `xfail` marker. If the test starts passing before the fix (e.g., incidental fix), pytest reports it as `XPASS` — a signal to remove the marker and close the issue.

---

## Agent Responsibilities

### uni-tester (Stage 3c / Phase 3)

1. Run `cargo test --lib` — existing requirement, unchanged.
2. Run integration harness: select suites based on what the feature touches (see table above).
3. If a test fails:
   - Determine if it's caused by the feature's changes → fix.
   - Determine if it's pre-existing → file GH Issue, mark `xfail`, continue.
   - Determine if the test is wrong → fix the test.
4. Report results in RISK-COVERAGE-REPORT.md, including integration test counts.

### uni-validator (Gate 3c)

1. Verify `pytest -m smoke` passed (minimum gate).
2. Verify any `xfail` markers have corresponding GH Issues.
3. Verify no tests were deleted or commented out to make the suite pass.
4. Include integration test results in the gate report.

### uni-rust-dev (Stage 3b)

1. **Do not modify integration tests** unless the IMPLEMENTATION-BRIEF explicitly assigns a harness component.
2. If your code change causes an integration test failure discovered during Stage 3c, the `uni-tester` will report it for rework.

### uni-bugfix-manager (Bug Fix Session)

1. When triaging a bug discovered by the integration harness, reference the specific test and GH Issue.
2. After the fix, verify the specific test passes and remove the `xfail` marker.
3. Run `pytest -m smoke` as regression baseline.

### Human

1. Review `xfail` markers periodically — they represent known debt.
2. When a GH Issue for a test-discovered bug is closed, verify the `xfail` marker was removed.
3. Before releases, run the full suite without `xfail` to see the true state.

---

## Adding New Tests

When a new feature is developed, the integration harness may need new tests. This follows the normal feature lifecycle:

1. **If the new test validates existing SCOPE acceptance criteria** — add it to the appropriate suite in the feature's PR. No separate issue needed.
2. **If the new test covers a gap discovered during development** — add it to the suite and note it in the RISK-COVERAGE-REPORT.md as "coverage expansion."
3. **If the harness needs a new suite or significant infrastructure changes** — file a GH Issue for a follow-up infra enhancement. Don't overload the feature PR.

### Test Naming Convention

```python
# Pattern: test_{tool_or_concept}_{specific_behavior}
def test_store_roundtrip(server): ...
def test_search_excludes_quarantined(server): ...
def test_confidence_wilson_min_votes(server): ...
def test_unicode_emoji_content(server): ...
```

### Fixture Selection

| Fixture | Scope | Use when... |
|---------|-------|-------------|
| `server` | function | Default. Fresh DB, no state leakage. Most tests. |
| `shared_server` | module | State accumulates. Volume/lifecycle suites. |
| `populated_server` | function | Need 50 pre-loaded entries. Search/briefing tests. |
| `admin_server` | function | Need admin-level operations (quarantine). |

---

## Suite Reference

### Smoke Tests (`-m smoke`)

~15 tests covering one critical path per major capability:
- Store + get roundtrip
- Search finds stored entry
- Correct creates chain
- Quarantine excludes from search
- Content scanning catches injection
- Capability enforcement blocks unauthorized writes
- Confidence in valid range
- Status report works
- Briefing returns content
- Restart preserves data

**Run time:** <60 seconds. Use as minimum gate.

### Full Suite

All 205 tests. **Run time:** ~20 minutes (varies with hardware, especially embedding model initialization and volume tests).

### Selective Suites

Use `TEST_SUITE=<name>` or `python -m pytest suites/test_<name>.py` to run individual suites. Useful during development when you know exactly what's affected.

### Availability Tests (`-m availability`)

5 runnable tests (+ 1 deferred stub) covering time-extended reliability:

| Test | What It Catches | Expected Result |
|------|----------------|-----------------|
| `test_tick_liveness` | Tick fires; server responds to MCP after tick | PASS |
| `test_cold_start_request_race` | No crash on immediate requests before warmup | PASS |
| `test_concurrent_ops_during_tick` | Mutex pressure — requests don't hang during tick | PASS |
| `test_read_ops_not_blocked_by_tick` | Read ops complete within deadline during tick | PASS |
| `test_sustained_multi_tick` | Server survives 3+ tick cycles (~100s) | PASS |
| `test_tick_panic_recovery` | Tick supervisor restarts after panic | SKIP (GH#276) |

**Run time:** ~15-20 minutes. Pre-release gate only — not per-feature or per-PR.

**Note on xfail:** These tests document known bugs. As fixes land, remove the
corresponding `@pytest.mark.xfail` decorator. If a test starts passing before
its fix is explicitly applied (`XPASS`), remove the marker and close the issue.
