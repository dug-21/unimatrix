"use strict";

const fs = require("fs");
const path = require("path");

/**
 * Regex patterns to identify Unimatrix-owned hook entries (ADR-004).
 * Matches both current "unimatrix" and pre-rename "unimatrix-server" commands
 * (local binary mode), whether bare names or absolute paths, AND the remote
 * node-client command form (vnc-026).
 *
 * The fifth (node-client) pattern resolves the Alignment WARN 2 spaced-path
 * defect: the defective `\S*\/hook-client\/index\.js` form could not cross a
 * space inside a spaced install path and never matched backslash separators.
 * The replacement uses three alternations (double-quoted / single-quoted /
 * bare path) and accepts both `/` and `\` separators. We only ever WRITE an
 * unquoted command when the path has no whitespace (buildHookClientCommand),
 * so `\S+` is correct for the bare arm. See pseudocode/init-remote.md §1.
 */
const UNIMATRIX_PATTERNS = [
  /^unimatrix\s+hook\s/,
  /^unimatrix-server\s+hook\s/,
  /\/unimatrix\s+hook\s/,
  /\/unimatrix-server\s+hook\s/,
  // node <path>/hook-client/index.js <EVENT> — quoted or bare, / or \ separators.
  /(^|[\s"'/\\])node(\.exe)?\s+("[^"]*[/\\]hook-client[/\\]index\.js"|'[^']*[/\\]hook-client[/\\]index\.js'|\S+[/\\]hook-client[/\\]index\.js)\s/,
];

const HOOK_EVENTS = [
  "SessionStart",
  "Stop",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
  "PostToolUseFailure",
  "PreCompact",
  "SubagentStart",
  "SubagentStop",
];

/** Matcher per event: "" for session-level, "*" for tool/agent-level */
const EVENT_MATCHERS = {
  SessionStart: "",
  Stop: "",
  UserPromptSubmit: "",
  PreToolUse: "*",
  PostToolUse: "*",
  PostToolUseFailure: "*",
  PreCompact: "",
  SubagentStart: "*",
  SubagentStop: "*",
};

/**
 * Build a remote hook-client command string for a given event.
 *
 * The command is `node <path> <event>`, with the path double-quoted iff it
 * contains whitespace. Double quotes work for both POSIX shells and Windows
 * cmd; settings.json hook commands run via shell, so an unquoted spaced path
 * would mis-parse — quoting is required for execution correctness, not just
 * for the ownership regex. See pseudocode/init-remote.md §1.
 *
 * @param {string} clientPath - Absolute path to lib/hook-client/index.js.
 * @param {string} event - Hook event name (the trailing argument).
 * @returns {string} The hook command string.
 */
function buildHookClientCommand(clientPath, event) {
  const quoted = /\s/.test(clientPath) ? '"' + clientPath + '"' : clientPath;
  return "node " + quoted + " " + event;
}

/**
 * Normalize a commandSource into the canonical
 * { events: string[], commandForEvent(event) -> string } shape.
 *
 * Back-compat: a string argument is the legacy local binaryPath call site.
 * It is mapped to the EXACT command string the pre-generalization
 * implementation produced — `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` —
 * over the full HOOK_EVENTS set. This preserves byte-identical local output
 * (AC-16), except for the two NEW events FR-21 adds, which is the intended fix.
 *
 * @param {object|string} cs - commandSource object, or legacy binaryPath string.
 * @returns {{ events: string[], commandForEvent: function }}
 */
function normalizeCommandSource(cs) {
  if (typeof cs === "string") {
    const binaryPath = cs;
    const binDir = path.dirname(binaryPath);
    return {
      events: HOOK_EVENTS,
      commandForEvent: (event) =>
        "LD_LIBRARY_PATH=" + binDir + " " + binaryPath + " hook " + event,
    };
  }
  return cs;
}

/**
 * Returns true if a hook entry is owned by Unimatrix, identified by
 * prefix-matching the command field against known patterns.
 *
 * @param {object} hookEntry - A hook entry object with a `command` field.
 * @returns {boolean}
 */
function isUnimatrixHook(hookEntry) {
  if (!hookEntry || !hookEntry.command || typeof hookEntry.command !== "string") {
    return false;
  }
  return UNIMATRIX_PATTERNS.some((pattern) => pattern.test(hookEntry.command));
}

/**
 * Merge Unimatrix hook configuration into .claude/settings.json.
 *
 * Implements ADR-004 prefix-match identification. Preserves all non-unimatrix
 * hooks, permissions, and other top-level keys. Idempotent: running twice
 * produces the same result.
 *
 * @param {string} filePath - Path to .claude/settings.json
 * @param {object|string} commandSource - Either { events, commandForEvent } or,
 *   for back-compat, the legacy local binaryPath string.
 * @param {object} options - { dryRun: boolean }
 * @returns {{ actions: string[], content: object }}
 */
function mergeSettings(filePath, commandSource, options) {
  const dryRun = (options && options.dryRun) || false;
  const source = normalizeCommandSource(commandSource);
  const actions = [];
  let content = {};

  // Step 1: Read existing file
  if (fs.existsSync(filePath)) {
    const raw = fs.readFileSync(filePath, "utf8").trim();

    if (raw === "") {
      content = {};
      actions.push("settings.json was empty, initializing");
    } else {
      try {
        content = JSON.parse(raw);
      } catch (parseError) {
        throw new Error(
          "Malformed .claude/settings.json: " +
            parseError.message +
            "\nFix the JSON syntax manually and re-run 'npx unimatrix init'." +
            "\nFile: " +
            filePath
        );
      }
    }
  } else {
    actions.push("Created .claude/settings.json");
  }

  // Step 2: Ensure hooks key exists
  if (!content.hooks) {
    content.hooks = {};
  }

  // Validate hooks is an object (not array, not primitive)
  if (typeof content.hooks !== "object" || Array.isArray(content.hooks)) {
    throw new Error(
      ".claude/settings.json 'hooks' key is not an object." +
        '\nExpected: { "hooks": { "EventName": [...] } }' +
        "\nFile: " +
        filePath
    );
  }

  // Step 3: For each hook event, merge the unimatrix entry
  for (const event of source.events) {
    const hookCommand = source.commandForEvent(event);
    const matcher = EVENT_MATCHERS[event];

    const newHookEntry = {
      type: "command",
      command: hookCommand,
    };

    // The settings format is: hooks.EventName = [ { matcher, hooks: [...] } ]
    if (!content.hooks[event]) {
      content.hooks[event] = [];
    }

    const eventArray = content.hooks[event];
    let merged = false;

    for (const matcherGroup of eventArray) {
      if (matcherGroup.matcher === matcher) {
        // Found a matcher group for our matcher value
        if (!matcherGroup.hooks) {
          matcherGroup.hooks = [];
        }

        // Look for existing unimatrix hook to update
        let existingIndex = -1;
        const duplicateIndices = [];
        for (let i = 0; i < matcherGroup.hooks.length; i++) {
          if (isUnimatrixHook(matcherGroup.hooks[i])) {
            if (existingIndex === -1) {
              existingIndex = i;
            } else {
              duplicateIndices.push(i);
            }
          }
        }

        // Remove duplicates in reverse order (dedup on re-run, per ADR-004)
        for (let j = duplicateIndices.length - 1; j >= 0; j--) {
          matcherGroup.hooks.splice(duplicateIndices[j], 1);
          actions.push("Removed duplicate unimatrix hook for " + event);
        }

        if (existingIndex >= 0) {
          matcherGroup.hooks[existingIndex] = newHookEntry;
          actions.push("Updated hook: " + event);
        } else {
          matcherGroup.hooks.push(newHookEntry);
          actions.push("Added hook: " + event);
        }

        merged = true;
        break;
      }
    }

    if (!merged) {
      // No matcher group found for our matcher value; create one
      eventArray.push({
        matcher: matcher,
        hooks: [newHookEntry],
      });
      actions.push("Added hook: " + event + " (new matcher group)");
    }
  }

  // Step 4: Write file (or prefix actions with [dry-run])
  if (!dryRun) {
    const dir = path.dirname(filePath);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(filePath, JSON.stringify(content, null, 2) + "\n", "utf8");
  }

  const finalActions = dryRun ? actions.map((a) => "[dry-run] " + a) : actions;

  return { actions: finalActions, content };
}

module.exports = {
  mergeSettings,
  isUnimatrixHook,
  buildHookClientCommand,
  normalizeCommandSource,
  HOOK_EVENTS,
  EVENT_MATCHERS,
  UNIMATRIX_PATTERNS,
};
