# vnc-025 Test Strategy Overview

Inputs: RISK-TEST-STRATEGY.md (R-01..R-15), SPECIFICATION.md (FR-01..21, NFR-01..09,
AC-01..13), ACCEPTANCE-MAP.md, ADR-001..008. All tests are Rust `cargo test` unless marked
integration-harness. The feature **ships dark** — every behavior except the empty-buffer
PreCompact path is verified test-only (Constraint 8).

## Test Levels

| Level | Where | What |
|-------|-------|------|
| Unit | `infra/session_transcript.rs` tests | `apply_delta` state machine, holes, ring-tail, arithmetic, Debug opacity |
| Unit | `uds/transcript_block.rs` tests | Moved extraction suite + `from_bytes` + constant pins |
| Component | `infra/session.rs` tests | Registry methods, drain/sweep shapes, poison recovery, concurrency |
| Integration (Rust) | `uds/listener.rs`, `mcp/tools.rs`, `infra/config.rs` tests | Dispatch arms, purge audit, cycle-review clear, config→cap chain |
| Integration (harness) | `product/test/infra-001` | MCP-surface regression gate (smoke + tools + protocol + lifecycle) |
| Static gates | grep/review | AC-12 content-leak gate, ADR-008 arithmetic gate, `listener.rs:1009` byte-identity |

## Risk-to-Test-Plan Mapping

| Risk | Priority | Test plan file(s) | Hard gate? |
|------|----------|-------------------|-----------|
| R-01 merge correctness | Critical | transcript-buffer.md §1 | Permutation harness mandatory |
| R-02 offset arithmetic | High | transcript-buffer.md §2 | Fuzz-ish no-panic test (NFR-09) |
| R-03 overflow × reorder | High | transcript-buffer.md §3 | Tail-window equivalence (Variance 1) |
| R-04 delta → durable row | High | dispatch-wiring.md §2 | vnc-024 zero-rows tests UNMODIFIED |
| R-05 content leak | High | transcript-buffer.md §5, dispatch-wiring.md §4, purge-audit.md §3 | Sentinel tests + static grep — both |
| R-06 lock/poison | High | registry-wiring.md §3 | Explicit poisoned-mutex test |
| R-07 audit emission | Medium | purge-audit.md §2 | #4379 emission-context review |
| R-08 drain/sweep + silent eviction | Medium | registry-wiring.md §2, purge-audit.md §1 | Named silently-evicted case mandatory |
| R-09 PreCompact parity | High | transcript-block.md §2, dispatch-wiring.md §3 | Golden test + empty-buffer byte-identity |
| R-10 cycle-review clear | Medium | cycle-review-purge.md | Attribution matrix + post-clear pinning |
| R-11 config plumbing | Low | config-knob.md | Scenario 5 end-to-end cap chain |
| R-12 HTTP convergence | Low | dispatch-wiring.md §5 | Pattern #4725 transform tests |
| R-13 prompt injection | Medium | transcript-block.md §4 | Budget-bound + document-and-accept |
| R-14 hook.rs move | Medium | transcript-block.md §1 | Pre/post test-name inventory + constant pins |
| R-15 hole exhaustion | Low | transcript-buffer.md §4 | Collapse-at-65 correctness |

## Cross-Component Test Dependencies

- **Shared fixture machinery**: R-01/R-02/R-03/R-15 use ONE permutation/property harness in
  `session_transcript.rs` tests with a tunable cap (set low to force overflow). Expected
  content is derived programmatically from the covered-range set — never hand-copied (#2984).
  Reuse ass-069 PoC fixtures (`product/research/ass-069/`).
- **Sentinel string convention**: all R-05 leak tests use one sentinel constant (e.g.
  `"SENTINEL_TRANSCRIPT_7f3a"`) so the static grep gate and dynamic capture assertions agree.
- **Golden parity fixture**: one JSONL transcript fixture serves transcript-block.md §2 (golden
  test) and dispatch-wiring.md §3 (handle_compact_payload end-to-end). Expected output is
  always computed by `extract_transcript_block(path)` at test time — no checked-in expectation.
- **Test accessors**: AC-01/AC-03 need buffer-content visibility from listener tests. Plan:
  `contiguous_tail` + metadata accessors suffice; no `#[cfg(test)]` content getter that could
  leak into Debug paths.
- **Mocked clock for sweep**: R-08 silently-evicted case needs the existing staleness-sweep
  test approach in `session.rs` (idle-threshold manipulation), not a real 4 h wait.

## Integration Harness Plan (infra-001)

vnc-025's new surfaces (UDS dispatch arms, HTTP `/observe`, session registry internals,
PreCompact `CompactPayload`) are **not reachable through the MCP JSON-RPC interface** the
harness drives. The only MCP-visible touchpoint is the `context_cycle_review` handler
(`mcp/tools.rs:1918`), whose output AC-09 requires to be **unchanged**. The harness therefore
acts as a regression gate, not a feature-verification vehicle.

**Suites to run (Stage 3c):**

| Suite | Why |
|-------|-----|
| `smoke` (`-m smoke`) | Mandatory minimum gate — any change at all |
| `tools` | `mcp/tools.rs` modified (cycle_review handler gate); exercises cycle_review params |
| `protocol` | cycle_review exercised here; graceful shutdown crosses the modified drain path |
| `lifecycle` | cycle_review multi-step flows; restart persistence unaffected by registry changes |

`security`, `confidence`, `contradiction`, `volume`, `edge_cases`: not required — no scanning,
scoring, or storage-schema change. Run only if Stage 3c triage implicates them.

**New integration tests: none.** Rationale per agent-definition exclusions: (a) cycle-review
output unchanged is already covered by existing `tools`/`lifecycle`/`protocol` cycle_review
tests passing unmodified — that *is* the AC-09 "output otherwise unchanged" evidence at the MCP
level; (b) delta ingest/purge/PreCompact have no MCP-visible effect (UDS/HTTP only — unit and
Rust-integration tests suffice); (c) the audit table is not exposed through MCP tools. If
Stage 3b adds any MCP-visible behavior (it must not — review flag), revisit.

**Gap acknowledged**: transcript purge audit rows and buffer state are unverifiable through the
harness by design (in-memory, never persisted, content-free audit only). Coverage lives
entirely in Rust tests; the RISK-COVERAGE-REPORT must say so explicitly rather than claim
harness coverage.

## Acceptance Criteria → Test Plan

| AC | Plan file | AC | Plan file |
|----|-----------|----|-----------|
| AC-01 | dispatch-wiring §1 | AC-08 | purge-audit §1 |
| AC-02 | transcript-buffer §1/§3 | AC-09 | cycle-review-purge |
| AC-03 | dispatch-wiring §1 | AC-10 | registry-wiring §4 (structural) |
| AC-04 | dispatch-wiring §4 | AC-11 | transcript-block §2 + dispatch-wiring §3 |
| AC-05 | dispatch-wiring §2 | AC-12 | transcript-buffer §5 + static gate (all files) |
| AC-06 | dispatch-wiring §5 | AC-13 | shell: `cargo audit` + Cargo.toml diff |
| AC-07 | transcript-buffer §3 | NFR-09 | transcript-buffer §2 + registry-wiring §3 |

## Execution Order (Stage 3c)

1. `cargo test --workspace 2>&1 | tail -30` (all Rust levels)
2. Static gates: grep checks (AC-12, ADR-008, `listener.rs:1009` diff)
3. `cargo audit` (AC-13)
4. Harness: `pytest suites/ -v -m smoke --timeout=60`, then `test_tools.py`,
   `test_protocol.py`, `test_lifecycle.py`
