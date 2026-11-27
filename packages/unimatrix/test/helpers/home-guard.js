"use strict";
// NFR-05 guard: a test HOME/USERPROFILE must live under os.tmpdir() and never be the real home.
const fs = require("fs");
const os = require("os");
const path = require("path");

function real(p) {
  try { return fs.realpathSync.native(p); } catch (_e) { return path.resolve(p); }
}

function assertTmpHome(home) {
  if (typeof home !== "string" || !home) throw new Error("NFR-05: test HOME is not set");
  const h = real(home);
  const tmp = real(os.tmpdir());
  const rh = real(os.homedir());
  if (h === rh) throw new Error("NFR-05: test HOME is the real home directory");
  if (h !== tmp && !h.startsWith(tmp + path.sep)) throw new Error("NFR-05: test HOME is not under os.tmpdir()");
  return home;
}

// Config-dir overrides that could redirect a spawned CLI into the real user config.
const OVERRIDE_VARS = ["CLAUDE_CONFIG_DIR", "OPENCODE_CONFIG_DIR", "XDG_CONFIG_HOME", "CODEX_HOME"];

function assertUnderTmp(p) {
  if (typeof p !== "string" || !p) throw new Error("NFR-05: override path is not set");
  const h = real(p);
  const tmp = real(os.tmpdir());
  const rh = real(os.homedir());
  if (h === rh) throw new Error("NFR-05: override path is the real home directory");
  if (h !== tmp && !h.startsWith(tmp + path.sep)) throw new Error("NFR-05: override path is not under os.tmpdir()");
  return p;
}

// Copy of process.env with every config-dir override removed; `extra` re-adds overrides only under tmp.
function scrubbedEnv(extra = {}) {
  const env = { ...process.env };
  for (const v of OVERRIDE_VARS) delete env[v];
  for (const [k, v] of Object.entries(extra)) {
    if (OVERRIDE_VARS.includes(k)) assertUnderTmp(v);
    env[k] = v;
  }
  return env;
}

// R-09 central isolation: loading this guard removes ambient overrides from process.env
// (CI runners set XDG_CONFIG_HOME etc.). Tests that need one set it explicitly, under tmp, after load.
function clearAmbientOverrides(env = process.env) {
  const cleared = [];
  for (const v of OVERRIDE_VARS) if (v in env) { delete env[v]; cleared.push(v); }
  return cleared;
}
const AMBIENT_CLEARED = clearAmbientOverrides();

module.exports = { assertTmpHome, assertUnderTmp, scrubbedEnv, clearAmbientOverrides, AMBIENT_CLEARED, OVERRIDE_VARS };
