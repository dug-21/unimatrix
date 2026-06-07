"use strict";

// AC-12 / C-04 zero-runtime-dependency audit (vnc-026). Two checks, both hard:
//   1. package.json has NO runtime `dependencies` (optionalDependencies are the
//      platform binary packages for the LOCAL Rust path — not consumed by the
//      pure-JS remote hook client — and are allowed).
//   2. Every `require(...)` reachable from lib/hook-client/ resolves to a Node
//      built-in module or a relative path. No bare external package specifiers.
//
// A static require-scan (not a runtime graph) is sufficient and deterministic:
// the client is plain CommonJS with literal require() calls (built-ins only by
// design). Any bare specifier that is not a known built-in fails the audit.

const fs = require("fs");
const path = require("path");
const Module = require("module");

const PKG_ROOT = path.resolve(__dirname, "..");
const HOOK_CLIENT = path.join(PKG_ROOT, "lib", "hook-client");

function fail(msg) {
  console.error("zero-dep audit FAIL: " + msg);
  process.exit(1);
}

// ── Check 1: no runtime dependencies in package.json ────────────────────────
const pkg = JSON.parse(fs.readFileSync(path.join(PKG_ROOT, "package.json"), "utf8"));
const deps = pkg.dependencies || {};
const depNames = Object.keys(deps);
if (depNames.length > 0) {
  fail('package.json "dependencies" must be empty; found: ' + depNames.join(", "));
}
console.log("OK: package.json has no runtime dependencies");

// ── Check 2: require-graph resolves only built-ins / relative paths ──────────
// Node >= 18 builtinModules; also accept the `node:` prefix form.
const builtins = new Set(Module.builtinModules);
function isBuiltin(spec) {
  if (spec.startsWith("node:")) return true;
  return builtins.has(spec);
}

// Match require('x') / require("x") with a literal string argument.
const REQUIRE_RE = /\brequire\(\s*(['"])([^'"]+)\1\s*\)/g;

function scanFile(file) {
  const src = fs.readFileSync(file, "utf8");
  const offenders = [];
  let m;
  while ((m = REQUIRE_RE.exec(src)) !== null) {
    const spec = m[2];
    if (spec.startsWith(".") || spec.startsWith("/")) continue; // relative/abs OK
    if (isBuiltin(spec)) continue; // built-in OK
    offenders.push(spec);
  }
  return offenders;
}

function walk(dir) {
  let files = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) files = files.concat(walk(full));
    else if (entry.isFile() && entry.name.endsWith(".js")) files.push(full);
  }
  return files;
}

const jsFiles = walk(HOOK_CLIENT);
if (jsFiles.length === 0) fail("no .js files found under lib/hook-client/");

const violations = [];
for (const f of jsFiles) {
  for (const spec of scanFile(f)) {
    violations.push(path.relative(PKG_ROOT, f) + " -> require('" + spec + "')");
  }
}

if (violations.length > 0) {
  fail("lib/hook-client/ requires non-built-in modules:\n  " + violations.join("\n  "));
}
console.log(
  "OK: all " + jsFiles.length + " hook-client module(s) require only Node built-ins / relative paths"
);
