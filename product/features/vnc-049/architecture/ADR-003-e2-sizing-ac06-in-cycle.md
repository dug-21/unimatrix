## ADR-003: E2 sizing — take AC-06 (per-model attribution) IN this cycle

### Context
SCOPE Open Question 1 routes the E2 sizing call to the architect: take per-model/local-model
attribution (AC-06) now, or fast-follow. The C18 ledger guardrail is explicit: C18 does **not** reach
`proven` on AC-01..05 alone; it stays `partial` until AC-06 local-vs-cloud distinctness is behaviorally
demonstrated on the assembled path — **regardless** of whether E2 lands this cycle. So there is no
ledger credit for deferring.

Two facts change the cost calculus (from ADR-001/002):
- AC-03 **already forces** an `observations` schema change (`source_domain` column), write-path
  provider persistence, and read-path plumbing. AC-06 adds **one more nullable column** (`model_id`) to
  the same migration and **one more field** through the same wire/plumbing seam.
- Deferring AC-06 means a **second** migration, a **second** wire change, and **re-touching the same
  over-cap files** (hook.rs, listener.rs, observation.rs) and the split-brain surface (normalize.js +
  parity corpus) a second time — more total churn, more blast radius, more split-brain exposure.

### Decision
**AC-06 is IN this cycle.** The model carrier (ADR-002) is delivered alongside the source_domain
persistence (ADR-001) in a single migration and a single pass through the wire/normalizer/listener/read
surfaces. AC-06 is asserted end-path: a real OpenCode local-model (`ollama/qwen3-coder`) session lands
a row queryable as distinct from a cloud-model row and from a claude-code row — no proxy, no injected
dependency, no tautological "field is populated" assertion (SR-01, SR-10, #4177, #4876, #4974).

C18 ledger status is unaffected by this call in principle (guardrail stands): C18 remains `partial`
until that behavioral demonstration is green. Taking E2 now is what makes the demonstration reachable
this cycle rather than in an interim-`partial` fast-follow.

### Consequences
Easier: one migration, one wire pass, one split-brain landing; C18 can move off `partial` this cycle;
avoids re-opening monoliths twice. Harder: this cycle's delivery+test surface is larger (the parity
corpus and the JS mirror must cover `model_id` now, and the AC-06 distinctness test must run the real
assembled path). If delivery discovers the plugin cannot reliably resolve `model:{}` on every event,
the fallback is a documented degraded leg (ADR-009 posture) — not a silent drop; C18 then stays
`partial` honestly. Cross-references ADR-001, ADR-002.
