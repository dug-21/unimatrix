#!/bin/bash
# Parse pytest JSON report and generate human-readable summary.
# Usage: report.sh /results/report.json

set -euo pipefail

REPORT_FILE="${1:-/results/report.json}"

if [ ! -f "$REPORT_FILE" ]; then
    echo "No report file found at $REPORT_FILE"
    exit 0
fi

python3 << 'PYEOF'
import json
import sys

report_file = sys.argv[1] if len(sys.argv) > 1 else "/results/report.json"

try:
    with open(report_file) as f:
        report = json.load(f)
except (FileNotFoundError, json.JSONDecodeError) as e:
    print(f"Error reading report: {e}")
    sys.exit(0)

summary = report.get("summary", {})
tests = report.get("tests", [])

print("=== Test Summary ===")
print(f"Total:    {summary.get('total', 0)}")
print(f"Passed:   {summary.get('passed', 0)}")
print(f"Failed:   {summary.get('failed', 0)}")
print(f"Errors:   {summary.get('error', 0)}")
print(f"Skipped:  {summary.get('skipped', 0)}")
print(f"Duration: {summary.get('duration', 0):.1f}s")
print()

suites = {}
for test in tests:
    nodeid = test.get("nodeid", "")
    parts = nodeid.split("::")
    if parts:
        suite_file = parts[0].replace("suites/test_", "").replace(".py", "")
        if suite_file not in suites:
            suites[suite_file] = {"passed": 0, "failed": 0, "error": 0, "skipped": 0}
        outcome = test.get("outcome", "unknown")
        if outcome in suites[suite_file]:
            suites[suite_file][outcome] += 1

if suites:
    print("=== Per-Suite Breakdown ===")
    print(f"{'Suite':<20} {'Pass':>6} {'Fail':>6} {'Error':>6} {'Skip':>6}")
    print("-" * 50)
    for suite_name in sorted(suites.keys()):
        s = suites[suite_name]
        print(f"{suite_name:<20} {s['passed']:>6} {s['failed']:>6} {s['error']:>6} {s['skipped']:>6}")
    print()

failures = [t for t in tests if t.get("outcome") == "failed"]
if failures:
    print("=== Failures ===")
    for t in failures:
        print(f"  FAIL: {t.get('nodeid', 'unknown')}")
        call_info = t.get("call", {})
        longrepr = call_info.get("longrepr", "")
        if longrepr:
            lines = str(longrepr).strip().split("\n")
            for line in lines[-3:]:
                print(f"    {line}")
    print()
PYEOF
