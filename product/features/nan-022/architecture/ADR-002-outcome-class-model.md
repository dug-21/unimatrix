## ADR-002 nan-022: Four-Valued Outcome-Class Model — INFRA-ERROR via Transport-Health Preflight + Bounded Deadlines, Intra-Transport Nondeterminism via Double-Capture-and-Diff

### Context
Two failure classes are routinely conflated in a cross-transport parity gate, and conflating
them poisons the red gate (SR-04): (a) **cross-transport divergence** = a real C0 parity defect
→ must RED; (b) **intra-transport nondeterminism** (HNSW approximate top-k membership flip
#4990/GH#746, HashMap-order #2610, `sort_unstable` ties, embedding cold-start) = a PRE-EXISTING
in-transport bug to file SEPARATELY, NOT a cloud-parity failure (OQ-2, human-fixed disposition).
A third hazard is transport infrastructure failure: SR-02/#5303 noted the #830 self-heal the
suite couples to covers only SIGNALLED (404) eviction, NOT a silent half-open-socket hang. The
specific #839 instance is now CLOSED (commit 5b6badad / PR #842, 2026-06-25) and is no longer a
gating dependency — delivery is UNBLOCKED. The half-open-hang CLASS remains a real hazard
(defense-in-depth): if any such hang reads as a dimension FAIL, ALL SIX dimensions throw a
false-RED. A bare two-valued PASS/FAIL gate cannot distinguish these; the design must make them
STRUCTURALLY distinct regardless of #839's status.

### Decision
Each dimension yields exactly ONE of **four** outcome classes (`harness/parity_outcome.py`,
`Outcome` enum): `PARITY_PASS`, `PARITY_FAIL`, `INFRA_ERROR`, `INTRA_TRANSPORT_NONDETERMINISM`.
They are decided in a fixed order so an infra hang or an intra-transport flake can NEVER read as
a cross-transport parity verdict.

(1) **INFRA-ERROR first, before any comparator.** `harness/transport_health.py` provides
`preflight_leg(leg, *, connect_deadline_s, idle_deadline_s)` raising `InfraError`, plus
generalized never-empty ingestion (`load_https_bundle` → `InfraError` on missing/stale/empty).
A bounded per-leg connect + idle deadline means a half-open socket hang (the #839 class — #839
itself closed) or any transport unreachability or an empty/un-ingestable capture surfaces as
`INFRA_ERROR` with a DISTINCT exit code — never green, never counted as a parity RED (SR-02). The
HTTPS leg's existing `run_smoke_gate` exit-code truth table (Docker-absent 3 → HARD FAIL,
unacquirable 4, broke 1) is preserved; the idle-deadline classification is what names a *hung*
(not failed) socket as INFRA. Defense-in-depth, not a #839 workaround.

(2) **INTRA-TRANSPORT-NONDETERMINISM next, via double-capture-and-diff.** For any dimension with
`intra_transport_check=True` (retrieval, proactive), EACH leg captures its dimension output TWICE
in the same drive; `intra_transport_stable(cap_a, cap_b, *, tolerance)` diffs them modulo the K3
ranking tolerance. If a leg's two captures differ, that leg is intra-unstable → the dimension is
classed `INTRA_TRANSPORT_NONDETERMINISM`, recorded in the evidence table, routed to a SEPARATELY
filed GH bug (GH#746 for retrieval ranking), and EXCLUDED from the red parity gate (OQ-2 fixed
disposition). The cross-leg comparator runs ONLY when BOTH legs are intra-stable.

(3) **PARITY-PASS / PARITY-FAIL last.** Only on two intra-stable, ingested captures does
`dim.comparator.compare()` run: clean modulo the closed set → `PARITY_PASS`; any non-excluded
diff → `PARITY_FAIL` (a real C0 defect → file a NEW GH bug per AC-10, gate stays RED, fix NOT
absorbed; emit the first-live-run field-by-field evidence record).

(4) **Roll-up** (`rollup`): GREEN iff every dimension is `PARITY_PASS`; any `PARITY_FAIL` → RED;
any `INFRA_ERROR` → distinct ERROR exit (not green, not a parity RED);
`INTRA_TRANSPORT_NONDETERMINISM` is recorded + filed but does NOT redden the gate. The
`Dimension.blocks_c0_proof` flag defaults True for ALL SIX — C0 #5304's `done_when` settles the
"six vs three" question ("parity is the bar… total"; confirmed 2026-06-25). The flag is the escape
valve for a HUMAN-SIGNED documented exception (an unreachable dimension), never a silent exclusion.

### Consequences
Easier: a transport hang (the #839 class) and an in-transport ranking flake (GH#746) can never
masquerade as cloud divergence — the red gate measures ONLY cross-transport parity (SR-02/SR-04
closed structurally); operators get an actionable, distinct signal per class; the OQ-2 disposition
is encoded, not left to a tester's judgement. Harder: the ordered classifier and double-capture add
real machinery (each intra-check dimension drives twice → more wire calls + latency); the bounded
deadlines must be tuned so a slow-but-healthy runner is not misread as INFRA (the deadline is a
new tunable); `blocks_c0_proof` defaults all six block (aligned with C0 #5304 done_when) and only
relaxes via a human-signed documented exception.

Related: SR-02, SR-04, OQ-2; AC-08, AC-10. Depends on nan-022 ADR-001 (registry/orchestrator),
ADR-004 (the ranking tolerance the intra-diff uses). Couples to #830 (ADR-002 nan-021 #5294); the
half-open-hang INFRA-ERROR class is defense-in-depth (#839/#5303 now CLOSED, not a blocker).
