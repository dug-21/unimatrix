# Test Plan: config.toml Full Rewrite

## Component Scope

File under test: `config.toml` (repo root)
Authority for defaults: `crates/unimatrix-server/src/infra/config.rs`

Acceptance criteria covered: AC-06, AC-07, AC-08, AC-09
Risks covered: R-01 (Critical), R-02 (Critical), R-03 (High), R-06 (High)

---

## Execution Order

Tests in this file must be run in this order:
1. TOML parse validity (AC-08, step 1) — if this fails, field-value checks are secondary
2. Section headers and coverage (AC-06)
3. Field-by-field default values (AC-08, steps 2-4, R-01/R-02/R-03)
4. observation.domain_packs example (AC-07)
5. NLI block is commented out (AC-09)

---

## AC-06: All 8 Section Headers Present

**Risk**: direct compliance check

```bash
grep -n "^\[profile\]\|^\[knowledge\]\|^\[server\]\|^\[agents\]\|^\[retention\]\|^\[observation\]\|^\[confidence\]\|^\[inference\]" config.toml
# Expected: 8 matches, one per section header
```

Manual verification: read config.toml. Assert:
- Section order is: [profile] → [knowledge] → [server] → [agents] → [retention] →
  [observation] → [confidence] → [inference] (matching ADR-002 canonical order)
- An "Advanced Configuration" block marker (comment heading) separates [confidence]
  and [inference] from the user-facing sections above
- Every uncommented field has an explanatory comment on the line immediately above it

**Pass criteria**: 8 grep matches in correct order. Every uncommented field has a comment.

---

## AC-07: observation.domain_packs Example Block

**Risk**: R-09 (secondary — the config must show users how to configure domain packs)

```bash
grep -n "observation.domain_packs\|source_domain\|event_types\|rule_file" config.toml
# Expected: all four strings appear; every matching line must be prefixed with # (commented)
```

Manual verification: read the `[[observation.domain_packs]]` example block. Assert:
- The header uses `[[observation.domain_packs]]` (double brackets — table-of-tables), not
  `[observation.domain_packs]` (single bracket, which would be wrong TOML)
- All four fields appear: `source_domain`, `event_types`, `categories`, `rule_file`
- Each field has an explanatory comment
- `source_domain` comment states it is REQUIRED (no default, omission = parse error)
- `event_types` and `categories` comments state they are REQUIRED
- `rule_file` comment states it is optional (None = no file-based rules)
- The entire block (header + all fields) is commented out

**Pass criteria**: All four grep terms appear; all lines prefixed with #; manual checks pass.

---

## AC-08: TOML Validity and Default Value Accuracy

### Step 1 — Parse check (R-06)

```bash
python3 -c "import tomllib; tomllib.load(open('config.toml','rb')); print('TOML OK')"
# Expected: prints "TOML OK" with no exceptions
```

If this fails, record the exact error (line number, field name) — this is a Critical blocker.

### Step 2 — Uncomment-and-reparse: observation.domain_packs block (R-06)

Make a temporary copy, uncomment the `[[observation.domain_packs]]` example block,
and run the parser again. The block must parse without error because all three required
fields (`source_domain`, `event_types`, `categories`) must be present in the example.

```bash
# Verify required fields are in the example block
grep -A 15 "observation.domain_packs" config.toml | grep "source_domain\|event_types\|categories"
# Expected: all three appear within 15 lines of the header
```

### Step 3 — Uncomment-and-reparse: NLI sub-block (R-06)

Make a temporary copy, uncomment the NLI inference fields
(`nli_enabled`, `nli_model_name`, `nli_model_path`, `nli_model_sha256`,
`nli_top_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`), and parse.
Must succeed.

### Step 4 — Uncomment-and-reparse: [confidence] custom weights block (R-06)

Make a temporary copy, uncomment the [confidence] weights block, and parse.
Must succeed. Additionally verify the six weights sum to 0.92 ± 1e-9:

```bash
# Extract the six weight values and sum them (manual arithmetic or:)
python3 -c "
import tomllib, pathlib
# After manually uncommenting the [confidence] block in a temp file:
# t = tomllib.loads(pathlib.Path('config.toml.tmp').read_text())
# w = t['confidence']['weights']
# s = sum([w['base'], w['usage'], w['fresh'], w['help'], w['corr'], w['trust']])
# assert abs(s - 0.92) < 1e-9, f'sum={s}'
print('placeholder: perform manually with temp file')
"
```

### Step 5 — Field-by-field default value verification (R-01)

Read `crates/unimatrix-server/src/infra/config.rs` default_* functions and Default impls.
For each uncommented field in config.toml, assert the value matches the ADR-002 verified
defaults table exactly. Key fields to verify:

| Field | Expected Value in config.toml | config.rs Authority |
|-------|------------------------------|---------------------|
| `preset` | `"collaborative"` | Preset::Collaborative is #[default] |
| `categories` | `["lesson-learned","decision","convention","pattern","procedure"]` | INITIAL_CATEGORIES |
| `boosted_categories` | `["lesson-learned"]` | default_boosted_categories() serde fn |
| `adaptive_categories` | `["lesson-learned"]` | default_adaptive_categories() serde fn |
| `default_trust` | `"permissive"` | AgentsConfig default |
| `session_capabilities` | `["Read","Write","Search"]` | case-sensitive capitals |
| `activity_detail_retention_cycles` | `50` | RetentionConfig default |
| `audit_log_retention_days` | `180` | RetentionConfig default |
| `max_cycles_per_tick` | `10` | RetentionConfig default |
| `phase_freq_lookback_days` | `30` | InferenceConfig default |
| `min_phase_session_pairs` | `5` | InferenceConfig default |
| `nli_enabled` | `false` (commented) | InferenceConfig default |
| `nli_top_k` | `20` (commented) | InferenceConfig default |
| `nli_entailment_threshold` | `0.6` (commented) | InferenceConfig default |
| `nli_contradiction_threshold` | `0.6` (commented) | InferenceConfig default |
| `ppr_alpha` | `0.85` (commented/internal) | InferenceConfig default |
| `ppr_iterations` | `20` (commented/internal) | InferenceConfig default |
| `ppr_blend_weight` | `0.15` (commented/internal) | InferenceConfig default |
| `ppr_expander_enabled` | `false` (commented/internal) | InferenceConfig default |
| `supports_cosine_threshold` | `0.65` (commented/internal) | InferenceConfig default |

### Step 6 — boosted_categories / adaptive_categories serde vs Rust Default (R-02 — Critical)

```bash
grep -n "boosted_categories\|adaptive_categories" config.toml
# Expected: both fields show ["lesson-learned"], NOT []
```

Assert: a comment adjacent to these fields explains the serde-vs-Default distinction
(serde default = ["lesson-learned"]; Rust Default::default() = []).

### Step 7 — rayon_pool_size dynamic formula (R-03 — High)

```bash
grep -n "rayon_pool_size" config.toml
```

Assert one of:
(a) `rayon_pool_size` is entirely commented out with a comment containing the formula
    `(num_cpus / 2).max(4).min(8)`
(b) `rayon_pool_size` is present as a value with a comment containing the formula
    and the phrase "dynamically computed at startup"

Assert: `rayon_pool_size` does NOT appear as a bare integer without formula explanation.

### Step 8 — session_capabilities case-sensitivity

```bash
grep -n "session_capabilities" config.toml
```

Assert: the value uses capital R/W/S — `"Read"`, `"Write"`, `"Search"` — not lowercase.
Case matters for authorization checks in the server.

**Pass criteria for AC-08**: All parse checks succeed; every field matches ADR-002; R-02
shows serde default with comment; R-03 shows formula not bare integer; R-07 shows capitals.

---

## AC-09: NLI Sub-block is Fully Commented Out

**Risk**: if NLI block is accidentally uncommented, server fails to start (no model file)

```bash
grep -n "nli_enabled\|nli_model_path\|nli_model_name\|nli_model_sha256" config.toml
# Expected: matches exist (the fields are present) AND every matching line begins with #
```

Confirm: no NLI field is active (uncommented) in config.toml.
Confirm: a comment near `nli_enabled` states that NLI requires an external ONNX model
file not bundled with Unimatrix.

**Pass criteria**: All NLI field lines begin with `#`. Note about external model is present.
