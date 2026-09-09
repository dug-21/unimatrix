# Collective onboarding directive

This file is an **executable procedure**, not prose to interpret. A participating Program's onboarding
agent runs the steps below **exactly**, in order, using only the fixed values carried in this file, the
one required path input named at trigger time, and the target file's own intrinsic public structure.
Following this directive on a clean repository, **given the same target path**, produces a
**deterministic, byte-for-byte reproducible** change; two independent runs with this directive and the
same target path as input produce an identical change set. The diff it produces — never the agent's
assertion that it followed the steps — is the evidence that it was followed.

The procedure edits **only** this repository, under this repository's own authority. It reads and writes
no identity value, no adopted-release value, and no digest. It **does not guess** which file to edit: the
product-owner orientation file is supplied to it. If that path is not supplied, does not resolve to
exactly one file, or does not pass the anchor check below, the procedure **installs nothing and stops**
(fail-closed). It never writes a partial change.

## What a successful run produces

Exactly two changes, both authored by this repository:

1. A copy of the projected participation skill directory into this repository's skill-discovery root,
   so ordinary skill discovery finds it.
2. The fixed **hook block** below, inserted once at the top of the supplied product-owner orientation
   file, so ordinary orientation reads it and is led to the authoritative surface.

## Required input

```
TARGET_PATH  = <the product-owner orientation file, supplied by the owner at trigger time>
```

`TARGET_PATH` is the path to this repository's product-owner orientation skill file (of the form
`.claude/skills/<skill>/SKILL.md`). It is named by the owner at manual-trigger time — the same moment the
owner points the product-owner agent at this directive. It is **not** an identity, release, or digest
value, and it is not detected from repository contents: the procedure refuses to select a target on its
own. If it is absent, the procedure fails closed.

## Fixed operands (carried verbatim; do not modify)

```
SKILL_SRC        = ".collective/skills/collective-participation/"   # projected skill directory to copy
SKILL_DEST_ROOT  = ".claude/skills/"                               # intrinsic skill-discovery root
SKILL_DEST       = ".claude/skills/collective-participation/"      # copy destination
```

No skill-name lookup table is carried, and no product-owner skill name is baked into this file. The
target is the supplied path; its expected frontmatter name is computed from that same path.

### The hook block (HOOK_BLOCK) — insert verbatim, byte-for-byte

The block below, including its opening and closing delimiter comment lines, is a single fixed literal.
It carries no logic and no per-repository value; it is identical for every participating repository.
Insert it exactly as written; do not reword, reformat, interpolate, or substitute any part of it.

```
<!-- collective:participation-hook -->
This repository participates in the collective. The authoritative collective surface is `.collective/`.
During orientation, read `.collective/README.md` and invoke the `collective-participation` skill to
resolve this Program's collective identity and adopted baseline. The `.collective/` surface is
collective-authoritative and is not edited locally.
<!-- /collective:participation-hook -->
```

## Procedure

Run these steps in order. On any **FAIL-CLOSED** outcome, make no change at all (any change already
staged in this run is discarded) and report the named condition.

```
1. REQUIRE TARGET (fail-closed):
     if TARGET_PATH is absent or empty:
         FAIL-CLOSED "target-path-not-supplied"   (write nothing)
     matches = the file(s) at exactly TARGET_PATH   # the supplied path itself; no globbing, no guessing
     if count(matches) != 1:
         FAIL-CLOSED "ambiguous-or-unresolvable-target"   (write nothing)   # 0 or >=2
     target = matches[0]

2. PARSE AND VALIDATE THE ANCHOR (fail-closed; parse the frontmatter — do NOT substring-match):
     text = read(target)
     The file MUST begin with a YAML frontmatter block: its first line is exactly "---", followed by
     frontmatter lines, terminated by a later line that is exactly "---".
     if there is no such leading frontmatter block, or it does not parse as YAML, or it has no `name` key:
         FAIL-CLOSED "frontmatter-unparseable"   (write nothing)
     name_value = the frontmatter `name` value, with surrounding quotes removed and whitespace trimmed
                  (so `name: "my-skill"`, `name: my-skill`, and `name:   'my-skill'` all yield the same value)
     expected   = the name of the directory that immediately contains TARGET_PATH
                  (the "<skill>" segment of ".claude/skills/<skill>/SKILL.md" — computed from the path,
                   not carried in this file)
     if name_value != expected:
         FAIL-CLOSED "frontmatter-name-mismatch"   (write nothing)
     insertion_point = the position immediately AFTER the frontmatter's closing "---" line
                       (the very top of the skill body, before any body content)

3. IDEMPOTENCE GUARD:
     if HOOK_BLOCK (the delimiter-bounded block above) already appears in text:
         skip step 4 (already installed — not a failure)

4. INSERT THE HOOK (deterministic position):
     at insertion_point, write:  one blank line, then HOOK_BLOCK verbatim, then one blank line
     Do not modify any other part of the file.

5. COPY THE SKILL (deterministic):
     if the directory at SKILL_SRC does not exist:
         FAIL-CLOSED "skill-source-absent"   (write nothing)
     copy every file under SKILL_SRC to SKILL_DEST byte-for-byte, preserving relative paths;
     create directories as needed; if a destination file already exists, overwrite it only with the
     identical bytes (the copy is idempotent).

6. RESULT:
     the change set = { the single hook insertion in target, the copied skill file(s) }.
     Report the exact paths changed. On any FAIL-CLOSED above, the change set is EMPTY.
```

## Fail-closed conditions (summary)

| Condition | Outcome |
|---|---|
| `TARGET_PATH` absent or empty | No install — `target-path-not-supplied` |
| `TARGET_PATH` resolves to zero or more than one file | No install — `ambiguous-or-unresolvable-target` |
| Target has no parseable leading YAML frontmatter, or no `name` key | No install — `frontmatter-unparseable` |
| Frontmatter `name` (unquoted, trimmed) does not equal the path's own skill-directory segment | No install — `frontmatter-name-mismatch` |
| Projected skill directory absent | No install — `skill-source-absent` |

The procedure never guesses a target, never detects the target by scanning repository contents, never
matches an unquoted literal anchor, and never writes a partial change. The target is the supplied path
only; the anchor is located by parsing the target's frontmatter.

## Properties this procedure guarantees

- **Reproducible, given the supplied path.** Steps 2–5 are pure functions of the fixed operands above,
  the supplied `TARGET_PATH`, and intrinsic frontmatter reads. Given this directive and the same target
  path as input, independent runs on a clean repository produce a byte/diff-identical change set. A
  hand-edited or steered change fails the diff-equality check.
- **Carries no resolved values.** This directive and its hook block contain no collective identity, no
  adopted-release identity, and no content digest, and no product-owner skill name. The supplied path is
  not an identity, release, or digest value. The collective identity and adopted baseline are resolved
  live, at orientation, by the `collective-participation` skill reading this repository's own
  `.collective/` state — never from this file. Altering any stored copy of a value elsewhere cannot
  change what the skill returns.
- **Install is distinct from adoption.** The hook lives on the orientation read-path; the adopted release
  is recorded separately under `.collective/`. They share no field. Running this directive records no
  adoption, and adoption records no install.
- **This repository's own act.** The change set is authored and owned by this repository. Nothing outside
  it is edited, and no external party edits this repository.
