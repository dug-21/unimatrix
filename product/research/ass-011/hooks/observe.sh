#!/usr/bin/env bash
# PostToolUse observation hook for ASS-010 research spike.
# Captures tool I/O for Write, Edit, Bash, and Unimatrix MCP tools.
# Appends a single JSON-line per captured event to a session-scoped spool file.
#
# This hook is observe-only. It cannot block or modify tool execution.
# Exit 0 always.

set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract tool name
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty')

# Filter: only capture tools in the capture set
case "$TOOL_NAME" in
  Write|Edit|Bash|mcp__unimatrix__context_briefing|mcp__unimatrix__context_search|mcp__unimatrix__context_store)
    ;;
  *)
    exit 0
    ;;
esac

# Extract session ID for file scoping
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')

# Ensure spool directory exists
SPOOL_DIR="${HOME}/.unimatrix/observation/spool"
mkdir -p "$SPOOL_DIR"

# Build the observation record.
# Truncate tool_response to 10KB to bound spool size.
RECORD=$(echo "$INPUT" | jq -c '{
  ts: (now | todate),
  tool_name: .tool_name,
  tool_input: .tool_input,
  tool_response: (.tool_response | tostring | if length > 10240 then .[0:10240] + "...[truncated]" else . end),
  tool_use_id: .tool_use_id,
  session_id: .session_id
}')

# Append to session-scoped spool file
echo "$RECORD" >> "${SPOOL_DIR}/${SESSION_ID}.jsonl"

exit 0
