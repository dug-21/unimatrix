## ADR-001: Keep-Target by Repointed-Entry Object Identity, Not Command-String Compare

### Context

vnc-031 generalizes the cross-matcher-group prune (Step 3b shape) to run for
*managed* events, removing every Unimatrix-owned hook that is not the freshly
written managed entry. The prune must answer one question per uni-owned entry:
"is this the entry to keep, or a stale one to remove?"

SCOPE.md OQ-1 proposed comparing each kept entry's `command` against the
just-written `newHookEntry.command`. SR-01 (High/Med) shows that compare is
fragile: if the command Step 3 actually placed differs from a re-derived
reference string by even one byte (whitespace, `LD_LIBRARY_PATH=` prefix,
arg order, future quoting changes in `buildHookClientCommand`), the keep-target
fails its own equality test and the prune deletes the entry meant to be kept —
zeroing all uni hooks for that event. The dogfood script hit exactly this class
of bug (#4931): a naive token compare shattered a quoted spaced path and nuked
the 8 freshly written hooks. The script *had* to reconstruct the target because
it ran *after* `mergeSettings` as a separate process and never saw the entry
object. `mergeSettings` has no such excuse: it is the writer.

### Decision

The keep-target is the **object reference** Step 3 produced/repointed for the
event — never a command string.

Step 3 already creates exactly one `newHookEntry` per event and either assigns
it into the managed group (`matcherGroup.hooks[existingIndex] = newHookEntry`)
or pushes it (`matcherGroup.hooks.push(newHookEntry)` / new-group create). We
capture that same object reference into a per-run map `keptEntryByEvent[event] =
newHookEntry`. The cross-group prune (new Step 3c) keeps an entry iff
`entry === keptEntryByEvent[event]` (`Object.is` / strict reference equality);
every other `isUnimatrixHook(entry) === true` entry, in any matcher group for
that event, is removed.

Because `keptEntryByEvent[event]` is the *identical object* that now lives in
the managed group, the kept entry is guaranteed present and guaranteed unique by
construction — there is no string to diverge. The repoint in Step 3 already
dedups within the managed group, so at most one uni entry there is the kept one;
the prune removes uni entries in all *other* groups unconditionally (ADR-002).

**Fail-loud invariant (AC-02):** after Step 3c, every managed event must hold
exactly one uni-owned entry, and it must be `keptEntryByEvent[event]`. The
design asserts this is impossible to violate by construction — the kept object
is never a prune candidate and was placed by Step 3 — so the invariant is a
guard against future regressions, not a runtime branch. The test suite encodes
it (`test_each_event_has_exactly_one_unimatrix_entry` extended cross-group); a
zero-uni-hook managed event is a hard test failure, never a silent pass.

### Consequences

Easier: SR-01 is closed by construction — divergence between "what was written"
and "what is kept" is unrepresentable because they are one object. No quoting,
no token heuristic, no `LD_LIBRARY_PATH` special-case (which the script needed,
#4931). Idempotency is trivial: on a second run Step 3 repoints the existing
managed entry in place and captures *that* reference, so the prune is a no-op.

Harder: the prune must run in the same pass that holds the live references —
it cannot be a detached post-processor over re-read content. This couples Step
3c to Step 3's in-memory `content` (acceptable; it is the same function). Tests
that build expected state by command equality must instead assert "exactly one
uni entry, in the managed group" — identity, not string (the test guidance in
ADR-002).

Cross-references: ADR-002 (prune-all-outside-managed rule consumes this keep
target); SR-01, SR-07; supersedes the script's command-token keep-rule (#4931),
which existed only because the script lacked the object reference.
