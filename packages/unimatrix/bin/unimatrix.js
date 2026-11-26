#!/usr/bin/env node
"use strict";

// The only harnesses `--harness` accepts (ADR-005 §2). Any other value is
// ambiguous → help, never a fall-through write (AC-16).
const VALID_HARNESSES = ["claude-code", "opencode", "codex-cli"];

/**
 * Print `wire` usage. The optional error message goes to stderr; the usage body
 * goes to stdout. Emitted (never a write) for an unknown `--harness` value or any
 * ambiguous invocation (AC-16). States the skills-only definition boundary (C-13).
 *
 * @param {string} [msg] - Optional one-line error to prefix on stderr.
 */
function printWireUsage(msg) {
  if (msg) {
    process.stderr.write(msg + "\n");
  }
  process.stdout.write(
    "usage: unimatrix wire [--harness <claude-code|opencode|codex-cli>] [--dry-run]\n" +
      "  Ensures MCP + hooks + retrieval for detected harnesses. Writes no definition files.\n" +
      "  --harness <name>   target one harness; also the opt-in to write a NEW entry into a user-owned config\n" +
      "  --dry-run          print intended actions, write nothing\n" +
      "  (definition scope is skills only; protocols/agents are not installed — run `init --force` to refresh skills)\n"
  );
}

/**
 * Print the `wire` action summary from the manifest-derived action lines. The
 * action set is identical on the real and dry-run paths (only the per-line
 * "[dry-run] " prefix differs — applied inside wire()); the header alone reflects
 * dry-run (AC-14).
 *
 * @param {string[]} actions - Action lines from wire()'s manifest.
 * @param {boolean} dryRun - Whether this was a dry run.
 */
function printWireSummary(actions, dryRun) {
  process.stdout.write(
    dryRun
      ? "\n--- Wire Dry Run Summary ---\n\n"
      : "\n--- Unimatrix Wire Complete ---\n\n"
  );
  for (const action of actions) {
    process.stdout.write("  " + action + "\n");
  }
  process.stdout.write("\n");
}

/**
 * Resolve the wire transport descriptor (local vs cloud), the same way init
 * chooses it, so the dry-run and real paths agree (NFR-10). Prefer the local
 * binary; fall back to the out-of-tree cloud credential when no binary is
 * installed. Neither available → empty descriptor: wire() reports MCP legs as
 * skipped (no transport) and NEVER throws.
 *
 * @param {string} projectRoot - Absolute project root (cloud store key oracle).
 * @returns {{binaryPath?:string, mcp?:{url:string, token:string}}}
 */
function resolveWireTransport(projectRoot) {
  let binaryPath = null;
  try {
    // null is normal on macOS/Windows (cloud-only); a throw means unresolvable.
    binaryPath = require("../lib/resolve-binary.js").resolveBinary() || null;
  } catch (_binaryErr) {
    binaryPath = null;
  }
  if (binaryPath) {
    return { binaryPath };
  }
  {
    // No local binary — try the out-of-tree cloud credential (keyed by hash).
    try {
      const { computeProjectHash } = require("../lib/hook-client/config.js");
      const credstore = require("../lib/hook-client/credstore.js");
      const cred = credstore.read(computeProjectHash(projectRoot));
      if (cred && cred.mcp_url) {
        return { mcp: { url: cred.mcp_url, token: cred.token } };
      }
    } catch (_cloudErr) {
      // Unreadable/malformed credential store → no transport (wire skips MCP
      // legs with a reason); never abort the wire path here.
    }
    return {};
  }
}

/**
 * Route the `wire` verb (ADR-005). Resolves project root + client path +
 * transport, then calls wire(projectRoot, opts). Skips installSkills and all
 * DB/validate work by construction (this branch never touches them). `--harness`
 * both selects the target and is the new-entry opt-in signal threaded into
 * wire(); `--force` is a no-op with a usage note and is NEVER forwarded (C-12).
 *
 * @param {string[]} args - process.argv.slice(2).
 */
function routeWire(args) {
  const valueAfter = (flag) => {
    const idx = args.indexOf(flag);
    return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
  };
  const dryRun = args.includes("--dry-run");
  const projectDir = valueAfter("--project-dir");
  const harness = valueAfter("--harness");

  // AC-16: validate --harness up front. Unknown value → help, write nothing,
  // usage-error exit. No path falls through to an unintended surface install.
  if (harness !== undefined && VALID_HARNESSES.indexOf(harness) === -1) {
    printWireUsage("unknown --harness: " + harness);
    process.exitCode = 2;
    return;
  }

  // C-12 / SR-04: --force is definitions-only. On `wire` it is a no-op with a
  // usage note — guaranteeing --force can never re-assert wiring (never forwarded).
  if (args.includes("--force")) {
    process.stderr.write(
      "note: --force has no effect on `wire` (it is definitions-only; " +
        "run `init --force` to refresh skills)\n"
    );
  }

  const path = require("path");

  // Resolve project root (throwing detectProjectRoot — same UX as init).
  let projectRoot;
  try {
    if (projectDir) {
      projectRoot = path.resolve(projectDir);
    } else {
      const { detectProjectRoot } = require("../lib/init.js");
      projectRoot = detectProjectRoot(process.cwd());
    }
  } catch (error) {
    process.stderr.write("unimatrix wire failed: " + error.message + "\n");
    process.exitCode = 1;
    return;
  }

  // Resolve the installed hook-client path (mirror init.js client resolution).
  let clientPath;
  try {
    clientPath = require.resolve("../lib/hook-client/index.js");
  } catch (_err) {
    clientPath = path.join(__dirname, "..", "lib", "hook-client", "index.js");
  }

  const transport = resolveWireTransport(projectRoot);

  // Call the shared wiring layer. `harness` carries the opt-in signal; the
  // orchestrator enforces the #960 intent gate — the CLI only surfaces the
  // signal (no double-implementation of the gate). --force is deliberately
  // absent from these opts (C-12).
  const { wire } = require("../lib/wire.js");
  let result;
  try {
    result = wire(
      projectRoot,
      Object.assign({ harness: harness || undefined, clientPath, dryRun }, transport)
    );
  } catch (error) {
    // Hard failures (unsafe shim path, unparseable config, missing recorded node)
    // throw: surface them, parity with init (exit 1).
    process.stderr.write("unimatrix wire failed: " + error.message + "\n");
    process.exitCode = 1;
    return;
  }

  printWireSummary(result.actions, dryRun);
  if (result.manifest.some((l) => l.harness === "codex-cli" && l.surface === "hooks" && /^(created|updated|unchanged)$/.test(l.action))) {
    console.log(require("../lib/codex-install.js").codexApprovalStep());
  }
  process.exitCode = 0;
}

function run(args, ctx) {
  ctx = ctx || {};

  // Route "init" to JS implementation (ADR-003)
  if (args[0] === "init") {
    const { init } = require("../lib/init.js");
    const valueAfter = (flag) => {
      const idx = args.indexOf(flag);
      return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
    };
    const projectDir = valueAfter("--project-dir");
    // --remote/--token (legacy F3) and --bundle/--slug (vnc-034) are plumbed
    // through init's own argv (interactive, user-typed). RQ-3 forbids the token
    // in the HOOK command line / checked-in files, not the init invocation
    // itself (the bundle carries the token, never logged).
    const remote = valueAfter("--remote");
    const token = valueAfter("--token");
    const bundle = valueAfter("--bundle");
    const slug = valueAfter("--slug");
    // --force is definitions-only (ADR-004): init threads it into installSkills
    // (overwrite Unimatrix-owned skills). It NEVER reaches the wire layer (C-12);
    // wiring stays always-additive regardless of --force (AC-02, SR-04).
    init({
      dryRun: args.includes("--dry-run"),
      force: args.includes("--force"),
      projectDir,
      remote,
      token,
      bundle,
      slug,
    })
      .then(() => {
        process.exitCode = 0;
      })
      .catch((error) => {
        process.stderr.write("unimatrix init failed: " + error.message + "\n");
        process.exitCode = 1;
      });
    return;
  }

  // Route "wire" to the per-harness wiring layer (ADR-005). The wire path runs
  // the wiring orchestrator ONLY: it skips definition copy (installSkills) and
  // all DB/validate steps (AC-03). `--harness <name>` targets one harness AND is
  // the new-entry opt-in signal (#960); an unknown value surfaces help rather
  // than a silent default install (AC-16). `--force` is NOT accepted here — it
  // is definitions-only and is never forwarded to wire() (C-12, SR-04).
  if (args[0] === "wire") {
    routeWire(args);
    return;
  }

  // Route "mcp-bridge" to the JS bridge (ADR-002, vnc-039). REQUIRED, not
  // stylistic: remote-only / non-Linux clients ship the pure-JS edge with no
  // Rust binary on disk, so a mcp-bridge that fell through to execFileSync
  // would throw on exactly the population the bridge exists to serve. The
  // bridge reads its credential from the out-of-tree store keyed by the
  // projectHash argument; no token ever on the command line (AC-09/AC-13).
  if (args[0] === "mcp-bridge") {
    const projectHash = args[1];
    // vnc-050 FR-21: headless launch (UNIMATRIX_BUNDLE or the env MCP URL)
    // needs no projectHash; the bridge resolves the credential from env.
    const headless = require("../lib/project-state.js").headlessEnvSet(process.env);
    if (!headless && (!projectHash || typeof projectHash !== "string")) {
      process.stderr.write("usage: unimatrix mcp-bridge <projectHash>\n");
      process.exitCode = 2;
      return;
    }
    // Lazy require: only loaded when the subcommand is invoked, so init and
    // the Rust exec fast paths pay no load cost. C2 owns the store read, the
    // pinned connection, the stdio loop, and its own fail-loud / EOF exits.
    // Pass an argv-shaped array so argv[2] === projectHash, identical to a
    // direct `node <bridge> <projectHash>` spawn (the .mcp.json shape).
    require("../lib/hook-client/mcp-bridge.js").main([
      "node",
      "mcp-bridge",
      projectHash,
    ]);
    return;
  }

  // Node-routed verbs (ADR-004): in-process hook client, mcp-stdio, status.
  if (args[0] === "hook") {
    return require("../lib/hook-route.js").run(args, ctx);
  }
  if (args[0] === "mcp-stdio") {
    return require("../lib/hook-route.js").runMcpStdio(args, ctx);
  }
  if (args[0] === "status") {
    require("../lib/status.js").run(args);
    return;
  }
  return passThrough(args);
}

// All other subcommands: Linux-only Rust pass-through (D-071, ADR-002/004).
function passThrough(args) {
  if (process.platform !== "linux") {
    process.stderr.write(
      "unimatrix: " + (args[0] || "") + " requires the local server (Linux only)\n"
    );
    process.exitCode = 1;
    return;
  }
  // The redirect is the one user-controlled pointer here: trust-check it first.
  try {
    const os = require("os");
    const fs = require("fs");
    const home = os.homedir();
    if (fs.existsSync(require("path").join(home, ".unimatrix", "redirect.json"))) {
      const t = require("../lib/trust.js");
      let inst = null;
      try {
        inst = JSON.parse(fs.readFileSync(require("path").join(home, ".unimatrix", "install.json"), "utf8"));
      } catch (_e) { /* trust set tolerates no install */ }
      t.checkTrust(t.trustSet(home, { installJson: inst, platform: "linux" }), "linux", { home });
    }
  } catch (error) {
    process.stderr.write(error.message + "\n");
    process.exitCode = 1;
    return;
  }

  let binaryPath;
  try {
    binaryPath = require("../lib/resolve-binary.js").resolveBinary();
  } catch (error) {
    process.stderr.write(error.message + "\n");
    process.exitCode = 1;
    return;
  }
  if (!binaryPath) {
    process.stderr.write("unimatrix: no local binary available (sources tried: UNIMATRIX_BINARY, redirect, install)\n");
    process.exitCode = 1;
    return;
  }

  // Ensure bundled shared libraries (libonnxruntime) are found at runtime
  const binDir = require("path").dirname(binaryPath);
  const ldPath = process.env.LD_LIBRARY_PATH;
  const env = Object.assign({}, process.env, {
    LD_LIBRARY_PATH: ldPath ? binDir + ":" + ldPath : binDir,
  });

  try {
    require("child_process").execFileSync(binaryPath, args, { stdio: "inherit", env: env });
  } catch (error) {
    // execFileSync throws on non-zero exit code
    // error.status contains the exit code from the child process
    if (error.status !== null && error.status !== undefined) {
      process.exitCode = error.status;
    } else {
      // Signal death or spawn failure
      process.stderr.write(
        "Failed to execute unimatrix: " + error.message + "\n"
      );
      process.exitCode = 1;
    }
  }
}

module.exports = { run };

if (require.main === module) {
  run(process.argv.slice(2));
}
