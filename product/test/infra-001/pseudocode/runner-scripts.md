# Pseudocode: C8 — Runner Scripts

## File: `scripts/run.sh`

```bash
#!/bin/bash
set -euo pipefail

# ── Configuration ────────────────────────────────────────
TEST_SUITE="${TEST_SUITE:-all}"
TEST_WORKERS="${TEST_WORKERS:-1}"
PYTEST_ARGS="${PYTEST_ARGS:-}"
RESULTS_DIR="/results"

# ── Suite Mapping ────────────────────────────────────────
# Map suite names to pytest paths
declare -A SUITE_MAP=(
  [protocol]="suites/test_protocol.py"
  [tools]="suites/test_tools.py"
  [lifecycle]="suites/test_lifecycle.py"
  [volume]="suites/test_volume.py"
  [security]="suites/test_security.py"
  [confidence]="suites/test_confidence.py"
  [contradiction]="suites/test_contradiction.py"
  [edge_cases]="suites/test_edge_cases.py"
)

# ── Build Test Path Args ─────────────────────────────────
TEST_PATHS=""
if [ "$TEST_SUITE" = "all" ]; then
    TEST_PATHS="suites/"
else
    # Support comma-separated suite names
    IFS=',' read -ra SUITES <<< "$TEST_SUITE"
    for suite in "${SUITES[@]}"; do
        suite=$(echo "$suite" | xargs)  # trim whitespace
        if [ -n "${SUITE_MAP[$suite]+x}" ]; then
            TEST_PATHS="$TEST_PATHS ${SUITE_MAP[$suite]}"
        else
            echo "ERROR: Unknown suite '$suite'. Valid: ${!SUITE_MAP[*]}" >&2
            exit 1
        fi
    done
fi

# ── Run pytest ───────────────────────────────────────────
echo "=== Unimatrix Integration Tests ==="
echo "Suite: $TEST_SUITE"
echo "Workers: $TEST_WORKERS"
echo "Paths: $TEST_PATHS"
echo "=================================="

pytest_exit=0
python -m pytest \
    $TEST_PATHS \
    --junitxml="${RESULTS_DIR}/junit.xml" \
    --json-report --json-report-file="${RESULTS_DIR}/report.json" \
    --timeout=60 \
    -v \
    $PYTEST_ARGS \
    || pytest_exit=$?

# ── Generate Report ──────────────────────────────────────
if [ -f "${RESULTS_DIR}/report.json" ]; then
    bash scripts/report.sh "${RESULTS_DIR}/report.json" > "${RESULTS_DIR}/summary.txt" 2>&1 || true
fi

echo ""
echo "=== Results ==="
echo "Exit code: $pytest_exit"
if [ -f "${RESULTS_DIR}/summary.txt" ]; then
    cat "${RESULTS_DIR}/summary.txt"
fi
echo ""
echo "JUnit XML: ${RESULTS_DIR}/junit.xml"
echo "JSON Report: ${RESULTS_DIR}/report.json"
echo "==============="

exit $pytest_exit
```

## File: `scripts/report.sh`

```bash
#!/bin/bash
# Parse pytest JSON report and generate human-readable summary.
# Usage: report.sh /results/report.json

set -euo pipefail

REPORT_FILE="${1:-/results/report.json}"

if [ ! -f "$REPORT_FILE" ]; then
    echo "No report file found at $REPORT_FILE"
    exit 0
fi

# Use Python to parse JSON (available in test-runtime image)
python3 -c "
import json
import sys

with open('$REPORT_FILE') as f:
    report = json.load(f)

summary = report.get('summary', {})
tests = report.get('tests', [])

# Overall summary
print('=== Test Summary ===')
print(f\"Total:    {summary.get('total', 0)}\")
print(f\"Passed:   {summary.get('passed', 0)}\")
print(f\"Failed:   {summary.get('failed', 0)}\")
print(f\"Errors:   {summary.get('error', 0)}\")
print(f\"Skipped:  {summary.get('skipped', 0)}\")
print(f\"Duration: {summary.get('duration', 0):.1f}s\")
print()

# Per-suite breakdown
suites = {}
for test in tests:
    # Extract suite name from nodeid: suites/test_protocol.py::test_name
    nodeid = test.get('nodeid', '')
    parts = nodeid.split('::')
    if parts:
        suite_file = parts[0].replace('suites/test_', '').replace('.py', '')
        if suite_file not in suites:
            suites[suite_file] = {'passed': 0, 'failed': 0, 'error': 0, 'skipped': 0}
        outcome = test.get('outcome', 'unknown')
        if outcome in suites[suite_file]:
            suites[suite_file][outcome] += 1

if suites:
    print('=== Per-Suite Breakdown ===')
    print(f\"{'Suite':<20} {'Pass':>6} {'Fail':>6} {'Error':>6} {'Skip':>6}\")
    print('-' * 50)
    for suite_name in sorted(suites.keys()):
        s = suites[suite_name]
        print(f\"{suite_name:<20} {s['passed']:>6} {s['failed']:>6} {s['error']:>6} {s['skipped']:>6}\")
    print()

# List failures
failures = [t for t in tests if t.get('outcome') == 'failed']
if failures:
    print('=== Failures ===')
    for t in failures:
        print(f\"  FAIL: {t.get('nodeid', 'unknown')}\")
        longrepr = t.get('call', {}).get('longrepr', '')
        if longrepr:
            # Print last 3 lines of failure detail
            lines = str(longrepr).strip().split('\n')
            for line in lines[-3:]:
                print(f\"    {line}\")
    print()
"
```

## File: `pytest.ini`

```ini
[pytest]
testpaths = suites
python_files = test_*.py
python_classes = Test*
python_functions = test_*

# Markers
markers =
    smoke: Critical-path tests for quick validation (~15 tests, <60s)
    slow: Tests that take more than 10 seconds
    volume: Scale and stress tests
    security: Security validation tests

# Default timeout per test (seconds)
timeout = 60

# Verbose output by default
addopts = -v

# Log settings
log_cli = false
log_cli_level = WARNING
log_file_level = DEBUG
```

## Key Design Decisions

- run.sh maps TEST_SUITE names to pytest paths; validates suite names
- Supports comma-separated suites: `TEST_SUITE=protocol,tools`
- pytest-json-report generates structured JSON for report.sh to parse
- JUnit XML for CI integration
- report.sh uses embedded Python for JSON parsing (no jq dependency)
- Exit code propagated from pytest through run.sh to Docker
- Per-test timeout of 60s (pytest-timeout) prevents infinite hangs
- Markers enable selective execution: `pytest -m smoke` for quick validation
