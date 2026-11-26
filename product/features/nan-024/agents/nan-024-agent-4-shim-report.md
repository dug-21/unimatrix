# nan-024-agent-4-shim report

Unimatrix entries consulted: #5898 (ADR-005), #5901 (ADR-004), #4779, #5784 (briefing/search)

## Report
Files created:
- packages/unimatrix/lib/shim/unimatrix.sh
- packages/unimatrix/lib/shim/unimatrix.cmd
- packages/unimatrix/test/helpers/shim-fixture.js
- packages/unimatrix/test/shim-sh.test.js, shim-cmd.test.js, shim-superset.test.js

Tests: 29 pass / 0 fail (node --test, Linux; win32 behaviour case skip-guarded). Orphan audit PASS (top-level test/*.test.js auto-claimed by the `package` set; no runner edit needed).

Notes:
- sh: pre-check clauses (a) loosened headless, (d) ancestor `.git` file, (b) root-marker path-component match, (c) legacy store. Placeholder `@@PRENODE_CALL@@` retained; node-not-found path always runs the check. Marker/.git contents never evaluated. Trailing `/` on marker root is stripped.
- cmd: pre-check disabled per D-088; no subroutine, no UH/REMOTE_VARS/PRENODE placeholders (only `@@NODE@@`). Consequence: node-not-found on Windows prints hint and exits 1 even outside a project (cannot be silent without the check). No latency finding forced enabling it.
- No parenthesised block in .cmd (a node path with `(x86)` would break it); hint omits the recorded path for the same reason.
- Quirk (Wave 1): requiring `lib/hook-client/config.js` before `lib/project-state.js` breaks project-state (circular require -> destructured undefined -> isUnimatrixProject returns null). Test helper loads project-state first.
- install-store must substitute `@@NODE@@`, `@@UHOME@@`, `@@REMOTE_VARS@@`, `@@PRENODE_CALL@@` in sh; only `@@NODE@@` in cmd. REMOTE_VARS in the sh template is a fixed list; Node's headless check accepts any non-empty `UNIMATRIX_REMOTE_*` except HEADERS, so install-store should pass the full set of names.
- Superset e2e through real shim-entry.js not run (shim-entry is a sibling's file); strict-superset cases assert Node predicate null + shim exit 0 silent with stub node.

## Knowledge Stewardship
- Queried: context_briefing, context_search (patterns), ADR-005 -- no shim-specific prior patterns
- Stored: nothing yet (circular-require quirk reported to leader instead)
