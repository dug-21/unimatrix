#!/usr/bin/env bash
# stub-read-marker.sh — controllable stand-in for the infra-003 two-store read
# primitive (read_marker -> vol cat db+-wal+-shm -> host sqlite3) used by
# release-gate-isolation-logic-test.sh. Touches no Docker volume, no sqlite3.
#
# Invoked by read_marker() as:  $SMOKE_READ_MARKER_CMD <store_dir> <table> <predicate>
# The marker literal is embedded in <predicate> (e.g.
#   topic_signal = 'infra003-obs-a-t1'
#   content LIKE '%infra003-mcp-a-t1%' OR topic = 'infra003-mcp-a-t1'
# ), so the stub keys "presence" off (store_dir, marker-substring-of-predicate).
# The four infra-003 markers are mutually non-substring (R-18), so a predicate
# carrying marker X never matches a present-entry for a different marker.
#
# Output: a row-count integer (>=0) OR the literal `INFRA` sentinel — exactly the
# contract read_marker's real path returns. Default is 0 (clean / absent).
#
# Env (space-separated lists of  <store_dir>::<marker>  entries):
#   STUB_PRESENT : entries that return count 1 (own-store positive OR planted leak).
#   STUB_INFRA   : entries that return the INFRA sentinel (e.g. missing main db).
# Env (deadline-poll retry, ONE entry  <store_dir>::<marker>::<absent_reads>):
#   STUB_RETRY        : return 0 for the first <absent_reads> reads of this entry,
#                       then 1 (proves the read-as-barrier retries, not a fixed sleep).
#   STUB_RETRY_COUNTER: file holding this entry's read counter (created on first use).
#   STUB_COUNT_FILE   : optional — every invocation appends a line (call-count proof).

store_dir="$1"
predicate="$3"

[ -n "${STUB_COUNT_FILE:-}" ] && printf '%s|%s\n' "$store_dir" "$predicate" >> "$STUB_COUNT_FILE"

# predicate_has <marker> : true iff the predicate string contains the marker.
predicate_has() { case "$predicate" in *"$1"*) return 0;; *) return 1;; esac; }

# INFRA entries first (a failed read dominates a planted-present).
if [ -n "${STUB_INFRA:-}" ]; then
  for e in $STUB_INFRA; do
    d="${e%%::*}"; m="${e##*::}"
    if [ "$d" = "$store_dir" ] && predicate_has "$m"; then
      printf 'INFRA'; exit 0
    fi
  done
fi

# Retry entry: count reads; absent until the threshold, then present.
if [ -n "${STUB_RETRY:-}" ]; then
  rd="${STUB_RETRY%%::*}"; rest="${STUB_RETRY#*::}"
  rm="${rest%%::*}"; rthresh="${rest##*::}"
  if [ "$rd" = "$store_dir" ] && predicate_has "$rm"; then
    cf="${STUB_RETRY_COUNTER:-/tmp/stub-read-marker.retry.$$}"
    n=0; [ -f "$cf" ] && n="$(cat "$cf")"
    n=$((n + 1)); printf '%s' "$n" > "$cf"
    if [ "$n" -gt "$rthresh" ]; then printf '1'; else printf '0'; fi
    exit 0
  fi
fi

# Present entries.
if [ -n "${STUB_PRESENT:-}" ]; then
  for e in $STUB_PRESENT; do
    d="${e%%::*}"; m="${e##*::}"
    if [ "$d" = "$store_dir" ] && predicate_has "$m"; then
      printf '1'; exit 0
    fi
  done
fi

# Default: absent (a genuine, trusted 0-row read).
printf '0'
