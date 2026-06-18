"use strict";

const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");
const { resolveBinary } = require("./resolve-binary.js");
const {
  mergeSettings,
  buildHookClientCommand,
  HOOK_EVENTS,
} = require("./merge-settings.js");
const transport = require("./hook-client/transport-http.js");
const { resolveGitFile, computeProjectHash } = require("./hook-client/config.js");
const { decodeBundle } = require("./hook-client/bundle.js");
const credstore = require("./hook-client/credstore.js");

/**
 * Loud, deterministic message emitted on the legacy `--remote`/`--token` path:
 * cloud MCP is bundle-only (ADR-005, OQ-2, #773). The legacy observe/telemetry
 * path still works; only the on-demand MCP surface requires a v:2 bundle. Exact
 * wording is a testable AC (AC-10, SR-06) — NOT a hard failure (init exits 0 on
 * the legacy path; this is an action line, not a throw).
 */
const LEGACY_MCP_UNSUPPORTED_MESSAGE =
  "Cloud MCP is unsupported on the legacy --remote/--token path: it requires a " +
  "v:2 bundle (run `unimatrix client-bundle` on the server, then " +
  "`init --bundle <bundle>`). No MCP server was wired. The observe/telemetry " +
  "path still works.";

/**
 * Detect project root by walking up from startDir to find `.git` (Rust
 * detect_project_root parity, ADR-003). A `.git` DIRECTORY marks the root; a
 * `.git` FILE marks a worktree, chased via resolveGitFile to the MAIN repo
 * root so init from a worktree writes where the hook client will look.
 *
 * @param {string} startDir - Directory to start searching from.
 * @returns {string} Absolute path to the project root.
 */
function detectProjectRoot(startDir) {
  let current = path.resolve(startDir);
  for (;;) {
    const gitPath = path.join(current, ".git");
    let st = null;
    try {
      st = fs.statSync(gitPath);
    } catch (_err) {
      // .git absent here — keep walking
    }
    if (st) {
      // FILE = worktree → main root (any failure → `current`, per project.rs).
      return st.isFile() ? resolveGitFile(gitPath, current) : current;
    }
    const parent = path.dirname(current);
    if (parent === current) {
      throw new Error(
        "Could not find project root (.git directory).\n" +
          "Run this command from within a git repository."
      );
    }
    current = parent;
  }
}

/**
 * Write or merge .mcp.json with the unimatrix server entry.
 * Preserves existing servers. Malformed JSON causes an error (ADR-004).
 *
 * @param {string} projectRoot - Absolute path to project root.
 * @param {string} binaryPath - Absolute path to the unimatrix binary.
 * @param {boolean} dryRun - If true, do not write the file.
 * @returns {string[]} Actions taken.
 */
function writeMcpJson(projectRoot, binaryPath, dryRun) {
  const mcpPath = path.join(projectRoot, ".mcp.json");
  const actions = [];
  let existing = {};

  if (fs.existsSync(mcpPath)) {
    try {
      existing = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    } catch (parseError) {
      throw new Error(
        "Malformed .mcp.json at " +
          mcpPath +
          ": " +
          parseError.message +
          "\nFix the JSON syntax and re-run 'npx unimatrix init'."
      );
    }
    actions.push("Updated .mcp.json (preserved existing servers)");
  } else {
    actions.push("Created .mcp.json");
  }

  if (!existing.mcpServers) {
    existing.mcpServers = {};
  }

  existing.mcpServers.unimatrix = {
    command: binaryPath,
    args: [],
    env: {
      LD_LIBRARY_PATH: path.dirname(binaryPath),
    },
  };

  if (!dryRun) {
    fs.writeFileSync(
      mcpPath,
      JSON.stringify(existing, null, 2) + "\n",
      "utf8"
    );
  } else {
    actions[actions.length - 1] = "[dry-run] " + actions[actions.length - 1];
  }

  return actions;
}

/**
 * Copy bundled skill files from the package's skills/ directory
 * into the project's .claude/skills/ directory. Overwrites existing
 * unimatrix skills, preserves non-unimatrix skills.
 *
 * @param {string} projectRoot - Absolute path to project root.
 * @param {boolean} dryRun - If true, do not copy files.
 * @returns {string[]} Actions taken.
 */
function copySkills(projectRoot, dryRun) {
  const actions = [];
  const targetDir = path.join(projectRoot, ".claude", "skills");
  const sourceDir = path.join(__dirname, "..", "skills");

  if (!fs.existsSync(sourceDir)) {
    actions.push("No bundled skills found (skipped)");
    return actions;
  }

  if (!dryRun) {
    fs.mkdirSync(targetDir, { recursive: true });
  }

  const skillDirs = fs
    .readdirSync(sourceDir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name);

  for (const skillDir of skillDirs) {
    const src = path.join(sourceDir, skillDir);
    const dst = path.join(targetDir, skillDir);

    if (!dryRun) {
      fs.mkdirSync(dst, { recursive: true });

      const files = fs.readdirSync(src);
      for (const file of files) {
        if (file.includes("..")) {
          throw new Error(
            "Path traversal detected in skill file: " + file
          );
        }

        const srcFile = path.join(src, file);
        const dstFile = path.join(dst, file);

        // Only copy files, not subdirectories
        const stat = fs.statSync(srcFile);
        if (stat.isFile()) {
          fs.copyFileSync(srcFile, dstFile);
        }
      }

      actions.push("Copied skill: " + skillDir);
    } else {
      actions.push("[dry-run] Would copy skill: " + skillDir);
    }
  }

  return actions;
}

/**
 * Read and parse a JSON file, returning {} if it does not exist.
 * Malformed JSON THROWS with a fix-it message (never clobber user content).
 *
 * @param {string} filePath - Path to the JSON file.
 * @param {string} label - Human label for the file in error messages.
 * @returns {object} Parsed object, or {} when the file is absent/empty.
 */
function readJsonOrEmpty(filePath, label) {
  if (!fs.existsSync(filePath)) {
    return {};
  }
  const raw = fs.readFileSync(filePath, "utf8").trim();
  if (raw === "") {
    return {};
  }
  try {
    return JSON.parse(raw);
  } catch (parseError) {
    throw new Error(
      "Malformed " +
        label +
        " at " +
        filePath +
        ": " +
        parseError.message +
        "\nFix the JSON syntax and re-run 'npx unimatrix init'."
    );
  }
}

/**
 * Write or merge the TOKEN-FREE stdio `unimatrix` bridge entry into .mcp.json
 * (AC-09 / FR-17). Remote analogue of writeMcpJson: same idempotent,
 * merge-preserving, dry-run-aware, malformed-throws contract (R-10/AC-07). The
 * entry invokes the JS bridge (`node <bridgePath> <projectHash>`); the bridge
 * resolves the credential from the out-of-tree store by projectHash — so the
 * entry carries no token, no mcp_url, and no fingerprint.
 *
 * @param {string} projectRoot - Absolute path to project root.
 * @param {string} bridgePath - Absolute path to the mcp-bridge.js module.
 * @param {string} projectHash - The store key (16 hex), the bridge's only arg.
 * @param {boolean} dryRun - If true, do not write the file.
 * @returns {string[]} Actions taken.
 */
function writeMcpBridgeEntry(projectRoot, bridgePath, projectHash, dryRun) {
  const mcpPath = path.join(projectRoot, ".mcp.json");
  const actions = [];
  let existing = {};

  if (fs.existsSync(mcpPath)) {
    try {
      existing = JSON.parse(fs.readFileSync(mcpPath, "utf8"));
    } catch (parseError) {
      throw new Error(
        "Malformed .mcp.json at " +
          mcpPath +
          ": " +
          parseError.message +
          "\nFix the JSON syntax and re-run 'npx unimatrix init'."
      );
    }
    actions.push("Updated .mcp.json (preserved existing servers)");
  } else {
    actions.push("Created .mcp.json");
  }

  if (!existing.mcpServers) {
    existing.mcpServers = {};
  }

  // TOKEN-FREE stdio entry (AC-09 / FR-17): the bridge reads the credential from
  // the store at spawn time; projectHash is the only argument it needs.
  existing.mcpServers.unimatrix = {
    command: "node",
    args: [bridgePath, projectHash],
    env: {},
  };

  if (!dryRun) {
    fs.writeFileSync(
      mcpPath,
      JSON.stringify(existing, null, 2) + "\n",
      "utf8"
    );
  } else {
    actions[actions.length - 1] = "[dry-run] " + actions[actions.length - 1];
  }

  return actions;
}

/**
 * Best-effort deletion of a stale in-tree `unimatrix.remote` credential subtree
 * from .claude/settings.local.json (ADR-004 §migration / OQ-5 residual). The
 * credential moved out of the tree to the credstore; an old in-tree copy is the
 * exact commit-leak this feature closes, so it is removed on (re-)init.
 *
 * Merge-preserving: ONLY `unimatrix.remote` is removed — other `unimatrix.*`
 * keys and Claude Code's keys survive verbatim. A malformed or unreadable
 * settings.local.json does NOT abort init (R-12 §migration): note and continue.
 *
 * @param {string} projectRoot - Absolute project root.
 * @param {boolean} dryRun - If true, do not write.
 * @returns {string[]} Actions taken.
 */
function cleanStaleRemoteSubtree(projectRoot, dryRun) {
  const actions = [];
  const slPath = path.join(projectRoot, ".claude", "settings.local.json");
  if (!fs.existsSync(slPath)) {
    return actions;
  }
  try {
    const parsed = readJsonOrEmpty(slPath, ".claude/settings.local.json");
    if (
      parsed.unimatrix &&
      typeof parsed.unimatrix === "object" &&
      Object.prototype.hasOwnProperty.call(parsed.unimatrix, "remote")
    ) {
      if (dryRun) {
        actions.push(
          "[dry-run] Would remove stale unimatrix.remote from " +
            ".claude/settings.local.json"
        );
      } else {
        delete parsed.unimatrix.remote;
        fs.writeFileSync(
          slPath,
          JSON.stringify(parsed, null, 2) + "\n",
          "utf8"
        );
        actions.push(
          "Removed stale unimatrix.remote credential from " +
            ".claude/settings.local.json"
        );
      }
    }
  } catch (e) {
    // Best-effort: a malformed settings.local.json must not block relocation.
    actions.push(
      "Note: could not clean stale .claude/settings.local.json (" +
        e.message +
        ")"
    );
  }
  return actions;
}

/**
 * Derive a legacy observe URL from a single legacy endpoint. The legacy
 * `{remote, token}` path predates the v:2 bundle and has NO server-composed
 * observe URL; preserve the prior behavior (transport appended `/observe`) by
 * deriving it ONCE here, on the legacy branch only. The bundle branch composes
 * NOTHING (ADR-001). Flag: legacy is not the #766 surface — keep it working,
 * do not extend it.
 *
 * @param {string} remote - Legacy endpoint URL.
 * @returns {string} The legacy observe URL (trailing slashes stripped + /observe).
 */
function legacyObserveFrom(remote) {
  return remote.replace(/\/+$/, "") + "/observe";
}

/**
 * Resolve the effective MCP URL, observe URL, token, and pinned fingerprint
 * from either the v:2 bundle path (`--bundle`) or the legacy F3 `{remote, token}`
 * path (backward-compat). Bundle decode is the C1 trust boundary (ADR-001); a
 * guard failure throws BundleError (token never in the message).
 *
 * BUNDLE PATH (dumb-client, ADR-001): the server composes BOTH finished URLs
 * (mcp_url, observe_url) into the bundle; the client appends NOTHING, derives NO
 * slug, composes NO path. The `--slug` flag is RETIRED for the bundle path — the
 * bundle URLs already encode the slug. The set of client-side path-composition
 * sites on this branch is EMPTY (NFR-01 invariant). The result bakes EXACTLY
 * ONE project's URLs; there is no field in which a second project can be named
 * (R-06 / AC-W1-C5 — cross-project fan-out is unrepresentable, not rejected).
 *
 * @param {object} options - { bundle?, remote?, token? }
 * @returns {{mcpUrl:string, observeUrl:string, token:string, pinnedFp:(string|null)}}
 */
function resolveRemoteTarget(options) {
  if (options.bundle) {
    const b = decodeBundle(options.bundle); // throws BundleError on any guard
    // ADR-001: NO endpointBase, NO "/v1" append, NO slug branch — verbatim URLs.
    return {
      mcpUrl: b.mcp_url,
      observeUrl: b.observe_url,
      token: b.token,
      pinnedFp: b.fp,
    };
  }

  // Legacy path (F3 backward-compat): {remote, token} provided directly. No pin.
  const remote = options.remote;
  const token = options.token;
  if (!remote || !token) {
    throw new Error("--remote and --token are both required");
  }
  let u;
  try {
    u = new URL(remote);
  } catch (_err) {
    throw new Error("invalid --remote URL: " + remote);
  }
  if (u.protocol !== "http:" && u.protocol !== "https:") {
    throw new Error(
      "--remote URL must be http: or https: (got " + u.protocol + ")"
    );
  }
  // Legacy supplies a single endpoint; map it to BOTH fields so downstream is
  // uniform. observe_url is derived ONLY here (the bundle branch composes none).
  return {
    mcpUrl: remote,
    observeUrl: legacyObserveFrom(remote),
    token: token,
    pinnedFp: null,
  };
}

/**
 * Remote-mode init: ingest the v:2 bundle (or legacy {remote,token}), pin the
 * server cert by the bundle fingerprint, wire the HTTP hook client into
 * .claude/settings.json, write the credential to the OUT-OF-TREE store
 * (~/.unimatrix/<hash>/remote.json, 0600 — vnc-039 Scope B), copy skills, and
 * validate via a PINNED Ping. No local binary, no database.
 *
 * On the BUNDLE path, also write a TOKEN-FREE stdio `unimatrix` .mcp.json entry
 * that spawns the JS MCP bridge (Scope A). On the LEGACY path, write NO bridge
 * entry and emit a loud, deterministic unsupported message (cloud MCP is
 * bundle-only — ADR-005). Either way, a stale in-tree unimatrix.remote subtree
 * is removed (migration). The CLAUDE.md knowledge block is NOT appended
 * (uni-init owns it); init prints the /unimatrix-init pointer only.
 *
 * Failures THROW → bin catches → stderr + exit 1 (init is interactive; the one
 * loud checkpoint, opposite the hook client's exit-0 posture). The token never
 * appears in any thrown message, stdout, or stderr (NFR-06).
 *
 * @param {object} options - { bundle?, slug?, remote?, token?, dryRun, projectDir }
 */
async function initRemote(options) {
  const dryRun = (options && options.dryRun) || false;
  const actions = [];

  // Step 0: resolve target (bundle path | legacy path) — LOUD on bad input.
  const target = resolveRemoteTarget(options);
  const mcpUrl = target.mcpUrl;
  const observeUrl = target.observeUrl;
  const token = target.token;
  const pinnedFp = target.pinnedFp;

  // Step 1: project root (throwing detectProjectRoot — correct for init UX).
  let projectRoot;
  if (options.projectDir) {
    projectRoot = path.resolve(options.projectDir);
  } else {
    projectRoot = detectProjectRoot(process.cwd());
  }
  actions.push("Project root: " + projectRoot);

  // Step 1b: derive the store key — the SAME oracle both consumers use (R-07,
  // ADR-003). One derivation; the writer and the hook client/bridge readers
  // cannot disagree on which ~/.unimatrix/<hash>/ directory to use.
  const projectHash = computeProjectHash(projectRoot);

  // The bundle path carries a real fingerprint -> the hook client pins. The
  // legacy {remote,token} path has no pin -> fingerprint:null (WARN-1: legacy
  // creds are STILL relocated out of tree, but stay unpinned and get no MCP).
  const isBundlePath = !!options.bundle;

  // Step 2: resolve the installed client path (absolute, platform-native).
  // require.resolve is the contract (ADR/pseudocode); the computed-path
  // fallback yields the identical absolute path once index.js exists.
  let clientPath;
  try {
    clientPath = require.resolve("./hook-client/index.js");
  } catch (_err) {
    clientPath = path.join(__dirname, "hook-client", "index.js");
  }

  // Step 3: write the credential to the out-of-tree store (Scope B). The token
  // and fingerprint NEVER land in the repo tree (closes the commit-leak). The
  // bundle path persists the real fingerprint so the hook client pins; the
  // legacy path persists fingerprint:null (universal relocation, unpinned).
  actions.push(
    ...credstore.write(
      projectHash,
      {
        mcp_url: mcpUrl,
        observe_url: observeUrl,
        token: token,
        fingerprint: isBundlePath ? pinnedFp : null,
      },
      { dryRun }
    )
  );

  // Step 3a: delete any stale in-tree unimatrix.remote subtree from a prior
  // in-tree credential write (ADR-004 §migration). Best-effort; never aborts.
  actions.push(...cleanStaleRemoteSubtree(projectRoot, dryRun));

  // Step 3b: wire the MCP surface.
  if (isBundlePath) {
    // Bundle path (Scope A): write the TOKEN-FREE stdio .mcp.json bridge entry.
    let bridgePath;
    try {
      bridgePath = require.resolve("./hook-client/mcp-bridge.js");
    } catch (_err) {
      bridgePath = path.join(__dirname, "hook-client", "mcp-bridge.js");
    }
    actions.push(
      ...writeMcpBridgeEntry(projectRoot, bridgePath, projectHash, dryRun)
    );
  } else {
    // Legacy path (ADR-005 bundle-only boundary): NO bridge entry; emit a loud,
    // deterministic unsupported message (AC-10, R-11). Not a failure — legacy
    // observe still works; init continues to hooks + skills + Ping.
    actions.push(LEGACY_MCP_UNSUPPORTED_MESSAGE);
  }

  // Step 4: merge hooks (full 9-event remote set; idempotent; preserves
  // foreign hooks). Command is ONLY `node <path> <EVENT>` (RQ-3 / R-16).
  const settingsPath = path.join(projectRoot, ".claude", "settings.json");
  const settingsResult = mergeSettings(
    settingsPath,
    {
      events: HOOK_EVENTS,
      commandForEvent: (event) => buildHookClientCommand(clientPath, event),
    },
    { dryRun }
  );
  actions.push(...settingsResult.actions);

  // Step 5: copy skills (FR-B7); remote mode DOES copy skills. Do NOT append a
  // CLAUDE.md knowledge block (uni-init owns it) — the /unimatrix-init pointer
  // printed by printSummary is the only onboarding pointer (AC-W1-C6).
  actions.push(...copySkills(projectRoot, dryRun));
  actions.push("Skipped binary/database steps: no local binary in remote mode");

  // Step 6: Ping validation over the PINNED TLS connection — the ONE loud
  // checkpoint (FR-19, ADR-005, R-18). The Ping posts to the bundle's
  // observe_url VERBATIM (AC-07 / #766 fix: the real per-slug /v1/{slug}/observe
  // route, was a 404 /v1/observe). A cert-fingerprint mismatch surfaces HERE,
  // diagnosably (FR-A11 / AC-CT-ROT).
  if (!dryRun) {
    const res = await transport.pingForInit(
      observeUrl,
      token,
      undefined,
      pinnedFp
    );
    if (!res.ok) {
      throw new Error(
        "Remote validation failed: " +
          res.message +
          "\nConfiguration files were written; fix the URL/token and re-run init."
      );
    }
    actions.push("Ping OK: " + res.message);
  } else {
    let host;
    try {
      host = new URL(observeUrl).host;
    } catch (_err) {
      host = "(invalid URL)";
    }
    actions.push("[dry-run] Would Ping " + host);
  }

  printSummary(actions, dryRun);
}

/**
 * Print summary of all actions taken during init.
 *
 * @param {string[]} actions - List of action descriptions.
 * @param {boolean} dryRun - Whether this was a dry run.
 */
function printSummary(actions, dryRun) {
  if (dryRun) {
    console.log("\n--- Dry Run Summary ---\n");
  } else {
    console.log("\n--- Unimatrix Init Complete ---\n");
  }

  for (const action of actions) {
    console.log("  " + action);
  }

  console.log("");
  if (!dryRun) {
    console.log("Next step: start a Claude Code session and run /unimatrix-init");
  }
}

/**
 * Deterministic, non-interactive, idempotent project wiring.
 * Configures MCP server, hooks, skills, and pre-creates the database.
 * Implemented in JavaScript per ADR-003.
 *
 * @param {object} options
 * @param {boolean} [options.dryRun=false] - Print actions without modifying files.
 * @param {string} [options.projectDir] - Override project root (skip .git walk).
 */
async function init(options) {
  const opts = options || {};

  // Remote mode: --bundle (vnc-034) or --remote / --token (legacy F3) routes to
  // the HTTP hook-client wiring. The local flow below is untouched (C-10).
  if (opts.bundle || opts.remote || opts.token) {
    return initRemote(opts);
  }

  const dryRun = opts.dryRun || false;
  const actions = [];

  // Step 1: Resolve project root
  let projectRoot;
  if (options && options.projectDir) {
    projectRoot = path.resolve(options.projectDir);
  } else {
    projectRoot = detectProjectRoot(process.cwd());
  }
  actions.push("Project root: " + projectRoot);

  // Step 2: Resolve binary path
  const binaryPath = resolveBinary();
  actions.push("Binary: " + binaryPath);

  // Step 3: Write/merge .mcp.json
  const mcpActions = writeMcpJson(projectRoot, binaryPath, dryRun);
  actions.push(...mcpActions);

  // Step 4: Merge hooks into .claude/settings.json
  const settingsPath = path.join(projectRoot, ".claude", "settings.json");
  const settingsResult = mergeSettings(settingsPath, binaryPath, { dryRun });
  actions.push(...settingsResult.actions);

  // Step 5: Copy skill files
  const skillActions = copySkills(projectRoot, dryRun);
  actions.push(...skillActions);

  // Shared env for all binary invocations: libonnxruntime lives next to the binary
  const binDir = path.dirname(binaryPath);
  const ldPath = process.env.LD_LIBRARY_PATH;
  const binaryEnv = Object.assign({}, process.env, {
    LD_LIBRARY_PATH: ldPath ? binDir + ":" + ldPath : binDir,
  });

  // Step 6: Pre-create database (exec Rust binary)
  if (!dryRun) {
    try {
      execFileSync(binaryPath, ["--project-dir", projectRoot, "version"], {
        stdio: "pipe",
        env: binaryEnv,
      });
      actions.push("Database: pre-created at ~/.unimatrix/{hash}/");
    } catch (error) {
      const stderr =
        error.stderr ? error.stderr.toString() : error.message;
      throw new Error("Database creation failed: " + stderr);
    }
  } else {
    actions.push(
      "[dry-run] Would pre-create database via: unimatrix version --project-dir " +
        projectRoot
    );
  }

  // Step 7: Validate binary
  if (!dryRun) {
    try {
      const versionOutput = execFileSync(binaryPath, ["version"], {
        stdio: "pipe",
        encoding: "utf8",
        env: binaryEnv,
      }).trim();
      actions.push("Validation: " + versionOutput);
    } catch (error) {
      const stderr =
        error.stderr ? error.stderr.toString() : error.message;
      throw new Error("Binary validation failed: " + stderr);
    }
  } else {
    actions.push("[dry-run] Would validate binary via: unimatrix version");
  }

  // Step 8: Print summary
  printSummary(actions, dryRun);
}

module.exports = {
  init,
  initRemote,
  resolveRemoteTarget,
  detectProjectRoot,
  writeMcpJson,
  writeMcpBridgeEntry,
  cleanStaleRemoteSubtree,
  copySkills,
  printSummary,
  readJsonOrEmpty,
  LEGACY_MCP_UNSUPPORTED_MESSAGE,
};
