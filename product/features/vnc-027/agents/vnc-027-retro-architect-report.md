# vnc-027 Retrospective — Architect

Feature: TS UDS hook client + hook-set reduction (F4a). PR #701. All three gates
(3a/3b/3c) PASS, zero gate rework. One post-gate fix (Windows CI). 7 ADRs.

## 1. Patterns

**No new pattern stored.** The two candidate cross-feature structures the briefing
named are already captured well:

- **Additive-wire-field ripple** (cap-before-allocate framing, additive
  `skip_serializing_if` field + new response variant, ~45 construction/match-site
  blast radius, exhaustive-match gotcha in observe.rs) → **#4831** (corrected from
  #4821). I folded the 35-cycle compile-thrash discipline into it (define field +
  variant, enumerate sites with `cargo test --workspace --no-run`, edit all ctor/
  match sites + ts-rs + observe.rs fallback BEFORE the first build). This IS the
  procedure the HOTSPOT telemetry points at.
- **accept↔Text content-negotiation coupling** → ADR-001 (#4802) + #4828.
- **Cap-before-allocate UDS framing / socket lifecycle** → ADR-003 (#4804) + #4824.

Storing a separate consolidated pattern would duplicate these. Skipped deliberately.

## 2. Procedures

**No new procedure entry.** The additive-Rust-wire-field workflow that the
`compile_cycles` HOTSPOT (35 cycles on wire|listener|hook|observe) maps to is now
captured as actionable how-to inside **#4831** rather than as a separate procedure,
to avoid a duplicate. #4831 carries the front-load-the-ripple discipline that
collapses the build-error-fix loop. The cross-language parity regen/run procedure
(#4790) and oracle-golden pipeline (#4789) from vnc-026 still hold unchanged.

## 3. ADR status

All 7 ADRs were validated by successful implementation + live Gate 3c re-runs. None
flagged for supersession.

| ADR | Entry | Status | Validation evidence |
|-----|-------|--------|---------------------|
| ADR-001 server-side preformatted accept+Text | #4802 | VALIDATED | live sync-trio parity; R-08 frozen-hook byte-identical; shared injection core single impl |
| ADR-002 SendResult adapter + transport selection | #4803 | VALIDATED | mapHookResponse realizes the mapping table; never-reject pinned; selection-once-per-spawn |
| ADR-003 socket lifecycle (flush-before-FIN) | #4804 | VALIDATED | live FNF large/truncation; settle-once; timers unref; p95 sync≈0.14ms |
| ADR-004 PreToolUse narrowed + SubagentStop opt-in | #4805 → amended **#4810** | VALIDATED | AC-08 opt-in/off/non-boolean/opt-out matrix; R-12 no-SubagentStop lifecycle. Original #4805 correctly deprecated; amendment added the opt-out-prune confirmation (gate-3a W3) |
| ADR-005 dual-limit size gate | #4806 | VALIDATED | self-test corpus; dual-limit triggers; size-gate-first merge order (git log) |
| ADR-006 FR-16 rekey TaskCompleted + pruneOffsets | #4807 → amended **#4811** | VALIDATED | canonical-event keying; Stop-negative assertable; pruneOffsets FNF-only. Original #4807 correctly deprecated; amendment pinned canonical-vs-frame-type keying |
| ADR-007 socketPath from projectHash | #4808 | VALIDATED | hash-fixture corpus (5 layouts + corrupt-worktree); R-05; empirical hash parity 0d62f3bf… |

**ADR-007 off-by-one note (assessed, no action needed):** the briefing flagged the
`dirname(dirname(stateDir))` prose as off-by-one. The Unimatrix ADR-007 entry
(#4808) does NOT contain that statement — the erroneous invariant lived only in
ARCHITECTURE/pseudocode prose. config.js correctly implements
`dirname(socketPath) === dirname(stateDir)` (both = `~/.unimatrix/{hash}`), and
**#4823** already records the corrected invariant prominently ("the ADR-007/
pseudocode prose … is WRONG; use the test-plan form"). The ADR entry is clean and
the downstream is protected; no ADR correction or supersession warranted.

**ADR-004 / ADR-006 deprecation is expected, not drift:** both originals show
`deprecated` because they were amended during design via context_correct (the
correct ADR lifecycle); the active versions #4810/#4811 are validated above.

## 4. Lessons

**#4832 (NEW, lesson-learned)** — Platform-specific features need platform
skip-guards on resource-bound tests AND a validator who reasons about the CI matrix,
not just the local dev OS. Root-caused the prime post-gate failure: all 3 gates PASS
on Linux, then windows-latest CI failed `listen EACCES` because the new UDS suites
bind real Unix sockets and Stage 3c execution + Gate 3c validation ran Linux-only.
Generalizable takeaway: platform-scoped resources (UDS/named-pipe/fs/signals) must
carry `process.platform` skip-guards from the start; the tester/validator must check
every OS in .github/workflows. Fix was test-only (commit 17fba5a2).

No other lessons — this was otherwise a clean, zero-rework delivery; manufacturing
additional lessons would dilute signal.

## 5. Stewardship actions

- **Corrected #4821 → #4831**: original stored with EMPTY tags metadata and leaked
  literal `</content>` / `<parameter>` tool-call fragments into the body. Restored
  the seven intended tags (+`compile-cycles`), cleaned content, folded in the
  compile-cycle discipline. Substance was high-value; defect was serialization-only.
- **Confirmed high-quality (no action):** #4820 (dual-limit size gate),
  #4822 (Layer-1 parity auto-discovery / two-file retire), #4823 (config no-network
  gate + ADR-007 invariant correction), #4824 (half-close UDS stub allowHalfOpen +
  chunk budget + FNF race), #4825 (default-bound-parameter DI for monkeypatch tests),
  #4826 (merge-settings opt-in/opt-out prune), #4827 (parity-corpus arm-key
  reconciliation), #4828 (UDS Layer 2 session-id split). All substantive with
  what/why/scope; distinct (the two parity-retirement entries #4822/#4827 sit at
  different layers — Layer-1 client corpus vs Rust generator arm-keys — not dups).
- **No deprecations beyond the expected design-time amendments** (#4805/#4807 already
  deprecated by their #4810/#4811 amendments).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search "vnc-027 …" + context_get #4802–#4808,
  #4820–#4828, #4810/#4811 lineage, and a cross-platform-CI search — confirmed no
  prior lesson covered the Windows CI-matrix gap; #4821 found defective.
- Stored: entry #4832 "Platform-specific features need platform skip-guards … CI
  matrix" via lesson-learned (the post-gate Windows failure); corrected #4821 → #4831
  (additive-wire-field ripple, malformed-content repair + compile-cycle discipline).
