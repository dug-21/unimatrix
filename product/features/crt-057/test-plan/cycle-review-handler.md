# Test Plan — `context_cycle_review` Handler

**File:** `unimatrix-server/src/mcp/tools.rs:~2125` (handler; four success returns at :2379/:2558/:3328/:3451)
**Risks:** R-06, R-07, R-10, R-13, R-18 · **ACs:** AC-03, AC-09, AC-10, AC-11, AC-12

> The handler threads the `transcript` scope through the four success returns, **deletes the four
> `purge_cycle_transcripts` calls**, and drops the `"summary"` alias. It gains ZERO destructive capability
> (NG-6). Behavioral matrix rows must PROVE which of the four returns executed (#4452) — no vacuous pass
> through the full-pipeline path.

---

## R-06 / R-10 — fully non-destructive, no purge verb (AC-03)
- `test_no_purge_on_any_path` — across default, `json`, `force:true`, and every `transcript` shape
  (`{}`, `{match}`, `{anchor,window}`, `{phase}`): assert `purge_cycle_transcripts` is **never invoked**
  (spy captured at handler return, deterministic) AND the buffer is **still present with the same content**
  after each call (synchronous read — the load-bearing negative proof, R-10). Source: assert the four
  `purge_cycle_transcripts(` calls are removed (shared with `distill-before-purge.md` source-assertion).
- `test_repeat_transcript_returns_identical_candidates` — a second identical `transcript:{}` review returns
  the same candidate set (buffer survived). (AC-03.)

## R-07 / R-12 four-site behavioral matrix (AC-12) — PATH-PROVEN
For each of the four success returns (purged-signals, cached-metrics, **memo-hit (site 3)**, full-pipeline),
a matrix row that asserts which site executed (memo-hit indicator / no-recompute — never assume, #4452):
- `test_site_{n}_transcript_present_scoped_candidates` — `transcript` present → scoped candidates present,
  buffer intact.
- `test_site_{n}_no_transcript_no_candidates` — no `transcript` → no candidate section, buffer intact.
- **Memo-hit (site 3) is non-optional**: `test_memohit_honors_transcript_identically_to_full_pipeline` —
  memo-hit + `transcript` present yields the same scoped candidates as the full-pipeline path for the same
  buffer state (CON-3 parity). The fold-landed assertion per site lives in `activity-fold.md`.

## R-18 — force orthogonality (AC-09)
- `test_force_alone_recomputes_no_candidates_buffer_intact` — `force:true`, no `transcript` → report
  recomputed from durable observations, NO candidate section, buffer untouched by `force`.
- `test_force_reproducible_before_and_after_reclamation` — `force:true` report body byte-identical before
  and after backstop reclamation (A-1 buffer-independence).
- `test_force_plus_transcript_orthogonal` — `force:true` + `transcript:{}` → report recomputed AND scoped
  slice returned; orthogonal, no precedence, `force` never reaches or purges the buffer.
- `test_report_body_invariant_under_buffer_state` — report body byte-identical whether buffer present /
  partial / gone (R-18 sc.1; the strongest guard that NO transcript signal entered the summary — NG-7).

## R-13 — AC#10 token reduction (AC-10) — NON-VACUOUS
- `test_token_reduction_ratio_populated_fixture` — a **populated** buffer (~62 KB candidate volume);
  assert `tokens(default_markdown) ≤ 0.20 × tokens(transcript_full_json)` — a **ratio**, not an absolute
  threshold.
- `test_ac10_vacuity_guard` — assert the `transcript`-json response is **materially larger** (candidates
  non-empty) BEFORE asserting the ratio, so an empty-buffer run fails loudly (#3548).

## Integration anchors
`suites/test_tools.py` (default no candidates, `transcript:{}` present, `format:"summary"` error) and
`suites/test_lifecycle.py` (non-destructive repeat) — OVERVIEW §6c.
