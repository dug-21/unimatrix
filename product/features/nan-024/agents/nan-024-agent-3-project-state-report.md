# nan-024-agent-3-project-state Report

Unimatrix entries consulted: #5901 (ADR-004), briefing (#3876, #5781, #5305 - not relevant beyond #5781 orphan-suite lesson)

## Report
Files: packages/unimatrix/lib/project-state.js; test/project-state.test.js; test/project-parity.test.js; test/helpers/project-state-fixtures.js (shared fixture table).
Tests: 86 (61 pass, 0 fail, 25 skipped = shim-entry parity rows until lib/shim-entry.js exists and exports isUnimatrixProject/effectiveEnv).
Runner: test/run-hook-client.js claims top-level test/*.test.js by directory listing (package set + orphan audit) - no edit needed.
OQ-3: check-zero-deps.js accepts relative requires (lib/hook-client/config.js -> ../project-state.js OK). Size gate counts only lib/hook-client/**; project-state.js lives in lib/ and is not counted. No gate change needed.
Gate status now: check-hook-client-size FAILS (stripped 114927/113639, raw 202903/201399) from other agents' uncommitted hook-client edits (config.js, index.js, last-hook.js), not this component. Needs human cap decision or trimming.
Note: circular require. project-state requires hook-client/config.js; when config.js requires ../project-state.js it must do so lazily (inside resolve) or destructure after module.exports is set.
Design notes: isInstalled throws TypeError without {env, home} (D-080). HEADLESS_VARS hardcoded (not imported from remote-resolve.VARS) to avoid a lib -> hook-client cycle. effectiveEnv strips UNIMATRIX_BUNDLE and all UNIMATRIX_REMOTE_* incl. HEADERS.

## Knowledge Stewardship
- Queried: context_briefing, context_get #5901 - ADR-004 binding spec
- Stored: nothing novel to store -- circular-require note is feature-specific
