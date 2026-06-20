## ADR-003: The Executable-Claim vs Narrative-Prose Boundary as an Operational Contract

### Context

nan-020 splits the documentation surface by testability: executable claims are guarded by
the doc-test; narrative prose gets manual rewrite plus a single `verified on vX` footer
stamp (D-3, not machine-checked). SR-06 flags this distinction as load-bearing yet defined
only by intent prose: an over-broad reading pulls narrative into the doc-test (gold-plating,
violates C-3); an under-broad reading leaves drifting commands untested (defeats the
feature). The boundary needs an operational definition and a worked example so `uni-docs`
knows what to stamp and the tester knows the exact tested set.

### Decision

**A doc line is an *executable claim* (MUST be doc-tested) iff it instructs the operator to
RUN a specific command whose success IS the claim** — all three holding: (1) it contains a
runnable copy-paste command (`unimatrix ...`, `npx ... init ...`, an instructional `curl`),
(2) its correctness is behavioral (works against the shipped artifact or not — not a matter
of phrasing), and (3) it lies on the canonical attach path the doc-test exercises, or is
reducible to it. **Everything else is narrative prose** (manual rewrite + one `verified on
vX` stamp per file): what a mode *is*, why pinning works, trust-model background,
when-to-use guidance, port/security notes.

**The tested set is exactly one canonical claim chain (AC-03), not every command in the
docs:** *"An operator emits a bundle with `unimatrix client-bundle <slug>` and attaches with
`init --bundle <blob>`, producing a successful `POST /v1/{slug}/observe` round-trip."*

Worked example against `docs/client-setup.md`:

| Doc line | Class | Guarded by |
|----------|-------|-----------|
| `unimatrix client-bundle <slug>` (emit) | Executable claim | doc-test Gate 5 |
| `npx @dug-21/unimatrix init --bundle <blob>` (attach) | Executable claim | doc-test Gate 6–7 |
| hook client POST to `/v1/<slug>/observe` | Executable claim | doc-test Gate 7 |
| "cloud MCP requires a v:2 bundle / pinning explained" | Narrative prose | manual + stamp |
| "TLS-only port 8443; GET /health unauthenticated" | Narrative prose | manual + stamp |
| token-rotation runbook steps | Narrative prose | manual + stamp |

Boundary discipline: do NOT add a doc-test gate per documented command (over-broad →
gold-plating); the tested set is the single canonical chain, extra commands are covered only
if they fall on it. Conversely, any NEW command added to the attach docs that is NOT
reducible to the canonical chain is a signal the chain is incomplete — raise it to design;
do not leave it untested by default (under-broad → drift persists).

### Consequences

- Easier: `uni-docs` has a deterministic rule for what to stamp vs. trust the gate to catch;
  the tester has an unambiguous tested set (one chain) keeping the doc-test minimal (C-3);
  reviewers can adjudicate "is this line tested?" without re-litigating intent.
- Harder: the boundary must be re-applied whenever the attach surface grows — a genuinely new
  documented command forces a design touchpoint rather than silent inclusion. This is
  intentional: it keeps the tested set honest and the mechanism small.
- Ties to D-3: because executable claims are gate-guarded, auto-checking the prose stamp's
  freshness would be redundant gold-plating — the stamp stays a human signal only.
  (Cross-ref ADR-002 for the canonical chain's mechanics; ADR-004 for who authors the prose.)
