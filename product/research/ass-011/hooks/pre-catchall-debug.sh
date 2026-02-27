#!/usr/bin/env bash
# ASS-011 Debug: Catch-all PreToolUse — logs every invocation
set -euo pipefail

DEBUG_LOG="${HOME}/.unimatrix/observation/test/pretooluse-catchall.log"
mkdir -p "$(dirname "$DEBUG_LOG")"

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // "unknown"')
echo "[$(date -u +%Y-%m-%dT%H:%M:%S.%NZ)] tool=$TOOL_NAME" >> "$DEBUG_LOG"

# Pass-through — don't modify anything
exit 0
