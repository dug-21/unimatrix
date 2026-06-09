# Eval Config-Knob Reference — `[graph_penalty]` and the Cost Proxy

Band-2 reference (nan-018, AC-11). Documents every newly-exposed eval lever so a
developer can author a steepness sweep **from this page alone** — meaning, valid
range, default, and effect for each knob — plus the cost-metric proxy fidelity
caveat. Pairs with:

- [Fixture-corpus authoring guide](./eval-fixture-authoring.md)
- [Schema-migration runbook](./eval-corpus-migration.md)
- [Two-corpus model](./eval-two-corpus-model.md)
- [Evaluation harness overview](./eval-harness.md)

---

## ⚠️ Boundary — these knobs are EVAL/MEASUREMENT-ONLY (ADR-006)

The `[graph_penalty]` section exists **to feed eval profiles**, not production.
It is **not** license to re-tune deployed defaults.

- A deployed Unimatrix server omits `[graph_penalty]` (or sets it equal to
  defaults); omission resolves every field to its crt-014 engine `const`
  **bit-for-bit**, so deployed ranking is identical to pre-nan-018.
- Non-default penalty values belong in **eval profile TOMLs** driving
  `unimatrix eval` sweeps — never in a production deployment config.
- The penalty-formula authority is **ASS-037 (#3984)**. Any decision to *adopt* a
  swept value as a new deployed default is an ASS-037 decision made on the
  evidence the sweep produces; it is out of nan-018's scope. nan-018 produces the
  evidence, it does not act on it.

This boundary is a documentation/convention guarantee, not a type-enforced one —
nothing in the code stops an operator from putting non-default penalties in a
production config. Don't.

---

## The `[graph_penalty]` section

A partial, defaulted section of a profile `UnimatrixConfig`. Any subset of fields
may be set; **an omitted field resolves to its engine `const` default**. An absent
section resolves *every* field to its const, reproducing current behavior
bit-for-bit (the dual-default discipline — `GraphPenaltyConfig::default()` ==
`GraphPenaltyParams::default()` == the engine consts).

Validation runs at config load (`validate_graph_penalty` in
`infra/config.rs`): an out-of-range value **aborts config load** with
`ConfigError::GraphPenaltyFieldOutOfRange` naming the field, value, and reason —
it is never silently clamped or ignored.

```toml
[profile]
name = "steeper-clean-replacement"
description = "Sweep clean_replacement harsher to test the steepness crossover"

[graph_penalty]
clean_replacement = 0.25   # was 0.40 (default)
# every other field omitted => resolves to its engine const
```

### Knob table

| Knob | Meaning | Valid range | Default (crt-014 const) | Effect when lowered |
|------|---------|-------------|--------------------------|----------------------|
| `orphan` | Penalty for a **deprecated entry with no successors** (dangling/orphan). | finite `[0.0, 1.0]` | `0.75` (`ORPHAN_PENALTY`) | Pushes orphaned deprecated entries further down. |
| `clean_replacement` | Penalty for an entry **cleanly replaced at depth 1**. **AMPLIFIED knob — see below.** | finite `[0.0, 1.0]` | `0.40` (`CLEAN_REPLACEMENT_PENALTY`) | Penalizes depth-1 replaced entries AND lowers the hop-decay clamp ceiling (same direction). |
| `hop_decay` | Per-additional-hop decay multiplier in the chain-penalty formula. **SHAPE param — never multiplier-scaled.** | finite `[0.0, 1.0]` | `0.60` (`HOP_DECAY_FACTOR`) | Decays penalty faster with chain depth. |
| `partial_supersession` | Penalty for **ambiguous (multi-successor) supersession**. | finite `[0.0, 1.0]` | `0.60` (`PARTIAL_SUPERSESSION_PENALTY`) | Penalizes fork-superseded entries more. |
| `dead_end` | Penalty for a **chain that reaches no Active terminal** (dead-end). | finite `[0.0, 1.0]` | `0.65` (`DEAD_END_PENALTY`) | Penalizes dead-end chain members more. |
| `fallback` | Flat penalty the **search layer** applies on cycle detection (the `FALLBACK_PENALTY` fallback branch). | finite `[0.0, 1.0]` | `0.70` (`FALLBACK_PENALTY`) | Penalizes the fallback case more. |
| `max_traversal_depth` | Maximum chain-traversal depth. **SHAPE param — never multiplier-scaled.** | integer `>= 1` | `10` (`MAX_TRAVERSAL_DEPTH`) | Limits how deep chain resolution walks. |
| `multiplier` | Optional convenience overlay scaling the **five severities** uniformly toward harsher. | finite `(0.0, 1.0]`, or omitted (`None`) | `None` (no scaling) | `effective = base * m` for the five severities. |

A "penalty" multiplies into a result score to push an entry **down** the ranking;
the values are scaling factors (`< 1.0` = a discount), so a **lower** value is a
**harsher** penalty.

---

## `clean_replacement` is an AMPLIFIED knob (ADR-001 — read sweeps accordingly)

`clean_replacement` does **two** things at once, by design:

1. it is the depth-1 clean-replacement penalty base; **and**
2. it is the **upper clamp ceiling** for the depth-≥2 hop-decay branch. The
   chain-penalty formula computes `raw = clean_replacement * hop_decay^(d-1)` then
   clamps to `[0.10, clean_replacement]`. The ceiling is `clean_replacement`
   *itself* — guaranteeing a depth-≥2 replacement is never penalized more harshly
   than a clean depth-1 one (the depth-2 ≤ depth-1 monotonicity rule).

Sweeping `clean_replacement` therefore moves **both** the depth-1 base AND the
depth-≥2 clamp ceiling/anchor **in the same direction**. This is an *amplified*
knob (one lever, coherent scaling of the whole clean-replacement severity family)
— **not** a confounded one: base and ceiling never diverge. When you read a
`clean_replacement` sweep result, read it as the **amplified** effect of that
severity family, not as an isolated single-parameter perturbation.

The ceiling is intentionally **not** a separate `[graph_penalty]` field: an
independent ceiling could drop below the base and break the monotonicity
guarantee.

---

## Multiplier semantics (OQ-2 — convenience overlay, per-field override wins)

`multiplier = Some(m)` is a one-knob coarse sweep. It scales **only the five
severities** — `orphan`, `clean_replacement`, `partial_supersession`, `dead_end`,
`fallback` — by `* m` toward harsher. It **never** scales `hop_decay` or
`max_traversal_depth` (those are shape, not severity).

**Per-field override wins over the multiplier.** A field you set explicitly is
*not* scaled; the multiplier applies only to fields left at their const default.

```toml
[graph_penalty]
multiplier = 0.5          # scale the 5 severities to half (harsher)
orphan     = 0.75         # explicit -> NOT scaled (override wins), stays 0.75
# clean_replacement, partial_supersession, dead_end, fallback omitted -> scaled to *0.5
```

### ⚠️ Multiplier caveat — deliberate set-to-default is ambiguous (ADR-001 / penalty-config)

Override detection uses an **equals-default heuristic**: a field counts as
"overridden" (and is therefore exempt from the multiplier) **iff its value differs
from its const default**. Consequence: setting a field *explicitly to its default
value* is indistinguishable from leaving it unset — it **will be
multiplier-scaled**.

Example: with `multiplier = 0.5` and `orphan = 0.75` (which *equals* the default),
`orphan` is treated as unset and scaled to `0.375`, **not** held at `0.75`. If you
mean "hold orphan at exactly 0.75 while halving the others," you cannot express it
through the multiplier path — set every severity explicitly to a non-default value
and omit the multiplier instead.

This ambiguity is **accepted and documented**, not removed: the struct field is a
plain `f64` (not `Option<f64>`), so "explicitly set" and "unset" are not
distinguishable at resolve time.

---

## Cost metric — `token_proxy` is a PROXY, not a real tokenizer (ADR-003 / NFR-08)

The token-weighted cost metric is `cost_tokens = Σ token_proxy(result)` over the
returned set (the tokens an agent pays to *read* the set). `k` (set size) is a
**secondary** axis derivable from `entries.len()` — the same `k` carries different
cost when token loads differ. `token_proxy` counts tokens over the **payload an
agent reads** (`title + content`), not the score metadata. `char/4` is explicitly
**rejected** (it ignores vocabulary and mis-ranks sets).

`token_proxy` is **two-tier**, and which tier produced a number changes its
fidelity:

| Tier | When it engages | Fidelity |
|------|-----------------|----------|
| **Faithful (default)** | The embedding model's `tokenizer.json` is present in the resolved model cache. | Real subword token count via the `tokenizers` crate (same family as all-MiniLM). Deterministic; the trusted signal. |
| **Documented fallback** | The tokenizer cannot be loaded in the eval context. | Whitespace-and-punctuation **word count × 1.3**. Known error: under-counts subword-split rare tokens, over-counts on punctuation-heavy text; **empirically within roughly ±20%** of the subword count on knowledge-base prose. |

The tier is **logged once per process** (`target: "eval::cost"`) so downstream
consumers read cost figures with the right confidence. **Read cost numbers with
the tier in mind:** a fallback-tier run carries the ±20% band; a faithful-tier run
does not. The cost metric is **advisory** in the report — any growth vs baseline
is reported (ε = 0.0) but it **blocks nothing** (`eval report` exit code
unchanged); see [eval-harness.md](./eval-harness.md#cost-growth-is-advisory).
