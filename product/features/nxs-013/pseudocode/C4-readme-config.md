# C4: README Configuration Section

## Purpose

Update the README to present per-project config as the canonical location and remove the `/etc/unimatrix/config.toml` bind-mount reference from the container quickstart.

## Target File

`README.md`, two locations:
1. Line 62 -- container description sentence
2. Lines 238-243 -- Configuration section opening

## Current State

### Line 62 (container description)
```
Data persists in the `unimatrix-data` named volume at `/data`. Optional config override via read-only bind mount at `/etc/unimatrix/config.toml`.
```

### Lines 238-243 (Configuration section)
```markdown
## Configuration

Unimatrix loads configuration from two optional TOML files at server startup. When neither file is present, all compiled defaults apply and no existing behavior changes.

- `~/.unimatrix/config.toml` — global config, applies to every project on the machine.
- `~/.unimatrix/{project-hash}/config.toml` — per-project override; values here shadow the global file, which shadows compiled defaults. List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely — there is no append behavior.
```

## Pseudocode

### Edit 1: Line 62

```
REPLACE the sentence:
  "Optional config override via read-only bind mount at `/etc/unimatrix/config.toml`."
WITH:
  "Config lives in the data volume; customize via `unimatrix config` or set `UNIMATRIX_CONFIG` for external override."
```

Keep the preceding sentence about data volume unchanged.

### Edit 2: Lines 240-243

```
REPLACE the opening paragraph and bullet list:

OLD:
  Unimatrix loads configuration from two optional TOML files at server startup. When neither file is present, all compiled defaults apply and no existing behavior changes.

  - `~/.unimatrix/config.toml` — global config, applies to every project on the machine.
  - `~/.unimatrix/{project-hash}/config.toml` — per-project override; values here shadow the global file, which shadows compiled defaults. List fields ...

NEW:
  Unimatrix loads configuration from up to two optional TOML files at server startup. When neither file is present, all compiled defaults apply.

  - `~/.unimatrix/{project-hash}/config.toml` — **primary** (per-project). Written automatically on first run. This is the canonical config location.
  - `~/.unimatrix/config.toml` — **defaults** (global). Optional cross-project defaults; values here apply to all projects unless overridden per-project. List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely — there is no append behavior.
```

Key changes:
- Per-project listed FIRST (was second).
- Per-project labeled "primary" and "canonical".
- Global labeled "defaults".
- Per-project notes it is written automatically on first run.
- Replace semantics explanation preserved on the global entry.

## Constraints

- C-06: Edits constrained to line 62 and lines 240-243 only -- no broad README revision.
- NFR-05: No other sections modified.
- Preserve existing explanation of replace semantics for list fields.

## Error Handling

Not applicable (prose edit).

## Key Test Scenarios

1. Per-project config is presented first in the bullet list.
2. Per-project is labeled "primary" and "canonical".
3. Global is labeled "defaults".
4. No reference to `/etc/unimatrix/config.toml` as primary container config pattern.
5. Replace semantics for list fields still documented.
6. PR diff shows changes only within the specified line ranges.
