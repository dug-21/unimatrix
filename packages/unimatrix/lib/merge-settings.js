"use strict";

const fs = require("fs");
const path = require("path");

/**
 * Regex patterns to identify Unimatrix-owned hook entries (ADR-004).
 * Matches both current "unimatrix" and pre-rename "unimatrix-server" commands,
 * whether bare names or absolute paths.
 */
const UNIMATRIX_PATTERNS = [
  /^unimatrix\s+hook\s/,
  /^unimatrix-server\s+hook\s/,
  /\/unimatrix\s+hook\s/,
  /\/unimatrix-server\s+hook\s/,
];

const HOOK_EVENTS = [
  "SessionStart",
  "Stop",
  "UserPromptSubmit",
  "PreToolUse",
  "PostToolUse",
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
  SubagentStart: "*",
  SubagentStop: "*",
};

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
 * @param {string} binaryPath - Absolute path to the unimatrix binary
 * @param {object} options - { dryRun: boolean }
 * @returns {{ actions: string[], content: object }}
 */
function mergeSettings(filePath, binaryPath, options) {
  const dryRun = (options && options.dryRun) || false;
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
  const binDir = path.dirname(binaryPath);
  for (const event of HOOK_EVENTS) {
    const hookCommand = "LD_LIBRARY_PATH=" + binDir + " " + binaryPath + " hook " + event;
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

module.exports = { mergeSettings, isUnimatrixHook, HOOK_EVENTS, EVENT_MATCHERS, UNIMATRIX_PATTERNS };
