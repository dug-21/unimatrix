## ADR-002: Behavioral Signature Catalog — One Shared-Pass RegexSet, Externalized, validate()-Bounded, Domain-Neutral, v1 = error/refusal Only

### Context
The class-count signals in Surface B (`class_counts`) need to match behavioral signatures in each transcript delta. Three failure modes: (1) the fold runs on the hot ingest path under the buffer lock — a pathological catalog (catastrophic backtracking, huge set) adds latency to every merge (SR-03 of the prior assessment; hot-path cost); (2) domain leakage — hard-coded SDLC literals; (3) unbounded growth. dsn-001 established config externalization; #4591's category allowlist proved runtime-extensible-but-capped, validated-at-load; `RetentionConfig` (`config.rs`) is the `#[serde(default)]` + `validate()`-rejects precedent.

Critically (ass-078 RQ-1): all signals share ONE `RegexSet` pass — cost is one byte scan per delta, not one per pattern. The cap therefore primarily bounds domain assumptions, secondarily the hot path.

This ADR is the producer-only successor of the prior crt-054 ADR-003 (#5001). Two things are **removed** from the prior version against the new SCOPE:
- the `reread` / `compaction` regex classes and the `role ∈ {Generic, Compaction, Reread}` field — there is no in-stream compaction or re-read marker; compaction is Surface A (ADR-007), and the compaction-gated reread reckoning is entirely a crt-055 review concern;
- the `token_bytes_per_unit` field — bytes is the binding honest unit (SCOPE §Out-of-scope; SR-06); no token estimate, no token-named config field.

The crt-055 producer contract (binding, now FINAL) fixes v1 classes and their indices: `0 = error`, `1 = refusal`, and fixes `MAX_SIGNAL_CLASSES = 16` — a shared compile-time constant that crosses the producer/consumer boundary via the `ActivitySnapshot.class_counts: [u32; MAX_SIGNAL_CLASSES]` array. It must **equal** crt-055's constant exactly (not `≤ 16`, not "decided jointly"); crt-055 has fixed it at 16.

### Decision
Externalize the catalog as a `[transcript_signals]` config section (sibling to `[retention]`), compiled once at load into one shared `RegexSet`, `validate()`-bounded.

Shape — per-entry `{ class_name, pattern, enabled }`, `#[serde(default)]`. **No `role` field. No `token_bytes_per_unit`.** A small **domain-neutral** default set: behavioral signatures only — model refusal phrasings, provider hard/overload errors — never SDLC literals.

One shared pass: a `SignatureScanner` compiles a single `regex::RegexSet`; the fold (ADR-001) runs `set.matches(bytes)` once per delta; each matched class index increments `class_counts[i]` by 1 — match-**presence** per delta (bounded by `delta_count`, avoids runaway), not total occurrences. A delta may match multiple classes in the one pass.

Hard cap via `validate()`: reject `> MAX_SIGNAL_CLASSES` enabled classes, reject invalid regex (compile at load, fail startup loud), reject duplicate `class_name`. The `regex` crate is linear-time (no backreferences/lookaround) — structurally precludes catastrophic backtracking, the hot-path worst case.

**`MAX_SIGNAL_CLASSES = 16`, PINNED — exactly equal, not `≤ 16`, not "decided jointly."** It is a shared compile-time constant crossing the producer/consumer boundary (the `ActivitySnapshot.class_counts` array width), so it must EQUAL crt-055's constant exactly; crt-055 has fixed it at 16 (dsn-001/#4591 precedent). Open Q2 is resolved.

**Default catalog = `error` + `refusal` only** — tiny, high-precision, anchored patterns. Because the fold is content-opaque (ADR-005), the false-positive rate of these counts can NEVER be audited after ship — there is no stored text to inspect. Therefore the default patterns must be **calibrated against real transcripts during delivery before locking**, and the set kept minimal (under-catalog; domains extend via config). The counts these patterns yield are **directional, not precise** — a signal that a cycle hit errors/refusals, not an exact tally; crt-055 surfaces them as such.

Numbers are class-counts only, never matched text (content opacity, ADR-005).

### Consequences
Easier: marginal per-pattern cost ~0 (one shared pass); catalog stays domain-agnostic; invalid configs die loud at startup; the cap bounds the assumption surface; index→class is stable config order, so crt-055 can land `class_counts[0]`→error / `[1]`→refusal columns by fixed index.

Harder: must ship a sensible domain-neutral default (product judgment); `validate()` + compile add a startup path; the scanner is injected into every `TranscriptBuffer` construction (ADR-001).

Cross-refs: dsn-001 / #4591 (externalization precedent), ADR-001 (scanner injection + fold), ADR-003 (`class_counts` exposed in the snapshot), ADR-005 (numbers-not-text bright line). Removed vs prior ADR-003: `reread`/`compaction` classes, `role`, `token_bytes_per_unit`.
