#!/usr/bin/env bash
# SubagentStop observer — logs every subagent completion
# Purely passive: no stdout output
set -euo pipefail

OUTPUT_DIR="$HOME/.unimatrix/observation"
OUTPUT_FILE="$OUTPUT_DIR/activity.jsonl"
mkdir -p "$OUTPUT_DIR"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // "unknown"')

TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

jq -n -c \
  --arg ts "$TS" \
  --arg hook "SubagentStop" \
  --arg session_id "$SESSION_ID" \
  --arg agent_type "$AGENT_TYPE" \
  '{ts:$ts, hook:$hook, session_id:$session_id, agent_type:$agent_type}' \
  >> "$OUTPUT_FILE" 2>/dev/null

# No stdout — purely passive
exit 0
