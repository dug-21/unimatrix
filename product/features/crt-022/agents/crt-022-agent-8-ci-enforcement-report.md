# crt-022 Agent 8: CI Enforcement — Report

**Agent ID**: crt-022-agent-8-ci-enforcement
**Component**: CI grep enforcement (AC-07)
**Wave**: Wave 4

---

## Summary

Added the CI enforcement step for AC-07: `spawn_blocking` must not appear at ONNX embedding inference call sites in `services/` or `background.rs`. Runs on every PR against `main`.

---

## Files Created

- `/workspaces/unimatrix/.github/workflows/ci.yml` — new PR-triggered workflow; runs on `pull_request` targeting `main`
- `/workspaces/unimatrix/scripts/check-inference-sites.sh` — enforcement script with 5 checks

---

## Implementation Notes

### Why embed-filtered approach

The spawn prompt suggested a plain `grep -r "spawn_blocking" services/ background.rs` check, but the current codebase has permitted non-inference `spawn_blocking` calls in those paths (`gateway.rs` registry writes, `usage.rs` DB batch writes, `search.rs` co-access boost). A plain grep would produce false positives.

The pseudocode's simpler alternative filters on the `"embed"` substring to isolate inference sites. This is the correct approach and matches what the pseudocode documents. The embed-filtered grep currently passes on the post-migration codebase.

### Script checks (5 total)

1. No `spawn_blocking` + `embed` in `services/` (inference sites)
2. No `spawn_blocking_with_timeout` + `embed` in `services/` (inference sites)
3. No `spawn_blocking` + `embed` in `background.rs` (inference sites)
4. No `AsyncEmbedService` in `async_wrappers.rs` (removal verification)
5. Exactly 1 `spawn_blocking` in `embed_handle.rs` (OnnxProvider::new guard, C-03)

### CI workflow

Only `release.yml` existed (tag-triggered only). Created `ci.yml` with a single `enforce-inference-sites` job. Kept it minimal — only the enforcement step, not full build/test (those run in release). Concurrency group cancels in-progress runs on force-push.

---

## Grep Verification

```
PASS — bash scripts/check-inference-sites.sh exits 0 on current codebase
```

All 5 checks pass on the post-migration codebase (other agents completed call-site migration in Waves 1–3).

---

## Self-Check

- [x] `cargo build --workspace` — not affected (no Rust code changes)
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in files added
- [x] Files within scope defined in brief (CI pipeline entry in Files to Create/Modify)
- [x] Script follows pseudocode logic (simpler alternative approach)
- [x] Grep verification: PASS

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` — not applicable for shell/CI work; skipped (no Rust crate patterns to query)
- Stored: nothing novel to store — the embed-filter grep pattern for distinguishing inference vs non-inference `spawn_blocking` in mixed files is useful, but it's already documented in the pseudocode. No runtime traps discovered (this is CI shell, not runtime Rust).
