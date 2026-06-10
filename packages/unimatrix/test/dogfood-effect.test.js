"use strict";

// dogfood-effect.test.js — nan-016 Component 3 effect-verification harness.
//
// Proves AC-02 (switchover repoints by EFFECT) and AC-03 (copy-install isolation
// == code freeze) by REAL effect: it runs the committed scripts/dogfood-install.sh
// and scripts/dogfood-switchover.sh against scratch fixtures + a test-scoped temp
// install, parses the resulting settings, and RE-FIRES a real hook against the
// installed entrypoint (execFileSync). It NEVER touches this repo's live
// .claude/settings.json (READ-ONLY shape source only; pre/post sha256 prove zero
// writes) and NEVER installs into the real ~/.unimatrix/dogfood-client/.
//
// Non-vacuous core (SR-04): two MANDATORY negative controls —
//   * R-01: a broken install path makes the re-fire assertion FAIL.
//   * R-04: a leaked/edited freshly-packed copy carries a marker the frozen
//           original does not — proving the isolation assertion is meaningful.
//
// C-8: READS the installed/shipped surfaces only. Modifies no lib/** , no
// merge-settings / init / config, no package.json. This is cumulative node --test
// infra under packages/unimatrix/test/.
//
// AC-02 CLEAN post-switch state: the migration now PRUNES stale uni hooks at the
// SOURCE — vnc-031 mergeSettings Step 3c (cross-matcher migration) — so the prior
// 8-of-9 "documented stale-'*' delta" reality (#4930) is REPLACED. The fixture
// seeds REAL legacy-shaped input: the live-shaped "*" PreToolUse Rust uni hook
// plus .bak / old-client-dir uni hooks and a foreign hook. The assertions require
// the CLEAN post-state: EVERY uni-owned hook points at the installed entrypoint,
// the stale "*" Rust uni hook count == 0, foreign hooks preserved untouched.
//
// vnc-031 GATE C: T-PARITY proves Step 3c ALONE (calling mergeSettings directly,
// NOT the script-with-fragment) cleanly migrates this real legacy input for both
// the promote (object) and rollback (string) arms (P1–P5/P7/P8) — the proof that
// unblocks deleting the dogfood-switchover.sh PRUNE_FRAGMENT in the next wave.
//
// vnc-031 GATE B: the prune NEGATIVE CONTROL (T1d) shares ONE assertion helper
// with the positive check; it feeds an UNPRUNED (no-Step-3c) post-state —
// reconstructed from the seed directly via unprunedPromoteContent, NOT via the
// now-pruning mergeSettings — to the SAME helper and asserts it FAILS. Routing the
// reconstruction through mergeSettings would make the control vacuous (#4932), so
// it is deliberately rebuilt without Step 3c. A regression to a no-op Step 3c then
// surfaces as a RED positive check (non-vacuous per R-01 / R-13).

const assert = require("assert");
const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync, spawnSync } = require("child_process");
const { before, after, describe, it } = require("node:test");

// ── repo + script paths ───────────────────────────────────────────────────────

function repoRoot() {
  // git rev-parse is the same root resolution the scripts use; realpath it so the
  // scratch-hash comparison (R-03) mirrors config.js canonicalization exactly.
  const out = execFileSync("git", ["rev-parse", "--show-toplevel"], {
    cwd: __dirname,
    encoding: "utf8",
  }).trim();
  return fs.realpathSync(out);
}

const REPO = repoRoot();
const INSTALL_SH = path.join(REPO, "scripts", "dogfood-install.sh");
const SWITCH_SH = path.join(REPO, "scripts", "dogfood-switchover.sh");
const LIVE_SETTINGS_PATH = path.join(REPO, ".claude", "settings.json");
const REAL_DOGFOOD_DIR = path.join(os.homedir(), ".unimatrix", "dogfood-client");
const WORKTREE_HC = path.join(REPO, "packages", "unimatrix", "lib", "hook-client");

// Imported shipped contract — from the INSTALLED copy, set in before(). Assertions
// track shipped semantics, not hand-copied literals (R-09 / R-03 / R-10).
let installed = null; // { dir, indexJs, merge, config }
let suiteSkipReason = null;

// Tracked temp artifacts, cleaned in after() regardless of pass/fail.
const tempDirs = [];

function rand() {
  return crypto.randomBytes(6).toString("hex");
}

// ── hashing helpers (content-addressed invariance proofs) ─────────────────────

function sha256File(p) {
  return crypto.createHash("sha256").update(fs.readFileSync(p)).digest("hex");
}

/** Stable sha256 over a directory tree: relative-path + bytes, sorted. */
function sha256Tree(root) {
  const h = crypto.createHash("sha256");
  const walk = (dir) => {
    const entries = fs.readdirSync(dir, { withFileTypes: true });
    entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    for (const e of entries) {
      const full = path.join(dir, e.name);
      const rel = path.relative(root, full);
      if (e.isDirectory()) {
        h.update("D:" + rel + "\n");
        walk(full);
      } else if (e.isSymbolicLink()) {
        h.update("L:" + rel + ":" + fs.readlinkSync(full) + "\n");
      } else {
        h.update("F:" + rel + ":");
        h.update(fs.readFileSync(full));
        h.update("\n");
      }
    }
  };
  walk(root);
  return h.digest("hex");
}

// ── install / scratch fixtures ────────────────────────────────────────────────

/** R-15 / ARCH-OQ-2: install into a test-scoped temp dir, NEVER the real fixed dir. */
function installToTemp() {
  const clientDir = path.join(os.tmpdir(), "dogfood-client-test-" + rand());
  // execFileSync throws on non-zero exit — a failed install surfaces loudly.
  execFileSync(INSTALL_SH, ["--target=" + clientDir], {
    encoding: "utf8",
    stdio: "pipe",
    timeout: 120000,
  });
  const realTmp = fs.realpathSync(os.tmpdir());
  const realClient = fs.realpathSync(clientDir);
  assert.ok(
    realClient.startsWith(realTmp + path.sep),
    "install target must be under os.tmpdir(), got " + realClient
  );
  assert.notStrictEqual(
    realClient,
    fs.realpathSync(path.dirname(REAL_DOGFOOD_DIR)) + path.sep + "dogfood-client",
    "must never be the real ~/.unimatrix/dogfood-client/"
  );
  tempDirs.push(clientDir);
  return clientDir;
}

/**
 * SEED_RUST_SHAPE: the live settings *shape* (read live settings READ-ONLY and
 * mirror its structure — never write live; R-08-3), reduced to Rust-binary
 * commands with a "*" PreToolUse matcher PLUS one clearly-foreign hook. This is
 * THIS REPO'S real-world shape: a stale "*" PreToolUse uni hook that legacy
 * mergeSettings did NOT auto-prune on promote (#4930) and that vnc-031 Step 3c
 * now prunes from mergeSettings alone (the GATE C parity proof, T-PARITY).
 *
 * GATE C real-legacy P-shapes (vnc-031): the seed is a REAL legacy-shaped input,
 * never a pre-narrowed cycle-matcher seed. It carries, under managed events,
 * every uni-owned stale shape the retired script PRUNE_FRAGMENT used to handle:
 *   P1  stale "*" PreToolUse Rust uni hook                              (PreToolUse "*")
 *   P3  a ".bak" backup-dir uni hook (different whole token)            (PreToolUse "Backup")
 *   P4  an old-client-dir uni hook (dogfood-client-OLD/...)             (PreToolUse "Legacy")
 *   P7  a clearly-foreign hook alongside the stale uni hooks            (PreToolUse "Bash")
 *   P8  a managed event whose SOLE entry is a stale uni hook, alone in
 *       a non-managed matcher group (group dropped, event key retained) (UserPromptSubmit "Legacy")
 * P2/P5 (node-client pruned / Rust kept-by-identity on rollback) are exercised by
 * the rollback arm of the parity proof. P6 (quoted spaced-path keep) is proven at
 * UNIT level in merge-settings.test.js (test_cross_group_quoted_spaced_path_target_kept):
 * a spaced install path is not realizable in the os.tmpdir() harness install dir.
 */
const FOREIGN_COMMAND = "/usr/bin/foreign-linter check --no-unimatrix";

function buildSeedSettings() {
  // Read live settings READ-ONLY purely to confirm its shape exists and to mirror
  // it. We do NOT copy live commands verbatim (they carry the repo path); we build
  // an equivalent Rust-binary "*"-PreToolUse shape so the fixture is self-contained
  // and the live file is never a write target.
  let liveModel = "claude-opus-4-8";
  try {
    const live = JSON.parse(fs.readFileSync(LIVE_SETTINGS_PATH, "utf8"));
    if (live && typeof live.model === "string") liveModel = live.model;
    // Sanity: live really does carry the stale "*" PreToolUse shape this fixture
    // models. If the live shape ever changes, the seed is still valid but we note
    // the assumption here (read-only, no failure if absent).
  } catch (_e) {
    // Live unreadable — fixture is still valid; we never depend on writing it.
  }
  const rustCmd = (event) => "/repo/target/release/unimatrix hook " + event;
  // P3: a backed-up old install dir — a uni-classified node-client hook whose
  // whole path token differs from the freshly-written entrypoint (the script's
  // whole-shell-token matcher pruned it; Step 3c prunes it by object identity).
  const bakCmd = (event) =>
    "node /old/dogfood-client.bak/lib/hook-client/index.js " + event;
  // P4: an old-client-dir uni hook from a prior install.
  const oldDirCmd = (event) =>
    "node /old/dogfood-client-OLD/lib/hook-client/index.js " + event;
  return {
    model: liveModel,
    hooks: {
      SessionStart: [
        { matcher: "", hooks: [{ type: "command", command: rustCmd("SessionStart") }] },
      ],
      // P8: a managed event whose SOLE entry is a stale uni hook living alone in
      // a NON-managed matcher group ("Legacy" ≠ EVENT_MATCHERS.UserPromptSubmit
      // which is ""). Step 3c empties + drops that group; the managed group Step 3
      // creates holds the fresh entry, so the event key is retained.
      UserPromptSubmit: [
        { matcher: "Legacy", hooks: [{ type: "command", command: rustCmd("UserPromptSubmit") }] },
      ],
      PreToolUse: [
        // P1: the stale "*" Rust uni hook — the real-world shape under test.
        { matcher: "*", hooks: [{ type: "command", command: rustCmd("PreToolUse") }] },
        // P3: a ".bak" backup-dir uni hook (different whole token).
        { matcher: "Backup", hooks: [{ type: "command", command: bakCmd("PreToolUse") }] },
        // P4: an old-client-dir uni hook (dogfood-client-OLD/...).
        { matcher: "Legacy", hooks: [{ type: "command", command: oldDirCmd("PreToolUse") }] },
        // P7: a clearly-foreign hook to prove preservation / no-clobber.
        { matcher: "Bash", hooks: [{ type: "command", command: FOREIGN_COMMAND }] },
      ],
    },
  };
}

/** makeScratchRoot — R-03: realpath'd scratch root with a real .git/ DIRECTORY. */
function makeScratchRoot() {
  let root = fs.mkdtempSync(path.join(os.tmpdir(), "dogfood-scratch-"));
  // Mirror config.js realpath (#4796 guard): a symlinked os.tmpdir() cannot
  // collapse the scratch root onto the live repo's realpath/hash.
  root = fs.realpathSync(root);
  tempDirs.push(root);
  fs.mkdirSync(path.join(root, ".git")); // real DIRECTORY -> walkToProjectRoot root
  fs.mkdirSync(path.join(root, ".claude"));
  const settingsPath = path.join(root, ".claude", "settings.json");
  fs.writeFileSync(settingsPath, JSON.stringify(buildSeedSettings(), null, 2) + "\n", "utf8");

  const scratchHash = installed.config.computeProjectHash(root);
  // R-03-1: scratch hash MUST differ from the live repo hash so the daemon-absent
  // fail-open path is genuinely exercised and we cannot perturb live runtime state.
  assert.notStrictEqual(
    scratchHash,
    installed.config.computeProjectHash(REPO),
    "scratch hash must differ from live repo hash"
  );
  // R-03-3: hash is over the realpath-resolved root (config.js parity).
  assert.strictEqual(
    scratchHash,
    installed.config.computeProjectHash(fs.realpathSync(root)),
    "scratch hash must be computed over the realpath'd root"
  );
  // R-03-2: daemon-absent precondition — no socket for the scratch hash.
  const sock = installed.config.socketPathFor(scratchHash);
  assert.ok(sock && !fs.existsSync(sock), "scratch hash must have no live socket");

  return { root, settingsPath, scratchHash };
}

/**
 * tmpdirGuard — R-08 safety boundary. Every script invocation routes its
 * --settings path through this FIRST: the resolved path must live under
 * os.tmpdir() and must NOT be the live settings path. Throws on violation.
 */
function tmpdirGuard(p) {
  const resolved = path.join(fs.realpathSync(path.dirname(p)), path.basename(p));
  const realTmp = fs.realpathSync(os.tmpdir());
  assert.ok(
    resolved.startsWith(realTmp + path.sep),
    "settings path must be under os.tmpdir(): " + resolved
  );
  assert.notStrictEqual(
    resolved,
    fs.realpathSync(path.dirname(LIVE_SETTINGS_PATH)) + path.sep + path.basename(LIVE_SETTINGS_PATH),
    "settings path must never be the live settings path"
  );
  return resolved;
}

// ── script drivers ────────────────────────────────────────────────────────────

function promote(settingsPath, clientDir) {
  tmpdirGuard(settingsPath);
  return execFileSync(
    SWITCH_SH,
    ["promote", "--settings=" + settingsPath, "--client=" + clientDir],
    { encoding: "utf8", stdio: "pipe", timeout: 60000 }
  );
}

function rollback(settingsPath, clientDir) {
  tmpdirGuard(settingsPath);
  return execFileSync(
    SWITCH_SH,
    ["rollback", "--settings=" + settingsPath, "--client=" + clientDir],
    { encoding: "utf8", stdio: "pipe", timeout: 60000 }
  );
}

/**
 * reFire — the OQ-C re-fire shape, in ONE place. Invokes the installed entrypoint
 * with a synthetic hook payload on stdin, cwd = scratch root (so it resolves the
 * scratch hash, NOT the live hash). execFileSync throws on non-zero exit; we
 * convert that into a structured { exitCode, stdout, stderr } so callers can
 * assert exit-0 / empty-stdout (success) or non-zero (the negative control).
 */
function reFire(indexJs, event, cwd, payloadObj) {
  // spawnSync (not execFileSync) so BOTH stdout and stderr are captured on EVERY
  // path — including the fail-open exit-0 path. The leak negative control (T3)
  // asserts on stderr of an exit-0 invocation, which execFileSync would discard.
  const r = spawnSync("node", [indexJs, event], {
    cwd,
    input: typeof payloadObj === "string" ? payloadObj : JSON.stringify(payloadObj),
    encoding: "utf8",
    timeout: 15000,
  });
  // A failure to spawn (e.g. broken path is handled by node itself exiting
  // non-zero; r.error covers timeouts / ENOENT on the node binary). status is
  // null when the process was killed or never started -> treat as non-zero.
  const exitCode = r.status === 0 ? 0 : typeof r.status === "number" ? r.status : 1;
  return {
    exitCode,
    stdout: r.stdout != null ? String(r.stdout) : "",
    stderr: r.stderr != null ? String(r.stderr) : "",
  };
}

// ── settings inspection helpers ───────────────────────────────────────────────

/** All uni-owned hook entries across all events, with their event + matcher. */
function uniHooks(settings) {
  const out = [];
  const hooks = (settings && settings.hooks) || {};
  for (const event of Object.keys(hooks)) {
    for (const group of hooks[event] || []) {
      for (const h of group.hooks || []) {
        if (installed.merge.isUnimatrixHook(h)) {
          out.push({ event, matcher: group.matcher, command: h.command });
        }
      }
    }
  }
  return out;
}

function foreignPresent(settings) {
  const pre = settings.hooks.PreToolUse || [];
  return pre.some((g) => (g.hooks || []).some((h) => h.command === FOREIGN_COMMAND));
}

/**
 * entryPrefixFor — the `node <installed>/lib/hook-client/index.js ` command
 * prefix the installed entrypoint produces. The post-promote target form (R-09).
 */
function entryPrefixFor(clientDir) {
  return "node " + path.join(clientDir, "lib", "hook-client", "index.js") + " ";
}

/**
 * assertCleanPromoteState — the SINGLE shared assertion helper for the AC-02
 * CLEAN post-promote state. Used by BOTH the positive promote check (T1) and the
 * prune NEGATIVE CONTROL (T1d): the positive feeds the real (pruned) post-state
 * and EXPECTS it to pass; the negative feeds the mergeSettings-alone / no-prune
 * post-state and EXPECTS this helper to THROW. Sharing one helper guarantees a
 * regression to a no-op prune surfaces (non-vacuous per R-01).
 *
 * Asserts the CLEAN post-state:
 *   (a) EVERY uni-owned hook (per shipped isUnimatrixHook, across ALL events/
 *       matchers) references the installed entrypoint — stale "*" Rust uni hook
 *       count == 0 (the inverted assertion vs #4930).
 *   (b) the PreToolUse cycle group's matcher === the imported PRETOOLUSE_CYCLE_MATCHER.
 *   (c) the foreign hook is preserved with no duplicates.
 *   (d) the registered uni event count matches the actual opt-in state.
 *
 * @param {object} settings - parsed settings.json post-promote.
 * @param {string} clientDir - the installed client dir.
 * @param {number} expectedEventCount - 8 (no opt-in) or 9 (SubagentStop opt-in).
 */
function assertCleanPromoteState(settings, clientDir, expectedEventCount) {
  const entryPrefix = entryPrefixFor(clientDir);
  const indexJs = path.join(clientDir, "lib", "hook-client", "index.js");
  const all = uniHooks(settings);

  // (a) stale "*" Rust uni hook count == 0 — EVERY uni hook is on the entrypoint.
  const stale = all.filter((h) => !h.command.startsWith(entryPrefix));
  assert.strictEqual(
    stale.length,
    0,
    "CLEAN post-state: zero stale uni hooks off the entrypoint (found " +
      stale.length +
      ": " +
      JSON.stringify(stale) +
      ")"
  );

  // every surviving uni hook equals the exact buildHookClientCommand form.
  for (const h of all) {
    const expected = installed.merge.buildHookClientCommand(indexJs, h.event);
    assert.strictEqual(
      h.command,
      expected,
      "uni hook for " + h.event + " must equal buildHookClientCommand form"
    );
  }

  // (d) event count matches the actual opt-in state.
  assert.strictEqual(
    all.length,
    expectedEventCount,
    "registered uni event count must be " + expectedEventCount
  );

  // (b) PreToolUse cycle group matcher === imported constant (not a literal).
  const cycleGroups = (settings.hooks.PreToolUse || []).filter((g) =>
    (g.hooks || []).some((h) => h.command.startsWith(entryPrefix))
  );
  assert.strictEqual(cycleGroups.length, 1, "exactly one cycle-matcher group");
  assert.strictEqual(
    cycleGroups[0].matcher,
    installed.merge.PRETOOLUSE_CYCLE_MATCHER,
    "PreToolUse matcher must equal imported PRETOOLUSE_CYCLE_MATCHER"
  );

  // (c) foreign preserved, no duplicate entrypoint commands.
  assert.ok(foreignPresent(settings), "foreign hook must survive promote");
  const cmds = all.map((h) => h.command);
  assert.strictEqual(new Set(cmds).size, cmds.length, "no duplicate entrypoint commands");
}

/**
 * unprunedPromoteContent — reconstructs the post-promote state as mergeSettings
 * WOULD have produced it BEFORE vnc-031 Step 3c existed: the fresh managed-group
 * entry is repointed/added under EVENT_MATCHERS[event], but the legacy stale
 * cross-group uni hooks (the "*" PreToolUse Rust hook, .bak, old-client-dir) are
 * NOT pruned — they lived OUTSIDE the managed group, and Step 3's in-group-only
 * repoint never touched them.
 *
 * GATE B / R-13 (#4932): this MUST NOT call installed.merge.mergeSettings — that
 * now bakes in Step 3c and would return a CLEAN state, making the negative control
 * vacuous (its assert.throws would stop throwing). We reconstruct the no-Step-3c
 * state directly from the SEED instead: apply ONLY the managed-matcher repoint per
 * event, leaving every other matcher group (foreign AND stale-uni) intact. This is
 * the pre-Step-3c mergeSettings behavior, reconstructed without invoking the
 * (now-pruning) mergeSettings. The fixture-sanity assertion in T1d
 * (staleNoPrune.length >= 1) guards that the stale hook genuinely survives here.
 */
function unprunedPromoteContent(settingsPath, clientDir) {
  const indexJs = path.join(clientDir, "lib", "hook-client", "index.js");
  const merge = installed.merge;
  const content = JSON.parse(fs.readFileSync(settingsPath, "utf8")); // the makeScratchRoot seed
  if (!content.hooks || typeof content.hooks !== "object") content.hooks = {};

  // SubagentStop opt-in filtering, mirroring mergeSettings' registered-event set
  // (the scratch seed has no settings.local.json, so SubagentStop stays out).
  let events = merge.HOOK_EVENTS;
  const optInFile = path.join(path.dirname(settingsPath), "settings.local.json");
  if (events.includes("SubagentStop") && !merge.subagentStopEnabled(optInFile)) {
    events = events.filter((e) => e !== "SubagentStop");
  }

  // Mirror Step 3 ONLY (in-group repoint/append under the managed matcher),
  // DELIBERATELY skipping the Step 3c cross-group prune so stale uni hooks under
  // other matcher groups survive.
  for (const event of events) {
    const matcher = merge.EVENT_MATCHERS[event];
    const freshEntry = {
      type: "command",
      command: merge.buildHookClientCommand(indexJs, event),
    };
    if (!Array.isArray(content.hooks[event])) content.hooks[event] = [];
    const eventArray = content.hooks[event];
    let group = eventArray.find((g) => g && g.matcher === matcher);
    if (!group) {
      eventArray.push({ matcher, hooks: [freshEntry] });
      continue;
    }
    if (!Array.isArray(group.hooks)) group.hooks = [];
    const idx = group.hooks.findIndex((h) => merge.isUnimatrixHook(h));
    if (idx >= 0) group.hooks[idx] = freshEntry;
    else group.hooks.push(freshEntry);
    // NOTE: no walk over the other groups — stale cross-group uni hooks survive.
  }
  return content;
}

// ── suite lifecycle ───────────────────────────────────────────────────────────

let LIVE_SETTINGS_HASH_PRE = null;
let REAL_DOGFOOD_HASH_PRE = null;
let WORKTREE_HC_HASH_PRE = null;

before(() => {
  // R-08-2 pre-hash + R-15-2 real-install baseline + R-13 clean-tree baseline.
  LIVE_SETTINGS_HASH_PRE = sha256File(LIVE_SETTINGS_PATH);
  REAL_DOGFOOD_HASH_PRE = fs.existsSync(REAL_DOGFOOD_DIR)
    ? sha256Tree(REAL_DOGFOOD_DIR)
    : null;
  WORKTREE_HC_HASH_PRE = sha256Tree(WORKTREE_HC);

  // ARCH-OQ-2: install into a test-scoped temp dir. If install cannot be staged
  // (e.g. npm/tar unavailable in this environment), SKIP loudly — never crash (R-05).
  try {
    const dir = installToTemp();
    const indexJs = path.join(dir, "lib", "hook-client", "index.js");
    // Import the shipped contract from the INSTALLED copy (honesty: AC-03 proves
    // installed === in-repo bytes, so importing installed makes the assertions
    // assert the *installed* semantics).
    const merge = require(path.join(dir, "lib", "merge-settings.js"));
    const config = require(path.join(dir, "lib", "hook-client", "config.js"));
    installed = { dir, indexJs, merge, config };
  } catch (e) {
    suiteSkipReason =
      "dogfood install could not be staged (R-05): " + (e && e.message ? e.message : e);
  }
});

after(() => {
  // Teardown FIRST cleans temp artifacts, THEN asserts the live surfaces are
  // provably untouched — even on a failure path (R-13-2). These assertions run
  // regardless of individual test outcomes.
  for (const d of tempDirs) {
    try {
      fs.rmSync(d, { recursive: true, force: true });
    } catch (_e) {
      // best-effort cleanup; never mask the invariant assertions below.
    }
  }
  // R-08-2: zero live-settings writes.
  assert.strictEqual(
    sha256File(LIVE_SETTINGS_PATH),
    LIVE_SETTINGS_HASH_PRE,
    "live .claude/settings.json must be byte-identical (zero live writes)"
  );
  // R-15-2: a human-staged real dogfood install is never disturbed.
  if (REAL_DOGFOOD_HASH_PRE !== null) {
    assert.strictEqual(
      sha256Tree(REAL_DOGFOOD_DIR),
      REAL_DOGFOOD_HASH_PRE,
      "real ~/.unimatrix/dogfood-client/ must be untouched"
    );
  }
  // R-13: the live working tree's hook-client is provably clean.
  assert.strictEqual(
    sha256Tree(WORKTREE_HC),
    WORKTREE_HC_HASH_PRE,
    "live working tree lib/hook-client/ must be clean after the suite"
  );
});

// ── tests ─────────────────────────────────────────────────────────────────────

describe("dogfood-effect: AC-02 switchover by effect", () => {
  it("T1: promote -> CLEAN post-state (every uni hook on installed entrypoint, stale '*' Rust hook count 0) + matcher delta + fail-open re-fire", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { root, settingsPath } = makeScratchRoot();

    promote(settingsPath, installed.dir);
    const s = JSON.parse(fs.readFileSync(settingsPath, "utf8"));

    const entryPrefix = entryPrefixFor(installed.dir);

    // ── AC-02 CLEAN post-state via the SHARED assertion helper (also exercised by
    // the prune negative control T1d). No opt-in -> 8 events (SubagentStop out).
    // This asserts: (a) stale "*" Rust uni hook count == 0 and EVERY uni hook
    // equals the buildHookClientCommand form, (b) matcher === imported constant,
    // (c) foreign preserved / no dupes, (d) event count == 8.
    assertCleanPromoteState(s, installed.dir, 8);

    // Explicit (b) the stale "*" Rust uni hook is genuinely GONE — the inverted
    // assertion vs #4930: it used to survive; the prune now removes it.
    const stale = uniHooks(s).filter((h) => !h.command.startsWith(entryPrefix));
    assert.strictEqual(stale.length, 0, "stale '*' PreToolUse Rust uni hook must be PRUNED");
    assert.ok(
      !(s.hooks.PreToolUse || []).some(
        (g) => g.matcher === "*" && (g.hooks || []).some((h) => installed.merge.isUnimatrixHook(h))
      ),
      "no uni hook may survive under the legacy '*' PreToolUse matcher"
    );

    // <installed> is the real test-scoped install dir, not a placeholder.
    const realTmp = fs.realpathSync(os.tmpdir());
    assert.ok(
      fs.realpathSync(installed.dir).startsWith(realTmp + path.sep),
      "installed entrypoint dir must be the real test-scoped tmp install"
    );

    // ── RE-FIRE (OQ-C) — the non-vacuous core (R-01, R-07, SR-08) ──
    const res = reFire(installed.indexJs, "SessionStart", root, {
      hook_event_name: "SessionStart",
    });
    assert.strictEqual(res.exitCode, 0, "re-fire exit 0 (fail-open). stderr: " + res.stderr);
    assert.strictEqual(res.stdout, "", "re-fire must write nothing to stdout (injection channel)");
  });

  it("T1d: prune NEGATIVE CONTROL — mergeSettings-alone (no prune) post-state FAILS the SAME clean-state helper (MANDATORY)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { settingsPath } = makeScratchRoot();

    // Reconstruct the post-state as mergeSettings produced it BEFORE vnc-031 Step
    // 3c existed (managed-group repoint only, NO cross-group prune) — the stale
    // "*" Rust uni hook + .bak + old-client-dir hooks survive un-repointed (#4930).
    // GATE B / R-13 (#4932): this reconstruction MUST NOT route through
    // mergeSettings (which now bakes in Step 3c and would return a CLEAN state,
    // making this control vacuous). Feeding the genuinely-unpruned state to the
    // SAME helper the positive T1 uses MUST throw: this proves the positive
    // assertion can detect an unpruned leftover and is not vacuously green. A
    // regression to a no-op Step 3c would make the real promote produce THIS state.
    const unprunedContent = unprunedPromoteContent(settingsPath, installed.dir);

    // sanity: the no-Step-3c content really still carries the stale uni hooks.
    const staleNoPrune = uniHooks(unprunedContent).filter(
      (h) => !h.command.startsWith(entryPrefixFor(installed.dir))
    );
    assert.ok(
      staleNoPrune.length >= 1,
      "no-prune post-state must still carry the stale uni hook (fixture sanity)"
    );

    assert.throws(
      () => assertCleanPromoteState(unprunedContent, installed.dir, 8),
      "the clean-state helper MUST fail on the unpruned post-state (prune is non-vacuous)"
    );
  });

  it("T1b: re-fire negative control — a broken install path FAILS the assertion (R-01 mandatory)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { root } = makeScratchRoot();
    const brokenIndex = path.join(os.tmpdir(), "no-such-dir-" + rand(), "index.js");
    const res = reFire(brokenIndex, "SessionStart", root, { hook_event_name: "SessionStart" });
    // Proves the exit-0 re-fire check is non-vacuous: a bad install is detectable.
    assert.notStrictEqual(res.exitCode, 0, "broken install path must NOT exit 0");
  });

  it("T1c: fail-open on malformed/empty stdin (R-07)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { root, scratchHash } = makeScratchRoot();
    const sock = installed.config.socketPathFor(scratchHash);
    assert.ok(sock && !fs.existsSync(sock), "precondition: no scratch socket");

    const malformed = reFire(installed.indexJs, "SessionStart", root, "{ this is : not json");
    assert.strictEqual(malformed.exitCode, 0, "malformed stdin must fail-open exit 0");
    assert.strictEqual(malformed.stdout, "", "malformed stdin must write nothing to stdout");

    const empty = reFire(installed.indexJs, "SessionStart", root, "");
    assert.strictEqual(empty.exitCode, 0, "empty stdin must fail-open exit 0");
    assert.strictEqual(empty.stdout, "", "empty stdin must write nothing to stdout");
  });

  it("T2: promote then rollback -> CLEAN Rust legacy form (no stale node-client uni hook survives), idempotent, foreign preserved (R-06)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { settingsPath } = makeScratchRoot();

    promote(settingsPath, installed.dir);
    rollback(settingsPath, installed.dir);
    const after1 = fs.readFileSync(settingsPath, "utf8");
    const s1 = JSON.parse(after1);

    const rustBinary = path.join(REPO, "target", "release", "unimatrix");
    const binDir = path.dirname(rustBinary);
    const expectedRust = (event) =>
      "LD_LIBRARY_PATH=" + binDir + " " + rustBinary + " hook " + event;
    const entryPrefix = entryPrefixFor(installed.dir);

    // (a) EVERY uni-owned hook (across ALL groups) is exactly the legacy Rust form.
    const all = uniHooks(s1);
    for (const h of all) {
      assert.strictEqual(
        h.command,
        expectedRust(h.event),
        "rolled-back uni hook for " + h.event + " must equal the legacy Rust form"
      );
    }
    const events = new Set(all.map((h) => h.event));
    assert.ok(events.size >= 8, "rollback re-owns the registered events in Rust form");

    // (b) NO stale node-client uni hook survives — the promote-side node-client
    // group is PRUNED on rollback (mirror of the promote-side Rust-group prune).
    const staleNode = all.filter((h) => h.command.startsWith(entryPrefix));
    assert.strictEqual(
      staleNode.length,
      0,
      "no node-client uni hook may survive rollback (found " + staleNode.length + ")"
    );

    // (c) foreign preserved, no duplicates.
    assert.ok(foreignPresent(s1), "foreign hook preserved through round-trip");
    const cmds = all.map((h) => h.command);
    assert.strictEqual(new Set(cmds).size, cmds.length, "no duplicate rolled-back commands");

    // Idempotent: a second rollback is a no-op on the settings content (idempotent
    // re-point AND idempotent prune — a stale node-client group is gone after the
    // first rollback and stays gone).
    rollback(settingsPath, installed.dir);
    const after2 = fs.readFileSync(settingsPath, "utf8");
    assert.strictEqual(after2, after1, "second rollback must be idempotent (byte-identical)");
  });

  // ── GATE C parity proof (vnc-031 ADR-003 / SR-04 / R-04 / #4938) ──
  // Proves the source prune (mergeSettings Step 3c) subsumes EVERY case the
  // retired script PRUNE_FRAGMENT handled, on REAL legacy-shaped input — a
  // settings file carrying a genuine "*" Rust PreToolUse uni hook plus .bak /
  // old-client-dir uni hooks. This calls installed.merge.mergeSettings DIRECTLY
  // (NOT promote()/rollback() — which still run the script-with-fragment in this
  // wave), so a GREEN result is attributable to Step 3c ALONE and not to the
  // script's bespoke prune. This is the proof that unblocks the fragment-deletion
  // commit in the next wave. P6 (quoted spaced-path keep) is proven at unit level
  // in merge-settings.test.js; this harness owns P1–P5/P7/P8 on real install.
  it("T-PARITY: mergeSettings ALONE (Step 3c, no script fragment) cleanly migrates real legacy input for BOTH arms (GATE C P1–P5/P7/P8)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);

    const indexJs = path.join(installed.dir, "lib", "hook-client", "index.js");
    const entryPrefix = entryPrefixFor(installed.dir);
    const rustBinary = path.join(REPO, "target", "release", "unimatrix");
    const binDir = path.dirname(rustBinary);
    const expectedRust = (event) =>
      "LD_LIBRARY_PATH=" + binDir + " " + rustBinary + " hook " + event;

    // ── PROMOTE arm (object commandSource) — P1/P3/P4/P7/P8 ──
    {
      const { settingsPath } = makeScratchRoot();
      // Pre-condition: the seed really is real legacy-shaped (not pre-narrowed) —
      // a genuine "*" PreToolUse Rust uni hook plus .bak and old-client-dir hooks.
      const seed = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
      const seedStaleStar = (seed.hooks.PreToolUse || []).find(
        (g) => g.matcher === "*" && (g.hooks || []).some((h) => installed.merge.isUnimatrixHook(h))
      );
      assert.ok(seedStaleStar, "GATE C precondition: seed carries a genuine '*' PreToolUse uni hook");
      const seedUni = uniHooks(seed);
      assert.ok(
        seedUni.some((h) => h.command.indexOf("dogfood-client.bak") !== -1),
        "GATE C precondition: seed carries a .bak uni hook (P3)"
      );
      assert.ok(
        seedUni.some((h) => h.command.indexOf("dogfood-client-OLD") !== -1),
        "GATE C precondition: seed carries an old-client-dir uni hook (P4)"
      );

      const result = installed.merge.mergeSettings(
        settingsPath,
        {
          events: installed.merge.HOOK_EVENTS,
          commandForEvent: (event) => installed.merge.buildHookClientCommand(indexJs, event),
        },
        { dryRun: true } // pure compute — attribute the clean state to Step 3c only.
      );
      const s = result.content;

      // P1/P3/P4: every stale cross-group uni hook (Rust "*", .bak, old-dir) is
      // pruned; the SAME shared clean-state helper the script-driven T1 uses passes
      // on the mergeSettings-ALONE post-state. (No script, no fragment touched it.)
      assertCleanPromoteState(s, installed.dir, 8);

      // Explicit P1: no uni hook survives under the legacy "*" PreToolUse matcher.
      assert.ok(
        !(s.hooks.PreToolUse || []).some(
          (g) => g.matcher === "*" && (g.hooks || []).some((h) => installed.merge.isUnimatrixHook(h))
        ),
        "P1: no uni hook may survive under the legacy '*' PreToolUse matcher (Step 3c)"
      );
      const promoteUni = uniHooks(s);
      // P3: no .bak survivor.
      assert.ok(
        !promoteUni.some((h) => h.command.indexOf("dogfood-client.bak") !== -1),
        "P3: the .bak uni hook must be pruned"
      );
      // P4: no old-client-dir survivor.
      assert.ok(
        !promoteUni.some((h) => h.command.indexOf("dogfood-client-OLD") !== -1),
        "P4: the old-client-dir uni hook must be pruned"
      );
      // P7: foreign preserved byte-for-byte.
      assert.ok(foreignPresent(s), "P7: foreign hook preserved through promote (Step 3c)");
      // P8: the UserPromptSubmit stale "Legacy" group is dropped (emptied of its
      // sole uni hook) while the event key is retained (managed "" group holds it).
      const ups = s.hooks.UserPromptSubmit || [];
      assert.ok(ups.length >= 1, "P8: UserPromptSubmit event key retained");
      assert.ok(
        !ups.some((g) => g.matcher === "Legacy"),
        "P8: the emptied non-managed 'Legacy' group must be dropped"
      );
      assert.ok(
        ups.some((g) => g.matcher === installed.merge.EVENT_MATCHERS.UserPromptSubmit),
        "P8: the managed UserPromptSubmit group survives with the fresh entry"
      );

      // The clean state is attributable to Step 3c: the cross-matcher action fired.
      assert.ok(
        result.actions.some((a) => a.indexOf("(cross-matcher migration)") !== -1),
        "Step 3c emitted the cross-matcher migration action (attribution)"
      );
    }

    // ── ROLLBACK arm (string commandSource) — P2/P3/P5/P7/P8 ──
    {
      const { settingsPath } = makeScratchRoot();
      // First repoint to node-client form (promote) WITHOUT the script, so the
      // rollback faces a genuine stale node-client uni hook to prune (P2) — again
      // attributable to mergeSettings alone.
      installed.merge.mergeSettings(
        settingsPath,
        {
          events: installed.merge.HOOK_EVENTS,
          commandForEvent: (event) => installed.merge.buildHookClientCommand(indexJs, event),
        },
        {} // write the node-client post-state so rollback has a real input.
      );

      // Rollback = legacy Rust binary STRING arm (normalizeCommandSource legacy form).
      const result = installed.merge.mergeSettings(settingsPath, rustBinary, { dryRun: true });
      const s = result.content;
      const all = uniHooks(s);

      // P5: every uni hook is EXACTLY the legacy Rust form — the just-written Rust
      // command is KEPT by object identity (no dirname heuristic, no tokenizer).
      for (const h of all) {
        assert.strictEqual(
          h.command,
          expectedRust(h.event),
          "P5: rolled-back uni hook for " + h.event + " must be the exact legacy Rust form (kept by identity)"
        );
      }
      assert.ok(new Set(all.map((h) => h.event)).size >= 8, "rollback re-owns the registered events");
      // P2: no stale node-client uni hook survives the rollback.
      const staleNode = all.filter((h) => h.command.startsWith(entryPrefix));
      assert.strictEqual(staleNode.length, 0, "P2: no node-client uni hook may survive rollback");
      // P3: no .bak survivor (carried through from the seed if any remained).
      assert.ok(
        !all.some((h) => h.command.indexOf("dogfood-client.bak") !== -1),
        "P3: the .bak uni hook must not survive rollback"
      );
      // P7: foreign preserved; no duplicates.
      assert.ok(foreignPresent(s), "P7: foreign hook preserved through rollback (Step 3c)");
      const cmds = all.map((h) => h.command);
      assert.strictEqual(new Set(cmds).size, cmds.length, "no duplicate rolled-back commands");
      // P8: UserPromptSubmit event key retained, no stale non-managed group.
      const ups = s.hooks.UserPromptSubmit || [];
      assert.ok(ups.length >= 1, "P8: UserPromptSubmit event key retained on rollback");
      assert.ok(
        !ups.some((g) => g.matcher === "Legacy"),
        "P8: the emptied non-managed 'Legacy' group must be dropped on rollback"
      );

      // Idempotent: a second mergeSettings rollback is a no-op on content.
      const r2 = installed.merge.mergeSettings(settingsPath, rustBinary, { dryRun: true });
      assert.deepStrictEqual(
        r2.content,
        s,
        "rollback migration is idempotent (Step 3c stable on second run)"
      );
    }
  });
});

describe("dogfood-effect: AC-03 copy-install isolation == code freeze", () => {
  it("T3: in-repo edit in a THROWAWAY copy does not change installed bytes/behavior; entrypoint is not a symlink (R-04, R-13, C-6)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    const { root: scratchRoot } = makeScratchRoot();

    // C-6 / R-04-2 structural anti-npm-link guarantee.
    assert.strictEqual(
      fs.lstatSync(installed.indexJs).isSymbolicLink(),
      false,
      "installed entrypoint must NOT be a symlink"
    );

    const hcDir = path.join(installed.dir, "lib", "hook-client");
    const hcHashBefore = sha256Tree(hcDir);
    const preEdit = reFire(installed.indexJs, "SessionStart", scratchRoot, {
      hook_event_name: "SessionStart",
    });
    assert.strictEqual(preEdit.exitCode, 0, "pre-edit re-fire exit 0");

    // ── OQ-D / ARCH-OQ-1: edit a THROWAWAY COPY, NEVER the live working tree (R-13).
    // Copy the in-repo client package into a tmp dir, `git init` it so the install
    // script's `git rev-parse` resolves the copy as repo root, inject a
    // behavior-changing marker into a hook-client source file, then re-pack it.
    const tmpRepo = path.join(os.tmpdir(), "dogfood-repo-copy-" + rand());
    tempDirs.push(tmpRepo);
    const pkgDst = path.join(tmpRepo, "packages", "unimatrix");
    fs.mkdirSync(pkgDst, { recursive: true });
    fs.cpSync(path.join(REPO, "packages", "unimatrix"), pkgDst, { recursive: true });
    // A minimal scripts/ so the copy can run its own install entrypoint.
    fs.mkdirSync(path.join(tmpRepo, "scripts"), { recursive: true });
    fs.copyFileSync(INSTALL_SH, path.join(tmpRepo, "scripts", "dogfood-install.sh"));
    fs.chmodSync(path.join(tmpRepo, "scripts", "dogfood-install.sh"), 0o755);
    execFileSync("git", ["init", "-q"], { cwd: tmpRepo });

    const LEAK = "DOGFOOD-LEAK-MARKER-" + rand();
    const targetSrc = path.join(pkgDst, "lib", "hook-client", "index.js");
    fs.appendFileSync(
      targetSrc,
      "\ntry { process.stderr.write(" + JSON.stringify(LEAK) + "); } catch (_e) {}\n"
    );

    const secondClient = path.join(os.tmpdir(), "dogfood-client-test-" + rand());
    tempDirs.push(secondClient);
    execFileSync(
      path.join(tmpRepo, "scripts", "dogfood-install.sh"),
      ["--target=" + secondClient],
      { cwd: tmpRepo, encoding: "utf8", stdio: "pipe", timeout: 120000 }
    );

    // ORIGINAL install is INVARIANT (NFR-3 byte freeze).
    assert.strictEqual(
      sha256Tree(hcDir),
      hcHashBefore,
      "editing a throwaway copy must not change the original installed bytes"
    );
    const postEdit = reFire(installed.indexJs, "SessionStart", scratchRoot, {
      hook_event_name: "SessionStart",
    });
    assert.strictEqual(postEdit.exitCode, 0, "post-edit re-fire of original exit 0");
    assert.strictEqual(postEdit.stdout, preEdit.stdout, "original behavior unchanged (stdout)");
    assert.strictEqual(
      postEdit.stderr.indexOf(LEAK),
      -1,
      "leak marker must be ABSENT from the frozen original's stderr"
    );

    // ── NEGATIVE CONTROL (R-04-1, mandatory): the marker IS real — the freshly
    // packed EDITED copy carries it. This proves the original's invariance is
    // meaningful, not a no-op tautology.
    const secondIndex = path.join(secondClient, "lib", "hook-client", "index.js");
    const leaked = reFire(secondIndex, "SessionStart", scratchRoot, {
      hook_event_name: "SessionStart",
    });
    assert.ok(
      leaked.stderr.indexOf(LEAK) !== -1,
      "the edited freshly-packed copy MUST carry the leak marker (negative control)"
    );

    // R-04-3 / SR-07 (#4923): isolation == byte/behavior freeze of the installed
    // lib/, NOT state-dir separation. This test deliberately does NOT require
    // separate {hash} state dirs — the shared ~/.unimatrix/{hash}/ keyed on PROJECT
    // ROOT is by design. We assert that explicitly: both clients, run from the SAME
    // scratch root, resolve the SAME project hash / socket (shared state is fine).
    const c1 = require(path.join(installed.dir, "lib", "hook-client", "config.js"));
    const c2 = require(path.join(secondClient, "lib", "hook-client", "config.js"));
    assert.strictEqual(
      c1.computeProjectHash(scratchRoot),
      c2.computeProjectHash(scratchRoot),
      "isolation is code-freeze, NOT state separation: shared {hash} state is by design (#4923)"
    );
  });
});

describe("dogfood-effect: live-settings guard", () => {
  it("T4: tmpdirGuard rejects the live settings path (R-08-1)", (t) => {
    if (suiteSkipReason) return t.skip(suiteSkipReason);
    assert.throws(
      () => tmpdirGuard(LIVE_SETTINGS_PATH),
      "tmpdirGuard must reject the live settings path"
    );
    // A tmp path is accepted (positive arm of the guard).
    const okPath = path.join(fs.realpathSync(os.tmpdir()), "guard-ok-" + rand() + ".json");
    fs.writeFileSync(okPath, "{}");
    tempDirs.push(okPath);
    assert.doesNotThrow(() => tmpdirGuard(okPath), "tmpdirGuard must accept a tmpdir path");
  });
});
