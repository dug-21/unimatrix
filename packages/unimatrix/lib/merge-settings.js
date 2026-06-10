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

/**
 * PreToolUse matcher (ADR-004 §1, vnc-027). Narrowed from "*" to the cycle
 * tools so the hook process no longer spawns for ordinary tool calls — the real
 * noise win is one fewer hook-process spawn per non-cycle tool invocation.
 * PreToolUse stays in HOOK_EVENTS; only the matcher narrows. Claude Code
 * regex-matcher semantics are load-bearing for cycle interception (R-11); the
 * client-side exact-equality sentinel is the defense-in-depth backstop.
 */
const PRETOOLUSE_CYCLE_MATCHER = "context_cycle|mcp__unimatrix__context_cycle";

/**
 * SubagentStop opt-in key (ADR-004 §2, vnc-027). Resolved from
 * {root}/.claude/settings.local.json. snake_case, matching unimatrix.remote.*.
 */
const SUBAGENT_STOP_EVENT = "SubagentStop";

/** Matcher per event: "" for session-level, "*" for tool/agent-level */
const EVENT_MATCHERS = {
  SessionStart: "",
  Stop: "",
  UserPromptSubmit: "",
  PreToolUse: PRETOOLUSE_CYCLE_MATCHER, // ADR-004 §1: narrowed from "*"
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
 * Read the SubagentStop opt-in key from settings.local.json (ADR-004 §2).
 *
 * Fail-open: an unreadable, missing, or malformed file, or a non-boolean value,
 * is treated as unset (false / default off). ONLY the literal boolean `true`
 * enables registration — string "true", 1, null, {} are all "unset" (the AC-08
 * type-confusion guard, R-12 security surface). This is an install-set decision,
 * not a hook-runtime path, so it never throws to the host.
 *
 * @param {string} optInFile - Path to {root}/.claude/settings.local.json.
 * @returns {boolean} true iff unimatrix.hooks.subagent_stop === true.
 */
function subagentStopEnabled(optInFile) {
  let parsed;
  try {
    parsed = JSON.parse(fs.readFileSync(optInFile, "utf8"));
  } catch (readOrParseError) {
    return false;
  }
  const hooks =
    parsed && typeof parsed === "object" && parsed.unimatrix && typeof parsed.unimatrix === "object"
      ? parsed.unimatrix.hooks
      : undefined;
  const value = hooks && typeof hooks === "object" ? hooks.subagent_stop : undefined;
  return value === true;
}

/**
 * Strip Unimatrix-owned hook entries for a single event from already-merged
 * content (ADR-004 §2 opt-out path). Scoped to Unimatrix-owned entries via
 * isUnimatrixHook — foreign hooks are never touched. Matcher groups we empty are
 * dropped, and the event key is removed if nothing remains, keeping the opt-in
 * matrix bidirectional and idempotent (AC-08).
 *
 * @param {object} content - The settings object (content.hooks must be an object).
 * @param {string} event - The hook event to prune.
 * @param {string[]} actions - Mutated: a line is pushed per removed entry.
 */
function pruneUnimatrixEvent(content, event, actions) {
  const eventArray = content.hooks[event];
  if (!Array.isArray(eventArray)) {
    return;
  }
  for (const group of eventArray) {
    if (!group || !Array.isArray(group.hooks)) {
      continue;
    }
    const before = group.hooks.length;
    group.hooks = group.hooks.filter((hook) => !isUnimatrixHook(hook));
    if (group.hooks.length !== before) {
      actions.push("Removed unimatrix hook: " + event + " (opt-out)");
    }
  }
  // Drop matcher groups we just emptied, then the event key if nothing remains.
  content.hooks[event] = eventArray.filter(
    (group) => group && Array.isArray(group.hooks) && group.hooks.length > 0
  );
  if (content.hooks[event].length === 0) {
    delete content.hooks[event];
  }
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

  // vnc-031 ADR-001: per-managed-event map of the kept entry BY OBJECT REFERENCE
  // (never a command string). Step 3 captures the newHookEntry it placed; Step 3c
  // keeps exactly that object and prunes every other uni-owned entry. Identity is
  // the load-bearing keep test — a command compare is forbidden (SR-01 / R-01).
  const keptEntryByEvent = {};

  // ADR-004 §2: SubagentStop is opt-in. Resolve the durable key from the
  // settings.local.json sibling of filePath (dirname(filePath) is {root}/.claude)
  // and filter it out of the registered event list unless explicitly enabled.
  const optInFile = path.join(path.dirname(filePath), "settings.local.json");
  let events = source.events;
  if (events.includes(SUBAGENT_STOP_EVENT) && !subagentStopEnabled(optInFile)) {
    events = events.filter((event) => event !== SUBAGENT_STOP_EVENT);
  }

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
  for (const event of events) {
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

    // vnc-031 ADR-001: capture the kept entry BY REFERENCE, once per event after
    // the merge resolves. newHookEntry is the same object placed in all three
    // branches (repoint / append / new group), so capturing here is correct and
    // branch-independent — including the R-02 case where the only pre-merge uni
    // hook is a stale cross-group one and Step 3 just created the managed entry.
    keptEntryByEvent[event] = newHookEntry;
  }

  // Step 3c: Cross-matcher-group stale-uni prune for MANAGED events (vnc-031,
  // ADR-002). After Step 3 has composed the managed EVENT_MATCHERS[event] group
  // and captured keptEntryByEvent[event], walk EVERY matcher group of this event
  // and remove every uni-owned entry that is NOT the kept object. This migrates a
  // legacy "*" (or any foreign-matcher) uni hook cleanly from mergeSettings alone.
  // Runs AFTER Step 3 and BEFORE Step 3b — the partition is load-bearing (SR-03):
  // Step 3c = managed events; Step 3b = HOOK_EVENTS \ events. The keep test is
  // OBJECT IDENTITY (hook !== kept) — never a command-string compare (ADR-001).
  for (const event of events) {
    const eventArray = content.hooks[event];
    if (!Array.isArray(eventArray)) {
      continue; // managed group guarantees presence; defensive.
    }
    const kept = keptEntryByEvent[event];

    for (const group of eventArray) {
      if (!group || !Array.isArray(group.hooks)) {
        continue; // mirror pruneUnimatrixEvent guard.
      }
      const before = group.hooks.length;
      group.hooks = group.hooks.filter(
        (hook) => !(isUnimatrixHook(hook) && hook !== kept)
      );
      if (group.hooks.length !== before) {
        actions.push(
          "Removed stale unimatrix hook: " + event + " (cross-matcher migration)"
        );
      }
    }

    // Drop matcher groups emptied solely by the uni removal; RETAIN the event key
    // (the managed group always holds `kept`). Reuse pruneUnimatrixEvent's filter
    // idiom but NEVER delete content.hooks[event] — unlike Step 3b, Step 3c can
    // never empty a managed event.
    content.hooks[event] = eventArray.filter(
      (group) => group && Array.isArray(group.hooks) && group.hooks.length > 0
    );
  }

  // Step 3b: Opt-out pruning (ADR-004 §2). Remove Unimatrix-owned entries for
  // any HOOK_EVENT we are NOT registering this run so a previously-registered
  // SubagentStop entry is stripped when the opt-in key is absent/false. In the
  // common path (source.events === HOOK_EVENTS) this only ever touches
  // SubagentStop; foreign hooks are preserved (isUnimatrixHook scope).
  for (const event of HOOK_EVENTS) {
    if (!events.includes(event)) {
      pruneUnimatrixEvent(content, event, actions);
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
  subagentStopEnabled,
  HOOK_EVENTS,
  EVENT_MATCHERS,
  UNIMATRIX_PATTERNS,
  PRETOOLUSE_CYCLE_MATCHER,
};
