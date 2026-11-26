"use strict";
const fs = require("fs");
const os = require("os");
const path = require("path");

function recordLastHook(hash, provider) {
  try {
    const dir = path.join(os.homedir(), ".unimatrix", hash);
    if (!provider || !fs.statSync(dir).isDirectory()) return;
    const file = path.join(dir, "last-hook.json");
    let cur = null;
    try { cur = JSON.parse(fs.readFileSync(file, "utf8")); } catch (_e) { /* replaced */ }
    if (!cur || typeof cur !== "object" || Array.isArray(cur)) cur = {};
    cur[provider] = new Date().toISOString();
    const tmp = file + "." + process.pid + "." + Date.now() + ".tmp";
    try {
      fs.writeFileSync(tmp, JSON.stringify(cur), { mode: 0o600 });
      fs.renameSync(tmp, file);
    } catch (_e) { try { fs.unlinkSync(tmp); } catch (_e2) { /* ignore */ } }
  } catch (_e) { /* best-effort */ }
}

module.exports = { recordLastHook };
