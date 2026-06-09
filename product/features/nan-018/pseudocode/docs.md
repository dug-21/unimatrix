# Component: Band-1/2 docs — `docs.md`

**Wave**: 2 (deferrable; ZERO code coupling to Wave-1 — NFR-04, R-14)
**Location**: `docs/testing/eval-harness.md` (modify) + new Band-2 guides under `docs/testing/`.
**ADR**: ADR-001 (clean_replacement amplified knob), ADR-003 (cost proxy error bars). **Risks**: R-07 (proxy labeling), R-14 (entanglement).

## Purpose

Document the new harness capabilities so a dev (human or agent) can author/migrate/sweep without
reverse-engineering code. These are prose artifacts that REFERENCE Wave-1 behavior — they import no
Wave-1 code and are NOT a Wave-1 code dependency (Wave-1 acceptance passes with these absent).

## Band-1: `docs/testing/eval-harness.md` (modify, FR-24)

Add sections covering: tunability levers, trust metric class, token-weighted cost, fixture corpus,
two-corpus model, drift guard. Capability-level overview; points to the Band-2 guides for detail.

## Band-2 guides (new, FR-25 — sufficient to author/migrate/sweep)

1. **Fixture-corpus authoring guide** — how to author the five status shapes; alias discipline
   (never literal ids); property assertions (redirect-to-head / absence / rank-below) with the
   ASYMMETRIC rank-below semantics spelled out (A absent => pass; B absent => FAIL); the
   authoring-DEPTH obligation (ADR-004 §5): enough variation, especially deprecated-but-connected,
   that the steepness crossover sits in a bracketed range. States the corpus is NOT frozen
   (one revision pass anticipated).
2. **Schema-migration runbook** — when the drift guard trips: re-stamp the corpus, bump
   `migration_number`, update assertions; when to bump `MANIFEST_VERSION` (input-set change). The
   diverged-dimension message tells you which class changed.
3. **Two-corpus model** — fixture = primary/durable (property assertions, carries the stamp,
   trust authority); snapshot = realism/ephemeral (realistic P@5/MRR, re-snapshot on drift).
   When to use which; how to re-snapshot.
4. **Config-knob reference** — for each of the 7 levers + multiplier: meaning, valid range, default
   (the crt-014 const), effect. MUST flag:
   - `clean_replacement` is an **AMPLIFIED knob** (moves base penalty AND clamp ceiling together,
     same direction — ADR-001) so a sweep is read as amplified, not isolated.
   - multiplier scales SEVERITIES only (not hop_decay / max_traversal_depth); per-field override wins.
   - the **cost-metric proxy caveat** (ADR-003 / NFR-08): token_proxy is a PROXY, not a real
     tokenizer; faithful subword tier default, word×1.3 fallback within ~±20%; labeled so
     downstream reads numbers with correct fidelity (R-07).
   - the penalty config is **eval/measurement-only** (ADR-006): NOT license to re-tune deployed
     defaults; deployed values stay at crt-014 v1 consts.

## Wave independence (NFR-04, R-14 — LOAD-BEARING)

These docs reference Wave-1 BEHAVIOR conceptually; they contain no code Wave-1 imports. A Wave-2
doc becoming a Wave-1 code dependency is a defect. The Wave-1 acceptance suite (AC-01…09 + AC-14)
must pass with `docs/` (the new guides) ABSENT.

## Data flow / Error handling

Documentation artifacts; no runtime logic, no error paths.

## Key test scenarios

- **Doc-review checklist (AC-10/AC-11)**: authoring guide, migration runbook, two-corpus doc,
  config-knob reference all exist; a dev could author a scenario / migrate the corpus / run a sweep
  from the docs alone.
- **Proxy-labeling gate (R-07.3, NFR-08)**: config-knob reference labels token_proxy as a proxy
  with its error bars.
- **Amplified-knob note (ADR-001)**: config-knob reference flags `clean_replacement` as amplified.
- **Eval-only boundary (ADR-006)**: docs state penalty config is measurement-only, not a deploy re-tune.
- **Wave-1-alone (R-14, NFR-04)**: Wave-1 acceptance passes with these docs absent.
