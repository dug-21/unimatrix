## ADR-007 nan-022: Augmented Single Workload — Deterministic Seed-Corpus + Query Phase Under ONE Identity/Token for Non-Degenerate Ranking

### Context
nan-021's manifest is a 3-call cycle that produces a `MetricVector`. Retrieval and briefing parity
(AC-02/AC-05) need a PRE-SEEDED multi-entry store + a non-trivial query set: a degenerate single-hit
ranking gives a VACUOUS parity pass (SR-06 / the nan-021 R-03 thin-workload hazard) — and the ADR-004
stable-prefix policy is meaningless without enough entries that the stable prefix is a real ranking
signal. OQ-1 asks: augment the one workload, or split into dimension sub-workloads? The fixed
constraint is the SR-05/#832 defense: ONE manifest, ONE stable session identity, ONE run-correlation
token — splitting into parallel sub-workloads re-opens the drift hazard nan-021 closed at the
architecture level. The seed must ALSO not violate the no-seed-of-attribution rule (nan-021 ADR-004
#5289): the workload seeds the store CONTENT (entries to retrieve/rank), never the `topic_signal`
OUTPUT.

### Decision
AUGMENT the single workload (OQ-1 option (a)) — keep ONE manifest / ONE identity / ONE token, add a
deterministic store-seed + query phase so retrieval and briefing rank over a real corpus.

(1) **One workload, augmented.** `default_workload()` is extended with (a) a deterministic
SEED-CORPUS phase that writes a fixed set of entries (enough that ranking is non-degenerate — the
exact count is OQ-C/spec) via the normal `context_store` path, and (b) a QUERY phase issuing the
retrieval (`context_search`/`lookup`/`get`) and `context_briefing` calls whose ranked outputs are the
retrieval/proactive captures. The cycle + observe frames (analytics/behavioral) are preserved. Still
ONE `ParityWorkload`, ONE `session_id` (= run-correlation token), driven byte-identically on both
legs (ADR-001 nan-021 #5286 / SR-05 / #832).

(2) **Deterministic, identically-seeded store on both legs.** Both legs seed the SAME corpus from the
SAME manifest before querying, so any cross-leg ranking difference is a transport effect, not a corpus
difference. The seed writes CONTENT only; `topic_signal` remains DERIVED over the wire (nan-021
ADR-004 #5289 — the forbidden-seed sites `_seed_observation_sql_lifecycle` /
`_seed_attributed_observations_832` / `make_stamped_event(...,topic_signal)` stay un-reachable, audited
by the ADR-003 single forbidden-seed set).

(3) **Corpus sized for a real stable prefix** (pairs with ADR-004): the corpus + query set must yield
a ranking deep enough that the ADR-004 stable-prefix is a meaningful parity signal and not a single
hit (SR-06). Architecture fixes the shape; spec fixes the numbers (OQ-C).

(4) **Per-slug isolation rides the same workload** (AC-07): the isolation probe writes to slug A and
reads from slug B within the same identity/token, building on the posture-smoke per-slug Gates — no
separate workload.

### Consequences
Easier: "one workload" (the SR-05/#832 defense) is preserved while retrieval/briefing get a real
ranking to compare; the identically-seeded corpus makes a cross-leg ranking diff genuinely a transport
signal; the seed-content-not-attribution split keeps nan-021 ADR-004's derivation guarantee intact.
Harder: the manifest grows from a 3-call cycle to a seed + query + cycle workload (a bigger declarative
artifact both the Python and JS/shell legs replay — the ADR-001 nan-021 "expressible for both drivers"
cost, amplified); the corpus size is a load-bearing tunable (too small → vacuous pass per SR-06; too
large → slow gate + more HNSW tail churn for ADR-004 to tolerate); the seed must use the real
`context_store` path on both legs so the corpus is identical, adding store calls to the bundle drive.

Related: SR-06, OQ-1 (option (a)); AC-02, AC-05, AC-07. Preserves nan-021 ADR-001 (#5286) one-workload/
one-identity and ADR-004 (#5289) derived-attribution no-seed. Feeds the ADR-004 ranking tolerance.
Spec resolves OQ-C (corpus/query numbers).
