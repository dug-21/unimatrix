# Scope Risk Assessment: nan-004

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | ONNX runtime (`ort =2.0.0-rc.9`) is pinned to a release candidate; shared library must match per-platform. CI native build avoids cross-compile, but future darwin targets will hit this. | High | Med | Architect should isolate ONNX packaging as a separate build step; validate shared lib bundling in the linux-x64 package before designing multi-platform. |
| SR-02 | `tokenizers` crate with `onig` feature pulls native Oniguruma C dependency. Static vs dynamic linking choice affects binary portability across glibc versions. | Med | Med | Specify musl or minimum glibc version in CI build matrix; test binary on a clean Ubuntu LTS container. |
| SR-03 | Patched `anndists` dependency (`patches/anndists`) requires the patch dir to be available at build time. CI checkout must include the patch, and npm package must bundle the resulting linked binary. | High | Low | CI workflow must verify patch presence before build; add a pre-build assertion. |
| SR-04 | Rust 1.89 (edition 2024) is newer than default GitHub Actions runner toolchain. CI will fail silently or with confusing errors if toolchain setup is missed. | Med | High | Use `dtolnay/rust-toolchain` action with explicit `1.89` pin as first CI step. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Boundary between nan-004 (mechanical wiring) and nan-003 (interactive onboarding) is clear in scope but blurry in user experience. If `npx unimatrix init` fails silently, users may blame nan-003 skills. | Med | Med | Init command must produce explicit pass/fail validation output (AC-07) with actionable diagnostics. |
| SR-06 | Binary rename from `unimatrix-server` to `unimatrix` (Resolved Q1) is a breaking change for existing hook configurations in this repo and any early adopters. | Med | High | Architect should design the rename as a separate early deliverable; existing hooks in `.claude/settings.json` must be updated atomically. |
| SR-07 | Scope includes both packaging infrastructure (npm packages, CI pipeline) and a CLI tool (`npx unimatrix init`). These are two distinct deliverables with different risk profiles. | Low | Med | Architect should consider phased delivery: package structure first, init command second. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | `settings.json` merge logic (AC-04, AC-08) must handle arbitrary existing user configurations without corruption. JSON merge on nested structures with arrays is error-prone. Historical lesson #367 confirms two-phase patterns reduce rework. | High | Med | Spec should define exact merge semantics with edge cases: empty file, malformed JSON, conflicting hook matchers, permission blocks. |
| SR-09 | Hook commands use bare `unimatrix` name relying on PATH resolution. Shell hooks execute outside npm/npx context where `node_modules/.bin/` may not be on PATH. | High | High | Architect must decide: absolute paths (breaks on project move) vs PATH shimming (requires shell profile modification) vs wrapper script. Scope says PATH-based but this needs validation. |
| SR-10 | `postinstall` ONNX model download (~90 MB) in corporate/CI environments may be blocked by firewalls or proxies. Graceful fallback exists but first-run latency becomes a surprise. | Med | Med | Document network requirements; consider bundling a smaller model or making model path configurable. |

## Assumptions

- **Binary size (~20 MB) is acceptable for npm distribution** (SCOPE line 34). If ONNX shared library must be bundled alongside, total could exceed 100 MB. Validate actual package size.
- **Schema migration is production-ready** (SCOPE line 87). True for sequential upgrades, but untested for version gaps (e.g., v5 to v11 in one jump). Version gap migration should be tested.
- **`node_modules/.bin/` PATH resolution works for shell hooks** (Resolved Q5). This assumption is the highest-risk item — hooks run in shell context, not npm context.
- **Single workspace version lockstep** (SCOPE line 169). All 9 crates move together. If any crate has independent consumers in the future, this becomes a constraint.

## Design Recommendations

1. **SR-09 is the top risk.** The architect must prototype hook PATH resolution in a real Claude Code session before committing to the bare-name approach. If it fails, the entire hook wiring strategy changes.
2. **SR-08 merge logic** should be specified with a test matrix of edge cases in the specification, not left to implementation discretion.
3. **SR-01/SR-02 native dependency bundling** should be validated with a throwaway CI build early — before designing the full release pipeline. A binary that compiles but segfaults on a clean system is the worst failure mode.
