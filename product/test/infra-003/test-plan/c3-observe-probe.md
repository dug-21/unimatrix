# Test Plan — C3: Observe write surface, both directions

> Pseudocode: `pseudocode/c3-observe-probe.md`. Risks: **R-09** (marker
> round-trip), R-08 (per-run unique), R-12 (SQL metachars), R-18 (substring),
> R-04 (WAL). ACs: **AC-02**.

C3 drives the two observe writes over the one cert-pinned bearer token:
`POST /v1/A/observe` with `topic_signal = infra003-obs-a-<run>` and
`POST /v1/B/observe` with `infra003-obs-b-<run>`. The test of C3 proves each write
hits the genuine funnel, returns 204, and that the marker round-trips verbatim
into `observations.topic_signal` (so a positive false-RED in C5 is not a C3 fault).

## What C3 must do (behavior under test)

- One bearer token authorizes both writes (slug in path, #4950); identity is the
  path, not the payload.
- Wire: `HookRequest::RecordEvent { event: ImplantEvent }`
  (`"type":"RecordEvent"`), marker in `ImplantEvent.topic_signal`.
- Each POST returns **204**. The 204 alone is **not** the verdict (must pair with
  the C5 read-as-barrier — a 204 does not prove right-store landing, and the write
  is not synced before the 204).

## Verification tier 1 — off-Docker / static

- `test_c3_marker_charset_safe` — both observe markers match `^[a-z0-9-]+$`: no
  `%`/`_`/`'` that would break or spuriously-match the `sqlite3` predicate (R-12).
- `test_c3_markers_per_run_unique` — both carry the shared `<run>` nonce
  (PID+timestamp); two invocations produce different literals (R-08).
- `test_c3_markers_mutually_non_substring` — `infra003-obs-a-<run>` and
  `infra003-obs-b-<run>` are pairwise non-substring (with the two MCP markers; the
  full 4-marker check lives in C7/R-18).
- `test_c3_one_token_two_slugs` — inspection: the same `$TOKEN` is used for both
  `/v1/A/observe` and `/v1/B/observe`; slug differs only in the URL path.

## Verification tier 2 — live run

- `test_c3_observe_a_returns_204` / `test_c3_observe_b_returns_204` — assert HTTP
  `204` for both (AC-02).
- `test_c3_marker_roundtrips_to_column` (R-09) — after each write, a positive
  read-back confirms the exact marker literal appears in
  `observations.topic_signal` of the **own** store (column-mapping self-test
  against `db.rs:865`/`analytics.rs:539-554`), proving no transform/truncation.
  This is the C5 positive-control read; here it doubles as the R-09 mapping proof.
- Documented fallback (R-09): if a future payload shape drops `topic_signal`,
  `observations.input` substring is the spec-named fallback — not an ad-hoc guess.

## Coverage requirement

Both observe writes traverse the real `parse_project_key → resolve_store →
dispatch_request` funnel, return 204, and land the exact marker verbatim in
`observations.topic_signal` of the addressed store (R-09); markers are charset-safe
and per-run unique (R-12/R-08).
