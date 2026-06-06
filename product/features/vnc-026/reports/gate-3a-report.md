# Gate 3a Report: vnc-026

> Gate: 3a (Design Review)
> Date: 2026-06-06
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 12 components match ARCHITECTURE decomposition; ADR-001..008 each traceable to pseudocode; one documented WARN-level refinement of ADR-008 (effectiveEnd) |
| Specification coverage | PASS | FR-01..FR-26 each map to pseudocode; no scope additions found |
| Risk coverage | PASS | All 20 risks (R-01..R-20) mapped in test-plan/OVERVIEW.md to concrete scenarios; edge cases, integration risks, security table, and failure modes from the strategy all carried into plans |
| Interface consistency | PASS (1 WARN) | Shared types/helpers (OVERVIEW.md) used consistently across delta/queue/build-request/index/transport; minor signature-name drift in test-plan "Concrete Assertions" prose |
| Knowledge stewardship | PASS | Pseudocode agent (read-only): Queried present. Test-plan agent: Queried + "nothing novel" with reason. Architect: Stored #4751/#4758. Risk strategist: Queried + reasoned declines |
| Spawn-prompt check: spaced-path regex fix (WARN 2) | PASS | Fixed in pseudocode/init-remote.md §1 (quoted command builder + new 3-alternation regex + pos/neg table); test-plan/init-remote.md `test_pattern_table` covers it |
| Spawn-prompt check: ADR-008 end-anchored elision + 4 pinned Layer-2 assertions | PASS (1 WARN) | delta.md pseudocode end-anchors; test-plan/delta.md `test_l2_elision_mid_session` enumerates all four pinned assertions; WARN on effectiveEnd vs file_len wording |
| Spawn-prompt check: ADR-002 literal templates | PASS | transform.md: single JSON.stringify on inner scalar, literal template, grep-gate enforced in plan |
| Spawn-prompt check: ADR-004 never-queued + uniform advance + amended AC-15 | PASS | delta.md uniform advance + no-enqueue; queue.md structural guard; test plans assert offset-non-advance + NO queue file |
| Spawn-prompt check: unknown-stdin-field parity | PASS | index.md parseHookInput preserves unknown keys (insertion order, null/{}거 distinction); build-request plan mandates `unknown-stdin-fields` corpus case |
| Spawn-prompt check: integration harness section | PASS | test-plan/OVERVIEW.md "Integration Harness Plan (infra-001)" — smoke-only with sound rationale (C-07, zero server production changes) |

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS

Every module in the ARCHITECTURE Component Breakdown has a pseudocode file with matching
responsibility, oracle reference, and constants:

- `index.md` mirrors `hook.rs::run()` step-for-step; the three documented deviations
  (PreCompact server-side restore, remote Ping no-stdout, ADR-003 queue format) are recorded
  in pseudocode/OVERVIEW.md and accepted in IMPLEMENTATION-BRIEF Stage 3a sign-offs.
- Sync/FNF classification on request type matches `hook.rs:244-251` exactly (Integration
  Surface row "Sync/FNF split").
- Hard-limits table in pseudocode/OVERVIEW.md matches the ARCHITECTURE limits row verbatim
  (stdin 1 MiB, body 1 MiB, 64 KiB = 48+12 KiB, MIN_QUERY_WORDS 5, MAX_GOAL_BYTES 1024,
  12,000 B window, MAX_PRECOMPACT_BYTES 3000) plus two oracle constants
  (TOOL_RESULT_SNIPPET_BYTES 300, TOOL_KEY_PARAM_BYTES 120) sourced from transcript_block.rs.
- ADR traceability: ADR-001 → parity-corpus.md (generator, manifest, drift gate);
  ADR-002 → transform.md; ADR-003 → queue.md + state.md (all mini-spec values present:
  500/5 MiB/24 h, 32 frames/256 KiB, O_EXCL `{ts_ms}-{pid}-{seq}.json`, 0700/0600, sanitized
  keys, 7-day offset prune); ADR-004 → delta.md + queue.md; ADR-005 → state.md breadcrumb +
  transport timeouts (750/2,000/3,000); ADR-006 → config.md (pinned env names, single-file
  resolution, split-brain guard); ADR-007 → delta.md/index.md (`Promise.allSettled`,
  separate POST, fstat gate); ADR-008 → delta.md (end-anchored declaration).
- Config-overridable timeout keys (`unimatrix.remote.timeouts.{connect_ms,sync_ms,fnf_ms}`)
  match the delivery-leader pin in IMPLEMENTATION-BRIEF.

WARN A (see Interface Consistency) is the only deviation worth recording.

### 2. Specification Coverage
**Status**: PASS

FR-by-FR walk: FR-01 (index.md: fd-0 read, 1 MiB cap, defensive parse incl. serde
whole-parse-failure parity), FR-02 (build-request.md full port incl. MIN_QUERY_WORDS,
is_bash_failure as_i64/as_bool parity, MultiEdit fan-out, context_cycle exact-match
interception, validate_cycle_params port with byte-vs-codepoint length distinctions),
FR-03 (transport-http.md headers/timeouts/raw session_id), FR-04 (transform.md),
FR-05 (index.md exit-0 + stderr one-liner + breadcrumb), FR-06 (config.md full ADR-006
matrix incl. partial-pair → `auth`), FR-07/FR-08 (delta.md), FR-09/FR-10 (index.md
runSync/runFireAndForget), FR-11 (rewrite guard, sanitized keys, atomic rename, monotonic),
FR-12–FR-16 (queue.md + state.md, incl. offset-delete-on-SessionClose in index.md),
FR-17–FR-20 (init-remote.md, incl. .mcp.json/binary skips and gitignore warning),
FR-21 (init-remote.md §2, blast radius confined), FR-22–FR-26 (parity-corpus.md).
No pseudocode implements anything outside scope (the `bin/unimatrix.js` flag plumbing is
implied by FR-17 and adds no behavior).

**Delivery Notes honored**: no test plan reopens the timeout defaults (test-plan/
transport-http.md explicitly says "do not flag vs NFR-02"), gate-note 1 stays closed
(index plan asserts the mechanism via grep-gate and tests Windows execution per R-14),
env-var names are the pinned ones (config.md constants), AC-15 is evaluated against the
amended letter everywhere.

### 3. Risk Coverage
**Status**: PASS

test-plan/OVERVIEW.md maps all 20 risks to plan files; spot-verified depth:

- R-01/R-02 (Critical/High): ADR-001 inventory is the Layer-1 case list; corpus MANIFEST
  with per-arm mapping + Rust-side branch-coverage assertion (parity-corpus plans).
- R-14 (Critical): fd-0 piped/empty/>1 MiB on Windows runners, OS-matrix CI, backslash
  walk, chmod no-op, "spawn produces a POST on Windows" smoke — all present (index, config,
  state, queue plans).
- R-04: mid-2/3/4-byte trims, span-inside-one-char, 10-spawn growth replay, property test
  `concat(shipped) == contiguous prefix`, persisted-VALUE assertions (delta plan).
- R-06: end-anchored frame-shape assertion explicitly "NOT the span start", plus the four
  pinned Layer-2 server-state assertions, marked gate-binding (delta plan).
- R-09/R-10/R-11: full FR-06 matrix with no-network proof per row; breadcrumb
  transition/content-free/write-failure matrix; regex pos/neg table + 4-shape re-run matrix
  + double-fire count.
- R-20: drift-check non-vacuity has three independent guards (run-marker, ≥1-test-ran
  assertion, corrupt-golden meta-verification) — directly applies lesson #4452.
- Strategy edge cases (stdin 1 MiB ± 1, TOCTOU, corrupt offset, URL forms, no-HOME,
  same-ms collisions, binary transcript, sync 200 empty body) and all security-table rows
  (traversal corpus, token-leak scans, no-delta-in-queue directory scan, gitignore warning)
  are present in the corresponding plans.

### 4. Interface Consistency
**Status**: PASS (1 WARN + minor notes)

The shared-type and wire-serialization rules in pseudocode/OVERVIEW.md (omit-vs-null
encoding per frame, `implantEvent` helper, flattened RecordEvent) are used consistently by
build-request.md, delta.md, and queue.md (whose delta guard correctly checks the flattened
top-level `event_type`). delta→transport `bodyBuf` option, index→transform
`(reqSource, SendResult)`, and config→state `stateDir/urlHost` handoffs all line up.

**WARN A — ADR-008 `effectiveEnd` refinement vs `file_len` letter.**
pseudocode/delta.md anchors the elided frame at `effectiveEnd` = `file_len` backed off ≤3
bytes when the file ends mid-UTF-8-char, and advances the offset to `effectiveEnd` — a
principled extension of the FR-07 boundary-trim rule to the elision path, explicitly
documented ("Pinned F2 consequences hold with fileLen ≡ effectiveEnd"). ADR-008, FR-08,
AC-07, and test-plan/delta.md state the formula as `offset = file_len − bytes.length` and
advance-to-`file_len`. Not a contradiction (effectiveEnd == file_len whenever the file ends
on a boundary — the JSONL norm), and pseudocode scenario 4 names the mid-char-file-end case.
**Action for Stage 3b/3c (no rework needed now)**: implement and assert per the pseudocode
(effectiveEnd), and ensure `test_elided_frame_end_anchored` / the Layer-2 helper read
"file_len" as "effectiveEnd" in the ≤3-byte edge case so the test doesn't encode the
unreachable literal.

**Minor notes (no action gate-side)**:
- Test-plan "Concrete Assertions" prose uses slightly different names than pseudocode
  (`send(request,{sync,config})` vs `post(config,frame,opts)`; `resolveConfig(input)` vs
  `resolve(cwd)`; normalize returning `{canonical, provider?}` vs the `[canonical, provider]`
  tuple). Pseudocode is the authority for Stage 3b signatures; the plans' behavioral
  assertions are unaffected.
- index.md uses stderr-line classes `config`/`parse`/`internal` beyond the five breadcrumb
  classes. FR-05's enum pins the breadcrumb (which stays in-enum); stderr vocabulary is
  unconstrained. Acceptable.
- ACCEPTANCE-MAP.md AC-15 row still carries "variance pending human approval" — stale
  relative to IMPLEMENTATION-BRIEF Delivery Note 1 (ACCEPTED, SCOPE amended). The test plans
  already honor the amended letter; Gate 3c must read AC-15 from the brief, not the map's
  stale note. Recommend a one-line map touch-up during Stage 3b (cosmetic).

### 5. Knowledge Stewardship Compliance
**Status**: PASS

- `agents/vnc-026-agent-1-pseudocode-report.md`: `## Knowledge Stewardship` present with
  `Queried:` entries (briefing #4714/#4720/#4743/#4740/#4739/#4758/#4751/#4306 +
  context_search #4703). Read-only agent — Queried requirement satisfied.
- `agents/vnc-026-agent-2-testplan-report.md`: `Queried:` entries + `Stored: nothing novel
  to store — {reason given}`. Satisfied.
- Design-phase active-storage agents: architect report has `Stored:` entries (#4751,
  #4758); risk-strategist report has `Queried:` + reasoned "nothing novel" declines.
  Satisfied.

## Rework Required

None.

## Spawn-Prompt Specific Checks (evidence)

1. **Spaced-path ownership regex (WARN 2)** — RESOLVED. pseudocode/init-remote.md §1:
   `buildHookClientCommand` quotes spaced paths; new pattern
   `/(^|[\s"'/\\])node(\.exe)?\s+("[^"]*[/\\]hook-client[/\\]index\.js"|'[^']*[/\\]hook-client[/\\]index\.js'|\S+[/\\]hook-client[/\\]index\.js)\s/`
   handles quoted/bare forms and both separators; 9-row pos/neg table committed as the R-11
   unit fixture; require.resolve output shapes documented per platform.
   test-plan/init-remote.md `test_pattern_table` + end-to-end spaced-path matrix cover it.
   **No open gate notes remain.**
2. **ADR-008 geometry** — delta.md buildDeltaFrame elided arm declares
   `offset = effectiveEnd − byteLength(bytes)` with the explicit "NEVER offset = last"
   prohibition; the four pinned Layer-2 assertions appear verbatim and gate-binding in
   test-plan/delta.md `test_l2_elision_mid_session` (hole-behind-content, high_water,
   seam-crossing contiguous_tail, no NULs) and the helper is owned by parity-corpus.md.
   See WARN A for the effectiveEnd wording.
3. **ADR-002** — transform.md uses the exact literal template from the brief; single
   `JSON.stringify` on the inner scalar; plan enforces via source-text grep-gate.
4. **ADR-004 + amended AC-15** — delta.md: failure → no advance, no enqueue; uniform
   advance `offset + byteLen`. queue.md: structural transcript_delta guard +
   `test_delta_never_queued` directory scan. test-plan/delta.md
   `test_delta_failure_no_advance_no_queue` asserts the amended letter; OVERVIEW binding
   decision 1 forbids reopening.
5. **Unknown-stdin-field parity** — index.md parseHookInput: unknown keys → `extra` in
   insertion order; `extra = {}` on clean parse, `null` on parse failure (Rust flatten
   parity); build-request plan mandates corpus case `unknown-stdin-fields`.
6. **Integration harness** — test-plan/OVERVIEW.md §"Integration Harness Plan (infra-001)":
   smoke-only, rationale sound (C-07: zero server production changes; HTTP client surface
   outside infra-001 scope), triage protocol referenced.

## Knowledge Stewardship
- Stored: nothing novel to store — gate result is PASS with feature-specific WARNs only;
  no recurring cross-feature gate-failure pattern observed.
