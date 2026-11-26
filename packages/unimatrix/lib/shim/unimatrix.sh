#!/bin/sh
NODE_REC='@@NODE@@'
UH='@@UHOME@@'

# Filter only (D-079): true => start Node; Node's isUnimatrixProject decides.
# Superset of that predicate; builtins only; file contents are never evaluated.
unimatrix_may_be_project() {
  if [ -n "$UNIMATRIX_PROJECT_ROOT" ]; then
    for v in @@REMOTE_VARS@@; do
      eval "x=\${$v}"
      [ -n "$x" ] && return 0
    done
  fi
  cwd=$(pwd -P 2>/dev/null) || cwd=$PWD
  d=$cwd
  while [ -n "$d" ]; do
    [ -f "$d/.git" ] && return 0
    d=${d%/*}
  done
  cr=$(printf '\r')
  for s in "$UH"/[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]; do
    [ -d "$s" ] || continue
    [ -f "$s/root" ] || return 0
    root=
    IFS= read -r root < "$s/root" || :
    root=${root%"$cr"}
    root=${root%/}
    case "$cwd/" in "$root"/*) return 0 ;; esac
  done
  return 1
}
@@PRENODE_CALL@@

if [ -x "$NODE_REC" ]; then
  NODE=$NODE_REC
else
  NODE=$(command -v node 2>/dev/null) || NODE=
  if [ -z "$NODE" ]; then
    unimatrix_may_be_project || exit 0
    echo "unimatrix: node not found (recorded $NODE_REC); run 'npx unimatrix init'" >&2
    exit 1
  fi
fi
exec "$NODE" "${0%/*}/shim-entry.js" "$@"
