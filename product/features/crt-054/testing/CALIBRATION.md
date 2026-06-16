# crt-054 — `[transcript_signals]` Default-Catalog Calibration (AC-10a / Coordination Item 3)

**Component**: 9 — `[transcript_signals]` config + `validate()` (`crates/unimatrix-server/src/infra/config.rs`).
**Scope**: the TWO v1 default classes only — `error` (index 0), `refusal` (index 1). Domain-neutral behavioral signatures (ADR-002, FR-C2). NO SDLC literals, NO `reread`/`compaction` class, no `token_*` field.
**Date**: 2026-06-16.

---

## Final locked patterns

Both are compiled in the **bytes domain** (`regex::bytes::RegexSet`) over raw transcript deltas, case-insensitive (`(?i)`), and anchored. They are pinned as `DEFAULT_ERROR_PATTERN` / `DEFAULT_REFUSAL_PATTERN` consts in `config.rs` and consumed by `TranscriptSignalsConfig::default()`.

### index 0 — `error` (provider / model HARD error + overload)

```
(?i)("type"\s*:\s*"(?:api_error|overloaded_error|rate_limit_error|invalid_request_error|authentication_error|permission_error|not_found_error|request_too_large|internal_server_error|api_status_error)")|\boverloaded_error\b|\brate.?limit(?:ed|_error)?\b|\bservice unavailable\b|\b(?:http\s*)?(?:status\s*(?:code\s*)?)?(?:429|500|503|529)\b\s*(?:error|overloaded|too many requests|service unavailable)
```

Anchored to: provider error **type tokens** (Anthropic-style `"type":"*_error"` envelope — the shape that appears verbatim in transcript bytes per `crates/unimatrix-engine/bindings/fixtures/response_error.json` and `wire.rs` error frames), explicit overload phrasing, and HTTP status codes **only when followed by an error-context word**. It deliberately does NOT match the bare word "error" in prose.

### index 1 — `refusal` (first-person model refusal stem)

```
(?i)\bI(?:'m| am)?\s+(?:cannot|can(?:'|’)?t|won(?:'|’)?t|will not|am not able to|(?:'m|m| am)?\s*(?:not able|unable)\s+to|am unable to)\b
```

Anchored to: `I ` + a refusal verb-phrase (`cannot` / `can't` / `won't` / `will not` / `am unable to` / `am not able to` / `I'm not able to`), straight and curly apostrophes. It does NOT match third-person prose ("the user cannot…", "Cannot reproduce…") or affirmative "I can do that".

---

## Sample / sources calibrated against

In-repo material searched for real provider-error / model-refusal transcript bytes:

| Source | Result |
|--------|--------|
| `product/research/ass-077/FINDINGS.md` | No literal error/refusal sample strings (faithful-reload findings). |
| `product/research/ass-078/FINDINGS.md` | Confirms the **posture** (domain-neutral, behavioral, anchored, capped catalog — §93/§97) but supplies NO literal sample text. |
| `crates/unimatrix-server/tests/fixtures/vnc-025/*` | Synthetic compaction-briefing envelopes only — no assistant refusal / provider error text. |
| `crates/unimatrix-engine/bindings/fixtures/transcript_delta_payload.json` | Synthetic delta (`user:…\nassistant:…`) — no error/refusal phrasing. |
| `crates/unimatrix-engine/bindings/fixtures/response_error.json`, `wire.rs` | Real `{"type":"Error",…}` JSON-RPC frame shape — informs the `error` type-token anchor, but is the SERVER wire error, not a provider/model transcript error. |

**Conclusion: no real provider-error or model-refusal TRANSCRIPT sample exists in-repo.** Per the AC-10a fallback, the patterns are therefore **anchored-by-construction and conservative**, not statistically calibrated against a real delta corpus. They were validated against hand-written positive/negative fixtures in `transcript_signals_config_tests.rs`:

- `test_default_error_pattern_matches_provider_errors` / `_low_false_positive`
- `test_default_refusal_pattern_matches_refusals` / `_low_false_positive`

These cover representative provider-error envelopes and refusal stems (positives) and ordinary prose using "error"/"cannot"/"I will" (negatives the patterns must reject).

---

## Precision / false-positive notes

- **Precision posture: high by construction.** Both patterns require a specific anchored shape (error **type token** / status-in-error-context; `I ` + refusal verb-phrase) rather than a loose keyword, so the most common false-positive sources (the word "error" in reasoning prose, third-person "cannot", affirmative "I will/can") are explicitly excluded and asserted in the negative tests.
- **Recall is intentionally under-catalogued.** ADR-002/FR-C2 directs "ship small, domain-neutral; deployments extend via config." A provider whose error envelope or refusal phrasing differs is expected to add a class in `[transcript_signals]`, not to be caught by the default.
- **DIRECTIONAL, NOT PRECISE (mandatory statement).** Because the producer is content-opaque by construction (ADR-005, NFR-1 — `ActivitySnapshot` carries only scalar counts and the raw delta bytes are never persisted), the post-ship false-positive rate of these patterns **can never be audited against real traffic**. The resulting `class_counts` are therefore **directional signals, not precise measurements**: a non-zero `error`/`refusal` count indicates "this cycle saw matches," not an exact, audited incident count. Any downstream (crt-055) use must treat them as directional.

---

## Handoff

The default patterns are LOCKED in `TranscriptSignalsConfig::default()`. If a real transcript corpus becomes available, re-run this calibration against it and update the patterns + this artifact before relying on the counts for anything beyond a directional signal.
