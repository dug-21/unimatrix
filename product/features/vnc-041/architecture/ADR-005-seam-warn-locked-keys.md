## ADR-005: The seam WARN derives its locked surface from `is_per_slug_overlayable`==false over the keys the per-slug file actually SETS; one warn per locked key per boot, WARN-only

### Context
SCOPE Goal 4 / AC-04 / R-13 / OQ-3 (RESOLVED, per ignored key, once per boot): when
`resolve_slug_config` (http_provision.rs:310) encounters a per-slug file that SETS a
global-locked key, it must emit a `tracing::warn` naming the ignored key + slug, instead of
the current silent ignore. Today only the `*_sha256` hash-pin divergence warns (inside
`merge_configs`, config.rs:3911/3941); the rest of the locked surface (transport,
`permissive`, `rayon_pool_size`, the embedding/nli descriptors when no pin diverges) is
ignored with no signal. vnc-040 ADR-002 (#5206) explicitly named this seam WARN an
"OPTIONAL future enhancement" — vnc-041 R-13 is that follow-up.

Two hard constraints shape the mechanism:

- SR-02 (High) / SR-07: the locked surface MUST derive from `is_per_slug_overlayable`
  returning false over the registry A owns — NEVER a hand-list in B. Critically,
  "GlobalLocked" is NOT one mechanism: `permissive` has no `UnimatrixConfig` field
  (daemon process flag); `tls`/`http` are transport, never read at the seam;
  `*_sha256` is merge-locked (global-wins, conditional on the global pin being Some);
  `rayon_pool_size` is the shared-pool descriptor. The WARN must treat all of them
  uniformly via the registry, not by reasoning about each mechanism.
- SR-06 (Med): R-13 is WARN-ONLY. No rejection, no new error path, no behavior change
  beyond the log line. The value is ALREADY ignored by the merge; the WARN only adds the
  signal.

The detection problem: `resolve_slug_config` deserializes the file straight into the typed
`UnimatrixConfig` via `load_single_config`. Once typed, a key SET-to-its-default is
indistinguishable from a key ABSENT — so the typed struct cannot tell whether the operator
*set* a locked key. Detecting "the file SETS a locked key" requires inspecting which keys
are PRESENT in the raw TOML.

### Decision
Add a WARN pass to `resolve_slug_config`'s file-present arm (the no-file arm is untouched —
byte-for-byte fallthrough preserved). The pass:

1. Parses the file text once into a raw `toml::Value` table (alongside, not replacing, the
   existing `load_single_config` typed parse) to enumerate the keys/sections the file
   actually CONTAINS — section-and-key granularity matching the registry's dotted `key`
   identifiers (e.g. `inference.embedding_model_sha256`, `tls`, `permissive`).
2. For each present key, consults `is_per_slug_overlayable(key)`. If it returns `false`
   (the key is in the registry as `GlobalLocked`, OR is an unknown/non-seam key — the
   conservative default), emit ONE `tracing::warn!(slug = %slug, key = %key, "per-slug
   config sets a global-locked key; value is ignored (managed globally)")`. The surface is
   thus DERIVED from the registry at runtime — no key list restated in B (SR-02/SR-07).
3. Granularity (OQ-3): one warn per ignored locked key, emitted at resolution time. The
   resolver runs once per slug at boot (per the per-slug loop, main.rs:1089), so this is
   once-per-boot per locked key — no per-request spam.
4. The pre-existing `*_sha256` divergence warn inside `merge_configs` is left UNCHANGED;
   the two warns are complementary (the merge warn fires on a *diverging* pin even if the
   key were overlayable-shaped; this pass fires on any *set* locked key). Mild duplicate
   logging for a set-and-diverging `*_sha256` is acceptable signal, not a defect.
5. WARN-ONLY (SR-06): the value remains ignored exactly as today; the merge/validate flow,
   the return type (`Cow<UnimatrixConfig>`), and all error paths are UNCHANGED. The raw
   parse for WARN must not introduce a new failure mode — if the raw parse fails, the
   existing `load_single_config` in the same arm already returns a loud, slug-named
   `ServerError::Config`; the WARN pass degrades to no-warn on an uninspectable file and
   NEVER turns a parseable file into an error.

### Consequences
- Easier: the silent-ignore support-ticket generator (R-13) becomes a self-explaining log
  line naming the key and slug, on exactly the hand-authored-config model Feature A targets.
- Easier: the locked surface is the registry's complement — adding a `GlobalLocked` entry to
  A's classification automatically extends the WARN surface; B never drifts (SR-02/SR-07).
- Easier: uniform treatment of the heterogeneous locked mechanisms — `permissive`, `tls`,
  `http`, `*_sha256`, `rayon_pool_size` all warn through the SAME `is_per_slug_overlayable`
  check; B reasons about the registry, not the mechanism.
- Cost: the file-present arm now parses the TOML twice (typed + raw `toml::Value`) — one
  extra parse per slug-with-a-file, at boot only, on a ≤64 KiB file. Negligible; the typed
  parse stays the source of the merged config.
- Bounded by SR-06: WARN-only — no rejection, no resolution change, no signature change. The
  no-file fallthrough arm is not touched, so the AC-02 (vnc-040) byte-for-byte sentinel for
  the single-project majority is unaffected.
- Known interaction: unknown (non-registry) keys in a per-slug file also warn (the
  conservative `false` default). This is desirable — a typo'd or non-seam key set by an
  operator is also silently ineffective and worth surfacing — but the spec/tests should
  note WARN fires for unknown keys too, not only registry-`GlobalLocked` keys.
- Cross-references vnc-040 ADR-004 (#5217 — the registry consumed here), vnc-040 ADR-002
  (#5206 — the documented residual this WARN closes), ADR-003 (the seed-time annotation that
  this WARN is the runtime mirror of — both derive from the same registry).
