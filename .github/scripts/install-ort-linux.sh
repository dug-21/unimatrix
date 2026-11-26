#!/usr/bin/env bash
# Install libonnxruntime from the hash-pinned PyPI manylinux wheel (ADR-002 #5927, GH #1031).
# github.com/microsoft/onnxruntime release downloads return 404. Fails closed on any mismatch.
# Env: ORT_VERSION (optional; must equal the pinned version), ORT_PREFIX (default /usr/local/lib),
#      ORT_LDCONFIG (default 1; 0 skips ldconfig, e.g. in a Docker artifact stage),
#      ORT_REQUIREMENTS (default: ort-requirements.txt next to this script).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
req="${ORT_REQUIREMENTS:-$here/ort-requirements.txt}"
prefix="${ORT_PREFIX:-/usr/local/lib}"
pinned="$(sed -n 's/^onnxruntime==\([^ ]*\).*/\1/p' "$req")"
[ -n "$pinned" ] || { echo "install-ort: no pinned version in $req" >&2; exit 1; }
if [ -n "${ORT_VERSION:-}" ] && [ "$ORT_VERSION" != "$pinned" ]; then
  echo "install-ort: ORT_VERSION=$ORT_VERSION != pinned $pinned" >&2; exit 1
fi
case "$(uname -m)" in
  x86_64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) echo "install-ort: unsupported arch $(uname -m)" >&2; exit 1 ;;
esac
d="$(mktemp -d)"
trap 'rm -rf "$d"' EXIT
python3 -m pip download --require-hashes -r "$req" --no-deps --only-binary=:all: \
  --platform "manylinux_2_27_${arch}" --python-version 3.10 -d "$d/whl" >/dev/null
whl=("$d"/whl/onnxruntime-*.whl)
if [ "${#whl[@]}" -ne 1 ] || [ ! -f "${whl[0]}" ]; then
  echo "install-ort: wheel not found" >&2; exit 1
fi
python3 -m zipfile -e "${whl[0]}" "$d/x"
libs=()
for f in "$d"/x/onnxruntime/capi/libonnxruntime.so*; do
  if [ -f "$f" ] && [ ! -L "$f" ]; then libs+=("$f"); fi
done
if [ "${#libs[@]}" -ne 1 ]; then
  echo "install-ort: expected 1 libonnxruntime.so*, found ${#libs[@]}" >&2; exit 1
fi
name="$(basename "${libs[0]}")"
if [ "$name" != "libonnxruntime.so.${pinned}" ]; then
  echo "install-ort: unexpected lib $name" >&2; exit 1
fi
sudo=""
if ! mkdir -p "$prefix" 2>/dev/null || [ ! -w "$prefix" ]; then
  sudo="sudo"
  $sudo mkdir -p "$prefix"
fi
$sudo cp "${libs[0]}" "$prefix/$name"
$sudo ln -sf "$name" "$prefix/libonnxruntime.so.1"
$sudo ln -sf "$name" "$prefix/libonnxruntime.so"
if [ "${ORT_LDCONFIG:-1}" != "0" ]; then $sudo ldconfig; fi
ls -la "$prefix"/libonnxruntime*
