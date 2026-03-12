#!/usr/bin/env bash
# check-versions.sh — Validates version synchronization across Cargo workspace and npm packages.
# Exit 1 with diagnostic if any mismatch found.
set -euo pipefail

ERRORS=0
ROOT_TOML="Cargo.toml"

# Extract workspace version from root Cargo.toml
CARGO_VERSION=$(grep -A5 '\[workspace\.package\]' "$ROOT_TOML" | grep '^version' | head -1 | sed 's/.*"\(.*\)".*/\1/')

if [ -z "$CARGO_VERSION" ]; then
    echo "FAIL: No version found in [workspace.package] in $ROOT_TOML"
    exit 1
fi

echo "Workspace version: $CARGO_VERSION"

# Check all 9 crates use version.workspace = true
for crate_toml in crates/*/Cargo.toml; do
    crate_name=$(basename "$(dirname "$crate_toml")")
    if grep -q '^version\.workspace = true' "$crate_toml"; then
        echo "  OK: $crate_name uses version.workspace = true"
    elif grep -q '^version = ' "$crate_toml"; then
        hardcoded=$(grep '^version = ' "$crate_toml" | sed 's/.*"\(.*\)".*/\1/')
        echo "  FAIL: $crate_name has hardcoded version = \"$hardcoded\" (expected version.workspace = true)"
        ERRORS=$((ERRORS + 1))
    else
        echo "  FAIL: $crate_name has no version field"
        ERRORS=$((ERRORS + 1))
    fi
done

# Check unimatrix-server uses workspace edition and rust-version
SERVER_TOML="crates/unimatrix-server/Cargo.toml"
if grep -q '^edition\.workspace = true' "$SERVER_TOML"; then
    echo "  OK: unimatrix-server uses edition.workspace = true"
else
    echo "  FAIL: unimatrix-server does not use edition.workspace = true"
    ERRORS=$((ERRORS + 1))
fi

if grep -q '^rust-version\.workspace = true' "$SERVER_TOML"; then
    echo "  OK: unimatrix-server uses rust-version.workspace = true"
else
    echo "  FAIL: unimatrix-server does not use rust-version.workspace = true"
    ERRORS=$((ERRORS + 1))
fi

# Check npm package versions (if packages exist)
for pkg_json in packages/*/package.json; do
    [ -f "$pkg_json" ] || continue
    pkg_name=$(basename "$(dirname "$pkg_json")")
    npm_version=$(python3 -c "import json; print(json.load(open('$pkg_json'))['version'])" 2>/dev/null || echo "PARSE_ERROR")
    if [ "$npm_version" = "$CARGO_VERSION" ]; then
        echo "  OK: npm $pkg_name version matches ($npm_version)"
    else
        echo "  FAIL: npm $pkg_name version ($npm_version) does not match Cargo ($CARGO_VERSION)"
        ERRORS=$((ERRORS + 1))
    fi
done

# Check optionalDependencies version match (if root package.json exists)
ROOT_PKG="packages/unimatrix/package.json"
if [ -f "$ROOT_PKG" ]; then
    opt_version=$(python3 -c "
import json
pkg = json.load(open('$ROOT_PKG'))
deps = pkg.get('optionalDependencies', {})
for name, ver in deps.items():
    print(ver)
" 2>/dev/null || echo "")
    if [ -n "$opt_version" ]; then
        if [ "$opt_version" = "$CARGO_VERSION" ]; then
            echo "  OK: optionalDependencies version matches ($opt_version)"
        else
            echo "  FAIL: optionalDependencies version ($opt_version) does not match Cargo ($CARGO_VERSION)"
            ERRORS=$((ERRORS + 1))
        fi
    fi
fi

if [ "$ERRORS" -gt 0 ]; then
    echo ""
    echo "FAILED: $ERRORS version mismatch(es) found"
    exit 1
else
    echo ""
    echo "PASSED: All versions synchronized at $CARGO_VERSION"
    exit 0
fi
