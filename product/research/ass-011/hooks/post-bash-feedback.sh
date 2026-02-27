#!/usr/bin/env bash
# ASS-011 Phase 1 Test: PostToolUse feedback injection (RQ-1d)
# Injects additionalContext after Bash calls to test real-time feedback delivery.
# Only injects for Bash calls containing "cargo test" to avoid noise.
set -euo pipefail

START_NS=$(date +%s%N)
INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')

# Only fire for Bash tool
[ "$TOOL_NAME" = "Bash" ] || exit 0

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // ""')
RESPONSE=$(echo "$INPUT" | jq -r '.tool_response // "" | tostring')

# Only inject feedback for cargo test commands
case "$COMMAND" in
  *"cargo test"*|*"cargo nextest"*)
    ;;
  *)
    # For non-test Bash calls, just log timing without injection
    END_NS=$(date +%s%N)
    ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))
    LOG_DIR="${HOME}/.unimatrix/observation/test"
    mkdir -p "$LOG_DIR"
    echo "{\"hook\":\"PostToolUse-Bash\",\"command\":\"${COMMAND:0:80}\",\"injected\":false,\"session_id\":\"${SESSION_ID}\",\"latency_ms\":${ELAPSED_MS},\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >> "${LOG_DIR}/hook-latency.jsonl"
    exit 0
    ;;
esac

# Extract test result signals from cargo test output
PASS_COUNT=$(echo "$RESPONSE" | grep -oP 'test result: ok\. (\d+) passed' | grep -oP '\d+' || echo "0")
FAIL_COUNT=$(echo "$RESPONSE" | grep -oP '(\d+) failed' | grep -oP '\d+' || echo "0")

FEEDBACK="[ASS011-POST-FEEDBACK] Test execution observed: ${PASS_COUNT} passed, ${FAIL_COUNT} failed. This feedback was injected by a PostToolUse hook — verifying that additionalContext delivery works for real-time course correction."

OUTPUT=$(jq -n --arg ctx "$FEEDBACK" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    additionalContext: $ctx
  }
}')

echo "$OUTPUT"

# Log timing
END_NS=$(date +%s%N)
ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))

LOG_DIR="${HOME}/.unimatrix/observation/test"
mkdir -p "$LOG_DIR"
echo "{\"hook\":\"PostToolUse-Bash\",\"command\":\"${COMMAND:0:80}\",\"injected\":true,\"pass\":${PASS_COUNT},\"fail\":${FAIL_COUNT},\"session_id\":\"${SESSION_ID}\",\"latency_ms\":${ELAPSED_MS},\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >> "${LOG_DIR}/hook-latency.jsonl"

exit 0
