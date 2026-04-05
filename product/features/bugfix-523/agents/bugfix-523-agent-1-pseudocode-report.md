# Agent Report: bugfix-523-agent-1-pseudocode

Agent ID: bugfix-523-agent-1-pseudocode
Task: Produce per-component pseudocode for the bugfix-523 four-item hardening batch.

## Output Files

| File | Component | Items |
|------|-----------|-------|
| `product/features/bugfix-523/pseudocode/OVERVIEW.md` | Cross-cutting | All 4 |
| `product/features/bugfix-523/pseudocode/nli-tick-gate.md` | `run_graph_inference_tick` PATH B gate | Item 1 |
| `product/features/bugfix-523/pseudocode/log-downgrade.md` | `run_cosine_supports_path` log levels | Item 2 |
| `product/features/bugfix-523/pseudocode/nan-guards.md` | `InferenceConfig::validate()` NaN guards | Item 3 |
| `product/features/bugfix-523/pseudocode/session-sanitization.md` | `dispatch_request` rework_candidate arm | Item 4 |

## Components Covered

1. `nli-tick-gate` — `services/nli_detection_tick.rs`: `run_graph_inference_tick` PATH B entry gate
2. `log-downgrade` — `services/nli_detection_tick.rs`: `run_cosine_supports_path` two warn→debug changes
3. `nan-guards` — `infra/config.rs`: `InferenceConfig::validate()` 19-field NaN/Inf guards
4. `session-sanitization` — `uds/listener.rs`: `dispatch_request` `post_tool_use_rework_candidate` arm

## Open Questions

None. All source documents had no open questions. All OQs from SCOPE.md were resolved prior to
architecture. ADR-001 resolved SR-01, SR-02, SR-03. The IMPLEMENTATION-BRIEF resolved WARN-1.

## Source Verification Notes

- PATH B entry gate sequence confirmed from source: line 544 (`run_cosine_supports_path` .await),
  line 546 (`// === PATH B entry gate ===` comment), lines 552–555 (`candidate_pairs.is_empty()`
  fast-exit), lines 560–568 (`get_provider().await`). Insertion point is between lines 555 and 557.
- `run_cosine_supports_path` three log sites confirmed from source: line 766 (non-finite cosine,
  `warn!` — unchanged), line 796 (src_id miss, `warn!` — change to `debug!`), line 806 (tgt_id
  miss, `warn!` — change to `debug!`).
- `fusion_weight_checks` loop confirmed at lines 1151–1169: slice type `&[(&'static str, f64)]`,
  loop iterates with `for (field, value)` where `value: &f64`. Auto-deref makes
  `value.is_finite()` correct; existing `*value` dereference in comparisons is preserved.
- `phase_weight_checks` loop confirmed at lines 1173–1187: identical structure to Group B.
- crt-046 guards at lines 1381–1411 confirmed already have `!v.is_finite()` — not re-touched.
- UDS rework_candidate arm capability check confirmed at lines 660–665. First `event.session_id`
  use is at line 690 (`record_rework_event`). Insertion point is between lines 665 and 666.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` ("NLI background tick gate patterns", category "pattern")
  — returned entries #3675, #3822, #3937, #1542, #3653. Entry #3675 (tick gate architecture) and
  #3653 (rayon batch dispatch pattern) confirm the gate-inside-function approach. No conflicts with
  pseudocode decisions.
- Queried: `mcp__unimatrix__context_search` ("bugfix-523 architectural decisions", category "decision")
  — returned entry #4143 (ADR-001 for bugfix-523). All architectural decisions in pseudocode are
  consistent with this ADR.
- Deviations from established patterns: none. All four items follow established patterns:
  `!v.is_finite()` prefix (lesson #4132), `sanitize_session_id` guard (entry #3921),
  `debug!` for expected degraded mode (entry #3467), behavioral-only log test coverage (lesson #3935 /
  ADR-001(c) entry #4143).
