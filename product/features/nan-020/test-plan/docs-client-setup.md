# Test Plan — `docs/client-setup.md` rewrite

> Component: rewrite to the current bundle/observe model; remove obsolete 501/W2-7/curl-observe
> content; document both attach modes (`--remote` MARKED legacy; canonical `--bundle`, no `--slug`);
> add `verified on vX` footer. Risks: R-08, R-09, R-12, R-16 (legacy marker). ACs: AC-01, AC-02.

## Test Vehicle

Grep assertions (pre-merge-provable) + reviewer inspection for the executable-claim classification.
A small doc-grep harness (or the static section of the gate-logic test) runs the literal/regex greps.

## R-12 / AC-01 — Obsolete content gone; current model present (grep)

- `test_docs_zero_501`: `grep -c -E '501' docs/client-setup.md` → 0.
- `test_docs_zero_w2_7`: `grep -c -E 'W2-7' docs/client-setup.md` → 0.
- `test_docs_no_curl_observe_block`: no fenced code block matches `curl .*/observe` (the hand-rolled
  curl hook scripts are gone — telemetry is the `init`-wired hook client).
- `test_docs_obsolete_model_sweep` (FR-3, beyond the literals): the file does NOT instruct
  "no local binary required" / "curl-based shell hooks" as the telemetry mechanism. Grep the prose
  for those obsolete phrasings → 0.
- `test_docs_positive_current_model`: positive grep finds `init --bundle` AND `/v1/{slug}/observe`
  (or `/v1/<slug>/observe`) documented as the current path.
- `test_docs_enumerated_defect_set` (R-12 confirmation): verify the rewrite addressed the enumerated
  #768 defect set (the 6× 501/W2-7 callouts, the three curl hook blocks, the broken `--remote`
  example) and that out-of-scope items (`README:62` ONNX owned by #767) are NOT touched here.

**Coverage requirement:** currency is NOT inferred from literal strings alone — the obsolete-model
sweep + enumerated-defect confirmation guard the R-12 "grep passes while drift remains" failure.

## R-09 / AC-02 — `--slug` retired on the bundle path (grep)

- `test_docs_no_slug_with_bundle`: zero occurrences of `--slug` paired with `--bundle` (OQ-A;
  `init.js:353` retires it). Regex-assert no `init --bundle … --slug` form.
- `test_docs_slug_only_serverside`: `--slug` appears (if at all) only as a server-side
  `project register` / `client-bundle` argument, never on `init --bundle` (scoped retirement, not
  blanket removal).

## R-10 / AC-02 — broken bundle-via-`--remote` form eliminated (grep, multi-occurrence)

- `test_docs_zero_broken_bundle_remote`: zero occurrences of `init --remote unimatrix-bundle:` AND
  zero `init --remote <bundle>`-style bundle-fed-to-`--remote` forms — regex, multi-occurrence, not
  line-pinned (a third undiscovered phrasing must also be caught).
- `test_docs_canonical_bundle_present`: canonical `init --bundle <blob>` IS present.

## R-16 (accepted residual) / AC-02 — legacy `--remote` MARKED legacy (the ONLY owed mitigation)

- `test_docs_remote_marked_legacy`: the `--remote <url> --token <tok>` documentation carries an
  explicit **"legacy"** marker, distinguishing it from the canonical `--bundle` path. This is the
  sole owed mitigation for the consciously-accepted R-16 gap — there is NO `--remote` round-trip
  scenario. (Reviewer inspection confirms the marker is unambiguous.)

## R-08 / AC-03 boundary — executable-claim vs prose classification (inspection + grep)

- `test_docs_executable_claims_on_canonical_chain` (inspection): the three executable claims —
  `client-bundle` emit, `init --bundle`, the hook-client observe — are the ONES the doc-test
  exercises (Gates 5/6/7). They map 1:1 to what `docker-http-posture-smoke.md` actually tests.
- `test_docs_prose_carries_stamp_not_gate` (inspection): prose rows (fingerprint rationale, TLS/port
  notes, token-rotation runbook) carry only the `verified on vX` footer, NOT a doc-test gate
  (no gate-per-command — gold-plating, C-3).
- `test_docs_no_unreducible_command` (inspection): any executable command in the attach docs is on,
  or reducible to, the canonical chain; a NEW non-reducible command is a signal the chain is
  incomplete → raise to design, not left untested by default (ADR-003 boundary discipline).

## Self-Check

- [x] AC-01 literal greps + obsolete-model sweep + enumerated-defect confirmation (R-12).
- [x] No `--slug` with `--bundle`; `--slug` server-side only (R-09).
- [x] Multi-occurrence regex for broken bundle-via-`--remote`; canonical present (R-10).
- [x] `--remote` MARKED legacy — the only owed R-16 mitigation; no round-trip owed.
- [x] Executable-claim classification matches the three doc-tested claims; prose excluded (R-08).
