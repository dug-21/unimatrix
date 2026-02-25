# Test Plan: C8 — Runner Scripts

## Scope

Runner scripts (run.sh, report.sh, pytest.ini) are validated by running the test suite through them.

## Validation Points

| AC | Test | Method |
|----|------|--------|
| AC-03 | Suite selection | Set TEST_SUITE=protocol, verify only protocol tests run |
| AC-14 | Smoke tests <60s | `pytest -m smoke` completes in <60s |
| AC-15 | Output files exist | JUnit XML, JSON report, summary after run |

## run.sh Tests

| Test | How Validated |
|------|--------------|
| Default runs all suites | `TEST_SUITE=all` or unset -> all suites/ tests run |
| Single suite selection | `TEST_SUITE=protocol` -> only test_protocol.py runs |
| Multiple suite selection | `TEST_SUITE=protocol,tools` -> both run |
| Invalid suite name errors | `TEST_SUITE=nonexistent` -> exit 1 with error message |
| PYTEST_ARGS pass-through | `PYTEST_ARGS="-m smoke"` -> only smoke tests run |
| Exit code propagated | pytest failure -> run.sh exits non-zero -> container exits non-zero |

## report.sh Tests

| Test | How Validated |
|------|--------------|
| Parses JSON report | summary.txt generated from report.json |
| Per-suite breakdown | Suite names and pass/fail counts in summary |
| Failure listing | Failed test names listed with details |

## pytest.ini Tests

| Test | How Validated |
|------|--------------|
| Test discovery | pytest finds all test_*.py in suites/ |
| Markers registered | `pytest -m smoke` doesn't warn about unknown marker |
| Timeout enforced | Long-running test killed after 60s |

## Risk Coverage

| Risk | Script Responsibility | Validation |
|------|---------------------|------------|
| R-07 | Default timeout in pytest.ini | Tests that would hang are killed |
| AC-15 | Report generation | Files exist after run |
