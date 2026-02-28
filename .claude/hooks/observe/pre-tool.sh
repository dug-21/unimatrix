#!/usr/bin/env bash
# PreToolUse observer — logs every tool call before execution
# Also injects agent_id for Unimatrix MCP calls
set -euo pipefail

OUTPUT_DIR="$HOME/.unimatrix/observation"
OUTPUT_FILE="$OUTPUT_DIR/activity.jsonl"
mkdir -p "$OUTPUT_DIR"

INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
# Truncate tool_input values over 2KB
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // {} | walk(if type == "string" and (. | length) > 2048 then .[0:2048] + "...[truncated]" else . end)')

TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

jq -n -c \
  --arg ts "$TS" \
  --arg hook "PreToolUse" \
  --arg session_id "$SESSION_ID" \
  --arg tool "$TOOL_NAME" \
  --argjson input "$TOOL_INPUT" \
  '{ts:$ts, hook:$hook, session_id:$session_id, tool:$tool, input:$input}' \
  >> "$OUTPUT_FILE" 2>/dev/null

# Inject agent_id for Unimatrix MCP calls
if [[ "$TOOL_NAME" == mcp__unimatrix__context_* ]]; then
  echo "$INPUT" | jq -c '{hookSpecificOutput: {hookEventName: "PreToolUse", updatedInput: (.tool_input + {agent_id: "human"})}}'
fi

exit 0
