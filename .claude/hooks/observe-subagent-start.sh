#!/usr/bin/env bash
# SubagentStart hook: captures agent spawn with type and prompt snippet.
# Appends a JSONL record to ~/.unimatrix/observation/<session_id>.jsonl
# Exits 0 unconditionally (FR-01.4).

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // empty' 2>/dev/null)
PROMPT_SNIPPET=$(echo "$INPUT" | jq -r '.prompt_snippet // empty' 2>/dev/null)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.%3NZ")

RECORD=$(jq -nc \
    --arg ts "$TS" \
    --arg hook "SubagentStart" \
    --arg session_id "$SESSION_ID" \
    --arg agent_type "$AGENT_TYPE" \
    --arg prompt_snippet "$PROMPT_SNIPPET" \
    '{ts: $ts, hook: $hook, session_id: $session_id, agent_type: $agent_type, prompt_snippet: $prompt_snippet}' \
    2>/dev/null) || exit 0

echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
