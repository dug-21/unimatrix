#!/usr/bin/env bash
# ASS-011 Phase 1 Test: PreToolUse identity injection (RQ-1b)
# Injects agent_id into MCP tool calls via updatedInput.
# Tests whether updatedInput reaches the Unimatrix MCP server.
set -euo pipefail

START_NS=$(date +%s%N)
INPUT=$(cat)

# Debug: log full input for session/agent ID analysis
DEBUG_LOG="${HOME}/.unimatrix/observation/test/pretooluse-debug.log"
mkdir -p "$(dirname "$DEBUG_LOG")"
echo "$INPUT" | jq -c '{session_id, tool_name, hook_event_name}' >> "$DEBUG_LOG" 2>/dev/null
echo "$INPUT" >> "${DEBUG_LOG}.raw" 2>/dev/null

TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')

# Only process MCP unimatrix tools
case "$TOOL_NAME" in
  mcp__unimatrix__context_*)
    ;;
  *)
    exit 0
    ;;
esac

# Read identity from state file if it exists, otherwise generate
STATE_FILE="${HOME}/.unimatrix/observation/test/identity-state.json"
if [ -f "$STATE_FILE" ]; then
  AGENT_ID=$(jq -r '.agent_id // "hook-injected-unknown"' "$STATE_FILE")
else
  AGENT_ID="hook-injected-${SESSION_ID:0:8}"
fi

# CRITICAL: updatedInput REPLACES tool_input entirely (not merge).
# Must extract original tool_input and add agent_id to it.
ORIGINAL_INPUT=$(echo "$INPUT" | jq '.tool_input // {}')
OUTPUT=$(echo "$ORIGINAL_INPUT" | jq --arg aid "$AGENT_ID" '{
  hookSpecificOutput: {
    hookEventName: "PreToolUse",
    updatedInput: (. + {agent_id: $aid})
  }
}')

echo "$OUTPUT"

# Log timing
END_NS=$(date +%s%N)
ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))

LOG_DIR="${HOME}/.unimatrix/observation/test"
mkdir -p "$LOG_DIR"
echo "{\"hook\":\"PreToolUse\",\"tool\":\"${TOOL_NAME}\",\"injected_id\":\"${AGENT_ID}\",\"session_id\":\"${SESSION_ID}\",\"latency_ms\":${ELAPSED_MS},\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >> "${LOG_DIR}/hook-latency.jsonl"

exit 0
