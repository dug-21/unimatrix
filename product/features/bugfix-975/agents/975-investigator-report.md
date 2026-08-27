# Agent Report: 975-investigator

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/975#issuecomment-5161538949

## Outcome
- Defects #1–#3: confirmed FIXED in tree (commit 7ac778df / PR #974) — `.dockerignore:13` re-include, compose `context: ../../..`, `cp -a …/*.so*`.
- Defect #4 root cause confirmed: `product/test/infra-001/Dockerfile` builder stage (lines 5-18) compiles `unimatrix-server` (→ `unimatrix-embed` → `ort =2.0.0-rc.9, default-features=false`) with no ORT lib installed, no `ORT_LIB_LOCATION`/`ORT_PREFER_DYNAMIC_LINK`, and no `.cargo/config.toml` copied → ort-sys emits the sentinel link flag → link fails. ORT lands only in Stage 2, after compile. `cargo test --lib --workspace` (line 18) is equally affected.
- Reference builds that do it right: root `Dockerfile:56-80` (TARGETARCH-aware, SHA-256-gated install + ENV) and `.github/workflows/release.yml:51-58,143-150`.

## Recommended Fix
Dedicated `ort-libs` artifact stage (arch-aware download, SHA-256 gate reusing root Dockerfile's pinned hashes) → `COPY --from=ort-libs` into builder (+ `ldconfig` + `ENV ORT_LIB_LOCATION=/usr/local/lib ORT_PREFER_DYNAMIC_LINK=1`, before both RUN steps) and into test-runtime (replacing the wget block). Rejected `.cargo/config.toml` copy (still needs the lib; imports the workstation-only `jobs = 6` cap).

## Scope for Secondary Asks
- IN: arch-aware download; SHA-256 gate (ADR-002 #4570); USAGE-PROTOCOL.md doc of build-from-source vs `IMAGE=` contract (doc-only commit).
- OUT (follow-up): making the from-source build run `tests/heal_roundtrip_972.rs` — needs model provisioning + skip-guard or it's a silent-skip false green; contract change, not portability fix.

## Missing Test
Cold-cache from-source build gate: `docker compose -f product/test/infra-001/docker-compose.yml build --no-cache` (x64 + arm64 via buildx) then smoke run. Warm cache and `IMAGE=` pulls masked the defect.

## Risk
Blast radius limited to infra-001 compose path; release pipeline untouched. Watch: COPY dereferences symlinks (preserve `libonnxruntime.so` naming). Confidence: high.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — ADR-002 #4570, lessons #4274/#5208/#4582, ADR #5186, procedure #840 (all applied); mcp__unimatrix__context_search — no prior builder-stage ORT lesson.
- Stored: entry #5722 "Multi-stage Docker builds compiling crates that link ort must provision ORT in the builder stage, not just runtime" via /uni-store-lesson (tagged caused_by_feature:infra-001).
