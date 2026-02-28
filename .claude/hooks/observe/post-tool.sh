#!/usr/bin/env bash
# PostToolUse observer — logs every tool call after execution
# Purely passive: no stdout output, no tool modification
set -euo pipefail

OUTPUT_DIR="$HOME/.unimatrix/observation"
OUTPUT_FILE="$OUTPUT_DIR/activity.jsonl"
mkdir -p "$OUTPUT_DIR"

INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
# Truncate tool_input values over 2KB
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // {} | walk(if type == "string" and (. | length) > 2048 then .[0:2048] + "...[truncated]" else . end)')

# Get response size and truncated snippet
RESPONSE=$(echo "$INPUT" | jq -r '.tool_response // ""')
RESPONSE_SIZE=${#RESPONSE}
if [ "$RESPONSE_SIZE" -gt 2048 ]; then
  RESPONSE_SNIPPET="${RESPONSE:0:2048}...[truncated]"
else
  RESPONSE_SNIPPET="$RESPONSE"
fi

TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

jq -n -c \
  --arg ts "$TS" \
  --arg hook "PostToolUse" \
  --arg session_id "$SESSION_ID" \
  --arg tool "$TOOL_NAME" \
  --argjson input "$TOOL_INPUT" \
  --argjson response_size "$RESPONSE_SIZE" \
  --arg response_snippet "$RESPONSE_SNIPPET" \
  '{ts:$ts, hook:$hook, session_id:$session_id, tool:$tool, input:$input, response_size:$response_size, response_snippet:$response_snippet}' \
  >> "$OUTPUT_FILE" 2>/dev/null

# No stdout — purely passive
exit 0
