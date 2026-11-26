# 1031-design-reviewer-v2 report (uni-architect)

ADR-002 corrected #5925 -> #5927: the source is now pip `--require-hashes`, and curl + sha256 is the fallback, used only after escalation. Created nan-014 `architecture/ADR-INDEX.md`, removed the stale on-disk ADR-002 file, and relinked nan-014 IMPLEMENTATION-BRIEF row 47. GH comment: https://github.com/dug-21/unimatrix/issues/1031#issuecomment-7200645220

**Pinned requirements** (one file, both arches, cp310 wheels):
```
onnxruntime==1.20.1 \
    --hash=sha256:d82daaec24045a2e87598b8ac2b417b1cce623244e80e663882e9fe1aae86410 \
    --hash=sha256:7b2908b50101a19e99c4d4e97ebb9905561daf61829403061c1adc1b588bc0de
```
**Command** (identical on every path; `<arch>` = `x86_64` | `aarch64`):
```
python3 -m pip download --require-hashes -r ort-requirements.txt --no-deps \
  --only-binary=:all: --platform manylinux_2_27_<arch> --python-version 3.10 -d "$d"
```
- `--platform` and `--python-version 3.10` are required. Without them, pip on python:3.12 picks the cp312 wheel (bb71a814...), which fails against these pins. Verified: it fails closed.
- `libonnxruntime.so.1.20.1` is byte-identical between cp310 and cp312, so the cp tag only affects which wheel hash applies.
- `--no-deps`: only the target wheel needs a hash.
- Verified with pip 22.0.2 and 23.0.1, for both arches. Hashes match PyPI JSON.

**Risks**
- slim images have no `unzip`. Extract with `python3 -m zipfile -e`, then glob `onnxruntime/capi/libonnxruntime.so*`. Require exactly one match.
- `.dockerignore` excludes `.github/`. A shared requirements file needs a `!` exception, or the Dockerfiles must carry lockstep copies (#5725).
- pip on the ubuntu-22.04-arm image has not been verified. D-005 covers this.
- A future ORT release may drop cp310 wheels. If it does, bump `--python-version` and both hashes together.

**Follow-up (recommendation only):** `ort` is `default-features=false`, and `ort-sys` default features are `[]`. The build therefore link-time links via `ORT_LIB_LOCATION` with rpath `$ORIGIN`; `load-dynamic` is not enabled. `ENV ORT_DYLIB_PATH` in the infra-001 Dockerfile is inert. Build mode was not changed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5925, #5725, #5722, #4274, #4582, #4285; context_lookup decision/nan-014 (7 ADRs)
- Stored: entry #5927 "ADR-002: ORT Supply Chain Verification via SHA-256 Gate" via /uni-store-adr (context_correct of #5925)
