#!/usr/bin/env bash
# Install libonnxruntime for linking/running the server build. github.com/microsoft/onnxruntime release
# downloads return 404 (as of nan-024 CI), so take the shared library from the PyPI manylinux wheel.
set -euo pipefail
VER="${ORT_VERSION:-1.20.1}"
d="$(mktemp -d)"
python3 -m pip download "onnxruntime==${VER}" --no-deps --only-binary=:all: --platform manylinux_2_27_x86_64 --python-version 3.10 -d "$d" >/dev/null
unzip -j -o "$d"/onnxruntime-*.whl "onnxruntime/capi/libonnxruntime.so.${VER}" -d "$d/lib" >/dev/null
sudo cp "$d/lib/libonnxruntime.so.${VER}" /usr/local/lib/
sudo ln -sf "libonnxruntime.so.${VER}" /usr/local/lib/libonnxruntime.so.1
sudo ln -sf "libonnxruntime.so.${VER}" /usr/local/lib/libonnxruntime.so
sudo ldconfig
ls -la /usr/local/lib/libonnxruntime*
