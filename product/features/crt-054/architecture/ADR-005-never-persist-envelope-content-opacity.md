## ADR-005: Never-Persist Envelope — Running-Fold-Only, Content-Opaque, No Token-Named Field; Bytes Is the Honest Unit

### Context
The load-bearing distinction keeping crt-054 categorically separate from ass-077's rejected R-A. R-A reads the assembled buffer at persist time and parses content to derive a number — adding a content-bearing read to the storage path, eroding the guarantee that storage never touches conversation bytes. crt-054 must not become R-A. Two bright lines must be testable invariants, not prose (SR-06): the **token-exclusion line** (a token-named/cost field re-imports FinOps drift and re-opens the prior crt-054↔crt-055 contradiction) and the **R-A line** (any signal needing the assembled buffer silently crosses into a content read).

This ADR is the producer-only successor of the prior crt-054 ADR-006 (#5004). The scope shifts: crt-054 now **persists nothing of Surface B** — the consumer (crt-055) lands the columns. So the envelope here governs (a) what crt-054 *produces* (Surface B `activity_snapshot()` + Surface A `compaction_events`) and (b) the structural guarantee that neither carries content. The "no content field on `RetrospectiveReport`/`CycleReviewRecord`" gate is now crt-055's to hold; crt-054's contribution is that the values it hands over are structurally content-free.

### Decision
Bind the envelope as structural, testable invariants:

1. **Running-fold-only.** Every Surface B signal MUST be expressible as a running fold over deltas (a counter), never a query over the assembled buffer. The content read stays on the ingest path (`apply_delta`); `activity_snapshot()` reads only integers. This is why thrash/rolling-hash and similarity are deferred (they need content shape).
2. **No content field on what crt-054 hands over.** `ActivitySnapshot` is `u64`/`u32`/`[u32; N]` only — no `Vec<u8>`/`String`/`&[u8]`, no `Display`, metadata-only `Debug`. `compaction_events` columns are `id`/`session_id`/`compacted_at`/`high_water` — integers and a session key, no payload, no `tracing` of content.
3. **No content-bearing read on any crt-054 read path.** `take_transcripts_for_feature`/`snapshot()` (byte-bearing) is not invoked by `activity_snapshot()` or its collector.
4. **No token-named field anywhere in crt-054** — no config field, no struct field, no column. `bytes_total` is the honest unit (`bytes`, not tokens). There is no `token_bytes_per_unit`, no "tokens (est.)". Real token accounting, if ever needed, is a separate harness-usage-stream feature.
5. **Disqualifying acceptance test:** "does this counter control / bill / schedule / block execution?" If yes, out of lane (RQ-8 vision boundary).
6. **Counters are categorically a network byte-counter** (vnc-025 ADR-002 #4740): a folded `bytes_total` is indistinguishable from counting TCP bytes; a regex match-count is a number, not the matched text.

### Consequences
Easier: the R-A / FinOps lines become compile- and test-enforced, not reviewer vigilance; "just add one signal" pressure hits the running-fold-only wall; the token contradiction with crt-055 is closed structurally (no token surface exists to drift).

Harder: assembled-buffer-shaped signals (dedup, similarity) are off-limits to v1; any future content-shaped signal needs a measured-evidence + content-opacity decision.

Cross-refs: ass-077 RQ-7 (R-A rejected), ass-078 RQ-7 (five-claim proof), vnc-025 ADR-002 #4740 (network byte-counter equivalence), ADR-001/ADR-003 (integers-only by construction), ADR-007 (`compaction_events` is content-free too). Removed vs prior ADR-006: the `token_bytes_per_unit` read-time estimate (no token surface at all now); the "columns on `CycleReviewRecord`" framing (crt-055 owns those columns).
