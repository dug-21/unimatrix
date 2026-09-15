# Agent Report — vnc-049-agent-2-testplan (uni-tester, Stage 3a)

Phase: Test Plan Design. Produced per-component test plans rooted in RISK-TEST-STRATEGY.md and
ACCEPTANCE-MAP.md, with an integration harness plan. All output under
`product/features/vnc-049/test-plan/`.

## Deliverables
- `test-plan/OVERVIEW.md` — strategy, R-01..R-17 → test mapping, cross-component deps, integration
  harness plan (suites, 8 new integration tests, smoke gate), open questions.
- `test-plan/c1-plugin-shim.md` .. `c9-installer.md` — 9 per-component plans, 1:1 with the Component Map.

## Risk coverage (all 17 risks mapped)
- Critical: R-01 (AC-06 gating, c5+infra-001), R-02 (AC-03 stored-record+negative, c5/c6),
  R-03 (parity drift, c8/c2/c3), R-04 (migration cascade, c5/c6).
- High: R-05 (opencode-only, c5), R-06/R-07 (subagent/observe-channel, c1/c5), R-08 (installer, c9).
- Medium/Low: R-09..R-17 assigned to owning components.

## Authoritative non-proxy assertions (as ACCEPTANCE-MAP demands)
- AC-03: `source_domain="opencode"` + mandatory negative NOT `"claude-code"` on the SELECTed row
  from an assembled dispatch→insert→read-back (c5), following `listener/tests/foreign_domain.rs`.
- AC-06 (GATING, R-01): local-vs-cloud distinctness on the QUERIED `model_id`, with anti-tautology
  negative (fails if carrier dropped between wire and INSERT); two local models mutually distinct (c5).
- R-05: opencode-only write-stamp matrix; T-SEC-12/13 (`test_parse_rows_unknown_event_type_passthrough`
  :1360, `test_parse_rows_hook_path_always_claude_code` :1390) stay green unchanged (c5/c6).
- Lesson #5670: model_id client→listener→DB crossing test = the R-01 test (c5) + parity-corpus case (c8).

## Integration suite plan (Stage 3c)
- Mandatory smoke gate + full unit run + #878 LINK smoke.
- Suites: tools, protocol, lifecycle, volume, security, edge_cases.
- 8 new infra-001 tests extending the GH#819 `daemon_server` + `UnimatrixHookClient` +
  `_observation_row_count` model (real UDS ingest, read-back SELECT). Anti-seed contract enforced.

## Findings / corrections for delivery (verified against tree)
1. **Parity-corpus file is misnamed in BRIEF/ARCHITECTURE.** C8's 7-event provider/normalizer cases
   belong in `uds/parity_corpus_cases*.rs` + `uds/parity_corpus_gen.rs` (drift gate `assert_coverage`
   @gen.rs:363, `all_arm_keys()` @gen.rs:92, `scripts/check-parity-drift.sh`), NOT
   `uds/parity_corpus_uds.rs` (which is UDS framing goldens only). Delivery/pseudocode must confirm.
2. **Schema version off-by-one.** `CURRENT_SCHEMA_VERSION` reads **31** (migration.rs:26); next is
   **32**. BRIEF/ARCHITECTURE say "32 as of today" — resolve live, do NOT hardcode; clone
   `migration_v30_to_v31.rs`.
3. **No assembled dispatch→insert→read-back Rust unit test exists yet** — `dispatch_record_event_
   returns_ack` (listener.rs:3767) stops at Ack. The AC-03/AC-06 tests are NET-NEW in
   `listener.rs mod tests` (:3456), modeled on `foreign_domain.rs`.
4. **Read-path coverage gap.** `load_observation_session_stats` (unimatrix-store/src/observations.rs:173)
   has no direct test; C6 changes its SELECT → c6 requires new direct coverage.

## Open questions (routed to delivery/Stage 3b)
- OQ-4: pin WHICH derivation site stamps stored `source_domain` (listener Site A vs DomainPackRegistry
  DEFAULT). Test keys its per-site behavioral matrix (#5427) on the pinned site. Assumes ADR-001
  provider-first ingest stamp at the listener insert site.
- OQ-3/R-11: session.idle→Stop over-count guard concrete assertion pinned to the delivery-time PoC.
- AC-06 plugin reachability from infra-001 (plugin TS end vs `unimatrix hook` CLI end) — Rust crossing
  test (c5) is the always-reachable authoritative proof.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get -- surfaced ADRs #5748
  (source_domain persist), #5741/#5743/#5744/#5746 (vnc-049 ADRs), pattern #5737 (three-touchpoint
  onboarding), #4373 (schema-version cascade checklist), #5427 (source-assertion string-counting is
  blind — per-site behavioral matrix), #5285 (derive-don't-seed / anti-seed traps), #4177/#3548
  (tautology/omitted-assertion gate history), #378/#4153 (old-schema DB migration tests). All folded
  into OVERVIEW + c5/c6/c8.
- Stored: nothing novel to store -- Stage 3a produces test plans, not reusable test infrastructure
  patterns; the applicable patterns (#5285, #5427, #4373, #5737) already exist. Any new fixture/harness
  technique discovered during Stage 3c execution will be stored then.
