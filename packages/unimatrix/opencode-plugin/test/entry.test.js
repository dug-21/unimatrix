// @ts-check
import { test } from "node:test";
import assert from "node:assert/strict";
import { UnimatrixObservePlugin } from "../unimatrix.js";
import { PLUGIN_PACKAGE, PLUGIN_EXPORT, PLUGIN_ENTRY_FILE } from "../../lib/opencode-install.js";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Plugin identity + C9 provisioning contract (MANDATORY coordination).

test("exports UnimatrixObservePlugin as an async Plugin factory returning Hooks", async () => {
  assert.equal(typeof UnimatrixObservePlugin, "function");
  const shell = (strings, ...subs) => { void strings; void subs; return Promise.resolve(); };
  const hooks = await UnimatrixObservePlugin({ $: shell, directory: "/p", worktree: "/p", client: {} }, {});
  assert.equal(typeof hooks.event, "function");
  assert.equal(typeof hooks["chat.message"], "function");
  assert.equal(typeof hooks["tool.execute.before"], "function");
  assert.equal(typeof hooks["tool.execute.after"], "function");
  assert.equal(typeof hooks["experimental.session.compacting"], "function");
});

test("plugin load never throws into the host (returns inert hooks on bad input)", async () => {
  const hooks = await UnimatrixObservePlugin(/** @type {any} */ (null), undefined);
  assert.equal(typeof hooks, "object");
});

test("package identity matches C9 installer constants exactly", () => {
  const pkgPath = fileURLToPath(new URL("../package.json", import.meta.url));
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  assert.equal(pkg.name, PLUGIN_PACKAGE);
  assert.equal(pkg.main, PLUGIN_ENTRY_FILE);
  // The named export the C9 re-export shim imports must exist.
  assert.equal(PLUGIN_EXPORT, "UnimatrixObservePlugin");
});
