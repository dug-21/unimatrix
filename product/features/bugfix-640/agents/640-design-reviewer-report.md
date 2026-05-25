# Agent Report: 640-design-reviewer

## Task
Design review of proposed bugfix for integration test directory leaks (GH Issue #640).

## Verdict
APPROVED WITH NOTES — one blocking finding requires amendment before implementation.

## Key Finding
The investigator's proposed fix (pass `base_dir` TempDir to test helpers) only addresses ~5 direct `ensure_data_directory` calls. The majority of leaks come from production functions (`run_export`, `run_import`, `run_snapshot`) that hardcode `base_dir=None` internally and are called by ~30+ integration tests. The fix must include `_with_base` variants of these production functions (following the existing `health.rs` `run_with_base` pattern).

## Findings Summary
1. BLOCKING: Production functions still leak — need `_with_base` variants
2. NON-BLOCKING: eval/ files also leak (6 additional call sites) — track for follow-up
3. NON-BLOCKING: Meta-assertion recommended in helpers
4. N/A: No hot-path, security, or test-isolation concerns

## Output
- Design review posted as comment on GH Issue #640
- https://github.com/dug-21/unimatrix/issues/640#issuecomment-4536496489
