#!/usr/bin/env bash
# ASS-011 Phase 1 Test: SubagentStart context injection (RQ-1a)
# Injects a known marker string via additionalContext.
# The marker can be checked in subagent output to verify receipt.
set -euo pipefail

START_NS=$(date +%s%N)
INPUT=$(cat)

AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // "unknown"')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')

# Build injection payload
MARKER="[ASS011-SUBAGENT-MARKER-$(date +%s)]"
OUTPUT=$(jq -n --arg ctx "INJECTED_CONTEXT: You have been assigned identity marker ${MARKER}. Your agent_type is ${AGENT_TYPE}. This context was injected by a SubagentStart hook — not part of your original prompt." '{
  hookSpecificOutput: {
    hookEventName: "SubagentStart",
    additionalContext: $ctx
  }
}')

echo "$OUTPUT"

# Log timing
END_NS=$(date +%s%N)
ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))

LOG_DIR="${HOME}/.unimatrix/observation/test"
mkdir -p "$LOG_DIR"
echo "{\"hook\":\"SubagentStart\",\"agent_type\":\"${AGENT_TYPE}\",\"session_id\":\"${SESSION_ID}\",\"marker\":\"${MARKER}\",\"latency_ms\":${ELAPSED_MS},\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >> "${LOG_DIR}/hook-latency.jsonl"

exit 0
