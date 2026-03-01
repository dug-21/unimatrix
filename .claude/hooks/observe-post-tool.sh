#!/usr/bin/env bash
# PostToolUse hook: captures tool response metadata after execution.
# Appends a JSONL record to ~/.unimatrix/observation/<session_id>.jsonl
# Truncates response snippet to 500 chars (FR-01.5).
# Exits 0 unconditionally (FR-01.4).

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // null' 2>/dev/null)
RESPONSE=$(echo "$INPUT" | jq -r '.response // empty' 2>/dev/null)
RESPONSE_SIZE=${#RESPONSE}
RESPONSE_SNIPPET=$(echo "$RESPONSE" | head -c 500)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

RECORD=$(jq -nc \
    --arg ts "$TS" \
    --arg hook "PostToolUse" \
    --arg session_id "$SESSION_ID" \
    --arg tool "$TOOL" \
    --argjson input "$TOOL_INPUT" \
    --argjson response_size "$RESPONSE_SIZE" \
    --arg response_snippet "$RESPONSE_SNIPPET" \
    '{ts: $ts, hook: $hook, session_id: $session_id, tool: $tool, input: $input, response_size: $response_size, response_snippet: $response_snippet}' \
    2>/dev/null) || exit 0

echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
