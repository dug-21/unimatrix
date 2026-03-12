# C10: Release Pipeline — Pseudocode

## Purpose

GitHub Actions workflow that builds the Rust binary, packages it into npm packages, publishes to npm, and creates a GitHub Release. Triggered by `v*` tags.

## New File: .github/workflows/release.yml

```yaml
name: Release
on:
  push:
    tags: ['v*']

# Ensure only one release runs at a time
concurrency:
  group: release
  cancel-in-progress: false

permissions:
  contents: write  # For creating GitHub releases

env:
  CARGO_TERM_COLOR: always
```

### Job 1: build-linux-x64

```
JOB build-linux-x64:
    runs-on: ubuntu-latest

    STEPS:
        // 1. Checkout with full history (needed for changelog generation in job 3)
        - uses: actions/checkout@v4

        // 2. Install Rust 1.89 explicitly (not default stable which may be older)
        - uses: dtolnay/rust-toolchain@1.89

        // 3. Cache cargo registry and build artifacts
        - uses: actions/cache@v4
          with:
            path: |
              ~/.cargo/registry
              ~/.cargo/git
              target
            key: linux-x64-release-${{ hashFiles('**/Cargo.lock') }}
            restore-keys: linux-x64-release-

        // 4. Assert patches/anndists exists (C-01, R-07)
        - name: Verify patched dependencies
          run: |
            IF NOT directory "patches/anndists" exists:
                echo "ERROR: patches/anndists directory missing"
                exit 1
            END IF

        // 5. Build release binary
        - name: Build
          run: cargo build --release

        // 6. Strip binary
        - name: Strip binary
          run: strip target/release/unimatrix

        // 7. Validate binary is self-contained (R-03)
        - name: Check shared libraries
          run: |
            ldd target/release/unimatrix
            // Verify no "not found" in ldd output
            IF ldd target/release/unimatrix 2>&1 | grep "not found":
                echo "ERROR: Binary has missing shared library dependencies"
                exit 1
            END IF

        // 8. Smoke test: binary runs on this system
        - name: Smoke test
          run: |
            target/release/unimatrix version
            // Verify output matches expected format
            OUTPUT=$(target/release/unimatrix version)
            IF NOT "$OUTPUT" starts with "unimatrix ":
                echo "ERROR: Unexpected version output: $OUTPUT"
                exit 1
            END IF

        // 9. Run tests
        - name: Test
          run: cargo test --release

        // 10. Report binary size
        - name: Binary size
          run: ls -lh target/release/unimatrix

        // 11. Upload binary artifact
        - uses: actions/upload-artifact@v4
          with:
            name: unimatrix-linux-x64
            path: target/release/unimatrix
            retention-days: 1
```

### Job 2: package-npm

```
JOB package-npm:
    needs: build-linux-x64
    runs-on: ubuntu-latest

    STEPS:
        // 1. Checkout
        - uses: actions/checkout@v4

        // 2. Setup Node.js
        - uses: actions/setup-node@v4
          with:
            node-version: '20'
            registry-url: 'https://registry.npmjs.org'

        // 3. Download binary artifact
        - uses: actions/download-artifact@v4
          with:
            name: unimatrix-linux-x64
            path: packages/unimatrix-linux-x64/bin/

        // 4. Set executable permission
        - name: Set permissions
          run: chmod +x packages/unimatrix-linux-x64/bin/unimatrix

        // 5. Copy skills from .claude/skills/ to packages/unimatrix/skills/
        - name: Bundle skills
          run: |
            mkdir -p packages/unimatrix/skills
            FOR EACH dir IN .claude/skills/*/:
                IF directory contains SKILL.md:
                    cp -r "$dir" packages/unimatrix/skills/
                END IF
            END FOR

        // 6. Extract version from Cargo.toml and validate match
        - name: Validate versions
          run: |
            // Extract version from root Cargo.toml [workspace.package]
            CARGO_VERSION = parse version from Cargo.toml
            // Extract version from tag (strip "v" prefix)
            TAG_VERSION = ${GITHUB_REF_NAME#v}

            // Validate Cargo version matches tag
            IF CARGO_VERSION != TAG_VERSION:
                echo "ERROR: Cargo.toml version ($CARGO_VERSION) does not match tag ($TAG_VERSION)"
                exit 1
            END IF

            // Validate npm package versions match
            FOR EACH package.json IN packages/*/package.json:
                NPM_VERSION = parse version from package.json
                IF NPM_VERSION != CARGO_VERSION:
                    echo "ERROR: $package.json version ($NPM_VERSION) does not match Cargo.toml ($CARGO_VERSION)"
                    exit 1
                END IF
            END FOR

        // 7. Publish platform package FIRST (R-15, C-12)
        - name: Publish @dug-21/unimatrix-linux-x64
          working-directory: packages/unimatrix-linux-x64
          run: npm publish --access restricted
          env:
            NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}

        // 8. Publish root package SECOND (after platform package is available)
        - name: Publish @dug-21/unimatrix
          working-directory: packages/unimatrix
          run: npm publish --access restricted
          env:
            NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

### Job 3: create-release

```
JOB create-release:
    needs: package-npm
    runs-on: ubuntu-latest

    STEPS:
        // 1. Checkout with full history
        - uses: actions/checkout@v4
          with:
            fetch-depth: 0

        // 2. Generate changelog from conventional commits
        - name: Generate changelog
          id: changelog
          run: |
            // Find the previous v* tag
            PREV_TAG = git describe --tags --abbrev=0 HEAD^ 2>/dev/null || ""

            IF PREV_TAG is empty:
                // First release: all commits
                RANGE = ""
            ELSE:
                RANGE = "${PREV_TAG}..HEAD"
            END IF

            // Extract conventional commits grouped by type
            FEATURES = git log $RANGE --format="%s" | grep "^feat:" | sed "s/^feat: //"
            FIXES = git log $RANGE --format="%s" | grep "^fix:" | sed "s/^fix: //"
            BREAKING = git log $RANGE --format="%b" | grep "BREAKING CHANGE"

            // Build changelog body
            BODY = ""
            IF FEATURES not empty: BODY += "### Features\n" + bullet list
            IF FIXES not empty: BODY += "### Fixes\n" + bullet list
            IF BREAKING not empty: BODY += "### Breaking Changes\n" + bullet list

            // Write to output
            echo "body<<EOF" >> $GITHUB_OUTPUT
            echo "$BODY" >> $GITHUB_OUTPUT
            echo "EOF" >> $GITHUB_OUTPUT

        // 3. Create GitHub Release
        - name: Create release
          uses: softprops/action-gh-release@v2
          with:
            name: ${{ github.ref_name }}
            body: ${{ steps.changelog.outputs.body }}
            draft: false
            prerelease: false
```

## Error Handling

| Condition | Behavior |
|-----------|----------|
| patches/anndists missing | Build job fails early with clear message |
| ldd shows missing .so | Build job fails before publish |
| Cargo version != tag version | Package job fails before publish |
| npm version != Cargo version | Package job fails before publish |
| Platform package publish fails | Root package publish skipped (job fails) |
| npm auth fails (NPM_TOKEN invalid) | Publish step fails with npm error |
| Binary not executable | Smoke test fails |

## Key Test Scenarios

1. Push `v0.5.0` tag -> workflow triggers.
2. Build produces stripped binary named `unimatrix`.
3. `ldd` check passes (no missing shared libraries).
4. Smoke test: `unimatrix version` outputs `unimatrix 0.5.0`.
5. Version validation: Cargo.toml version matches tag, npm versions match Cargo.
6. Platform package published before root package.
7. If platform publish fails, root publish does not run.
8. GitHub Release created with changelog.
9. Cargo test --release passes.
