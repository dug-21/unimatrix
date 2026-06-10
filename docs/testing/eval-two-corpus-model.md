# The Two-Corpus Model

Band-2 guide (nan-018, AC-07/AC-11). The eval harness runs against **two**
corpora with deliberately different roles. This page states which is which, when
to use each, and how to re-snapshot the realism layer. Pairs with:

- [Fixture-corpus authoring guide](./eval-fixture-authoring.md)
- [Schema-migration runbook](./eval-corpus-migration.md)
- [Config-knob reference](./eval-config-knobs.md)
- [Evaluation harness overview](./eval-harness.md)

---

## The two roles

| | **Fixture corpus** | **Production snapshot** |
|---|---|---|
| **Role** | PRIMARY / **durable** | REALISM / **ephemeral** |
| **Source** | hand-authored entry-graphs in-repo (`eval/corpus/fixtures/`) | real `query_log` traffic via `unimatrix snapshot` |
| **Ground truth** | **property assertions** (redirect-to-head / absence / rank-below) | soft ground truth from actual past results (`baseline`) |
| **Carries the shape stamp?** | **yes** — drift guard HARD-ERRORS on mismatch | no — drift guard WARNs on mismatch |
| **Authority for** | trust / correctness | realistic P@5 / MRR baselines |
| **Lifecycle** | version-controlled; re-stamped on shape change | re-snapshotted when shape drifts; never the trust authority |

Both flow through the **same** `EvalServiceLayer` and the same replay/metric
machinery. No code branch distinguishes them at replay time — the difference is
the **assertion style** and the **durability contract**, documented here, not
enforced by type.

---

## Why two corpora

A single curated corpus goes stale: schema/shape evolution silently invalidates it,
turning a "primary dataset" into a perpetual re-authoring tax. The split resolves
this:

- The **fixture corpus** is the stable spine for trust/correctness. Its
  **property-based** assertions survive re-snapshots that literal-id assertions
  would not, and its **shape stamp** makes staleness *loud* (a hard error) instead
  of silent. This is the durable institutional memory of "what good retrieval looks
  like."
- The **production snapshot** supplies realism — P@5/MRR baselines from real
  traffic — that a small hand-authored corpus cannot. It is *meant* to be thrown
  away and re-taken when the schema's retrieval shape drifts.

You get durability where it matters (correctness) and realism where it matters
(quality baselines), without forcing the realistic data to be durable or the
durable data to be realistic.

---

## When to use which

| Goal | Corpus |
|------|--------|
| Sweep a penalty steepness lever and check **trust holds** (absence / rank-below) | **Fixture** — it carries the property assertions |
| Verify a chain redirects to its head / a deprecated entry stays below the weakest active | **Fixture** |
| Get a **realistic** P@5/MRR baseline to compare a candidate against | **Snapshot** |
| Confirm a confidence-weight change doesn't regress real-traffic ranking | **Snapshot** |
| The AC-14 correlated sweep (trust + P@5/MRR + cost in one run) | **Fixture** (`run_fixture_sweep`) |

Trust assertions are **only** authored on the fixture corpus (property-based ground
truth, no literal ids — crt-013 #703). The snapshot path does not carry trust
assertions; it answers "how does retrieval quality look on real traffic."

### How each is invoked

- **Snapshot path (CLI):** `unimatrix snapshot` → `unimatrix eval scenarios` →
  `unimatrix eval run --db <snap> --scenarios <jsonl> --configs <profiles>` →
  `unimatrix eval report`. The `[graph_penalty]` levers (see the
  [config-knob reference](./eval-config-knobs.md)) are live in this path via the
  profile layer, so a snapshot-based penalty sweep is fully CLI-accessible.
- **Fixture path:** `run_fixture_sweep` (`eval/runner/sweep.rs`) loads the fixture
  corpus, runs the drift guard, embeds-at-load so the snapshot is searchable, and
  replays each profile threading the alias map so trust evaluates non-vacuously. It
  is the AC-14 proof-by-use harness, exercised by the `sweep_tests.rs` proof test.

---

## How to re-snapshot the realism layer

The snapshot is ephemeral — re-take it when the schema's retrieval shape drifts (or
when its data ages out of relevance). The path is unchanged from
[the harness overview](./eval-harness.md):

```bash
# 1. Snapshot the live DB (never commit snapshots — they carry full interaction history)
unimatrix snapshot --out /tmp/eval/$(date +%Y%m%d)-snap.db

# 2. Extract scenarios from query_log
unimatrix eval scenarios --db /tmp/eval/$(date +%Y%m%d)-snap.db --out /tmp/eval/scenarios.jsonl

# 3. Run against your profiles, then report
unimatrix eval run   --db /tmp/eval/$(date +%Y%m%d)-snap.db --scenarios /tmp/eval/scenarios.jsonl \
                     --configs /tmp/eval/baseline.toml,/tmp/eval/candidate.toml --out /tmp/eval/results/
unimatrix eval report --results /tmp/eval/results/ --scenarios /tmp/eval/scenarios.jsonl --out /tmp/eval/report.md
```

**Paired-snapshot requirement:** the scenarios file and the snapshot DB used for
`eval run` must originate from the same DB state — generating scenarios from one
state and running against a fresh snapshot measures KB drift, not retrieval quality
(the GH #500 trap).

When the **fixture** corpus drifts instead, you do **not** re-snapshot — you
re-stamp. See the [migration runbook](./eval-corpus-migration.md).
