## ADR-004: Non-Destructive Definition Install — Install-If-Absent Default, `--force` Overwrites Unimatrix-Owned Skills Only

### Context

`copySkills` (init.js:130) runs `fs.copyFileSync` on **every** run over every shipped skill file
(docstring: "Overwrites existing unimatrix skills"). A developer who edits an installed skill loses
those edits on the next `init` (AC-01 violation). SCOPE decided the policy (Goal 4): default
install-if-absent, `--force` overwrites Unimatrix-owned definitions with shipped versions, **skills
only** (protocols/agents stay out), rejecting hash-tracking / 3-way merge as needless complexity since
definitions are git-tracked and recoverable. This ADR fixes the mechanism, including how "Unimatrix-
owned" is determined and how `--force` is bounded (SR-04: a writer that re-asserts wiring on `--force`
silently violates AC-02).

### Decision

Replace `copySkills`' blanket overwrite with `installSkills(projectRoot, {force, dryRun}) -> string[]`:

1. **Ownership set = the shipped skills manifest.** A skill directory/file present in the package's
   `skills/` source tree is "Unimatrix-owned". `installSkills` only ever considers files that exist in
   the shipped source; **foreign** files under `.claude/skills/` (skills the package does not ship) are
   never read, written, or deleted — on any path, including `--force` (AC-02).
2. **Default = install-if-absent.** For each shipped skill file, write it **only if the destination
   does not exist**. An existing (possibly edited) Unimatrix-owned skill survives byte-for-byte
   (AC-01). Report each file as `installed` or `kept (exists)`.
3. **`--force` = overwrite Unimatrix-owned skills with shipped versions.** With `force:true`, shipped
   skill files overwrite their destinations regardless of prior content. Still scoped to the shipped
   manifest — foreign files are never touched (AC-02).
4. **`--force` is definitions-only.** It changes nothing in the wiring layer; wiring stays
   always-additive and is not re-asserted by `--force` (AC-02, SR-04). In `bin`, `--force` gates
   `installSkills` only; it is never forwarded to `wire()`.
5. **Scope = skills only.** Protocols (`.claude/protocols/`) and agents (`.claude/agents/`) are not
   brought under install-if-absent/`--force` (SCOPE non-goal). Help/summary output states the
   skills-only boundary explicitly so "parity" is not misread as all-definitions (SR-05).
6. **Dry-run**: prints intended per-file actions with `[dry-run]`, writes nothing (AC-14). Path-
   traversal guard on shipped filenames is retained.

### Consequences

Easier: user skill edits survive re-init (AC-01); `--force` is a clean recovery to shipped versions
with a bounded blast radius (Unimatrix-owned skills, never wiring, never foreign files); no hash store
or merge engine to maintain. Harder: `installSkills` must diff shipped-vs-present per file rather than
blind-copy (more logic than `copyFileSync`, but small and local to init.js — no new module, modularity
OK); the ownership definition ("present in shipped `skills/`") means a skill removed from a future
package release is **not** cleaned from a consumer (left as a known, acceptable gap — recoverable via
git, consistent with the install-if-absent philosophy); operators wanting protocol/agent refresh must
do so out of band until a later feature widens scope. Cross-refs ADR-005 (`--force` routed only to
`installSkills`, never `wire`).
