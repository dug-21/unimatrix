# nan-004 Retrospective Architect Report

Agent: nan-004-retro-architect
Mode: retrospective (post-shipment knowledge extraction)
Feature: nan-004 -- Versioning & Packaging
PR: #221 (merged 2026-03-12)

## 1. Patterns

### Updated (stale entries corrected)

| Original ID | New ID | Title | Change |
|-------------|--------|-------|--------|
| #1160 | #1191 | Sync CLI Subcommand Pattern for unimatrix | Binary renamed unimatrix-server -> unimatrix. Added thin-subcommand variant (version, model-download in main.rs). Added nan-004 instances. |
| #1104 | #1192 | Procedure: Adding a Sync CLI Subcommand to unimatrix | Updated binary name, added thin subcommand path, added main_tests.rs extraction note, added 500-line limit warning. |

### New patterns stored

| ID | Title | Reason |
|----|-------|--------|
| #1193 | npm optionalDependencies Pattern for Platform-Specific Rust Binary Distribution | New infrastructure pattern -- first npm packaging in the project. Reusable for future platform targets. |
| #1194 | JS Shim Routing Pattern: JS-Handled Commands vs Native Binary Passthrough | New pattern for hybrid JS/native CLI routing. Reusable if more JS-handled commands are added. |
| #1195 | Prefix-Match Settings Merge Pattern for Shared JSON Config Files | Generic pattern for merging tool config into shared JSON files. Reusable for any tool that writes to settings.json. |
| #1196 | Workspace Version Synchronization Pattern (Cargo.toml as Single Source of Truth) | Cross-domain versioning (Rust + npm). Reusable for any multi-package release. |
| #1197 | Tag-Triggered Release Pipeline Pattern (Rust + npm via GitHub Actions) | CI/CD pattern for hybrid Rust+npm projects. Reusable as-is for future platform targets. |

### Skipped (one-off, not stored)

| Component | Reason |
|-----------|--------|
| C6 Postinstall (ONNX model download) | One-off: specific to ONNX model caching. No generic pattern beyond "postinstall exits 0 unconditionally." |
| C4 Init command (project wiring) | The init command's overall flow is project-specific. The reusable parts (settings merge, binary resolution) are stored as separate patterns. |
| C11 /release skill | The skill file is the procedure itself. Stored as procedure #1208. |

## 2. Procedures

| ID | Title | Status |
|----|-------|--------|
| #1192 | Procedure: Adding a Sync CLI Subcommand to unimatrix | Updated (corrected from #1104) |
| #1208 | Procedure: Creating a Unimatrix Release | New -- documents the /release skill workflow and CI pipeline |

No schema migration changes in nan-004. No new build/test process changes beyond the binary rename (handled in pattern corrections).

## 3. ADR Status

All 5 ADRs validated by successful implementation. None flagged for supersession.

| ADR | File | Unimatrix ID | Validated | Notes |
|-----|------|-------------|-----------|-------|
| ADR-001: Absolute Paths for Hook Command Resolution | ADR-001-hook-path-resolution.md | #1198 | Yes | All 7 hooks use absolute paths. Integration tests pass. |
| ADR-002: Binary Rename | ADR-002-binary-rename.md | #1199 | Yes | Rename complete. Operational lesson: broke hooks mid-delivery (see lesson #1205). |
| ADR-003: Init in JavaScript | ADR-003-init-in-javascript.md | #1200 | Yes | JS/Rust delegation model worked cleanly. 20 unit tests pass. |
| ADR-004: Settings Merge Strategy | ADR-004-settings-merge-strategy.md | #1201 | Yes | 22 unit tests. Note: ADR text has stale tee pipeline reference for UserPromptSubmit (implementation correctly omits tee). |
| ADR-005: Version Source of Truth | ADR-005-version-source-of-truth.md | #1202 | Yes | All 9 crates + 2 npm packages at 0.5.0. Static verification passes. |

Note on ADR-004 stale text: The ADR file (line 42) and ARCHITECTURE.md (lines 316-320) retain a reference to `UserPromptSubmit` retaining a tee pipeline. The implementation correctly does NOT use tee. This was flagged as WARN in gate 3a but not corrected because the pseudocode (the binding artifact) was correct. The stale text remains in the file but the Unimatrix ADR entry (#1201) documents the correct behavior.

## 4. Lessons

| ID | Title | Source |
|----|-------|--------|
| #1203 | Gate Validators Must Check All Files in One Pass to Prevent Cascading Rework | Gate 3b cascading file-size failure (main.rs then init.test.js) |
| #1204 | Test Plan Must Cross-Reference Pseudocode for Edge-Case Behavior Assertions | Gate 3a C3 pseudocode/test-plan contradiction |
| #1205 | Binary Renames Must Be Atomic with Hook/Config Infrastructure Updates | Binary rename broke hook feeds mid-delivery (human-reported) |
| #1206 | MCP Tool Permission Errors on context_store Block Knowledge Stewardship | 5 failed ADR store attempts during delivery (-32003 permission) |
| #1207 | Node.js Built-in Test Runner Requires Explicit Imports (Not Globals Like Mocha/Jest) | Gate 3b merge-settings.test.js missing node:test import |

## 5. Retrospective Findings

### Hotspot-derived analysis

| Hotspot | Assessment | Action |
|---------|-----------|--------|
| permission_retries (context_store, 5 retries) | Confirms #1164 lesson from nan-002. Same root cause: missing allowlist entry. | Stored lesson #1206 with MCP-specific guidance. |
| compile_cycles (16 cycles, 9 clusters) | Lower than nan-002 (60 cycles) despite larger feature. Confirms #1165 lesson is being partially adopted. | No new lesson -- existing #1165 covers this. |
| sleep_workarounds (4 instances) | Agents used sleep to wait for file operations (git index.lock, package creation). | No new lesson -- existing retrospective recommendation (use run_in_background) is sufficient. |
| context_load (156KB before first write) | Expected for a design session. Protocol and agent definition files must be read before any design work. 115 distinct files is high but justified by 11-component feature touching 94 files. | Not a problem -- design sessions have inherently high context load. |
| cold_restart (302min gap, 34 re-reads) | Session timeouts forced context reconstruction. | No actionable lesson -- session timeouts are environmental. |
| session_timeout (5h, 2.5h gaps) | Long gaps between sessions. | Environmental -- not actionable. |

### Baseline outlier notes

| Metric | Value | Assessment |
|--------|-------|-----------|
| total_tool_calls: 742 (1.7 sigma) | High but proportional to 11-component feature (largest yet). ~67 calls per component is reasonable. | Expected for scope. |
| parallel_call_rate: 0.7 (1.7 sigma positive) | Good. Agents used parallel tool calls effectively. | Positive signal. |
| context_load: 155.7KB (1.7 sigma) | Design session with 11 components requires reading many files up front. | Expected for scope. |
| session_hotspot_count: 4 (1.7 sigma) | More hotspots than usual, but most are environmental (timeouts, cold restarts). | Acceptable. |

### Recommendation actions

| Recommendation | Action Taken |
|----------------|-------------|
| Add common build/test commands to settings.json allowlist | Already documented in lesson #1164 (nan-002). nan-004 confirms the need -- 15 Bash permission retries. Not actioned here (infrastructure change, not knowledge). |
| Use run_in_background instead of sleep polling | Documented in retrospective #1190. No new lesson needed. |
| Consider incremental compilation or targeted cargo test | Already documented in lesson #1165 (nan-002). nan-004 shows improvement (16 vs 60 cycles). |

## Knowledge Stewardship

- Queried: 8 searches across pattern, procedure, convention, lesson-learned, and decision categories
- Found and corrected: #1160 -> #1191 (pattern), #1104 -> #1192 (procedure) -- stale binary name
- Stored: 5 new patterns (#1193-#1197), 1 new procedure (#1208), 5 ADRs (#1198-#1202), 5 lessons (#1203-#1207)
- Total: 16 new Unimatrix entries, 2 corrections
