#!/usr/bin/env bash
# ASS-011 Phase 1 Test: PostToolUse MCP output replacement (RQ-1c)
# Replaces context_briefing output via updatedMCPToolOutput.
# Appends a marker to the original output to verify replacement works.
set -euo pipefail

START_NS=$(date +%s%N)
INPUT=$(cat)

TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')

# Only fire for context_briefing
[ "$TOOL_NAME" = "mcp__unimatrix__context_briefing" ] || exit 0

SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // "unknown"')
ORIGINAL=$(echo "$INPUT" | jq -r '.tool_response // "" | tostring')

# Append marker to original output (don't replace entirely — preserve value)
MARKER="[ASS011-MCP-REPLACE-MARKER-$(date +%s)]"
ENRICHED="${ORIGINAL}

---
${MARKER}
HOOK-ENRICHED: This briefing was enriched by a PostToolUse hook via updatedMCPToolOutput. The hook can inject workflow-state-specific content, current phase information, or observation-based feedback without modifying the MCP server."

OUTPUT=$(jq -n --arg enriched "$ENRICHED" '{
  hookSpecificOutput: {
    hookEventName: "PostToolUse",
    updatedMCPToolOutput: $enriched
  }
}')

echo "$OUTPUT"

# Log timing
END_NS=$(date +%s%N)
ELAPSED_MS=$(( (END_NS - START_NS) / 1000000 ))

LOG_DIR="${HOME}/.unimatrix/observation/test"
mkdir -p "$LOG_DIR"
echo "{\"hook\":\"PostToolUse-MCP-Replace\",\"tool\":\"${TOOL_NAME}\",\"marker\":\"${MARKER}\",\"original_len\":${#ORIGINAL},\"session_id\":\"${SESSION_ID}\",\"latency_ms\":${ELAPSED_MS},\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}" >> "${LOG_DIR}/hook-latency.jsonl"

exit 0
