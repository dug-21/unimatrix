#!/usr/bin/env bash
# release-tag-parity-test.sh — pre-merge HARD gate (nan-019 T2 / FR-11 / R-09).
#
# The tag-strip defect that ACTUALLY OCCURRED: the first draft resolved the push tag
# stripped (`${GITHUB_REF_NAME#v}` => :1.2.3-<arch>) while build-container-* pushes
# un-stripped (`pattern=v{{version}}-<arch>` => :v1.2.3-<arch>) — a guaranteed docker
# pull 404 on every release. This static check converts that post-tag surprise into a
# pre-merge gate: it asserts the smoke's resolved per-arch tag is BYTE-IDENTICAL to what
# the build jobs push, with NO tag push and NO Docker.
#
# Non-vacuousness is structural: the two sides are derived from DIFFERENT sources.
#   * SMOKE side  : `source release-gate-lib.sh` and call the SHIPPED resolve_image() —
#                   the exact bytes release.yml's smoke jobs run.
#   * BUILD side  : READ release.yml's docker/metadata-action `tags:` patterns
#                   (type=semver,pattern=v{{version}}-<arch> + type=raw,value=latest-<arch>)
#                   and MODEL the metadata-action's documented semantics. Never copied
#                   from the smoke side. A future edit to either side that diverges -> RED.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/release-gate-lib.sh"
# release.yml lives at repo root .github/workflows/release.yml
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../../.." && pwd)"
RELEASE_YML="${REPO_ROOT}/.github/workflows/release.yml"
OWNER="acme"   # arbitrary; suffix/version parity is what matters, not the owner

# shellcheck source=release-gate-lib.sh
source "$LIB"
if ! declare -F resolve_image >/dev/null; then
  echo "FATAL: resolve_image not found after sourcing $LIB" >&2
  exit 1
fi

PASS=0
FAIL=0
pass() { PASS=$((PASS+1)); printf '  ok   %s\n' "$1"; }
oops() { FAIL=$((FAIL+1)); printf '  FAIL %s\n' "$1"; [ -n "${2:-}" ] && printf '       %s\n' "$2"; }

# ---- SMOKE side: the SHIPPED resolution, stripped to bare tag for comparison -------------
# resolve_image emits a full ghcr ref; we compare the per-arch TAG portion (after the ':').
smoke_tag() {
  local event="$1" ref="$2" arch="$3"
  local full
  full="$(resolve_image "$OWNER" "$event" "$ref" "$arch")"
  printf '%s' "${full##*:}"   # everything after the last ':' => v1.2.3-amd64 / latest-amd64
}

# ---- BUILD side: independently READ + MODEL release.yml's metadata-action patterns -------
# Confirm the patterns we model are the ones actually present in release.yml, so a future
# edit to the YAML that we don't mirror cannot pass silently. We require, per arch:
#   type=semver,pattern=v{{version}}-<arch>
#   type=raw,value=latest-<arch>
assert_yml_patterns_present() {
  local arch="$1"
  grep -qF "type=semver,pattern=v{{version}}-${arch}" "$RELEASE_YML" \
    || { oops "yml_pattern_present_semver_${arch}" "missing type=semver,pattern=v{{version}}-${arch} in $RELEASE_YML"; return 1; }
  grep -qF "type=raw,value=latest-${arch}" "$RELEASE_YML" \
    || { oops "yml_pattern_present_raw_${arch}" "missing type=raw,value=latest-${arch} in $RELEASE_YML"; return 1; }
  return 0
}

# semverVersion: docker/metadata-action {{version}} for a `vX.Y.Z` tag strips the leading v.
# (We then re-prepend the literal `v` from `pattern=v{{version}}`.)
semver_version() {
  local ref="$1"
  printf '%s' "${ref#v}"
}

# build_pushed_tag MODELS docker/metadata-action against the patterns read above:
#   workflow_dispatch (branch ref) -> semver emits nothing; type=raw value=latest-<arch> applies.
#   push v* tag                    -> pattern=v{{version}}-<arch> => v + semver(ref) + -<arch>.
build_pushed_tag() {
  local event="$1" ref="$2" arch="$3"
  if [ "$event" = "workflow_dispatch" ]; then
    printf 'latest-%s' "$arch"                       # from type=raw,value=latest-<arch>
  else
    printf 'v%s-%s' "$(semver_version "$ref")" "$arch"  # literal v re-prepended by pattern
  fi
}

# ---- parity assertion: smoke side == build side, byte-identical --------------------------
# allow_neg: when "neg", we EXPECT a mismatch (discrimination self-check) and flip the verdict.
assert_parity() {
  local name="$1" event="$2" ref="$3" arch="$4" mode="${5:-pos}"
  local s b
  s="$(smoke_tag "$event" "$ref" "$arch")"
  b="$(build_pushed_tag "$event" "$ref" "$arch")"
  if [ "$mode" = "neg" ]; then
    if [ "$s" != "$b" ]; then
      pass "$name (discrimination: smoke='$s' != build='$b' as required)"
    else
      oops "$name (discrimination)" "expected MISMATCH but both = '$s' — assertion is vacuous!"
    fi
    return
  fi
  if [ "$s" = "$b" ]; then
    pass "$name (smoke='$s' == build='$b')"
  else
    oops "$name" "smoke='$s' != build='$b'"
  fi
}

echo "== confirm release.yml carries the patterns we model (non-vacuous build side) =="
assert_yml_patterns_present amd64 && pass "yml_patterns_present_amd64"
assert_yml_patterns_present arm64 && pass "yml_patterns_present_arm64"

echo "== T2 tag parity (push) =="
assert_parity test_tag_parity_push_amd64       push v1.2.3 amd64
assert_parity test_tag_parity_push_arm64       push v1.2.3 arm64
assert_parity test_tag_parity_push_amd64_v082  push v0.8.2 amd64

echo "== T2 tag parity (workflow_dispatch) =="
assert_parity test_tag_parity_dispatch_amd64 workflow_dispatch main amd64
assert_parity test_tag_parity_dispatch_arm64 workflow_dispatch main arm64

echo "== un-stripped v kept (the OCCURRED defect guard) =="
# Resolved push tag MUST be v1.2.3-amd64, NOT 1.2.3-amd64.
T="$(smoke_tag push v1.2.3 amd64)"
case "$T" in
  v1.2.3-amd64) pass "test_tag_no_v_strip (resolved '$T', v kept)";;
  1.2.3-amd64)  oops "test_tag_no_v_strip" "v was STRIPPED -> '$T' (the first-draft 404 defect)";;
  *)            oops "test_tag_no_v_strip" "unexpected resolved tag '$T'";;
esac

echo "== per-arch suffix, no swap =="
A="$(smoke_tag push v1.2.3 amd64)"
B="$(smoke_tag push v1.2.3 arm64)"
[ "$A" = "v1.2.3-amd64" ] && pass "test_tag_suffix_no_swap (amd64 -> '$A')" || oops "test_tag_suffix_no_swap" "amd64 resolved '$A'"
[ "$B" = "v1.2.3-arm64" ] && pass "test_tag_suffix_no_swap_arm64 (arm64 -> '$B')" || oops "test_tag_suffix_no_swap_arm64" "arm64 resolved '$B'"

echo "== discrimination self-check: a strip / swap / extra-v MUST go RED =="
# These prove the assertion is NOT vacuously true. We construct each defective BUILD-side
# value and confirm the parity assertion (smoke vs that value) goes RED. We mutate the
# BUILD model locally (the build side is the independently-derived side), assert MISMATCH,
# then nothing persists — release-gate-lib.sh is never touched.

# 1) re-introduced `${...#v}` strip on the build side: v1.2.3 -> 1.2.3-amd64 (smoke keeps v)
strip_build() { printf '%s-%s' "$(semver_version "$2")" "$3"; }
s="$(smoke_tag push v1.2.3 amd64)"; b="$(strip_build push v1.2.3 amd64)"
[ "$s" != "$b" ] && pass "discrimination_v_strip (smoke='$s' != stripped='$b')" \
  || oops "discrimination_v_strip" "strip not detected: both '$s'"

# 2) swapped suffix: smoke amd64 vs build arm64
s="$(smoke_tag push v1.2.3 amd64)"; b="$(build_pushed_tag push v1.2.3 arm64)"
[ "$s" != "$b" ] && pass "discrimination_suffix_swap (smoke='$s' != '$b')" \
  || oops "discrimination_suffix_swap" "swap not detected: both '$s'"

# 3) extra v: vv1.2.3-amd64
s="$(smoke_tag push v1.2.3 amd64)"; b="vv1.2.3-amd64"
[ "$s" != "$b" ] && pass "discrimination_extra_v (smoke='$s' != '$b')" \
  || oops "discrimination_extra_v" "extra-v not detected: both '$s'"

echo
echo "release-tag-parity-test: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ]
