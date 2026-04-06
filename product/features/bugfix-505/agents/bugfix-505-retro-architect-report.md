# Retrospective Report: bugfix-505-retro-architect

Feature: bugfix-505 / crt-043
Agent: bugfix-505-retro-architect (uni-architect)
Date: 2026-04-06

---

## 1. Patterns

**new**: none  
**updated**: none  
**skipped**:
- `set_ready_for_test` for service handles — already covered by #4174 (lesson: fire-and-forget spawn paths require stub provider) and #4175 (pattern: inline mocks for embed in other crates). Both stored by fix agent during the session.
- Fire-and-forget spawn test coverage — covered by #4174 and procedure #2326 (fire-and-forget async test strategy). No new entry.

---

## 2. Procedures

**new**: none  
**updated**: none  
**skipped**:
- #1372 (include distilled API signatures in spawn prompts) — the 38 compile cycles in this session were caused by discovering the `#[cfg(any(test, feature = "test-support"))]` gate on `unimatrix_embed::test_helpers`, not by missing third-party API signatures. That failure mode is already covered by #4175 (inline mock pattern). The #1372 lesson addresses a different root cause (loading cargo registry source files for rmcp-style APIs); no update warranted.

---

## 3. ADR Status

N/A — no new ADRs were created during this bugfix. Validated: correct, the fix was pure test infrastructure with no architectural decision involved.

---

## 4. Lessons

**stored**:
- **#4177** (new): "Tautological assertion slips past authoring but is caught at gate — assertion exists but is structurally always-true"
  - `assert "error" not in str(result).lower() or result is not None` in test_lifecycle.py was vacuously true whenever `result is not None` (always the case on success). Gate and security reviewer caught it; rust-dev and tester did not.
  - Distinct from #3548 (assertion absent from test body) — here the assertion is present but cannot fail.
  - Tags: testing, assertion-quality, gate-3c, tautology, python, security-review, source:retrospective, bugfix-505

**skipped**:
- Compile-cycle lesson (38 cycles from cross-crate mock discovery) — covered by #4173 (complete all struct/field changes before first compile) and #4175 (inline mock pattern explaining the `test-support` feature gate). The specific failure was discovering the feature gate, not iterative field-level compilation. No separate entry needed.
- Assertion-correctness gate check (candidate B: should the gate enforce load-bearing assertions?) — not actionable enough at the gate tooling level. The gate reviewer caught it as a WARN via manual inspection; no structural check could reliably detect tautological boolean logic. Stored as a lesson for human reviewers (#4177) rather than a gate procedure change.

---

## 5. Retrospective Findings

### Hotspot Notes

**compile_cycles (38)**: Root cause was cross-crate mock discovery, not field-level iteration. The agent had to determine empirically that `unimatrix_embed::test_helpers` is inaccessible without enabling `test-support` in Cargo.toml, producing `E0433` errors across multiple attempts. Now captured in #4175 — future rust-dev agents get this upfront and avoid the discovery loop entirely.

**cold_restart (72-min gap + 40 re-reads)**: Structural to the swarm handoff between fix and testing phases. No generalizable action beyond existing protocol; the re-reads confirm the tester correctly loaded context independently.

**context_load (155 KB cold for security reviewer)**: Expected for a cold-start security review of two large Rust files (embed_handle.rs, listener.rs). No action — this is the correct behavior for a security agent that must read changed files from scratch.

**file_breadth (25 files)**: The investigator phase accessed many files to understand EmbedServiceHandle's state machine and the NLI precedent. This is inherent to investigation; not a sign of scope creep.

**tool_failure_hotspot (9 Read failures in testing phase)**: Pre-existing issue with large file offsets in listener.rs (7600+ lines). Not actionable in this retrospective.

### Recommendation Actions

The retro's compile_cycles recommendation ("batch field additions before compiling") is already well-covered by #4173 (updated most recently 2026-04-06) and #4175. No new procedure entry is warranted. Future architect and investigator spawn prompts for `unimatrix-server` should reference #4175 to short-circuit the cross-crate test_helpers discovery cost.

---

## Unimatrix Entries Stored This Session

| ID | Category | Title |
|----|----------|-------|
| #4177 | lesson-learned | Tautological assertion slips past authoring but is caught at gate |

Previously stored (by fix/verify agents, not this agent):
| #4174 | lesson-learned | Service handle fire-and-forget spawn paths require stub provider to be unit-testable |
| #4175 | pattern | unimatrix_embed::test_helpers is NOT available in other crates' #[cfg(test)] |
