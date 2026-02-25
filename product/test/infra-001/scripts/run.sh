#!/bin/bash
set -euo pipefail

# -- Configuration -----------------------------------------------------
TEST_SUITE="${TEST_SUITE:-all}"
TEST_WORKERS="${TEST_WORKERS:-1}"
PYTEST_ARGS="${PYTEST_ARGS:-}"
RESULTS_DIR="${RESULTS_DIR:-/results}"

# -- Suite Mapping -----------------------------------------------------
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

# -- Build Test Path Args ---------------------------------------------
TEST_PATHS=""
if [ "$TEST_SUITE" = "all" ]; then
    TEST_PATHS="suites/"
else
    IFS=',' read -ra SUITES <<< "$TEST_SUITE"
    for suite in "${SUITES[@]}"; do
        suite=$(echo "$suite" | xargs)
        if [ -n "${SUITE_MAP[$suite]+x}" ]; then
            TEST_PATHS="$TEST_PATHS ${SUITE_MAP[$suite]}"
        else
            echo "ERROR: Unknown suite '$suite'. Valid suites: ${!SUITE_MAP[*]}" >&2
            exit 1
        fi
    done
fi

# -- Run pytest --------------------------------------------------------
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

# -- Generate Report ---------------------------------------------------
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
