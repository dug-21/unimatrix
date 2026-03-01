#!/usr/bin/env bash
# PreToolUse hook: captures tool name and input before execution.
# Appends a JSONL record to ~/.unimatrix/observation/<session_id>.jsonl
# Exits 0 unconditionally (FR-01.4).

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // null' 2>/dev/null)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

RECORD=$(jq -n \
    --arg ts "$TS" \
    --arg hook "PreToolUse" \
    --arg session_id "$SESSION_ID" \
    --arg tool "$TOOL" \
    --argjson input "$TOOL_INPUT" \
    '{ts: $ts, hook: $hook, session_id: $session_id, tool: $tool, input: $input}' \
    2>/dev/null) || exit 0

echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
