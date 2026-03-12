# C1: npm Package Structure — Pseudocode

## Purpose

Create the directory layout and package.json files for the two npm packages: the root distribution package (`@dug-21/unimatrix`) and the platform binary package (`@dug-21/unimatrix-linux-x64`). Also bundle skill files into the root package.

## Files Created

### packages/unimatrix/package.json

```json
{
  "name": "@dug-21/unimatrix",
  "version": "0.5.0",
  "description": "Unimatrix knowledge engine for multi-agent development",
  "bin": {
    "unimatrix": "bin/unimatrix.js"
  },
  "scripts": {
    "postinstall": "node postinstall.js"
  },
  "optionalDependencies": {
    "@dug-21/unimatrix-linux-x64": "0.5.0"
  },
  "files": [
    "bin/",
    "lib/",
    "skills/",
    "postinstall.js"
  ],
  "license": "MIT OR Apache-2.0",
  "repository": {
    "type": "git",
    "url": "https://github.com/anthropics/unimatrix"
  },
  "engines": {
    "node": ">=18"
  },
  "publishConfig": {
    "access": "restricted"
  }
}
```

### packages/unimatrix-linux-x64/package.json

```json
{
  "name": "@dug-21/unimatrix-linux-x64",
  "version": "0.5.0",
  "description": "Unimatrix linux-x64 platform binary",
  "os": ["linux"],
  "cpu": ["x64"],
  "files": [
    "bin/"
  ],
  "license": "MIT OR Apache-2.0",
  "publishConfig": {
    "access": "restricted"
  }
}
```

### packages/unimatrix-linux-x64/bin/

Directory placeholder. The actual `unimatrix` binary is populated by CI (C10). Include a `.gitkeep` so the directory exists in the repo.

### packages/unimatrix/skills/

Copy of all 13 skill directories from `.claude/skills/`. These are the bundled skills that `npx unimatrix init` (C4) copies into the target project.

Skill directories to bundle (enumerate from `.claude/skills/`):
- knowledge-lookup
- knowledge-search
- query-patterns
- store-adr
- store-lesson
- store-procedure
- record-outcome
- review-pr
- uni-git
- retro
- store-pattern
- unimatrix-init
- unimatrix-seed

Each directory contains a `SKILL.md` file. Copy the entire directory structure.

## Error Handling

No runtime error handling -- this is static configuration. Validation occurs at npm publish time (C10 CI pipeline validates package.json structure).

## Key Test Scenarios

1. `package.json` for root package has correct `bin`, `optionalDependencies`, `scripts.postinstall`, and `files` fields.
2. `package.json` for platform package has correct `os`, `cpu`, and `files` fields.
3. Both packages have `version: "0.5.0"`.
4. Both packages have `publishConfig.access: "restricted"`.
5. All 13 skill directories are present under `packages/unimatrix/skills/`.
6. `packages/unimatrix-linux-x64/bin/` directory exists.
