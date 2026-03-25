#!/usr/bin/env bash
# uni-watch.sh — real-time visibility into Unimatrix queries, injections, and SubagentStart events.
#
# Usage:
#   ./scripts/uni-watch.sh                    # auto-detect active DB
#   ./scripts/uni-watch.sh /path/to/db        # explicit DB path
#   ./scripts/uni-watch.sh --since 5m         # only show events from last 5 minutes
#
# Output columns:
#   QUERY   — every ContextSearch: time, session (8 chars), result count, query text
#   INJECT  — every entry injected: time, session, entry ID, confidence, title
#   SUBAGENT — every SubagentStart: time, session, query text sent to search
#
# Requires: sqlite3

set -euo pipefail

# -- Config --
POLL_INTERVAL=2
DB_BASE="${HOME}/.unimatrix"

# -- Helpers --
die() { echo "ERROR: $*" >&2; exit 1; }

find_active_db() {
    local db
    db=$(ls -t "${DB_BASE}"/*/unimatrix.db 2>/dev/null | head -1)
    [[ -n "$db" ]] || die "No unimatrix.db found under ${DB_BASE}"
    echo "$db"
}

query() {
    sqlite3 -separator $'\t' "$DB" "$1" 2>/dev/null
}

# -- Argument parsing --
DB=""
SINCE_SECS=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --since)
            shift
            case "$1" in
                *m) SINCE_SECS=$(( ${1%m} * 60 )) ;;
                *h) SINCE_SECS=$(( ${1%h} * 3600 )) ;;
                *s) SINCE_SECS=${1%s} ;;
                *)  die "Unknown --since unit: $1 (use 5m, 1h, 30s)" ;;
            esac
            shift ;;
        --*)
            die "Unknown option: $1" ;;
        *)
            DB="$1"
            shift ;;
    esac
done

[[ -n "$DB" ]] || DB=$(find_active_db)
[[ -f "$DB" ]] || die "DB not found: $DB"

# -- Watermarks --
NOW_SECS=$(date +%s)
SINCE_TS=$(( NOW_SECS - SINCE_SECS ))

LAST_QID=$(query "SELECT COALESCE(MAX(query_id), 0) FROM query_log WHERE ts >= ${SINCE_TS};")
LAST_LID=$(query "SELECT COALESCE(MAX(log_id), 0) FROM injection_log WHERE timestamp >= ${SINCE_TS};")
LAST_OID=$(query "SELECT COALESCE(MAX(id), 0) FROM observations WHERE hook='SubagentStart' AND ts_millis >= ${SINCE_TS}000;")

# Use 0 if empty
LAST_QID=${LAST_QID:-0}
LAST_LID=${LAST_LID:-0}
LAST_OID=${LAST_OID:-0}

# -- Header --
echo "uni-watch  DB: $DB"
echo "Watermarks — query_id:${LAST_QID}  log_id:${LAST_LID}  obs_id:${LAST_OID}"
echo "Polling every ${POLL_INTERVAL}s. Ctrl-C to stop."
echo "$(printf '%0.s-' {1..100})"
printf "%-10s %-19s %-9s %-5s %s\n" "TYPE" "TIME" "SESSION" "EXTRA" "DETAIL"
echo "$(printf '%0.s-' {1..100})"

# -- Poll loop --
while true; do

    # New queries
    while IFS=$'\t' read -r qid ts session_id result_count query_text; do
        [[ -z "$qid" ]] && continue
        ts_fmt=$(date -d "@${ts}" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -r "${ts}" '+%Y-%m-%d %H:%M:%S')
        sess="${session_id:0:8}"
        detail="${query_text:0:80}"
        printf "%-10s %-19s %-9s %-5s %s\n" "QUERY" "$ts_fmt" "$sess" "(${result_count})" "$detail"
        LAST_QID=$qid
    done < <(query "
        SELECT query_id, ts, session_id, result_count, query_text
        FROM query_log
        WHERE query_id > ${LAST_QID}
        ORDER BY query_id
        LIMIT 50;
    ")

    # New injections (joined with entry title)
    while IFS=$'\t' read -r lid ts session_id entry_id confidence title; do
        [[ -z "$lid" ]] && continue
        ts_fmt=$(date -d "@${ts}" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -r "${ts}" '+%Y-%m-%d %H:%M:%S')
        sess="${session_id:0:8}"
        conf_fmt=$(printf "%.2f" "$confidence")
        detail="#${entry_id} ${conf_fmt}  ${title:0:60}"
        printf "%-10s %-19s %-9s %-5s %s\n" "INJECT" "$ts_fmt" "$sess" "" "$detail"
        LAST_LID=$lid
    done < <(query "
        SELECT il.log_id, il.timestamp, il.session_id, il.entry_id, il.confidence,
               COALESCE(e.title, '(title unknown)')
        FROM injection_log il
        LEFT JOIN entries e ON il.entry_id = e.id
        WHERE il.log_id > ${LAST_LID}
        ORDER BY il.log_id
        LIMIT 50;
    ")

    # New SubagentStart observations
    while IFS=$'\t' read -r oid ts_millis session_id input_text; do
        [[ -z "$oid" ]] && continue
        ts_secs=$(( ts_millis / 1000 ))
        ts_fmt=$(date -d "@${ts_secs}" '+%Y-%m-%d %H:%M:%S' 2>/dev/null || date -r "${ts_secs}" '+%Y-%m-%d %H:%M:%S')
        sess="${session_id:0:8}"
        detail="${input_text:0:80}"
        printf "%-10s %-19s %-9s %-5s %s\n" "SUBAGENT" "$ts_fmt" "$sess" "" "$detail"
        LAST_OID=$oid
    done < <(query "
        SELECT id, ts_millis, session_id, COALESCE(input, '(no input)')
        FROM observations
        WHERE hook = 'SubagentStart' AND id > ${LAST_OID}
        ORDER BY id
        LIMIT 50;
    ")

    sleep $POLL_INTERVAL
done
