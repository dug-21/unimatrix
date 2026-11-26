# Alignment Report: nan-024

> Reviewed: 2026-11-25 (rerun against register D-001..D-068)
> Artifacts reviewed:
>   - product/features/nan-024/architecture/ARCHITECTURE.md (+ ADR-001..ADR-006)
>   - product/features/nan-024/specification/SPECIFICATION.md
>   - product/features/nan-024/RISK-TEST-STRATEGY.md (rev 6)
>   - product/features/nan-024/SCOPE.md, SCOPE-RISK-ASSESSMENT.md, ACCEPTANCE-MAP.md, IMPLEMENTATION-BRIEF.md, DECISIONS.md
> Vision source: product/PRODUCT-VISION.md; goal #5836 (personal-cloud)
> Per D-067, Unimatrix ADR ID differences between register and brief snapshot are not defects.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances `personal-cloud`; matches Principle 6 (JS/TS client for hooks) |
| Milestone Fit | PASS | Nanoprobes (build/deploy); no future-milestone capability built |
| Scope Gaps | PASS | AC-01..AC-16 and Goals 1-7 all traced to FR/AC/ADR/RTS |
| Scope Additions | WARN | Small unregistered extensions (W-4, W-5) |
| Architecture Consistency | WARN | ADR-003 exit-code wording (W-2); minor list omissions (W-6) |
| Risk Completeness | WARN | RTS header stops at D-066; R-40 scenario 3 beyond D-068 contract (W-3) |

Counts: PASS 3, WARN 3, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | none | Every SCOPE AC maps to SPEC AC, ACCEPTANCE-MAP row and RTS scenario |
| Addition | macOS/Windows pass-through message + exit 1 (FR-02, ADR-004) | Not a register entry; derived from D-032. Acceptable |
| Addition | OQ-6 audit extended to packed npm tarball and OpenCode plugin dist (FR-21a, brief, AC map) | D-035 says "repo, docs and installers". Broader than decided; low risk |
| Addition | R-40 scenario 3: `status` "flags duplicate" when project and per-machine entries coexist | D-068 / ADR-004 / FR-12g define only `legacy:` lines; no duplicate flag |
| Simplification | D-038 server-side signal deferred to #1026 | D-054; condition "if cheap" not met |

## Variances Requiring Approval

None. All findings are WARN.

## Findings (WARN)

- **W-1 Terminology drift: "portable" MCP.** Brief Goal: "`wire --harness opencode` keeps a portable retrieval MCP entry". D-046/D-060/D-063 make every MCP entry per-machine; "portable" is the dropped R5 term. Fix: "per-machine".
- **W-2 `~/.claude.json` parse-failure outcome.** ADR-003: "A file that fails to parse aborts the MCP leg with an error". FR-07j, AC-16 verification, RTS R-37 and brief Constraints: init "exit 1". SCOPE AC-16: "init aborts without overwriting". ADR wording permits continuing other legs with exit 0. Fix: ADR-003 states exit 1.
- **W-3 RTS coverage stamp / R-40.** RTS header "Covers decisions D-015..D-066"; D-067 (no impact) and D-068 (covered by R-40 scenario 2) not stamped. R-40 scenario 3 tests a duplicate flag no contract defines (see Additions). Fix: bump header to D-068; restate scenario 3 as "`status` prints the `legacy:` line; re-init leaves only per-machine".
- **W-4 SCOPE Goal 6 wording vs D-016/D-024.** Goal 6: "a redirect file ... or `UNIMATRIX_BINARY`, and both the binary and the Node client resolve from it". D-024: `UNIMATRIX_BINARY` governs the binary only. AC-14, FR-03, ADR-002 are correct; only the goal sentence drifts. Fix: "the binary resolves from either; the client from the redirect".
- **W-5 Brief inconsistencies.**
  - Risk Focus lists R-25 as Medium; RTS rates R-25 Low.
  - Resolved Decisions table: D-064 row lacks the ADR File cell (should cite ADR-003); D-068 row has an extra cell.
  - `status` field list places `mcp node:` after the hook lines with "(right after `trust:`)"; ADR-004 order is trust, mcp node, hook. Order is unambiguous only via the annotation.
  - "Alignment Status" section and the D-049/D-055 row describe the previous pass (D-001..D-063 / D-058). Stale after this report.
  - "Later entries win" omits D-066 > D-057 and D-063/D-064 amendments.
- **W-6 Minor list omissions (no contradiction).** ADR-001 Decision "init removes owned Unimatrix entries from project `.mcp.json` and project `.codex/config.toml`" omits `opencode.json` (D-064; covered in ADR-003). SPEC Domain Model "Scope" MCP list omits `~/.config/opencode/opencode.json` and project `opencode.json`, which FR-07i and FR-12d include.
- **SCOPE-RISK-ASSESSMENT.md** still references R5 (SR-01), 25 ms (SR-02) and "headless mcp-bridge unchanged". Historical input doc, superseded by D-017/D-046/D-052; RTS traceability resolves each. No action required.

## Detailed Findings

### Vision Alignment
Goal #5836 / `personal-cloud` ("one command"): local and cloud share one wiring form and resolve at runtime (ARCH System Overview; FR-07). Principle 6 ("JS/TS for hooks, the binary for MCP"): FR-01, FR-08, NFR-02 zero Rust hook processes. Principle 5 (graceful degradation) mirrored by fire-time outcomes (FR-12): silent outside a project, loud only on resolution failure. Principles 1-4, 7, 8: N/A (client-side wiring; no DB, no secrets stored in DB — credentials stay in `remote.json`). D-052 cross-repo leak fix supports `integrity` (data routing).

### Milestone Fit
Build/deploy wiring only. Deferred items correctly held out: OpenCode hooks (#998), dogfood (#1025), server signal (#1026), uninstall, macOS/Windows local mode, Gemini writer (SCOPE Non-Goals; SPEC NOT in Scope; brief NOT in Scope).

### Architecture Review
Register checks (later entry wins), all consistent:
- D-023/D-042/D-050/D-051: ADR-004 exit table — Linux not installed 0+hint; macOS/Windows 0 silent; cloned repo 0 silent; resolution failure 1; connection codes unchanged.
- D-043: `mcp-stdio` exit 1 inside a project on every OS, 0 outside (ADR-004).
- D-024/D-032: ADR-002 binary env > redirect > install, client redirect > install; binary only for local transport, Linux pass-through always.
- D-026/D-060/D-062/D-063/D-064: hooks `settings.local.json` and `~/.codex/hooks.json`; MCP per-machine; project `.mcp.json`, `.codex/config.toml`, `opencode.json` cleaned (ADR-003, ADR-006).
- D-027/D-028/D-041: absolute quoted shim path, byte-fixed, PowerShell unsupported (ADR-001).
- D-029/D-036: trust check incl. `trust.json` (ADR-002).
- D-045/D-052/D-053: predicate with `UNIMATRIX_PROJECT_ROOT` containment, hooks and bridge (ADR-004); delivery task listed.
- D-047/D-057/D-066: fallback rewrite on node re-record; `status` + preflight exit 1; spawn ENOENT accepted (ADR-001, ADR-004 `mcp node:`).
- D-065: foreign per-machine key left, warned, exit unaffected (ADR-003).
- D-068: `legacy:` lines in fixed order (ADR-004).
- D-034/D-035/D-048/D-058: Delivery Order. D-039, D-040, D-044, D-046: covered.
Exception: W-2, W-6.

### Specification Review
FR/AC cover every register decision above; SPEC AC-16 and FR-12g carry D-065 and D-068 text verbatim. Exit codes restricted to 0/1 (Constraints). NFR-05 fixture isolation honours D-009. Exception: W-6 Domain "Scope" list.

### Risk Strategy Review
40 risks; counts in Coverage Summary match the register (1/13/15/11). Every SR traced. D-060 risks (R-36..R-40), D-052 (R-34), D-053 (R-35), D-066 (R-25) covered. Failure Modes names the escalation path for a failed `.cmd` hook form (no register fallback). Exception: W-3.

## Knowledge Stewardship
- Queried: goal #5836 (personal-cloud) for success criteria.
- Stored: nothing novel — findings are feature-specific wording drift from successive register amendments.
