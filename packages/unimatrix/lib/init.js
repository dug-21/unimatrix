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
const { resolveGitFile } = require("./hook-client/config.js");
const { decodeBundle } = require("./hook-client/bundle.js");

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
 * Write the unimatrix.remote subtree into .claude/settings.local.json,
 * merge-preserving (only unimatrix.remote is touched; Claude Code's keys and
 * other unimatrix.* keys survive verbatim). Mode 0600 (ADR-006, FR-18).
 *
 * Stores BOTH server-composed URLs VERBATIM (ADR-001 dumb-client): mcp_url and
 * observe_url are byte-equal to the validated bundle fields — no normalization,
 * no trailing-slash edit, no host substitution. The hook transport (Component 4)
 * reads observe_url as its post target. (ADR-002 v:2 subtree.)
 *
 * @param {string} projectRoot - Absolute project root.
 * @param {string} mcpUrl - Server-composed MCP URL (verbatim).
 * @param {string} observeUrl - Server-composed observe URL (verbatim).
 * @param {string} token - Bearer token.
 * @param {string|null} pinnedFp - Pinned cert fingerprint (bundle path), or null.
 * @param {boolean} dryRun - If true, do not write.
 * @returns {string[]} Actions taken.
 */
function writeRemoteSettingsLocal(
  projectRoot,
  mcpUrl,
  observeUrl,
  token,
  pinnedFp,
  dryRun
) {
  const actions = [];
  const slPath = path.join(projectRoot, ".claude", "settings.local.json");
  const existing = readJsonOrEmpty(slPath, ".claude/settings.local.json");

  existing.unimatrix = existing.unimatrix || {};
  existing.unimatrix.remote = Object.assign(
    {},
    existing.unimatrix.remote,
    {
      mcp_url: mcpUrl,
      observe_url: observeUrl,
      token: token,
    },
    // Bundle path pins the cert; legacy {remote,token} path leaves it absent.
    pinnedFp ? { fingerprint: pinnedFp } : {}
  );

  if (!dryRun) {
    fs.mkdirSync(path.dirname(slPath), { recursive: true });
    fs.writeFileSync(slPath, JSON.stringify(existing, null, 2) + "\n", {
      mode: 0o600,
    });
    // Re-assert mode in case the file pre-existed with looser perms.
    // Wrapped: chmod is a no-op on Windows but must not abort init.
    try {
      fs.chmodSync(slPath, 0o600);
    } catch (_err) {
      // best-effort (Windows / unsupported fs)
    }
    actions.push(
      "Wrote unimatrix.remote to .claude/settings.local.json (mode 0600)"
    );
  } else {
    actions.push(
      "[dry-run] Would write unimatrix.remote to .claude/settings.local.json (mode 0600)"
    );
  }

  return actions;
}

/**
 * Best-effort check that .claude/settings.local.json (token-bearing) is
 * gitignored; WARN when not. Common patterns only — no glob engine (FR-18).
 *
 * @param {string} projectRoot - Absolute project root.
 * @returns {string[]} Actions (warning, or none when covered).
 */
function gitignoreWarning(projectRoot) {
  const giPath = path.join(projectRoot, ".gitignore");
  let giLines = [];
  if (fs.existsSync(giPath)) {
    giLines = fs
      .readFileSync(giPath, "utf8")
      .split("\n")
      .map((l) => l.trim());
  }
  const coverPatterns = [
    ".claude/settings.local.json",
    "settings.local.json",
    "**/settings.local.json",
    ".claude/",
    "*.local.json",
  ];
  const covered = giLines.some((l) => coverPatterns.includes(l));
  if (covered) {
    return [];
  }
  return [
    "WARNING: .claude/settings.local.json is not gitignored — " +
      "it contains your token; add it to .gitignore",
  ];
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
 * Remote-mode init: ingest the C1 bundle (or legacy {remote,token}), pin the
 * server cert by the C2 fingerprint, wire the HTTP hook client into
 * .claude/settings.json, write credentials to settings.local.json (0600), copy
 * skills, and validate via a PINNED Ping. No local binary, no .mcp.json, no
 * database. The CLAUDE.md knowledge block is NOT appended (uni-init owns it);
 * init prints the /unimatrix-init pointer only (FR-B7 / AC-W1-C6).
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

  // Step 2: resolve the installed client path (absolute, platform-native).
  // require.resolve is the contract (ADR/pseudocode); the computed-path
  // fallback yields the identical absolute path once index.js exists.
  let clientPath;
  try {
    clientPath = require.resolve("./hook-client/index.js");
  } catch (_err) {
    clientPath = path.join(__dirname, "hook-client", "index.js");
  }

  // Step 3: write settings.local.json unimatrix.remote (ADR-006; FR-18). Both
  // server-composed URLs are stored VERBATIM (ADR-001); the hook transport reads
  // observe_url. The pinned fingerprint (bundle path) is persisted so the hook
  // client pins on every subsequent request.
  actions.push(
    ...writeRemoteSettingsLocal(
      projectRoot,
      mcpUrl,
      observeUrl,
      token,
      pinnedFp,
      dryRun
    )
  );

  // Step 3b: gitignore warning (best-effort, no glob engine).
  actions.push(...gitignoreWarning(projectRoot));

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
  actions.push(
    "Skipped .mcp.json: remote mode does not register a local MCP server"
  );
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
  copySkills,
  printSummary,
  readJsonOrEmpty,
  writeRemoteSettingsLocal,
  gitignoreWarning,
};
