# Agent Report: vnc-046-agent-1-pseudocode-rework

> Gate 3a REWORK — OQ-2 (per-slug signature scanner parity)
> Agent: uni-pseudocode · Scope: pseudocode half of the ratified fix

## Task

Close Gate 3a OQ-2: the per-slug `SessionRegistry` in `project-provisioner.md` omitted the
`SignatureScanner`, so a slug declaring `[transcript_signals]` emitted class **names** with all-**zero**
counts (hollow FR-9, AC-07 HTTPS≠UDS parity break, R-16). ADR-002 was updated to ratify a per-slug
scanner compiled from `r.transcript_signals`. This report covers the pseudocode + test-expectation edits.

## Changes

- `pseudocode/project-provisioner.md`
  - P1 block renamed registry+hold **PAIR** → registry+hold+scanner **TRIPLE**. Added scanner
    construction (step 2): `SignatureScanner::compile(&r.transcript_signals.enabled_patterns())` mapped
    to `ServerError::Config` and propagated with `?`, chained via `.with_signature_scanner(...)` as the
    third registry builder call. Removed the "SEE OPEN QUESTION 2 / do not invent a param" hold.
  - Scanner is built + chained **before** the `server.session_registry` set, so it travels with the
    registry the `main.rs:1229` tick loop clones (F1/SR-03 pairing constraint preserved).
  - Purpose, imports (`SignatureScanner` from `crate::infra::transcript_activity`), Error Handling (new
    fallible step, R-10 no-silent-fallback), and Key Test Scenarios updated.
  - Added non-zero-count test expectation: declared-signal slug + signal-bearing delta ⇒ non-zero
    `signal_class_counts` (not just names); kept the empty-`[transcript_signals]` `"{}"` edge distinct.
- `pseudocode/OVERVIEW.md` — Open Question 2 rewritten as RESOLVED with the ratified per-slug decision.
- `pseudocode/isolation-suite.md`
  - AC-07 parity: input MUST be signal-bearing with a non-zero matched-class count on both legs
    (anti-fake-green; an empty input would pass even with an empty HTTPS scanner).
  - INV-C1/C2: each slug driven a signal-bearing delta matching its OWN class pattern; assert non-zero
    per-slug counts.

## Confirmation

- Scanner is **per-slug** — compiled from `r.transcript_signals`, NOT the daemon's shared
  `Arc<SignatureScanner>` (mirrors daemon `main.rs:820/852` construction, per-slug source).
- Scanner is **fallible** — `SignatureScanner::compile` → `Result<_, ScannerError>` mapped to
  `ServerError::Config` and propagated with `?` (daemon-faithful, `main.rs:70-72`); no From impl exists,
  so `.map_err` is used, not a bare `?`. `build_project_server` already returns `Result<_, ServerError>`.
- No per-slug re-validate: `r.transcript_signals` is the validated `resolve_slug_config` output
  (vnc-040); the daemon's file-anchored `transcript_signals.validate(path)` is not re-run.

## Added Test Expectations (for Stage 3b)

1. project-provisioner: declared-signal slug + signal-bearing delta ⇒ non-zero `signal_class_counts`
   (fails against an empty scanner; distinct from the empty-config `"{}"` edge).
2. isolation-suite AC-07: signal-bearing UDS-vs-HTTPS parity input with matched-class count > 0 on both
   legs.
3. isolation-suite INV-C1/C2: per-slug signal-bearing drivers yielding non-zero own-class counts.

## Open Questions / Gaps

None new. Non-blocking Stage-3b follow-ups from Gate 3a (OQ-1 ADR-003 `&IsolationProbe` ratify, OQ-3
`categories` NFR-5 stale-prose note) are unchanged and out of this rework's scope.

## Knowledge Stewardship

- Queried: reused Gate 3a report + ADR-002 (already ratified by architect) + source sites
  `main.rs:820/852`, `transcript_activity.rs:172`, `config.rs:2236`, `error.rs:47`; no new
  `context_briefing` call — the design decision was already made and stored by the architect, this is a
  mechanical pseudocode translation. Findings: `SignatureScanner::compile(&[String]) -> Result<_,
  ScannerError>`; daemon maps to `ServerError::Config`; `enabled_patterns()`/`enabled_class_names()`
  both exist on the resolved config.
- Stored: nothing — read-only tier (pseudocode); no reusable pattern beyond what ADR-002 already records.
- Deviations from established patterns: none — the fix mirrors the daemon registry-build triple exactly.
