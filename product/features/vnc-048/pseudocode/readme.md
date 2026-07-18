# Component 5 — README canonical restore procedure (`README`)

## Purpose

The README is the canonical home for the per-slug restore sequence (FR-16/AC-12/OQ-3, SR-07).
The daemon's `project register` output alone is not sufficient discovery; import's `--slug`
help points here. Not code — this file specifies the doc section's required content and the
assertions the tester will make against it.

## Section to add

A new section (e.g. "Backup and restore a per-slug project") documenting the load-bearing,
supported sequence. Required content:

### Backup (export)

```
# exec into the container (HOME=/data), then:
unimatrix --project-dir <dir> export --slug <slug> -o dump.jsonl
# prints to stderr: exported N entries, M audit rows → dump.jsonl
```

Note: `--slug` targets the running project's actual per-slug store
(`{base}/<slug>/unimatrix.db`), not the CLI path-hash store. `exported 0 entries` means the
resolve found an empty/wrong store — check the slug and `--project-dir`.

### Restore (import) — CANONICAL sequence (order is load-bearing)

```
1. project register <slug>     # creates {base}/<slug>/{unimatrix.db, vector}, writes [[projects]]
2. stop                        # daemon releases per-slug stores; the live-PID gate clears
3. import --slug <slug> -i dump.jsonl
4. start                       # daemon boots, loads the rebuilt index; slug serves restored corpus incl. vector search
```

Must explicitly state:

- **Why `stop` is mandatory:** importing while the daemon is live hard-errors (a live daemon
  would clobber the rebuilt vector index at shutdown). This is a refusal, not a warning; there
  is no `--force` override.
- **Restore target must be a freshly-registered slug** (empty `audit_log`). Re-importing into
  an already-used slug fails loud with "register a fresh slug and import there" — register a
  new slug and import into it.
- **Vector search works from `start` onward** — the rebuilt `{base}/<slug>/vector` index is the
  one the daemon loads.

## Data flow / integration

Import's `--slug` help text (Component 4) references this section by name. Keep the section
heading stable so the help pointer and the AC-12 README assertion match.

## Error handling

N/A (documentation). The operator-facing failure messages themselves live in Components 1 and
3; the README describes the happy-path sequence and names the two refusals so the operator
recognizes them.

## Key test scenarios (hints)

- **AC-12 / FR-16 / R-12:** an assertion (doc test or grep-style check) that the README
  contains the ordered `project register → stop → import --slug → start` sequence.
- **AC-07:** import `--slug` help output contains a pointer to this README section.
