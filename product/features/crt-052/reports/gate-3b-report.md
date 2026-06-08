# Gate 3b Report: crt-052

> Gate: 3b (Code Review)
> Date: 2026-06-08
> Result: PASS (2 WARNs, neither blocking)
> Branch: feature/crt-052 (#689)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity (C1..C10) | PASS | Code matches validated pseudocode + ratified contract; signatures verbatim |
| 2. Architecture compliance (ARCH §2/§3/§4, ADR-001..009) | PASS | Component boundaries, four-return flow, exhaustive retention gate, Wave A/B severance all honored |
| 3. Interface implementation (signatures/types) | PASS | `TranscriptSnapshot` naming pin; readopt 2-arg; hold_on_drain 3-arg; `SessionLossInfo.dropped_candidates` present — all match the Gate-3a-ratified contract |
| 4. Test-case alignment (component test plans) | PASS | Every merge-gate test present and named per plan; 514 observe + 3746 server lib tests pass |
| 5. Code quality (build / anti-stub / unwrap / 500-line) | WARN | Builds clean; no stubs/TODO; no production `.unwrap()`. `distill_handler.rs` 806 lines total (≈299 production, rest inline tests) — over the 500 literal but production-thin |
| 6. Security (leak / fuzz / injection / audit) | WARN | AC-06 structural absence + metadata-only Debug + content-free audit verified; AC-V-FUZZ no-panic verified. `cargo audit` reports 1 pre-existing transitive CVE (rsa via sqlx-mysql) — NOT introduced by crt-052 |
| 7. Knowledge stewardship (impl agents) | PASS | All 11 implementation agent reports carry `## Knowledge Stewardship` with `Queried:` + `Stored:`/reasoned-none |

## Merge-Gate Verification

| Merge gate | AC / Risk | Status | Evidence |
|------------|-----------|--------|----------|
| Snapshot-and-release no-parse-under-lock | AC-01 / R-08 | PASS | `take_transcripts_for_feature` two-phase: Arc-clone under registry lock (released), `snapshot()` byte-copy under buffer lock. `test_seam_no_parse_under_lock` (source assertion, forbidden symbols select/reconstruct/parse/marker/Regex/from_utf8) + `test_concurrent_deltas_during_seam_consistent` (4-writer stress) |
| Four-return exhaustiveness | AC-05 / R-07 | PASS | One shared `distill_before_purge` helper at all four `result.is_ok()` returns (tools.rs:2110/2248/2950/3059); `test_exhaustiveness_fifth_return_fails` asserts exactly 4 purge sites == 4 distill == 4 attach; `test_distill_strictly_before_purge_at_each_return`. Distill strictly before purge confirmed |
| Content-leak | AC-06 / R-04, R-19 | PASS | `RetrospectiveReport` has NO candidate field (structural — `test_candidates_structurally_absent_from_memoized_report`); attach is assembly-level on `CallToolResult`; manual metadata-only `Debug` on `TranscriptSnapshot`/`HoleInfo`/`HeldBuffer` (R-19 tests assert sentinel bytes absent); `TranscriptCandidate` Debug MAY show text per architect ruling; content-free audit detail; zero tracing of content in pure modules |
| No-panic on adversarial JSONL | AC-V-FUZZ / R-10 | PASS | `jsonl.rs` skip-with-count, size/depth/utf8 bounds, never Err/panic; `corpus_tests` malformed corpus (truncated/non-UTF-8/unknown-type/embedded-NUL); `test_handler_fully_corrupt_snapshot_normal_response` at handler level |
| Single-reader seam | AC-V-SEAM / R-06 | PASS (1 WARN) | Exactly two public byte-returning methods (`contiguous_tail`, `snapshot()`); both route through private `snapshot_block`; `data` private — invariant by construction. `test_700_reuse_parses_snapshot_bytes_without_contiguous_tail` + all-four-metadata test. WARN: no explicit grep-style "only two readers" source-assertion test (invariant is structural, not asserted) |
| Continuity simulated lifecycle | AC-11 / R-01,02,03,05 | PASS | `continuity_simulated_lifecycle` drives 3 drains through production entry points with inter-drain deltas; asserts (a) cross-turn TURN1/2/3 content, (b) loud re-adopt match + fail-loud mismatch, (c) bounded held-count + 3 observable evictions, (d) TTL reclaim without review, (e) exactly-once audit per held session |
| Wave A/B revertability | R-11 | PASS | `distill/`, seam, handler, response-types have ZERO `use`/path reference to `transcript_hold.rs` (`test_distill_module_no_transcript_hold_reference`, `test_wave_a_handler_no_transcript_hold_dependency`); session.rs reaches hold only via `Option<Arc<dyn HeldBufferScan>>` (trait owned in session.rs, methods defaulted). AC-09 batch filter (`listener.rs`) untouched — no diff |
| Exhaustive retention match | AC-10 / R-18 | PASS | No wildcard arm at distill gate (distill_handler.rs:58) AND purge site (server.rs:562); `RetainDays(_)` neither distills nor purges; rejected at `validate()` |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
Every C1..C10 component matches its validated pseudocode and the Gate-3a-ratified contract:
- **C1 seam** (`session.rs:469`): `take_transcripts_for_feature(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>` — verbatim. Two-phase lock; Arc-identity dedup for registered∪held (R-13); poison-recovery (R-16).
- **C2 types/primitive** (`session_transcript.rs`): `TranscriptSnapshot { bytes, base_offset, high_water, elided_bytes, holes }`, `HoleInfo { start, end }`, `snapshot()`/`snapshot_block()`. Naming pin honored — no `SessionTranscriptSnapshot`.
- **C3 selection** (`distill/select.rs:27`): `select_candidates(bytes, session_id, base_offset, session_cap) -> Vec<TranscriptCandidate>` — verbatim; logical byte_offset (R-12); deterministic keep-earliest (R-15).
- **C4 types** (`types.rs`): all five types + `dropped_candidates` (ratified). `RetrospectiveReport` carries no candidate field.
- **C5 reconstruct** (`distill/reconstruct.rs:72`): signature verbatim; `topic_source` accessor returns `None` (inert for v1 — vnc-030 shipped SQL-column-only), stable-sort no-op never a filter (SR-06/R-14 preserved). Sound per leader-accepted decision.
- **C6 handler** (`distill_handler.rs:48`): `distill_before_purge(registry, feature_cycle, &observations, cfg) -> Option<TranscriptCandidatesSection>`; shared fallback predicate drives both routing and provenance label (no recomputation, ADR-007).
- **C7 retention gate** (`server.rs:561`): exhaustive match.
- **C8 hold** (`transcript_hold.rs`): `hold_on_drain` 3-arg, `readopt` 2-arg, `sweep_expired`, `purge_held_for_feature` — ratified arities. Empty-cycle buffers not held (#981).
- **C9 knobs** (`config.rs:1587+`): five knobs with serde defaults matching the brief (24 KB / 256 KB / 0.5 / 64 / 86400) + validate().

### 2. Architecture compliance — PASS
Data flow matches ARCH §3: distill (gate→snapshot→select/reconstruct→aggregate-cap→assembly-attach) strictly before purge, at all four success returns. ADRs honored: ADR-001 snapshot-and-release; ADR-002 single content reader by construction; ADR-003 pure observe module; ADR-004 candidates outside memoized struct; ADR-005 one-helper-four-returns exhaustive; ADR-006 hole/elision fallback trigger + topic_source soft; ADR-007 loss visibility; ADR-008 held store cap+TTL loud re-adopt; ADR-009 audit-shape move (no-consumer survey recorded CLEAN). server.rs's reference to `transcript_hold` is the C7/Wave B purge orchestration (a Wave B integration point, allowed); the R-11 invariant constrains Wave A *modules*, all of which are clean.

### 3. Interface implementation — PASS
All ratified contract additions implemented as ratified by `crt-052-agent-arch-ratify-report.md`: `SessionLossInfo.dropped_candidates: u64`, `readopt` 2-arg (supersedes ADR-008 1-arg), `hold_on_drain` 3-arg, `transcript_fallback_hole_fraction: f64` knob. Production `.unwrap()` absent; the two `.expect()` in `readopt` are infallible-by-construction (preceded by a confirmed `guard.get`); the one `.expect()` in markers.rs is on a statically-valid empty RegexSet.

### 4. Test-case alignment — PASS
514 observe lib tests + 3746 server lib tests pass. NFR-2 perf (`test_select_4mib_under_50ms`) passes. The single server failure (`http::token::tests::test_concurrent_creation_no_corruption`) is a named pre-existing flaky test unrelated to crt-052 (Stage 3c triage item per spawn prompt) — not a crt-052 file, not a regression.

### 5. Code quality — WARN (non-blocking)
Workspace build exits 0. No `todo!()`/`unimplemented!()`/`TODO`/`FIXME`/placeholder in production code. Scoped clippy reports ZERO warnings in any crt-052 new module (distill/, transcript_hold.rs, distill_handler.rs, session_transcript.rs); the 25 server-lib warnings are all in pre-existing untouched code and the workspace is not gated `-D warnings` (large pre-existing backlog).

**WARN (500-line rule):** `distill_handler.rs` totals 806 lines and `types.rs` totals 2010 lines, exceeding the literal 500-line file limit (`.claude/rules/rust-workspace.md`). Mitigating: `distill_handler.rs` production code is ≈299 lines (tests 301–806); `types.rs` is a pre-existing file extended additively (crt-052 production additions ≈80 lines, the rest pre-existing + inline tests). The brief's Constraint 10 explicitly exempts thin-wiring of pre-existing large files. Production logic per module is focused and well under 500. This is an inline-test idiom WARN, not a production-bloat FAIL.

### 6. Security — WARN (non-blocking)
- **Leak posture (R-04/R-19):** verified structurally (no memoized field), at-assembly attach, metadata-only Debug across all raw-buffer types, content-free audit detail, content-free mismatch diagnostic, zero content tracing in new paths.
- **Untrusted input (R-10):** parser operates on `&[u8]`, skip-with-count, bounded line/depth/text size, never Err/panic; verified at module and handler level.
- **No injection / path traversal:** the parser produces only `TranscriptCandidate` value types; no code/path/SQL constructed from content (confirmed by RISK §Security).
- **`cargo audit` — 1 vulnerability (RUSTSEC-2023-0071, rsa 0.9.10 via sqlx-mysql→sqlx 0.8).** PRE-EXISTING transitive dependency present on `main` (sqlx 0.8 unchanged), no upstream fix available, not in any crt-052-touched chain. crt-052's ONLY added dependency is `regex = "1"` (resolves 1.12.3, clean) — AC-13 "regex-class only, no new heavyweight dep" satisfied. WARN logged against AC-13's literal "cargo audit passes"; the gate concern (crt-052 introducing a vulnerable dep) is NOT met.

### 7. Knowledge stewardship — PASS
All 11 implementation agent reports (`crt-052-agent-3-*`, `crt-052-agent-arch-ratify`) contain a `## Knowledge Stewardship` section with `Queried:` entries (evidence of pre-implementation pattern queries — e.g. #3793, #4750, #3753, #4764, #4799, #981) and `Stored:` entries with real IDs (#4861, #4866, #4867, …) or a reasoned "nothing novel to store -- {reason}". No missing blocks; no unreasoned nothing-novel. No WARN.

## Rework Required

None blocking. Two WARNs for awareness (carry to Stage 3c / merge-readiness, neither blocks 3b):

| Item | Severity | Note |
|------|----------|------|
| `distill_handler.rs` (806) / `types.rs` (2010) exceed the literal 500-line file limit | WARN | Production portions are thin (≈299 / ≈80 new lines); remainder is inline tests / pre-existing. Optional: split test modules into sibling `_tests.rs` files (the pattern already used for transcript_hold/session_transcript) for strict compliance. |
| `cargo audit`: RUSTSEC-2023-0071 (rsa via sqlx-mysql) | WARN | Pre-existing, no fix available, not introduced by crt-052. Workspace-wide sqlx-mysql transitive pull; out of crt-052 scope. AC-13 (regex-only) met. |
| AC-V-SEAM "only two readers" source assertion is structural, not an explicit test | WARN (minor) | Invariant holds by construction (private `data`/`snapshot_block`); #700-reuse test proves the contract positively. Consider a grep-style source-assertion test for defense-in-depth. |

## Result Rationale

Every component's code matches its validated pseudocode and the Gate-3a-ratified contract (signatures, naming pin, the four ratified additions). All six merge gates plus R-11/AC-10 are implemented AND tested with named, faithful tests — including the AC-11 ≥3-drain continuity simulation driven through production entry points, the AC-05 four-return exhaustiveness regression guard, AC-06 structural leak absence, and AC-V-FUZZ handler-level no-panic. Lock discipline, content-opacity, and Wave A/B severance are correct by construction and asserted. Build is clean; crt-052 introduces zero stubs, zero production unwraps, zero clippy warnings, and one regex-class dependency. The two WARNs (inline-test file size; a pre-existing transitive CVE outside crt-052) are non-blocking under the gate rule "All checks PASS (WARNs acceptable)." **PASS.**

## Knowledge Stewardship
- Queried: read the binding sources (ARCHITECTURE + ADR-001..009, SPECIFICATION, RISK-TEST-STRATEGY, IMPLEMENTATION-BRIEF, gate-3a-report) and the implemented code/tests as the source of truth for this gate; no Unimatrix query needed (all source-of-truth feature-local).
- Stored: nothing novel to store -- gate findings are feature-specific (live in this report). The cross-feature observation worth watching ("a 500-line rule that counts inline test modules pushes test-heavy new modules over the limit; split test modules into sibling `_tests.rs` files") is a single occurrence here and the project already applies the sibling-file pattern selectively; not yet a 2+-feature lesson, so no `/uni-store-lesson` per stewardship rules.
