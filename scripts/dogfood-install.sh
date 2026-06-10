#!/usr/bin/env sh
# dogfood-install.sh — Build + copy-install the in-repo TS hook client.
#
# npm pack `packages/unimatrix`, extract the frozen runtime tree to a sibling
# staging dir, then atomic `mv` clean-replace into the install target
# (default ${HOME}/.unimatrix/dogfood-client). Idempotent. The F6 soak-reset
# point (AC-01). This is TOOLING: it MAY be loud and exit non-zero on failure
# (unlike the hooks it installs, which fail-open). C-6 copy install, never
# npm link. C-8: does not modify any packages/unimatrix/lib/** surface.
#
# Usage:
#   scripts/dogfood-install.sh [--target <dir>] [--print-target|--dry-run]
#                              [--keep-staging]
#     --target <dir>    install destination (DEFAULT ${HOME}/.unimatrix/dogfood-client)
#     --print-target    resolve + guard the target, print it, exit 0 (no writes)
#     --dry-run         alias of --print-target
#     --keep-staging    do not remove the staging dir on success (R-02 evidence)
#
# Exit codes:
#   0  installed + completeness/smoke asserts passed (or --print-target)
#   2  unknown / malformed argument
#   3  --target empty / unsafe / forbidden (guard reject, BEFORE any rm)
#   4  npm pack failure / tarball not produced
#   5  incomplete tree / platform binary present / symlink entrypoint
#   6  fail-open smoke failed (entrypoint exited non-zero)
set -eu

die() {
  # die <message> <code>
  echo "dogfood-install: $1" >&2
  exit "${2:-1}"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1" 1
}

# is_ancestor_of <ancestor> <descendant>: true if ancestor == descendant or a
# parent dir of descendant. Compares realpath'd absolute paths with a trailing
# slash so /a/bc is not treated as an ancestor of /a/bcd.
is_ancestor_of() {
  _anc="$1"
  _desc="$2"
  [ "$_anc" = "$_desc" ] && return 0
  case "$_desc/" in
    "$_anc"/*) return 0 ;;
    *) return 1 ;;
  esac
}

# guard_target <target>: SHARED clean-replace safety guard. Validates the
# target resolves under ${HOME}/.unimatrix/ OR is an explicit absolute path
# that is not a forbidden ancestor — BEFORE any removal. Loud non-zero on
# rejection. Sets RESOLVED to the validated absolute path.
guard_target() {
  _target="$1"
  [ -n "$_target" ] || die "empty --target" 3

  # Only an absolute target is acceptable. A relative path would be resolved
  # against the (unknown) cwd into a surprise absolute path — reject up front.
  case "$_target" in
    /*) : ;;
    *) die "target must be absolute or under ~/.unimatrix/: $_target" 3 ;;
  esac

  _base=$(basename "$_target")
  [ "$_base" != "" ] && [ "$_base" != "." ] && [ "$_base" != ".." ] \
    && [ "$_base" != "/" ] \
    || die "unsafe target basename: $_target" 3

  _rp_parent=$(realpath -m "$(dirname "$_target")")
  # Collapse a trailing slash on the parent so "/" -> "" yields "/$_base",
  # never "//$_base" (basename "/" is "/", handled by the basename guard above).
  RESOLVED="${_rp_parent%/}/$_base"
  _home_rp=$(realpath -m "$HOME")
  _home_rp="${_home_rp%/}"

  case "$RESOLVED" in
    "$_home_rp"/.unimatrix/*)
      # default + temp installs under ~/.unimatrix/
      return 0
      ;;
    /*)
      # explicit absolute --target: reject forbidden paths and repo ancestors
      for _forbidden in "$_home_rp" "/" "$_home_rp/.unimatrix" "$REPO"; do
        [ "$RESOLVED" = "$_forbidden" ] \
          && die "refusing to rm forbidden path: $RESOLVED" 3
      done
      is_ancestor_of "$RESOLVED" "$REPO" \
        && die "target is ancestor of repo: $RESOLVED" 3
      return 0
      ;;
    *)
      die "target must be absolute or under ~/.unimatrix/: $RESOLVED" 3
      ;;
  esac
}

# pack <pkgdir> <staging>: npm pack into staging; echo the tarball path.
# Tarball name globbed (version bumps; #4328) — never hardcode the version.
pack() {
  _pkgdir="$1"
  _staging="$2"
  ( cd "$_pkgdir" && npm pack --silent --pack-destination "$_staging" ) \
    >/dev/null 2>&1 || die "npm pack failed" 4
  # exactly one tarball expected; first glob match is authoritative.
  for _tgz in "$_staging"/dug-21-unimatrix-*.tgz; do
    [ -f "$_tgz" ] || die "tarball not produced" 4
    echo "$_tgz"
    return 0
  done
  die "tarball not produced" 4
}

# extract <tgz> <staging>: extract package/ (strip-components=1) so the result
# is the client tree root. postinstall.js is copied INERT — extraction is a
# pure file copy; no lifecycle script runs (FR-3). Echo the extracted dir.
extract() {
  _tgz="$1"
  _staging="$2"
  _extracted="$_staging/extracted"
  mkdir -p "$_extracted"
  tar -xzf "$_tgz" -C "$_extracted" --strip-components=1 \
    || die "extract failed" 4
  echo "$_extracted"
}

# clean_replace <extracted> <target>: staged sibling + atomic mv (R-02). The
# target is only ever the OLD complete tree or the NEW complete tree, never a
# partial one. Prior install moved fully aside before swap (replace, not overlay).
clean_replace() {
  _extracted="$1"
  _target="$2"
  _parent=$(dirname "$_target")
  mkdir -p "$_parent"
  _staged="$_parent/.dogfood-client.staging.$$"
  _old="$_parent/.dogfood-client.old.$$"
  rm -rf "$_staged"
  mv "$_extracted" "$_staged"
  if [ -e "$_target" ]; then
    rm -rf "$_old"
    mv "$_target" "$_old"
  fi
  mv "$_staged" "$_target"
  rm -rf "$_old" 2>/dev/null || true
}

# assert_complete <target>: completeness (FR-2, SR-01 / R-11). Full files[] set
# present; platform binary absent; entrypoint not a symlink (C-6).
assert_complete() {
  _target="$1"
  for _required in \
    lib/hook-client/index.js \
    lib/merge-settings.js \
    lib/hook-client/config.js; do
    [ -f "$_target/$_required" ] || die "incomplete install: missing $_required" 5
  done
  for _d in bin lib skills protocols; do
    [ -d "$_target/$_d" ] || die "incomplete install: missing dir $_d" 5
  done
  [ -f "$_target/postinstall.js" ] \
    || die "incomplete install: missing postinstall.js (expected inert copy)" 5
  # full hook-client/*.js set present (index.js + siblings).
  _count=0
  for _f in "$_target"/lib/hook-client/*.js; do
    [ -f "$_f" ] && _count=$((_count + 1))
  done
  [ "$_count" -ge 1 ] || die "no hook-client js files" 5
  # platform binary MUST be absent (optionalDependency, never bundled).
  if find "$_target" -type f -perm -u+x -name 'unimatrix' 2>/dev/null \
    | grep -q .; then
    die "unexpected platform binary in frozen tree" 5
  fi
  # non-symlink entrypoint (C-6 structural anti-npm-link guarantee).
  [ -L "$_target/lib/hook-client/index.js" ] \
    && die "entrypoint is a symlink (npm link leak)" 5
  return 0
}

# smoke <target>: fail-open smoke (SR-08, AC-01 step b). The entrypoint runs
# and fail-opens with no daemon and empty stdin.
smoke() {
  _target="$1"
  if node "$_target/lib/hook-client/index.js" SessionStart </dev/null \
    >/dev/null 2>&1; then
    return 0
  fi
  die "smoke failed: entrypoint exit non-zero (expected fail-open 0)" 6
}

main() {
  TARGET="${HOME}/.unimatrix/dogfood-client"
  PRINT_ONLY=0
  KEEP_STAGING=0

  while [ $# -gt 0 ]; do
    case "$1" in
      --target)
        [ $# -ge 2 ] || die "--target requires a value" 2
        TARGET="$2"
        shift 2
        ;;
      --target=*)
        TARGET="${1#--target=}"
        shift
        ;;
      --print-target | --dry-run)
        PRINT_ONLY=1
        shift
        ;;
      --keep-staging)
        KEEP_STAGING=1
        shift
        ;;
      *)
        die "unknown arg: $1" 2
        ;;
    esac
  done

  REPO=$(git rev-parse --show-toplevel 2>/dev/null) \
    || die "not inside a git repository" 1
  PKGDIR="$REPO/packages/unimatrix"
  [ -f "$PKGDIR/package.json" ] || die "client package not found: $PKGDIR" 1

  # Expand a leading ~ and ${HOME}/$HOME so the guard sees an absolute path.
  case "$TARGET" in
    "~") TARGET="$HOME" ;;
    "~/"*) TARGET="$HOME/${TARGET#~/}" ;;
  esac
  TARGET=$(printf '%s' "$TARGET" | sed "s|\${HOME}|$HOME|g; s|\$HOME|$HOME|g")

  guard_target "$TARGET"

  if [ "$PRINT_ONLY" -eq 1 ]; then
    # Non-destructive: report the resolved target and exit without writing.
    echo "$RESOLVED"
    exit 0
  fi

  require_cmd npm
  require_cmd node
  require_cmd tar

  STAGING=$(mktemp -d "${TMPDIR:-/tmp}/dogfood-install.XXXXXX") \
    || die "could not create staging dir" 1
  _parent=$(dirname "$TARGET")
  # shellcheck disable=SC2064
  trap "rm -rf \"$STAGING\" \"$_parent/.dogfood-client.staging.$$\" \"$_parent/.dogfood-client.old.$$\" 2>/dev/null || true" EXIT INT TERM

  TGZ=$(pack "$PKGDIR" "$STAGING")
  EXTRACTED=$(extract "$TGZ" "$STAGING")
  clean_replace "$EXTRACTED" "$TARGET"
  assert_complete "$TARGET"
  smoke "$TARGET"

  if [ "$KEEP_STAGING" -eq 1 ]; then
    # Debug affordance (R-02 evidence): leave staging in place and report it so
    # a test can assert it is a sibling-tree under TMPDIR, not at the target.
    trap - EXIT INT TERM
    echo "staging retained: $STAGING" >&2
  fi

  echo "installed dogfood client -> $TARGET" >&2
  exit 0
}

main "$@"
