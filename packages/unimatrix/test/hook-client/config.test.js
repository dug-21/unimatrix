"use strict";

const { describe, it, beforeEach, afterEach } = require("node:test");
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");

const config = require("../../lib/hook-client/config.js");
const {
  ENV_URL,
  ENV_TOKEN,
  DEFAULT_TIMEOUTS,
  resolve,
  walkToProjectRoot,
  computeProjectHash,
  mergeTimeouts,
} = config;

const GOLDENS = JSON.parse(
  fs.readFileSync(
    path.join(__dirname, "..", "fixtures", "parity", "project-hash-goldens.json"),
    "utf8"
  )
);

// ── Helpers (temp-dir fixtures) ─────────────────────────────────────

/** Create a temp project dir; opts.git=false skips the .git dir. */
function makeProject(opts) {
  const o = opts || {};
  const root = fs.realpathSync(
    fs.mkdtempSync(path.join(os.tmpdir(), "unimatrix-config-test-"))
  );
  if (o.git !== false) {
    fs.mkdirSync(path.join(root, ".git"));
  }
  return root;
}

/** Write {root}/.claude/settings.local.json (object or raw string). */
function writeLocalSettings(root, content) {
  const fp = path.join(root, ".claude", "settings.local.json");
  fs.mkdirSync(path.dirname(fp), { recursive: true });
  fs.writeFileSync(
    fp,
    typeof content === "string" ? content : JSON.stringify(content, null, 2),
    "utf8"
  );
  return fp;
}

function remoteSettings(url, token, extra) {
  return { unimatrix: { remote: Object.assign({ url, token }, extra || {}) } };
}

// Env isolation across every test.
let savedUrl;
let savedTok;
beforeEach(function () {
  savedUrl = process.env[ENV_URL];
  savedTok = process.env[ENV_TOKEN];
  delete process.env[ENV_URL];
  delete process.env[ENV_TOKEN];
});
afterEach(function () {
  if (savedUrl === undefined) delete process.env[ENV_URL];
  else process.env[ENV_URL] = savedUrl;
  if (savedTok === undefined) delete process.env[ENV_TOKEN];
  else process.env[ENV_TOKEN] = savedTok;
});

// ── FR-06 Resolution Matrix (R-09) ──────────────────────────────────

describe("config.resolve — FR-06 resolution matrix", function () {
  it("test_env_pair_wins_over_file", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_URL] = "https://env.example.com";
    process.env[ENV_TOKEN] = "env-token";

    const reads = [];
    const origRead = fs.readFileSync;
    fs.readFileSync = function (...args) {
      reads.push(String(args[0]));
      return origRead.apply(fs, args);
    };
    let result;
    try {
      result = resolve(root);
    } finally {
      fs.readFileSync = origRead;
    }

    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "env");
    assert.strictEqual(result.url, "https://env.example.com");
    assert.strictEqual(result.token, "env-token");
    assert.deepStrictEqual(result.timeouts, { ...DEFAULT_TIMEOUTS });
    assert.strictEqual(result.urlHost, "env.example.com");
    // File never consulted for url/token on the env path.
    assert.ok(
      !reads.some((p) => p.includes("settings.local.json")),
      "settings.local.json must not be read when env pair is set"
    );
  });

  it("test_partial_env_pair_is_misconfig (url only)", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_URL] = "https://env.example.com";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
    assert.strictEqual(result.token, undefined, "no token on failure shapes");
    assert.strictEqual(result.url, undefined, "no url on failure shapes");
  });

  it("test_partial_env_pair_is_misconfig (token only)", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://file.example.com", "file-token"));
    process.env[ENV_TOKEN] = "env-token";

    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "partial_env");
  });

  it("test_file_resolution_happy", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://remote.example.com:8443", "s3cr3t"));

    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.source, "file");
    assert.strictEqual(result.url, "https://remote.example.com:8443");
    assert.strictEqual(result.token, "s3cr3t");
    assert.deepStrictEqual(result.timeouts, { ...DEFAULT_TIMEOUTS });
    assert.strictEqual(result.urlHost, "remote.example.com:8443");
  });

  it("test_missing_file", function () {
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "missing");
    assert.strictEqual(result.projectRoot, root);
    assert.ok(result.projectHash);
    // Distinct from the misconfigured (partial_env) class.
    assert.notStrictEqual(result.reason, "partial_env");
  });

  it("test_file_without_remote_key", function () {
    const root = makeProject();
    // Claude Code key-drop simulation: file present, unimatrix.remote absent.
    writeLocalSettings(root, { permissions: { allow: ["Read"] } });
    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "missing");
  });

  it("test_file_with_incomplete_remote_key", function () {
    const root = makeProject();
    writeLocalSettings(root, { unimatrix: { remote: { url: "https://x.example.com" } } });
    const result = resolve(root);
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "missing");
  });

  it("test_malformed_settings_json", function () {
    const root = makeProject();
    writeLocalSettings(root, "{ this is not json !!!");
    let result;
    assert.doesNotThrow(function () {
      result = resolve(root);
    });
    assert.strictEqual(result.ok, false);
    assert.strictEqual(result.reason, "malformed");
  });

  it("test_subdirectory_cwd", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://sub.example.com", "tok"));
    const sub = path.join(root, "a", "b", "c");
    fs.mkdirSync(sub, { recursive: true });

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, root, "root found via .git walk");
    assert.strictEqual(result.url, "https://sub.example.com");
  });

  it("test_stdin_cwd_overrides_process_cwd", function () {
    // config.resolve uses the cwd it is GIVEN (index.js passes stdin.cwd when
    // non-empty); process.cwd() is unrelated to the project being resolved.
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://stdin-cwd.example.com", "tok"));
    assert.notStrictEqual(walkToProjectRoot(process.cwd()), root);

    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, root);
  });

  it("test_empty_cwd_falls_back_to_process_cwd", function () {
    const expected = walkToProjectRoot(process.cwd());
    const result = resolve("");
    assert.strictEqual(result.projectRoot, expected);
  });

  it("test_resolve_never_throws_on_junk_cwd", function () {
    for (const junk of [null, undefined, 42, {}, ""]) {
      assert.doesNotThrow(function () {
        const r = resolve(junk);
        assert.strictEqual(typeof r.ok, "boolean");
      });
    }
  });
});

// ── Root Walk + Hash (split-brain prevention) ───────────────────────

describe("config root walk + hash", function () {
  it("test_nested_git_monorepo_nearest_root_wins", function () {
    const outer = makeProject();
    writeLocalSettings(outer, remoteSettings("https://outer.example.com", "outer-tok"));
    const pkg = path.join(outer, "pkg");
    fs.mkdirSync(path.join(pkg, ".git"), { recursive: true });
    writeLocalSettings(pkg, remoteSettings("https://inner.example.com", "inner-tok"));
    const sub = path.join(pkg, "sub");
    fs.mkdirSync(sub);

    const result = resolve(sub);
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.projectRoot, pkg, "nearest .git wins");
    assert.strictEqual(result.url, "https://inner.example.com", "config read from nearest root");
    // Split-brain assertion: the SAME root string feeds the state-dir hash.
    assert.strictEqual(result.projectHash, computeProjectHash(result.projectRoot));
  });

  it("test_no_git_root_is_resolved_cwd", function () {
    const root = makeProject({ git: false });
    const sub = path.join(root, "deep", "er");
    fs.mkdirSync(sub, { recursive: true });

    const reads = [];
    const origRead = fs.readFileSync;
    fs.readFileSync = function (...args) {
      reads.push(String(args[0]));
      return origRead.apply(fs, args);
    };
    let result;
    try {
      result = resolve(sub);
    } finally {
      fs.readFileSync = origRead;
    }

    assert.strictEqual(result.projectRoot, path.resolve(sub), "no .git → resolved cwd");
    // One file read, no multi-location probing (ADR-006).
    assert.strictEqual(reads.length, 1, "exactly one file read");
    assert.strictEqual(
      reads[0],
      path.join(path.resolve(sub), ".claude", "settings.local.json")
    );
  });

  it("test_git_file_worktree_accepted", function () {
    const root = makeProject({ git: false });
    fs.writeFileSync(path.join(root, ".git"), "gitdir: /elsewhere/.git/worktrees/x\n");
    assert.strictEqual(walkToProjectRoot(root), root);
  });

  it("test_hash_parity_with_rust", function () {
    // Goldens generated from project.rs::compute_project_hash — never hand-written.
    assert.ok(GOLDENS.cases.length >= 3, "fixture must carry at least 3 golden cases");
    for (const c of GOLDENS.cases) {
      assert.strictEqual(
        computeProjectHash(c.normalized),
        c.hash,
        `hash parity for ${c.normalized}`
      );
      assert.match(c.hash, /^[0-9a-f]{16}$/);
    }
  });

  it("test_trailing_slash_normalized_before_hash", function () {
    if (process.platform === "win32") return; // posix path shape
    const c = GOLDENS.cases.find((x) => x.input.endsWith("/"));
    assert.ok(c, "trailing-slash golden present");
    assert.strictEqual(path.resolve(c.input), c.normalized);
    // walkToProjectRoot resolves before hashing, so the trailing slash never
    // reaches the hash input (same-root spawns hash identically).
    assert.strictEqual(computeProjectHash(path.resolve(c.input)), c.hash);
  });

  it("test_state_dir_path_shape", function () {
    const root = makeProject();
    const result = resolve(root);
    assert.strictEqual(
      result.stateDir,
      path.join(os.homedir(), ".unimatrix", result.projectHash, "hook-client")
    );
  });

  it("test_hash_deterministic_same_root_different_subdir", function () {
    const root = makeProject();
    const sub = path.join(root, "x", "y");
    fs.mkdirSync(sub, { recursive: true });
    const a = resolve(root);
    const b = resolve(sub);
    assert.strictEqual(a.projectRoot, b.projectRoot);
    assert.strictEqual(a.projectHash, b.projectHash);
  });
});

// ── Cross-Platform (R-14) ───────────────────────────────────────────

describe("config cross-platform", function () {
  it(
    "test_root_walk_windows_separators",
    { skip: process.platform !== "win32" },
    function () {
      // Windows runner only: walk with native backslash separators and a
      // drive-letter root; .git dir found from a backslash subpath.
      const root = makeProject();
      const sub = path.join(root, "a", "b");
      fs.mkdirSync(sub, { recursive: true });
      assert.strictEqual(walkToProjectRoot(sub.split(path.sep).join("\\")), root);
    }
  );

  it("test_windows_backslash_root_hashes_deterministically", function () {
    // Pure string hash — verifiable on every OS against the Rust golden.
    const c = GOLDENS.cases.find((x) => x.normalized.includes("\\"));
    assert.ok(c, "windows-path golden present");
    assert.strictEqual(computeProjectHash(c.normalized), c.hash);
  });

  it("test_homedir_resolution_degrades_to_null_state_dir", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("https://home.example.com", "tok"));
    const origHomedir = os.homedir;
    os.homedir = function () {
      throw new Error("no home directory");
    };
    let result;
    try {
      assert.doesNotThrow(function () {
        result = resolve(root);
      });
    } finally {
      os.homedir = origHomedir;
    }
    assert.strictEqual(result.ok, true, "resolution still returns; sends still possible");
    assert.strictEqual(result.stateDir, null, "persistence disabled, not fatal");
  });

  it("test_homedir_empty_degrades_to_null_state_dir", function () {
    const root = makeProject();
    const origHomedir = os.homedir;
    os.homedir = function () {
      return "";
    };
    let result;
    try {
      result = resolve(root);
    } finally {
      os.homedir = origHomedir;
    }
    assert.strictEqual(result.stateDir, null);
  });
});

// ── Timeout Overrides (ADR-005; keys pinned at Stage 3a) ────────────

describe("config timeout overrides", function () {
  it("test_timeout_overrides_applied", function () {
    const root = makeProject();
    writeLocalSettings(
      root,
      remoteSettings("https://t.example.com", "tok", {
        timeouts: { connect_ms: 100, sync_ms: 1500, fnf_ms: 2500 },
      })
    );
    const result = resolve(root);
    assert.strictEqual(result.ok, true);
    assert.deepStrictEqual(result.timeouts, { connectMs: 100, syncMs: 1500, fnfMs: 2500 });
  });

  it("test_partial_timeout_override_keeps_defaults", function () {
    const t = mergeTimeouts({ sync_ms: 900 });
    assert.deepStrictEqual(t, { connectMs: 750, syncMs: 900, fnfMs: 3000 });
  });

  it("test_junk_timeout_overrides_ignored", function () {
    for (const junk of [
      { connect_ms: "fast", sync_ms: -5, fnf_ms: 9e9 },
      { connect_ms: NaN, sync_ms: Infinity, fnf_ms: 0 },
      { connect_ms: null, sync_ms: {}, fnf_ms: [] },
      "not-an-object",
      ["array"],
      null,
      undefined,
      42,
    ]) {
      assert.deepStrictEqual(
        mergeTimeouts(junk),
        { ...DEFAULT_TIMEOUTS },
        `defaults retained for ${JSON.stringify(junk)}`
      );
    }
  });

  it("test_fractional_timeout_floored", function () {
    assert.strictEqual(mergeTimeouts({ connect_ms: 100.9 }).connectMs, 100);
  });

  it("test_defaults_match_adr_005", function () {
    assert.deepStrictEqual({ ...DEFAULT_TIMEOUTS }, { connectMs: 750, syncMs: 2000, fnfMs: 3000 });
  });
});

// ── Security (R-16 adjunct) ─────────────────────────────────────────

describe("config security posture", function () {
  it("test_module_has_no_network_or_output_surface", function () {
    // config.js must be incapable of network I/O or echoing the token:
    // no network/process modules, no console/stderr/stdout writes.
    const src = fs.readFileSync(
      require.resolve("../../lib/hook-client/config.js"),
      "utf8"
    );
    assert.ok(
      !/require\(["'](?:node:)?(?:http|https|net|tls|dgram|child_process)["']\)/.test(src),
      "no network-capable module required"
    );
    assert.ok(!src.includes("console."), "no console output");
    assert.ok(!src.includes("process.stderr"), "no stderr writes");
    assert.ok(!src.includes("process.stdout"), "no stdout writes");
  });

  it("test_unparseable_url_not_rejected_here", function () {
    const root = makeProject();
    writeLocalSettings(root, remoteSettings("not a url at all", "tok"));
    const result = resolve(root);
    // Fail-open: transport classifies at send time; init is the loud check.
    assert.strictEqual(result.ok, true);
    assert.strictEqual(result.urlHost, "");
  });
});
