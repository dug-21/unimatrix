## ADR-004 nan-021: Derived-Attribution Assertion Contract — topic_signal == feature, No Seed Anywhere in the Path

### Context
AC-03 requires every observation produced by the driven cycle to have `topic_signal == feature`, DERIVED
by the real attribution chain (`extract_topic_signal` → `enrich_topic_signal_with_source`) from the
observed content over the wire — with NO `topic_signal` seeded, neither SQL-injected
(`_seed_observation_sql_lifecycle`) nor struct-injected (`make_stamped_event`) anywhere in this test's
setup or assertion path.

This is the core conversion the feature exists to perform. Today's tests SEED the attribution join:
`_seed_observation_sql_lifecycle` (`test_lifecycle.py:1253`) injects observation rows directly via SQL
with hand-chosen `feature_ids` that become the `topic_signal` column; the #832 regression guards
(`_seed_attributed_observations_832`, BA-1/BA-2) pass an explicit `topic_signal` because the bridge was
un-drivable in-harness. The Rust unit fixture `make_stamped_event(..., topic_signal: Option<String>)`
(`uds/listener/tests/stamp_read.rs:28`) does the same at the struct layer. The real derivation path is
bypassed entirely. nan-021's whole value is driving the wire so the column is DERIVED.

SR-06 (High/Med) is the failure mode: the derivation chain depends on the Bash command's observed content
carrying a parseable feature-ID AND on the cycle-join holding (the #832 stable-session-id contract). A
NEAR-MISS yields `unattributed`, not `feature` — so a weak assertion (e.g. "review returned non-empty")
could PASS on the wrong value, or the cycle-join could break and the test green on a degraded path. The
attribution scanner resolves the final column via a priority chain: declared registry feature → extracted
valid feature-ID → registry-fill → vote → unattributed; `topic_signal == feature` holds when a DECLARED
cycle is active and the content parses to a VALID registry feature-ID.

### Decision
**Assert the value exactly AND assert it was derived — with a structurally seed-free path.**

1. **No seed in setup or assertion (AC-03, hard constraint).** This test's path contains NO call to
   `_seed_observation_sql_lifecycle`, NO `_seed_attributed_observations_832`, NO `make_stamped_event`
   with a `topic_signal`, and NO direct SQL write to the `observations.topic_signal` /
   `observations.topic_source` columns. The ONLY way a `topic_signal` appears is the real cycle crossing
   the wire and the server deriving it. (A spec/test-plan checklist item: grep the test's call graph for
   these symbols → must be absent.)

2. **Load-bearing, explicit Bash content (SR-06).** The driven `Bash` tool call's content carries an
   EXPLICIT, VALID feature-ID token (the literal `feature` under test — Open Question 4) that the
   attribution scanner parses (`extract_topic_signal` recognizes file paths under
   `product/features/<id>/`, `feature/<id>` git checkout, free-text feature-ID tokens). The same content
   is byte-identical across HTTPS and UDS legs (ADR-001 manifest), so both legs derive the SAME column.

3. **Declared cycle for the `declared` resolution.** The workload runs `context_cycle(start, topic=<the
   feature>)` so `enrich_topic_signal_with_source` resolves via the DECLARED-registry-feature arm to
   `topic_signal == feature` (not the weaker extracted/vote arms). The `feature` MUST be a valid registry
   feature for the registered slug, or resolution degrades to `unattributed` (SR-06).

4. **Assert exactly, never "non-empty as proxy" (SR-06).** The assertion is `topic_signal == feature` for
   EVERY observation the driven cycle produced (read back from the per-slug store / review output), AND
   that observations exist (count > 0). A `unattributed` or a command-fragment value (e.g. `apt-get`,
   `ls-files` — the #832 BA-2 demotion case) is a FAILURE, not an empty pass. This converts #832's BA-1
   from a seeded join-level guard into a real HTTPS reproduction and a permanent regression guard for the
   #818/#819 silent-observe family.

### Consequences
- **Easier:** the attribution chain is proven end-to-end over the real wire for the first time; the
  seed-free path means a green result genuinely means "derivation worked over HTTPS," closing the gap the
  feature exists to close. The #832 regression guard becomes a live reproduction (stronger than the
  seeded BA-1). The explicit feature-ID token makes the derivation deterministic and debuggable.
- **Harder:** the test is sensitive to the attribution chain's real behavior — if the shipped chain has a
  cloud-path defect (degraded `unattributed`), the test goes RED (correctly — that is a separate bugfix,
  not a tolerance to relax; Non-Goals). The Bash content and the declared `feature` must be coordinated
  with the registered slug's registry so resolution hits the `declared` arm; a mismatch silently degrades
  to `unattributed` and must be caught by the exact-value assertion, not masked. The seed-free constraint
  forbids the convenient SQL shortcut every existing cycle-review test uses — the wire MUST be driven.

Related: AC-03, SR-06; #832 (the cycle-join / stable-session-id root cause), #818/#819 (silent-observe
family this guards). Attribution chain: `uds/hook.rs:extract_event_topic_signal`,
`unimatrix-observe/src/attribution.rs:extract_topic_signal`, `uds/listener.rs:enrich_topic_signal_with_source`.
Forbidden seeds: `_seed_observation_sql_lifecycle` (`test_lifecycle.py:1253`),
`_seed_attributed_observations_832`, `make_stamped_event` (`uds/listener/tests/stamp_read.rs:28`). Pairs
with ADR-001 (the stable identity that holds the cycle-join) and ADR-003 (the review the derived cycle
makes comparable).
