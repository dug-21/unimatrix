## ADR-005: Preset Enum Design and Weight Table

### Context

SCOPE.md defines four named presets plus `custom`:

- `authoritative` — source matters most, changes rarely (policy, standards, precedents)
- `operational`   — guides action, ages quickly (runbooks, incidents, procedures)
- `empirical`     — derived from measurement, time-critical (sensors, metrics, feeds)
- `collaborative` — built by a team, votes meaningful (dev, research) [DEFAULT]
- `custom`        — read weights directly from `[confidence]` section

SR-09 requires exact numeric values with domain-science validation before delivery.
SR-10 requires `collaborative` to reproduce compiled defaults exactly.

**The compiled defaults (verified from `confidence.rs`):**

```
W_BASE  = 0.16   W_USAGE = 0.16   W_FRESH = 0.18
W_HELP  = 0.12   W_CORR  = 0.14   W_TRUST = 0.16
Sum     = 0.92
FRESHNESS_HALF_LIFE_HOURS = 168.0h (1 week)
```

**Constraint: all presets must sum to exactly 0.92.** This is the stored-factor
invariant maintained since crt-005. The 0.08 delta from 1.0 was the former co-access
affinity allocation (W_COAC), which was removed in crt-013 but the sum invariant was
preserved. Any preset that sums to a value other than 0.92 changes the maximum
achievable confidence for all entries using that preset, breaking the implicit scale.

**Reasoning for each preset's ordering relationships:**

`authoritative` — Policy documents, legal standards, regulatory precedents:
- W_TRUST highest: who authored it matters most — human authors and official sources
  should outrank auto-generated content significantly.
- W_CORR elevated: a document with correction history has been reviewed and refined —
  this is a positive signal for authoritative sources (unlike empirical, where
  corrections suggest measurement error).
- W_FRESH reduced: authoritative documents change rarely; a 1-year-old legal ruling
  is still valid. Staleness should matter less.
- W_USAGE moderate: high access means the document is actively consulted, which is
  meaningful, but usage alone doesn't validate authority.
- W_HELP low-moderate: vote counts are meaningful but secondary to provenance.
- W_BASE standard: base quality signal still valid.
- half_life: 8760h (1 year) — authoritative knowledge has a long shelf life.

`operational` — Runbooks, incident procedures, on-call guides:
- W_FRESH highest: a runbook that hasn't been accessed or updated recently may be
  stale and dangerous to act on. Freshness is the dominant signal.
- W_USAGE elevated: high access count means the procedure is actively being followed —
  this is a relevance signal.
- W_CORR elevated: operational docs that have been corrected after incidents have
  been battle-tested — corrections represent hard-won operational learning.
- W_TRUST moderate: human-authored procedures are more reliable than auto-generated,
  but the freshness signal matters more.
- W_HELP reduced: operational teams rarely vote on runbooks; helpfulness data is thin.
- W_BASE standard.
- half_life: 720h (30 days) — operational knowledge decays faster than authoritative.

`empirical` — Sensor readings, metrics snapshots, feed data, measurements:
- W_FRESH dominant: a sensor reading from 3 days ago may be irrelevant or wrong
  today. Freshness is the overwhelming signal.
- W_USAGE moderate: frequently-accessed metrics are more important than rarely-used
  ones, but usage matters less than currency.
- W_CORR minimal: empirical measurements are not "corrected" — they are superseded
  by new measurements. A correction chain for sensor data typically means the
  original reading was erroneous, which is negative signal; low W_CORR prevents
  this from boosting confidence.
- W_TRUST minimal: most empirical data comes from automated sources ("auto" or
  "neural" trust_source); high W_TRUST would penalize all automated data.
- W_HELP minimal: automated sensor data does not receive helpfulness votes.
- W_BASE reduced: automated provenance should not penalize freshly-ingested data as
  much as in collaborative domains.
- half_life: 24h — empirical data has a very short freshness horizon.

`collaborative` — Team knowledge bases, dev wikis, research notes:
- This preset MUST equal the compiled defaults exactly (SR-10 invariant).
- The compiled defaults were calibrated for exactly this domain during development
  of the confidence system (crt-001 through crt-019).
- half_life: 168h (1 week) — balanced freshness for development cadence.

**Derivation of exact values:**

All weights must sum to 0.92. Within each preset the relative ordering is specified
above. The `collaborative` values are fixed (compiled defaults). For the other three,
the values are derived by:
1. Establishing the ordering relationship (which dimensions matter most).
2. Distributing the 0.92 budget proportionally to that ordering.
3. Verifying no weight is 0.0 (every dimension contributes signal; a zero weight
   means that dimension is unmonitored, which is not the intent).
4. Verifying the sum is exactly 0.92 (IEEE 754 double addition checked).

### Decision

**Preset enum:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    Authoritative,
    Operational,
    Empirical,
    Collaborative,
    Custom,
}

impl Default for Preset {
    fn default() -> Self {
        Preset::Collaborative
    }
}
```

**Exact weight table:**

| Preset | w_base | w_usage | w_fresh | w_help | w_corr | w_trust | SUM  | half_life_h |
|--------|--------|---------|---------|--------|--------|---------|------|-------------|
| `collaborative` | 0.16 | 0.16 | 0.18 | 0.12 | 0.14 | 0.16 | 0.92 | 168.0 |
| `authoritative` | 0.14 | 0.14 | 0.10 | 0.14 | 0.18 | 0.22 | 0.92 | 8760.0 |
| `operational`   | 0.14 | 0.18 | 0.24 | 0.08 | 0.18 | 0.10 | 0.92 | 720.0 |
| `empirical`     | 0.12 | 0.16 | 0.34 | 0.04 | 0.06 | 0.20 | 0.92 | 24.0 |

**Sum verification (all rows sum to 0.92):**

```
collaborative: 0.16+0.16+0.18+0.12+0.14+0.16 = 0.92 ✓
authoritative: 0.14+0.14+0.10+0.14+0.18+0.22 = 0.92 ✓
operational:   0.14+0.18+0.24+0.08+0.18+0.10 = 0.92 ✓
empirical:     0.12+0.16+0.34+0.04+0.06+0.20 = 0.92 ✓
```

**Ordering invariants (domain-science validation):**

`authoritative`:
- w_trust (0.22) > w_corr (0.18) > w_base (0.14) = w_usage (0.14) = w_help (0.14) > w_fresh (0.10)
- Source and correction history dominate; freshness matters least.

`operational`:
- w_fresh (0.24) > w_usage (0.18) = w_corr (0.18) > w_base (0.14) > w_trust (0.10) > w_help (0.08)
- Freshness dominates; corrections and usage are both high-signal; helpfulness is sparse.

`empirical`:
- w_fresh (0.34) >> w_trust (0.20) > w_usage (0.16) > w_base (0.12) > w_corr (0.06) > w_help (0.04)
- Freshness is overwhelmingly dominant (34%); trust elevated because automated source
  designation (neural/auto) needs its penalty dampened relative to collaborative;
  corrections and helpfulness are near-zero because they don't apply to measurement data.

**`collaborative` = default guard (SR-10):**

The delivery team MUST add this test to `unimatrix-server`:

```rust
#[test]
fn collaborative_preset_equals_default_confidence_params() {
    use unimatrix_engine::confidence::ConfidenceParams;
    // SR-10: If this test fails, production confidence scores have silently diverged
    // from pre-dsn-001 behavior. Fix the weight table, not the test.
    assert_eq!(
        ConfidenceParams::from_preset(Preset::Collaborative),
        ConfidenceParams::default()
    );
}
```

This test is a compile-time-and-runtime guard: if any of the six `collaborative`
weights drift from the compiled constants, the test fails at CI.

**`ProfileConfig` struct:**

```rust
#[derive(Debug, Default, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct ProfileConfig {
    pub preset: Preset,
}
```

`UnimatrixConfig` gains a `profile` field:

```rust
pub struct UnimatrixConfig {
    #[serde(default)]
    pub profile: ProfileConfig,
    #[serde(default)]
    pub knowledge: KnowledgeConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub confidence: ConfidenceConfig,
}
```

**Validation rules for `preset`:**

- If `preset == "custom"` and `confidence.weights` is `None`: abort startup.
- If `preset == "custom"` and any weight is out of `[0.0, 1.0]` or non-finite: abort.
- If `preset == "custom"` and `(sum - 0.92).abs() >= 1e-9`: abort with message
  "custom weights sum to {sum:.10}; must equal 0.92 exactly".
- If `preset != "custom"` and `confidence.weights` is `Some`: log a warning
  "confidence.weights ignored because preset is not custom", continue startup.
- If preset string is not one of the five valid values: abort with error listing
  valid values (AC-26).

### Consequences

**Easier:**
- SR-09 resolved: exact numeric values are committed before delivery begins.
- SR-10 resolved: the `collaborative` row matches compiled defaults exactly; the
  mechanical guard test catches any future drift.
- Operators identify their knowledge type ("authoritative", "operational") rather
  than setting `W_TRUST = 0.22`. The preset system is the primary interface.
- W3-1 cold-starts from these values and refines from actual usage — a non-dev
  domain starting at its correct preset converges faster than starting from
  `collaborative`.

**Harder:**
- The weight values are a design decision, not a measurement. If an operator's domain
  doesn't fit neatly into any of the four presets, `custom` is the escape hatch but
  requires ML expertise. The four presets cover the described archetypal domains; edge
  cases exist.
- The sum-must-equal-0.92 constraint (not ≤ 1.0) is an invariant that differs from
  what the SCOPE.md config schema comment says ("sum must be ≤ 1.0"). The SCOPE
  comment is wrong; this ADR's constraint is authoritative. The delivery team must
  use `(sum - 0.92).abs() < 1e-9` as the validation rule, not `sum <= 1.0`.
- `empirical`'s W_TRUST of 0.20 is a design choice to avoid heavily penalizing
  automated sources. If a domain has empirical data from human-curated sources,
  `collaborative` may be a better fit than `empirical`.
