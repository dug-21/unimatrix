#!/usr/bin/env bash
# SubagentStart observer — logs every subagent spawn
# Purely passive: no stdout output
set -euo pipefail

OUTPUT_DIR="$HOME/.unimatrix/observation"
OUTPUT_FILE="$OUTPUT_DIR/activity.jsonl"
mkdir -p "$OUTPUT_DIR"

INPUT=$(cat)

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // "unknown"')
# Truncate prompt to first 1KB
PROMPT=$(echo "$INPUT" | jq -r '.prompt // ""')
if [ ${#PROMPT} -gt 1024 ]; then
  PROMPT_SNIPPET="${PROMPT:0:1024}...[truncated]"
else
  PROMPT_SNIPPET="$PROMPT"
fi

TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

jq -n -c \
  --arg ts "$TS" \
  --arg hook "SubagentStart" \
  --arg session_id "$SESSION_ID" \
  --arg agent_type "$AGENT_TYPE" \
  --arg prompt_snippet "$PROMPT_SNIPPET" \
  '{ts:$ts, hook:$hook, session_id:$session_id, agent_type:$agent_type, prompt_snippet:$prompt_snippet}' \
  >> "$OUTPUT_FILE" 2>/dev/null

# No stdout — purely passive
exit 0
