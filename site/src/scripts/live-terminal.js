/**
 * Continuous, bounded Ax homepage terminal demo.
 * Illustrative output only; never accesses a real workspace or sends network requests.
 */
export const MAX_TERMINAL_LINES = 38;

export const terminalScenarios = [
  {
    command: "$ ax init",
    category: "CLI",
    rows: [
      ["info", "workspace detected                 /workspace/acme"],
      ["metric", "indexing source files              TypeScript · Rust · C#"],
      ["success", "symbols and relationships         ready"],
      ["memory", "local memory store                loaded"],
      ["success", "IDE / MCP configuration           connected"],
    ],
  },
  {
    command: "mcp → ax_preflight",
    category: "MCP",
    rows: [
      ["tool", "request  task: 'review auth session handling'"],
      ["info", "resolving project + session scope"],
      ["memory", "related decisions                 3 matched"],
      ["policy", "required project policies        included"],
      ["success", "Smart Output                     context prepared"],
    ],
  },
  {
    command: "mcp → ax_explore",
    category: "MCP",
    rows: [
      ["tool", "query    authentication / session"],
      ["trace", "source   src/auth/session.ts:87"],
      ["trace", "callers  6 symbols across 4 files"],
      ["metric", "impact   2 tests · 1 service boundary"],
      ["success", "source references                attached"],
    ],
  },
  {
    command: "mcp → ax_recall",
    category: "MCP",
    rows: [
      ["tool", "query    token refresh decisions"],
      ["memory", "memory  previous implementation notes"],
      ["memory", "memory  earlier fix + reasoning"],
      ["success", "relevant context                 restored"],
    ],
  },
  {
    command: "mcp → ax_rules",
    category: "MCP",
    rows: [
      ["tool", "scope   current project / current task"],
      ["policy", "POLICY  secure-auth · mandatory"],
      ["policy", "POLICY  tests-required · mandatory"],
      ["success", "policy provenance               retained"],
    ],
  },
  {
    command: "$ ax sync",
    category: "CLI",
    rows: [
      ["info", "checking changed source paths"],
      ["metric", "incremental parse                3 changed files"],
      ["trace", "updated symbol references        src/auth/session.ts"],
      ["success", "local project index              synchronized"],
    ],
  },
  {
    command: "$ ax ship --evaluate",
    category: "CLI",
    rows: [
      ["tool", "quality  index → diff → test impact"],
      ["metric", "affected tests                   8 selected"],
      ["success", "tests                           8 / 8 passed"],
      ["success", "quality gate                    passed"],
      ["memory", "evaluation summary              retained in session"],
    ],
  },
  {
    command: "mcp → ax_expand",
    category: "MCP",
    rows: [
      ["tool", "request  expand saved context handle"],
      ["memory", "restoring detailed memory        source-bound"],
      ["trace", "recovering symbol references     4 source paths"],
      ["success", "expanded context                 delivered"],
    ],
  },
  {
    command: "mcp → ax_status",
    category: "MCP",
    rows: [
      ["metric", "index    healthy"],
      ["metric", "memory   project scoped"],
      ["metric", "transport stdio / remote MCP"],
      ["success", "status snapshot                 ready"],
    ],
  },
  {
    command: "$ ax bootstrap --verify",
    category: "CLI",
    rows: [
      ["policy", "checking architecture seed"],
      ["trace", "entities · relationships · rules · skills"],
      ["success", "seed verification               complete"],
      ["info", "awaiting next agent request…"],
    ],
  },
];

const timestamp = (date) => date.toTimeString().slice(0, 8);

/**
 * Kept independent of Astro. Called only for the designated demo container.
 */
export function mountLiveTerminal(root) {
  if (!root || root.dataset.mounted === "true") return () => {};
  root.dataset.mounted = "true";
  const lines = root.querySelector("[data-ax-log-lines]");
  const history = root.querySelector("[data-ax-call-history]");
  const commands = root.querySelector("[data-ax-command-history]");
  const subtitle = root.querySelector("[data-ax-terminal-status]");
  const activity = root.querySelector("[data-ax-activity]");
  const counter = root.querySelector("[data-ax-event-count]");
  if (!lines || !history || !commands || !subtitle || !activity || !counter) return () => {};

  const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
  let stopped = false;
  let timer;
  let scenarioIndex = 0;
  let eventIndex = 0;
  let rowIndex = -1;
  let displayedEvents = 0;
  let clock = new Date("2026-10-11T10:14:20");
  let current = terminalScenarios[0];
  const callHistory = [];
  const commandHistory = [];

  function clockNext() {
    clock = new Date(clock.getTime() + 1000);
    return timestamp(clock);
  }
  function append(type, message) {
    const node = document.createElement("div");
    node.className = "ax-terminal-line is-" + type;
    const at = document.createElement("span");
    at.className = "ax-terminal-time";
    at.textContent = clockNext();
    const body = document.createElement("span");
    body.className = "ax-terminal-body";
    body.textContent = message;
    node.append(at, body);
    lines.appendChild(node);
    while (lines.children.length > MAX_TERMINAL_LINES) {
      lines.firstElementChild?.remove();
    }
    displayedEvents++;
    counter.textContent = String(displayedEvents).padStart(4, "0");
    lines.scrollTop = lines.scrollHeight;
  }
  function listInto(container, rows, length) {
    const fragment = document.createDocumentFragment();
    for (const row of rows.slice(0, length)) {
      const li = document.createElement("li");
      const name = document.createElement("span");
      name.textContent = row.name;
      const detail = document.createElement("small");
      detail.textContent = row.detail;
      li.append(name, detail);
      fragment.append(li);
    }
    container.replaceChildren(fragment);
  }
  function pushCall(name, detail) {
    callHistory.unshift({ name, detail });
    if (callHistory.length > 5) callHistory.pop();
    listInto(history, callHistory, 4);
  }
  function pushCommand(name) {
    commandHistory.unshift({ name, detail: clockNext() });
    if (commandHistory.length > 4) commandHistory.pop();
    listInto(commands, commandHistory, 3);
  }
  function isPaused() {
    return document.hidden || motion.matches || document.documentElement.dataset.axMotion === "paused";
  }
  function delay(ms) {
    if (!stopped) timer = window.setTimeout(tick, ms);
  }
  function tick() {
    if (stopped) return;
    if (isPaused()) {
      subtitle.textContent = motion.matches ? "Motion reduced · static preview" : "Replay paused";
      delay(650);
      return;
    }
    if (rowIndex < 0) {
      current = terminalScenarios[scenarioIndex % terminalScenarios.length];
      pushCommand(current.command);
      if (current.category === "MCP") {
        pushCall(current.command.replace("mcp → ", ""), "queued · active project");
      }
      append("command", "› " + current.command);
      activity.textContent = current.category + " · processing";
      subtitle.textContent = "Streaming context and activity…";
      rowIndex = 0;
      delay(480);
      return;
    }
    if (rowIndex < current.rows.length) {
      const [type, message] = current.rows[rowIndex++];
      append(type, "  ▸ " + message);
      if (type === "tool") {
        pushCall(current.command.replace("mcp → ", "").replace("$ ", ""), "completed · demo");
      }
      if (type === "policy") activity.textContent = "POLICY · matched";
      if (type === "memory") activity.textContent = "MEMORY · recalled";
      if (type === "success") activity.textContent = "OK · context healthy";
      delay(380 + ((displayedEvents * 73) % 360));
      return;
    }
    scenarioIndex++;
    rowIndex = -1;
    delay(420);
  }

  append("info", "illustrative session attached · no real project access");
  append("success", "context engine ready                 demo replay");
  pushCall("ax_preflight", "project context · queued");
  pushCommand("$ ax init");
  // Begin quickly, then stream forever without resetting the DOM.
  delay(380);

  const onVisibility = () => {
    if (!document.hidden && !motion.matches) subtitle.textContent = "Streaming context and activity…";
  };
  document.addEventListener("visibilitychange", onVisibility);
  motion.addEventListener("change", onVisibility);
  const onMotionChange = (event) => {
    if (event.detail?.paused) subtitle.textContent = "Replay paused";
  };
  window.addEventListener("ax:motion", onMotionChange);
  const destroy = () => {
    stopped = true;
    window.clearTimeout(timer);
    document.removeEventListener("visibilitychange", onVisibility);
    motion.removeEventListener("change", onVisibility);
    window.removeEventListener("ax:motion", onMotionChange);
    root.dataset.mounted = "false";
  };
  window.addEventListener("pagehide", destroy, { once: true });
  return destroy;
}
