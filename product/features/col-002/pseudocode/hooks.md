# Pseudocode: hooks

## Purpose

Four shell scripts for Claude Code hook events. Each captures telemetry and appends to per-session JSONL files.

## Common Pattern (all scripts)

```bash
#!/usr/bin/env bash
# Exit 0 unconditionally (FR-01.4)
set -e  # but trap ensures exit 0

OBS_DIR="${HOME}/.unimatrix/observation"

# Read JSON from stdin
INPUT=$(cat)

# Extract session_id (sanitize for filename safety)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')

# Bail silently if no session_id
if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Ensure observation directory exists (FR-01.3)
mkdir -p "$OBS_DIR"

# Build and append record (hook-specific, see below)
# ...

exit 0
```

## File: `hooks/observe-pre-tool.sh`

```bash
#!/usr/bin/env bash
# PreToolUse hook: captures tool name and input before execution

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

# Extract fields
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // null' 2>/dev/null)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Build JSONL record (FR-01.6 PreToolUse schema)
RECORD=$(jq -n \
    --arg ts "$TS" \
    --arg hook "PreToolUse" \
    --arg session_id "$SESSION_ID" \
    --arg tool "$TOOL" \
    --argjson input "$TOOL_INPUT" \
    '{ts: $ts, hook: $hook, session_id: $session_id, tool: $tool, input: $input}' \
    2>/dev/null) || exit 0

# Append to session file
echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
```

## File: `hooks/observe-post-tool.sh`

```bash
#!/usr/bin/env bash
# PostToolUse hook: captures tool response metadata

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

# Extract fields
TOOL=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null)
TOOL_INPUT=$(echo "$INPUT" | jq -c '.tool_input // null' 2>/dev/null)
RESPONSE=$(echo "$INPUT" | jq -r '.response // empty' 2>/dev/null)
RESPONSE_SIZE=${#RESPONSE}
# Truncate response snippet to 500 chars (FR-01.5)
RESPONSE_SNIPPET=$(echo "$RESPONSE" | head -c 500)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

# Build JSONL record (FR-01.6 PostToolUse schema)
RECORD=$(jq -n \
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
```

## File: `hooks/observe-subagent-start.sh`

```bash
#!/usr/bin/env bash
# SubagentStart hook: captures agent spawn with type and prompt

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

# Extract fields (FR-01.6 SubagentStart schema)
AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // empty' 2>/dev/null)
PROMPT_SNIPPET=$(echo "$INPUT" | jq -r '.prompt_snippet // empty' 2>/dev/null)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

RECORD=$(jq -n \
    --arg ts "$TS" \
    --arg hook "SubagentStart" \
    --arg session_id "$SESSION_ID" \
    --arg agent_type "$AGENT_TYPE" \
    --arg prompt_snippet "$PROMPT_SNIPPET" \
    '{ts: $ts, hook: $hook, session_id: $session_id, agent_type: $agent_type, prompt_snippet: $prompt_snippet}' \
    2>/dev/null) || exit 0

echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
```

## File: `hooks/observe-subagent-stop.sh`

```bash
#!/usr/bin/env bash
# SubagentStop hook: captures agent completion

OBS_DIR="${HOME}/.unimatrix/observation"
INPUT=$(cat)
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' 2>/dev/null | tr -cd 'a-zA-Z0-9_-')
[ -z "$SESSION_ID" ] && exit 0
mkdir -p "$OBS_DIR"

# Extract fields (FR-01.6 SubagentStop schema)
# agent_type is empty in practice (platform constraint)
AGENT_TYPE=$(echo "$INPUT" | jq -r '.agent_type // empty' 2>/dev/null)
TS=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")

RECORD=$(jq -n \
    --arg ts "$TS" \
    --arg hook "SubagentStop" \
    --arg session_id "$SESSION_ID" \
    --arg agent_type "$AGENT_TYPE" \
    '{ts: $ts, hook: $hook, session_id: $session_id, agent_type: $agent_type}' \
    2>/dev/null) || exit 0

echo "$RECORD" >> "${OBS_DIR}/${SESSION_ID}.jsonl"
exit 0
```

## Error Handling

- All scripts exit 0 unconditionally (FR-01.4)
- jq failures suppressed with `|| exit 0`
- Missing session_id -> silent exit
- Missing fields -> empty strings or null
- Invalid JSON input -> jq fails silently

## Key Test Scenarios

- Pipe valid PreToolUse JSON -> JSONL file created at correct path (AC-01, AC-02)
- Pipe valid PostToolUse with large response -> snippet truncated to 500 chars (AC-05)
- Pipe invalid JSON -> exit 0, no JSONL written (AC-03)
- Pipe JSON with missing session_id -> exit 0 (R-07 scenario 3)
- Verify JSONL record has all required fields (AC-04)
- Observation dir created if missing (FR-01.3, R-12)
- SubagentStart records agent_type and prompt_snippet
- SubagentStop records with empty agent_type
