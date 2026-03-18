# Agent Report: alc-003-agent-0-scope-risk

## Output
- SCOPE-RISK-ASSESSMENT.md: `product/features/alc-003/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- High: 3 (SR-04, SR-05, SR-06)
- Medium: 4 (SR-01, SR-02, SR-03, SR-07)
- Low: 1 (SR-08)

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-05 (High)** — AC-03 contradicts Goals §5 on capability resolution. AC-03 says capabilities come from the caller's own registry record; Goals §5 says capabilities always come from the session with no registry lookup. This must be resolved in the spec before architecture begins or the implementation will have two interpretable paths.

2. **SR-04 (High)** — No named abstraction boundary for the OAuth swap. The forward path to W2-2/W2-3 requires swapping the identity source, but the SCOPE.md proposes this without naming the abstraction. Without a `SessionIdentitySource` trait or equivalent, both startup paths (`tokio_main_daemon`, `tokio_main_stdio`) will need surgery at W2-2.

3. **SR-06 (High)** — Test blast radius is understated. The SCOPE.md references "27 tests" asserting permissive behavior, but code inspection shows `SecurityGateway::new_permissive()` (27 usages) is a rate-limit bypass unrelated to `PERMISSIVE_AUTO_ENROLL`. The actual registry-permissive-dependent test count is smaller but unverified. A pre-flight run with `PERMISSIVE_AUTO_ENROLL=false` is required before coding begins to enumerate the real blast radius.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — returned outcome entries (pass) and conventions; no directly relevant lessons for identity/auth feature scope risks.
- Queried: /uni-knowledge-search for "outcome rework identity authentication" — no auth-specific rework outcomes found.
- Queried: /uni-knowledge-search for "risk pattern" (category: pattern) — returned entry #261 "AuditSource-Driven Behavior Differentiation for Caller-Specific Security" (relevant to audit attribution separation) and #1260 (conditional protocol steps, not relevant).
- Stored: nothing novel to store — the "spec-contradicts-goals" failure mode is feature-specific. The "name the abstraction boundary before forward-compatibility claims" pattern is potentially generalizable but needs a second data point before storing.
