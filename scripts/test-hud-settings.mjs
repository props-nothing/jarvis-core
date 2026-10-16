import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";

const source = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), "..", "apps", "jarvisd", "src", "hud", "settings.js"), "utf8");

// Isolated DOM/storage stand-ins: no network, providers, timers or owner daemon/profile are used.
class Element {
  constructor(tag, document) {
    this.tagName = tag.toUpperCase();
    this.document = document;
    this.children = [];
    this.attributes = new Map();
    this.listeners = new Map();
    this.dataset = {};
    this.hidden = false;
    this.disabled = false;
    this.checked = false;
    this.value = "";
    this.className = "";
    this.scrollTop = 0;
    this.tabIndex = -1;
    this.text = "";
  }
  set innerHTML(value) { throw new Error("Settings must not insert markup"); }
  insertAdjacentHTML() { throw new Error("Settings must not insert markup"); }
  set className(value) { this.attributes.set("class", value); }
  get className() { return this.attributes.get("class") || ""; }
  set id(value) { this.attributes.set("id", value); }
  get id() { return this.attributes.get("id") || ""; }
  set textContent(value) { this.replaceChildren(); this.text = String(value); }
  get textContent() { return this.text + this.children.map(child => child.textContent).join(""); }
  get firstChild() { return this.children[0] || null; }
  appendChild(child) { child.parent = this; this.children.push(child); return child; }
  insertBefore(child, before) {
    if (!before) return this.appendChild(child);
    child.parent = this;
    this.children.splice(this.children.indexOf(before), 0, child);
    return child;
  }
  replaceChildren(...children) {
    this.children.forEach(child => { child.parent = null; });
    this.children = [];
    this.text = "";
    children.forEach(child => this.appendChild(child));
  }
  remove() {
    if (this.parent) this.parent.children.splice(this.parent.children.indexOf(this), 1);
    this.parent = null;
  }
  setAttribute(name, value) { this.attributes.set(name, String(value)); }
  getAttribute(name) { return this.attributes.get(name) ?? null; }
  addEventListener(type, listener) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(listener);
  }
  focus() { this.document.activeElement = this; }
  contains(node) { return node === this || this.children.some(child => child.contains(node)); }
  click() { if (!this.disabled) this.dispatch("click"); }
  dispatchEvent(event) { return this.dispatch(event.type); }
  scrollIntoView() {}
  dispatch(type, values = {}) {
    const event = {
      type, target: this, defaultPrevented: false,
      preventDefault() { this.defaultPrevented = true; },
      stopPropagation() { this.stopped = true; },
      ...values
    };
    for (let node = this; node && !event.stopped; node = node.parent) {
      for (const listener of node.listeners.get(type) || []) listener(event);
    }
    return event;
  }
}
function descendants(node) { return node.children.flatMap(child => [child, ...descendants(child)]); }
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function flush() { await Promise.resolve(); await Promise.resolve(); }

function harness({ stored = null, readThrows = false, writeThrows = false, storageThrows = false, storageEntries = {}, putHandler = null, apiResponses = {}, popup = null, pageLocation = { protocol: "https:", hostname: "hud.example.test" } } = {}) {
  const document = {
    createElement(tag) { return new Element(tag, this); },
    createElementNS(namespace, tag) { const node = this.createElement(tag); node.namespaceURI = namespace; return node; },
    createTextNode(text) { const node = this.createElement("#text"); node.textContent = text; return node; },
    getElementById(id) { return [this.documentElement, ...descendants(this.documentElement)].find(node => node.id === id) || null; },
    addEventListener() {}
  };
  document.documentElement = document.createElement("html");
  const el = (tag, className = "", text = "") => {
    const node = document.createElement(tag);
    node.className = className;
    node.textContent = text;
    return node;
  };
  const $ = id => document.getElementById(id);
  for (const id of ["settings", "open-settings", "attention"]) {
    const node = el("div"); node.id = id; document.documentElement.appendChild(node);
  }
  for (const id of ["settings-note", "settings-hint", "settings-restart", "settings-tabs", "settings-body", "settings-close", "settings-section-label", "settings-search", "settings-search-empty"]) {
    const node = el(id === "settings-search" ? "input" : "div"); node.id = id; $("settings").appendChild(node);
  }
  $("settings").hidden = true;
  const writes = [], apiCalls = [], putCalls = [], settings = [], tools = [], confirmations = [];
  const entries = new Map(Object.entries(storageEntries));
  if (stored !== null) entries.set("jarvis-appearance", stored);
  const storage = {
    getItem(key) {
      assert.equal(key, "jarvis-appearance");
      if (readThrows) throw new Error("Storage blocked");
      return entries.get(key) ?? null;
    },
    setItem(key, value) {
      if (writeThrows) throw new Error("Storage blocked");
      writes.push([key, value]);
      entries.set(key, value);
    }
  };
  const openedWindows = [];
  const window = { location: pageLocation, confirm(text) { confirmations.push(text); return false; }, open(...args) { openedWindows.push(args); return popup; } };
  Object.defineProperty(window, "localStorage", { get() {
    if (storageThrows) throw new Error("Storage access blocked");
    return storage;
  } });
  const timers = new Map();
  let timerId = 0;
  runInNewContext(source, {
    window, document, Event, URL,
    setTimeout(callback, delay) { timers.set(++timerId, { callback, delay }); return timerId; },
    clearTimeout(id) { timers.delete(id); },
    setInterval() {}
  }, { filename: "settings.js" });
  function initialize() {
    window.JarvisSettings({
      $, el,
      api(path) {
        apiCalls.push(path);
        const request = deferred();
        if (path === "/settings") settings.push(request);
        else if (path === "/tools") tools.push(request);
        else request.resolve(apiResponses[path] || {});
        return request.promise;
      },
      putJson(path, method, payload) {
        putCalls.push([path, method, payload && JSON.parse(JSON.stringify(payload))]);
        return putHandler ? putHandler(path, method, payload) : Promise.resolve({ applied: true });
      },
      explain(error, fallback) { return fallback; },
      families(items) { return [{ tools: items }]; },
      familyLabel() { return el("span", "", "Tools"); }
    });
  }
  function search(text) { $("settings-search").value = text; $("settings-search").dispatch("input"); }
  function tab(id) { return $("settings-tab-" + id); }
  function open() { $("open-settings").dispatch("click"); }
  function select(id) { tab(id).dispatch("click"); }
  function runTimer(id) {
    const timer = timers.get(id);
    assert.ok(timer, "the timer is scheduled");
    timers.delete(id);
    timer.callback();
  }
  return { document, $, initialize, writes, entries, apiCalls, putCalls, settings, tools, confirmations, openedWindows, search, tab, open, select, timers, runTimer };
}

const defaultAppearance = { density: "comfortable", motion: "system", accent: "mint", atmosphere: "cinematic" };
const fixture = {
  settings: [{ key: "daemon.push_topic", kind: "text", value: "saved-topic", default: "", help: "Push topic.", unset_means: "No push topic.", group: "advanced" }],
  keys: {}, postures: {}
};
const visibleTabs = h => descendants(h.$("settings-tabs")).filter(node => node.getAttribute("role") === "tab" && !node.hidden);
const selectedId = h => descendants(h.$("settings-tabs")).find(node => node.getAttribute("aria-selected") === "true").id;
const draftField = h => descendants(h.$("settings-body")).find(node => node.tagName === "INPUT");
const classNodes = (h, name) => descendants(h.$("settings-body")).filter(node => node.className.split(/\s+/).includes(name));
const fieldLabel = (h, field) => descendants(h.$("settings-body")).find(node => node.tagName === "LABEL" && node.getAttribute("for") === field.id);
const buttonNamed = (h, text) => descendants(h.$("settings-body")).find(node => node.tagName === "BUTTON" && node.textContent === text);
const savedSetting = (key, value, kind = "text") => ({ key, value, kind, default: "", help: "A saved configuration choice.", unset_means: "Use the default choice.", group: "advanced" });
const configuredFixture = {
  settings: [
    ...fixture.settings,
    savedSetting("daemon.executor_model_name", "isolated-model"),
    savedSetting("daemon.executor_base_url", "http://provider.invalid/v1"),
    savedSetting("daemon.executor_fallback_model_name", null),
    savedSetting("daemon.executor_fallback_base_url", null),
    savedSetting("daemon.executor_reasoning_effort", "medium"),
    savedSetting("daemon.tool_workspace_roots", "C:\\isolated-one, C:\\isolated-two", "folders"),
    savedSetting("daemon.google_client_id", "isolated-google-client"),
    savedSetting("daemon.phone_agent_id", "isolated-phone-agent"),
    savedSetting("daemon.phone_number_id", "isolated-phone-number"),
    savedSetting("daemon.phone_owner_numbers", "isolated-owner"),
    savedSetting("daemon.notifications", "on", "flag")
  ],
  keys: { model: { state: "set", value: "key-value-must-stay-hidden" }, google: { state: "set" }, telegram: { state: "set" }, voice: { state: "set" }, search: { state: "unset" } },
  postures: { "mail.send": "ask" },
  providers: [{ id: "custom", name: "Custom provider", url: "", needs_key: true, help: "Use your provider address." }]
};
async function loadFixture(h, reply = fixture) {
  h.settings.at(-1).resolve(reply);
  h.tools.at(-1).resolve({ tools: [] });
  await flush();
}

async function openService(h, id) {
  h.select("connections");
  h.$("settings-service-" + id).click();
  await flush();
}

test("coding agent setup offers normal sign-in without environment-name controls", async () => {
  const h = harness({
    apiResponses: {
      "/code-agents": {
        selected: "codex", needs: { folder: true, git: true, node: true }, env: ["EXISTING_PROVIDER_KEY"],
        presets: [{ id: "codex", name: "ChatGPT/Codex", selected: true, available: true, via: "installed",
          summary: "Coding agent", sign_in: "Use ChatGPT sign-in.", env: ["OPENAI_API_KEY"] }]
      },
      "/code-agents/tasks": { tasks: [] }
    },
    putHandler(path) {
      assert.equal(path, "/code-agents/test");
      return Promise.resolve({ ok: true, message: "Ready to sign in.", methods: [{ id: "chatgpt", name: "ChatGPT" }] });
    }
  });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture,
    settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", "codex-acp", "words")]
  });
  h.select("files"); await flush();
  assert.doesNotMatch(h.$("settings-body").textContent, /Environment names|Save names|EXISTING_PROVIDER_KEY/);
  assert.equal(descendants(h.$("settings-body")).some(node => node.placeholder === "OPENAI_API_KEY"), false);
  buttonNamed(h, "Test").click(); await flush();
  const signIn = buttonNamed(h, "Sign in with ChatGPT");
  assert.ok(signIn, "the agent's normal sign-in remains available");
  assert.equal(signIn.disabled, false);
  assert.deepEqual(h.putCalls, [["/code-agents/test", "POST", {}]]);
});

test("coding tasks expose durable review evidence as text, not markup or verified test results", async () => {
  const h = harness({ apiResponses: {
    "/code-agents": { needs: { folder: true, git: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [{
      task_id: "review-task", repository: "repo", task: "Fix the parser", status: "completed",
      branch: "jarvis/parser", worktree: "C:\\worktrees\\parser", allow_commands: true,
      created_at: "2026-10-14T10:00:00Z", finished_at: "2026-10-14T10:01:00Z",
      result: {
        outcome: "completed", base_commit: "abc123", commit: "def456", files_changed: 2,
        permissions: { allowed: 3, refused: 1 },
        agent_message: ["<script>alert('agent')</script> Tests passed."],
        agent_tool_calls: ["- [execute] cargo test (completed)"],
        agent_requests: ["- [fetch] Read private URL -> refused"],
        stat: [" parser.rs | 2 +-"], patch: ["+<img src=x onerror=alert('diff')>"],
        patch_was_cut: true, notes: ["The owner's checkout changed."],
        next: "git diff abc123..jarvis/parser"
      }
    }] }
  } });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture,
    settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  visibleTabs(h).find(node => /Folders/.test(node.textContent)).click(); await flush();
  const reviews = classNodes(h, "code-task");
  assert.equal(reviews.length, 1);
  assert.equal(reviews[0].tagName, "DETAILS");
  const text = reviews[0].textContent;
  for (const value of ["C:\\worktrees\\parser", "abc123", "def456", "Tests passed.",
    "cargo test", "Read private URL", "<script>", "<img", "The owner's checkout changed."]) {
    assert.ok(text.includes(value), `missing review evidence: ${value}`);
  }
  assert.match(text, /not independently verified/i);
  assert.match(text, /3 allowed.*1 refused/);
  assert.match(text, /diff.*truncated|truncated.*diff/i);
  assert.equal(descendants(reviews[0]).some(node => ["SCRIPT", "IMG"].includes(node.tagName)), false);
  assert.equal(h.putCalls.length, 0, "reviewing a result must never execute a task or command");
});

test("coding task defaults and independent evidence describe the Settings workspace without implying browser verification", async () => {
  const directory = "C:/owner/websites";
  const h = harness({ apiResponses: {
    "/code-agents": { needs: { folder: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [{
      task_id: "in-place-task", repository: directory, task: "Build the website",
      status: "failed", minutes: 0, allow_commands: true,
      result: { directory, mode: "in_place", outcome: "verification_failed",
        verification: { status: "failed", checks: [{ command: { program: "npm", arguments: ["run", "build"] }, passed: false }] } }
    }] }
  } });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture,
    settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  h.select("files"); await flush();
  const setup = h.$("settings-body").textContent;
  assert.match(setup, /first folder by default/);
  assert.match(setup, /No Git repository, branch or automatic commit/);
  assert.match(setup, /without an overall time limit/);
  assert.match(setup, /Git is optional; normal in-place coding does not need it/);
  assert.doesNotMatch(setup, /install git first|in a copy of your repository/);
  const review = classNodes(h, "code-task")[0];
  assert.ok(review.textContent.includes(directory));
  assert.match(review.textContent, /in_place/);
  assert.match(review.textContent, /None/);
  assert.match(review.textContent, /verification_failed/);
  assert.match(review.textContent, /Independent checks/);
  assert.ok(review.textContent.includes("Separate desktop/mobile inspection required"));
  assert.equal(h.putCalls.length, 0);
});

test("coding task refresh preserves expanded reviews and focus, reports failures and clears stale empty results", async () => {
  const task = { task_id: "refresh-task", repository: "repo", task: "Fix the parser", status: "running", result: null };
  const responses = {
    "/code-agents": { needs: { folder: true, git: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [task] }
  };
  const h = harness({ apiResponses: responses });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture,
    settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  h.select("files"); await flush();
  classNodes(h, "code-task")[0].open = true;
  const refresh = descendants(h.$("settings-body")).find(node => node.getAttribute("aria-label") === "Refresh coding tasks");
  assert.ok(refresh);
  const update = deferred(); responses["/code-agents/tasks"] = update.promise;
  refresh.focus(); refresh.click();
  assert.equal(refresh.disabled, true);
  assert.match(h.$("settings-body").textContent, /Loading coding tasks/);
  update.resolve({ tasks: [{ ...task, status: "failed", error: "The agent exited." }] }); await flush();
  assert.equal(classNodes(h, "code-task")[0].open, true);
  assert.match(classNodes(h, "code-task")[0].textContent, /The agent exited/);
  assert.equal(h.document.activeElement, refresh);
  assert.equal(refresh.disabled, false);
  const failure = deferred(); responses["/code-agents/tasks"] = failure.promise;
  refresh.click(); failure.reject(new Error("offline")); await flush();
  assert.equal(refresh.disabled, false);
  assert.match(h.$("settings-body").textContent, /Coding tasks could not be loaded/);
  assert.equal(classNodes(h, "code-task")[0].open, true);
  responses["/code-agents/tasks"] = { tasks: [] };
  refresh.click(); await flush();
  assert.equal(classNodes(h, "code-task").length, 0);
  assert.match(h.$("settings-body").textContent, /No background coding tasks/);
  assert.doesNotMatch(h.$("settings-body").textContent, /The agent exited|could not be loaded/);
  assert.equal(h.putCalls.length, 0);
});

test("live coding activity refreshes in place without executing work or losing focus and disclosures", async () => {
  const task = { task_id: "live-task", repository: "repo", task: "Read config", status: "running", result: null,
    activity: [{ sequence: 1, summary: "<img src=x onerror=alert(1)> Read config", created_at: "2026-10-14T10:00:00Z" }] };
  const completed = { task_id: "old-task", repository: "repo", task: "Previous task", status: "completed",
    activity: [], result: { agent_message: ["Earlier report"] } };
  const responses = {
    "/code-agents": { needs: { folder: true, git: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [task, completed] }
  };
  const h = harness({ apiResponses: responses });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture, settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  h.select("files"); await flush();
  const review = classNodes(h, "code-task")[0]; review.open = true; review.firstChild.focus();
  const event = classNodes(h, "code-task-event")[0];
  assert.ok(event, "persisted activity is visible during the task");
  const evidence = classNodes(h, "code-evidence")[0]; evidence.open = true;
  const command = descendants(h.$("settings-body")).find(node => node.tagName === "INPUT" && String(node.placeholder).startsWith("e.g. npx"));
  command.value = "unsaved custom agent";
  const poll = [...h.timers].find(([, timer]) => timer.delay === 2000);
  assert.ok(poll, "running tasks are refreshed automatically");
  responses["/code-agents/tasks"] = { tasks: [{ ...task, activity_limited: true,
    activity: [...task.activity, { sequence: 3, summary: "Config read [completed]", created_at: "2026-10-14T10:00:02Z" }] }, completed] };
  h.runTimer(poll[0]); await flush();
  assert.equal(classNodes(h, "code-task")[0], review);
  assert.equal(review.open, true);
  assert.equal(h.document.activeElement, review.firstChild);
  assert.equal(classNodes(h, "code-task-event")[0], event);
  assert.equal(classNodes(h, "code-evidence")[0], evidence);
  assert.equal(evidence.open, true);
  assert.equal(command.value, "unsaved custom agent");
  assert.match(review.textContent, /Config read \[completed\]/);
  assert.match(review.textContent, /Some activity was not retained/);
  assert.equal(descendants(review).some(node => ["IMG", "SCRIPT"].includes(node.tagName)), false);
  assert.equal(h.putCalls.length, 0);
  const next = [...h.timers].find(([, timer]) => timer.delay === 2000);
  responses["/code-agents/tasks"] = { tasks: [{ ...task, status: "completed", activity: task.activity, result: { outcome: "completed" } }, completed] };
  h.runTimer(next[0]); await flush();
  assert.equal(classNodes(h, "code-task")[0], review);
  assert.equal(h.document.activeElement, review.firstChild);
  assert.equal([...h.timers.values()].some(timer => timer.delay === 2000), false);
});

test("live coding activity retries a failed refresh and stops polling when Settings closes", async () => {
  const task = { task_id: "live-task", repository: "repo", task: "Read config", status: "running", result: null,
    activity: [{ sequence: 1, summary: "Reading config", created_at: "2026-10-14T10:00:00Z" }] };
  const responses = {
    "/code-agents": { needs: { folder: true, git: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [task] }
  };
  const h = harness({ apiResponses: responses });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture, settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  h.select("files"); await flush();
  const poll = [...h.timers].find(([, timer]) => timer.delay === 2000);
  assert.ok(poll);
  const failure = deferred(); responses["/code-agents/tasks"] = failure.promise;
  h.runTimer(poll[0]); failure.reject(new Error("offline")); await flush();
  assert.match(h.$("settings-body").textContent, /Coding tasks could not be loaded/);
  assert.match(h.$("settings-body").textContent, /Reading config/);
  const retry = [...h.timers].find(([, timer]) => timer.delay === 2000);
  assert.ok(retry);
  const before = h.apiCalls.length;
  h.$("settings").hidden = true;
  h.runTimer(retry[0]); await flush();
  assert.equal(h.apiCalls.length, before);
  assert.equal([...h.timers.values()].some(timer => timer.delay === 2000), false);
});

test("interrupted coding tasks keep the missing-result state explicit without inventing evidence", async () => {
  const h = harness({ apiResponses: {
    "/code-agents": { needs: { folder: true, git: true, node: true }, presets: [] },
    "/code-agents/tasks": { tasks: [{
      task_id: "interrupted-task", repository: "repo", task: "Continue the parser", status: "interrupted",
      result: null, error: "The daemon stopped.", allow_commands: false
    }] }
  } });
  h.initialize(); h.open(); await loadFixture(h, {
    ...configuredFixture,
    settings: [...configuredFixture.settings, savedSetting("daemon.code_agent", null, "words")]
  });
  h.select("files"); await flush();
  const review = classNodes(h, "code-task")[0];
  assert.match(review.textContent, /Interrupted; no final result was recorded/);
  assert.match(review.textContent, /The daemon stopped/);
  assert.match(review.textContent, /Not allowed/);
  assert.equal(descendants(review).some(node => node.tagName === "PRE"), false);
  assert.equal(h.putCalls.length, 0);
});

for (const [label, options, expected] of [
  ["missing", {}, { density: "comfortable", motion: "system" }],
  ["valid", { stored: '{"density":"compact","motion":"reduced"}' }, { density: "compact", motion: "reduced" }],
  ["corrupt", { stored: "{" }, { density: "comfortable", motion: "system" }],
  ["invalid", { stored: '{"density":"tiny","motion":"animated"}' }, { density: "comfortable", motion: "system" }],
  ["partially valid", { stored: '{"density":"compact","motion":false}' }, { density: "compact", motion: "system" }],
  ["scalar", { stored: "17" }, { density: "comfortable", motion: "system" }],
  ["array", { stored: '["compact","reduced","ice","quiet"]' }, {}],
  ["all valid", { stored: '{"density":"compact","motion":"reduced","accent":"amethyst","atmosphere":"quiet"}' }, { density: "compact", motion: "reduced", accent: "amethyst", atmosphere: "quiet" }],
  ["ice accent", { stored: '{"accent":"ice"}' }, { accent: "ice" }],
  ["invalid new values", { stored: '{"accent":"red","atmosphere":"animated"}' }, {}],
  ["mixed new values", { stored: '{"accent":"ice","atmosphere":false,"density":null,"motion":"reduced","key":"must-not-persist"}' }, { accent: "ice", motion: "reduced" }],
  ["blocked reads", { readThrows: true }, { density: "comfortable", motion: "system" }],
  ["blocked storage access", { storageThrows: true }, { density: "comfortable", motion: "system" }]
]) {
  test(`appearance applies before initialization: ${label}`, () => {
    const h = harness(options);
    assert.deepEqual(h.document.documentElement.dataset, { ...defaultAppearance, ...expected });
    assert.equal(h.writes.length, 0);
    assert.equal(h.apiCalls.length, 0);
  });
}

test("Console works offline, persists four validated preferences immediately and needs no restart", async () => {
  const h = harness(); h.initialize(); h.open();
  h.settings[0].reject({ status: 503 }); h.tools[0].reject({ status: 503 }); await flush();
  assert.match(h.$("settings-body").textContent, /Could not load daemon settings \(503\)/);
  h.select("console");
  const density = h.$("settings-console-density"), motion = h.$("settings-console-motion");
  assert.ok(density && motion);
  assert.match(h.$("settings-body").textContent, /Stored only in this browser/);
  assert.doesNotMatch(h.$("settings-body").textContent, /Could not load daemon settings/);
  density.value = "compact"; density.dispatch("change");
  motion.value = "reduced"; motion.dispatch("change");
  const accent = h.$("settings-console-accent"), atmosphere = h.$("settings-console-atmosphere");
  accent.value = "ice"; accent.dispatch("change");
  atmosphere.value = "quiet"; atmosphere.dispatch("change");
  assert.deepEqual(h.document.documentElement.dataset, { density: "compact", motion: "reduced", accent: "ice", atmosphere: "quiet" });
  assert.equal(h.writes.at(-1)[0], "jarvis-appearance");
  assert.deepEqual(JSON.parse(h.writes.at(-1)[1]), { density: "compact", motion: "reduced", accent: "ice", atmosphere: "quiet" });
  assert.deepEqual(h.apiCalls, ["/settings", "/tools"]);
  assert.equal(h.putCalls.length, 0);
  assert.equal(h.$("settings-hint").textContent, "Nothing is waiting for a restart.");
  assert.match(h.$("settings-body").textContent, /Applied immediately; no daemon restart/);
  assert.equal(descendants(h.$("settings-body")).find(node => node.getAttribute("for") === density.id).textContent, "Density");
  assert.ok(h.$(motion.getAttribute("aria-describedby")));
  const reloaded = harness({ stored: h.writes.at(-1)[1] });
  assert.deepEqual(reloaded.document.documentElement.dataset, h.document.documentElement.dataset);
  motion.value = "system"; motion.dispatch("change");
  assert.equal(h.document.documentElement.dataset.motion, "system");
  assert.equal(JSON.parse(h.writes.at(-1)[1]).motion, "system");
  const count = h.writes.length;
  density.value = "invalid"; density.dispatch("change");
  assert.equal(h.document.documentElement.dataset.density, "compact");
  assert.equal(h.writes.length, count);
  h.select("alerts");
  assert.match(h.$("settings-body").textContent, /Could not load daemon settings \(503\)/);
});

test("blocked writes apply for this page and are not falsely reported as persisted", () => {
  const h = harness({ writeThrows: true }); h.initialize(); h.open(); h.select("console");
  const density = h.$("settings-console-density"); density.value = "compact"; density.dispatch("change");
  assert.equal(h.document.documentElement.dataset.density, "compact");
  assert.match(h.$("settings-body").textContent, /will not be remembered after a reload/);
  assert.doesNotMatch(h.$("settings-body").textContent, /Appearance saved/);
  h.select("alerts"); h.select("console");
  assert.equal(h.$("settings-console-density").value, "compact");
  assert.match(h.$("settings-body").textContent, /will not be remembered/);
});

test("daemon replies and failures do not replace or unfocus Console controls", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("console");
  const density = h.$("settings-console-density"); density.focus();
  h.settings[0].resolve(fixture); h.tools[0].resolve({ tools: [] }); await flush();
  assert.equal(h.$("settings-console-density"), density);
  assert.equal(h.document.activeElement, density);
  h.open();
  const reopened = h.$("settings-console-density"); reopened.focus();
  h.settings[1].reject({ status: 401 }); h.tools[1].reject({ status: 401 }); await flush();
  assert.equal(h.$("settings-console-density"), reopened);
  assert.equal(h.document.activeElement, reopened);
});

test("navigation filters labels and descriptions without replacing unsaved fields or selection", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("alerts");
  h.settings[0].resolve(fixture); h.tools[0].resolve({ tools: [] }); await flush();
  const field = draftField(h), children = [...h.$("settings-body").children]; field.value = "unsaved-topic";
  h.search("  REDUCED  motion  ");
  assert.deepEqual(visibleTabs(h).map(node => node.id), ["settings-tab-console"]);
  assert.equal(h.tab("console").tabIndex, 0);
  assert.equal(selectedId(h), "settings-tab-alerts");
  assert.deepEqual(h.$("settings-body").children, children);
  assert.equal(draftField(h), field);
  assert.equal(field.value, "unsaved-topic");
  const groups = h.$("settings-tabs").children.filter(node => node.className === "navgroup");
  assert.deepEqual(groups.map(node => node.hidden), [true, true, true, false]);
  h.search("no section matches this");
  assert.equal(visibleTabs(h).length, 0);
  assert.equal(h.$("settings-search-empty").hidden, false);
  assert.match(h.$("settings-search-empty").textContent, /No settings sections match/);
  assert.ok(groups.every(node => node.hidden));
  assert.equal(h.$("settings-search-empty").getAttribute("role"), "status");
  h.search("voice");
  assert.ok(visibleTabs(h).some(node => node.id === "settings-tab-voice"));
  h.search("");
  assert.equal(visibleTabs(h).length, 13);
  assert.equal(h.$("settings-search-empty").hidden, true);
  assert.equal(h.tab("alerts").tabIndex, 0);
  h.select("alerts");
  assert.equal(draftField(h), field, "selecting the current tab must not discard a draft");
});

test("ARIA relationships, keyboard wrapping and filtered roving focus follow the selected tab", () => {
  const h = harness(); h.initialize(); h.open();
  assert.equal(h.document.activeElement, h.tab("overview"));
  assert.equal(h.$("settings-tabs").getAttribute("role"), "tablist");
  assert.equal(h.$("settings-tabs").getAttribute("aria-orientation"), "vertical");
  assert.equal(h.$("settings-search").getAttribute("aria-controls"), "settings-tabs");
  for (const tab of visibleTabs(h)) assert.equal(tab.getAttribute("aria-controls"), "settings-body");
  assert.equal(h.$("settings-body").getAttribute("role"), "tabpanel");
  assert.equal(h.$("settings-body").getAttribute("aria-labelledby"), h.tab("overview").id);
  assert.equal(h.tab("overview").dispatch("keydown", { key: "ArrowDown" }).defaultPrevented, true);
  assert.equal(selectedId(h), "settings-tab-brain");
  assert.equal(h.document.activeElement, h.tab("brain"));
  h.tab("brain").dispatch("keydown", { key: "ArrowDown" });
  assert.equal(selectedId(h), "settings-tab-voice");
  assert.equal(h.$("settings-section-label").textContent, "Voice");
  h.tab("voice").dispatch("keydown", { key: "Home" });
  assert.equal(selectedId(h), "settings-tab-overview");
  h.tab("overview").dispatch("keydown", { key: "ArrowUp" });
  assert.equal(selectedId(h), "settings-tab-advanced");
  h.tab("advanced").dispatch("keydown", { key: "ArrowDown" });
  assert.equal(selectedId(h), "settings-tab-overview");
  h.tab("overview").dispatch("keydown", { key: "End" });
  assert.equal(selectedId(h), "settings-tab-advanced");
  assert.equal(visibleTabs(h).filter(node => node.tabIndex === 0).length, 1);
  h.search("motion"); h.tab("console").focus();
  h.tab("console").dispatch("keydown", { key: "ArrowDown" });
  assert.equal(selectedId(h), "settings-tab-console");
  assert.equal(h.$("settings-body").getAttribute("aria-labelledby"), "settings-tab-console");
  assert.equal(h.document.activeElement, h.tab("console"));
  assert.equal(h.tab("console").dispatch("keydown", { key: "Tab" }).defaultPrevented, false);
});

test("server saves preserve the selected tab, search, navigation nodes and panel scroll", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("alerts");
  h.settings[0].resolve(fixture); h.tools[0].resolve({ tools: [] }); await flush();
  const tab = h.tab("alerts"); h.search("alerts"); h.$("settings-body").scrollTop = 64;
  draftField(h).value = "changed-topic";
  descendants(h.$("settings-body")).find(node => node.tagName === "BUTTON" && node.textContent === "Save").dispatch("click");
  await flush();
  assert.deepEqual(h.putCalls, [["/settings/daemon.push_topic", "PUT", { values: ["changed-topic"] }]]);
  h.settings[1].resolve(fixture); h.tools[1].resolve({ tools: [] }); await flush();
  assert.equal(h.$("settings-search").value, "alerts");
  assert.equal(h.tab("alerts"), tab);
  assert.equal(selectedId(h), "settings-tab-alerts");
  assert.equal(h.$("settings-section-label").textContent, "Notifications");
  assert.equal(h.$("settings-body").scrollTop, 64);
  assert.match(h.$("settings-hint").textContent, /Waiting for a restart: Push topic/);
});

test("configuration summaries distinguish saved configuration from unknown availability", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("console");
  assert.match(h.tab("brain").getAttribute("aria-label"), /Configuration unknown/);
  h.settings[0].resolve({ settings: [{ key: "daemon.executor_model_name", value: "model" }], keys: {} });
  await flush();
  assert.match(h.tab("brain").getAttribute("aria-label"), /Configured in saved settings; runtime availability has not been checked/);
  assert.match(h.tab("connections").getAttribute("aria-label"), /status unknown/);
  assert.match(h.tab("console").getAttribute("aria-label"), /no daemon connection required/);
  assert.equal(h.tab("brain").children[2].getAttribute("aria-hidden"), "true");
  h.open();
  h.settings[1].resolve({ settings: [{ key: "daemon.executor_model_name", value: null }], keys: { telegram: { state: "unset" } } }); await flush();
  assert.match(h.tab("brain").getAttribute("aria-label"), /Not fully configured in saved settings/);
  assert.match(h.tab("telegram").getAttribute("aria-label"), /Not fully configured in saved settings/);
  h.open(); h.settings[2].reject({ status: 503 }); await flush();
  assert.match(h.tab("brain").getAttribute("aria-label"), /Configuration unknown/);
  assert.equal(h.tab("brain").children[2].className, "dot");
});

test("failed tool loading is not presented as an empty configured tool list", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("permissions");
  h.settings[0].resolve(fixture); await flush();
  assert.match(h.$("settings-body").textContent, /Loading tool permissions/);
  h.tools[0].reject({ status: 503 }); await flush();
  assert.match(h.$("settings-body").textContent, /Could not load tool permissions \(503\)/);
  assert.doesNotMatch(h.$("settings-body").textContent, /No tools are available/);
});

test("existing outward-tool confirmation still fails closed when declined", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("permissions");
  h.settings[0].resolve(fixture);
  h.tools[0].resolve({ tools: [{ id: "mail.send", title: "Send mail", risk: "high", asks_first: true, effects: ["external_communication"] }] });
  await flush();
  const select = descendants(h.$("settings-body")).find(node => node.tagName === "SELECT");
  select.value = "trusted"; select.dispatch("change");
  assert.equal(h.confirmations.length, 1);
  assert.equal(h.putCalls.length, 0);
  assert.equal(select.value, "default");
});

test("older daemon failures cannot override a newer settings load", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("alerts"); h.open();
  h.settings[1].resolve(fixture); h.tools[1].resolve({ tools: [] }); await flush();
  const field = draftField(h);
  h.settings[0].reject({ status: 503 }); h.tools[0].reject({ status: 503 }); await flush();
  assert.equal(draftField(h), field);
  assert.doesNotMatch(h.$("settings-body").textContent, /Could not load/);
});

test("legacy preferences migrate on the first write without retaining unknown or sensitive properties", () => {
  const h = harness({ stored: '{"density":"compact","motion":"reduced","key":"must-not-persist","transcript":"must-not-persist"}' });
  h.initialize(); h.open(); h.select("console");
  assert.equal(h.$("settings-console-density").value, "compact");
  assert.equal(h.$("settings-console-motion").value, "reduced");
  assert.equal(h.$("settings-console-accent").value, "mint");
  assert.equal(h.$("settings-console-atmosphere").value, "cinematic");
  const accent = h.$("settings-console-accent"); accent.value = "amethyst"; accent.dispatch("change");
  assert.deepEqual(JSON.parse(h.writes[0][1]), { density: "compact", motion: "reduced", accent: "amethyst", atmosphere: "cinematic" });
  assert.doesNotMatch(h.writes[0][1], /must-not-persist|transcript|key/);
});

test("all appearance choices have visible labels and update the CSS-only preview without replacing controls", () => {
  const h = harness(); h.initialize(); h.open(); h.select("console");
  const preview = classNodes(h, "appearance-preview")[0], caption = classNodes(h, "preview-caption")[0];
  assert.equal(preview.getAttribute("aria-label"), "Live appearance preview");
  assert.equal(classNodes(h, "preview-orbit")[0].getAttribute("aria-hidden"), "true");
  assert.equal(classNodes(h, "preview-core")[0].getAttribute("aria-hidden"), "true");
  assert.equal(caption.textContent, "Mint / Cinematic / Comfortable / System motion");
  for (const [key, title, values] of [
    ["accent", "Accent", ["ice", "amethyst", "mint"]],
    ["atmosphere", "Atmosphere", ["quiet", "cinematic"]],
    ["density", "Density", ["compact", "comfortable"]],
    ["motion", "Motion", ["reduced", "system"]]
  ]) {
    const field = h.$("settings-console-" + key); field.focus();
    assert.equal(fieldLabel(h, field).textContent, title);
    assert.ok(h.$(field.getAttribute("aria-describedby")));
    assert.deepEqual(field.children.map(node => node.value).sort(), [...values].sort());
    for (const value of values) {
      field.value = value; field.dispatch("change");
      assert.equal(h.document.documentElement.dataset[key], value);
      assert.equal(JSON.parse(h.writes.at(-1)[1])[key], value);
      assert.equal(h.document.activeElement, field);
      assert.equal(h.$("settings-console-" + key), field);
      assert.equal(classNodes(h, "appearance-preview")[0], preview);
    }
  }
  assert.equal(h.writes.length, 9);
  assert.equal(h.putCalls.length, 0);
  assert.equal(h.$("settings-restart").disabled, true);
  assert.match(h.$("settings-body").textContent, /Approval, listening and error colors keep their own meaning/);
});

test("invalid appearance changes fail closed for every preference without changing the DOM or stored schema", () => {
  const h = harness(); h.initialize(); h.open(); h.select("console");
  for (const key of Object.keys(defaultAppearance)) {
    const field = h.$("settings-console-" + key);
    for (const value of ["", "UNKNOWN", "null", "<svg onload=invalid>"]) {
      field.value = value; field.dispatch("change");
      assert.equal(field.value, defaultAppearance[key]);
      assert.deepEqual(h.document.documentElement.dataset, defaultAppearance);
      assert.equal(h.writes.length, 0);
      assert.match(h.$("settings-body").textContent, /previous preference is unchanged/);
    }
  }
  assert.equal(h.putCalls.length, 0);
});

for (const [name, options] of [["blocked writes", { writeThrows: true }], ["blocked storage getter", { storageThrows: true }]]) {
  test(`new appearance choices remain truthful and usable with ${name}`, () => {
    const h = harness(options); h.initialize(); h.open(); h.select("console");
    for (const [key, value] of [["accent", "amethyst"], ["atmosphere", "quiet"]]) {
      const field = h.$("settings-console-" + key); field.value = value; field.dispatch("change");
      assert.equal(h.document.documentElement.dataset[key], value);
    }
    assert.match(h.$("settings-body").textContent, /will not be remembered after a reload/);
    assert.doesNotMatch(h.$("settings-body").textContent, /Appearance saved|reset to defaults in this browser/);
    assert.equal(h.writes.length, 0);
    h.select("overview");
    assert.match(h.$("settings-body").textContent, /appearance applies to this page only/);
    h.select("console");
    assert.equal(h.$("settings-console-accent").value, "amethyst");
    assert.match(classNodes(h, "preview-caption")[0].textContent, /Amethyst \/ Quiet/);
    h.$("settings-console-reset").click();
    assert.deepEqual(h.document.documentElement.dataset, defaultAppearance);
    assert.match(h.$("settings-body").textContent, /will not be remembered after a reload/);
    assert.equal(h.putCalls.length, 0);
  });
}

test("unreadable stored preferences disclose defaults, and a successful later write reports persistence accurately", () => {
  const h = harness({ readThrows: true }); h.initialize(); h.open(); h.select("console");
  assert.match(h.$("settings-body").textContent, /Saved appearance could not be read/);
  assert.doesNotMatch(h.$("settings-body").textContent, /Appearance saved/);
  const atmosphere = h.$("settings-console-atmosphere"); atmosphere.value = "quiet"; atmosphere.dispatch("change");
  assert.match(h.$("settings-body").textContent, /Appearance saved in this browser/);
  assert.doesNotMatch(h.$("settings-body").textContent, /could not be read|will not be remembered/);
});

test("reset is offline safe, restores four defaults and touches only the local appearance key", async () => {
  const h = harness({ stored: '{"density":"compact","motion":"reduced","accent":"ice","atmosphere":"quiet"}', storageEntries: { "jarvis-credential": "isolated-credential", "unrelated-setting": "keep-this" } });
  h.initialize(); h.open(); h.settings[0].reject({ status: 503 }); h.tools[0].reject({ status: 503 }); await flush();
  h.select("console");
  const reset = h.$("settings-console-reset");
  assert.equal(reset.textContent, "Reset appearance");
  reset.focus(); reset.click();
  assert.deepEqual(h.document.documentElement.dataset, defaultAppearance);
  for (const [key, value] of Object.entries(defaultAppearance)) assert.equal(h.$("settings-console-" + key).value, value);
  assert.deepEqual(h.writes, [["jarvis-appearance", JSON.stringify(defaultAppearance)]]);
  assert.equal(h.entries.get("jarvis-credential"), "isolated-credential");
  assert.equal(h.entries.get("unrelated-setting"), "keep-this");
  assert.equal(h.document.activeElement, reset);
  assert.match(h.$("settings-body").textContent, /Appearance reset to defaults in this browser/);
  assert.deepEqual(h.apiCalls, ["/settings", "/tools"]);
  assert.equal(h.putCalls.length, 0);
  assert.equal(h.$("settings-restart").disabled, true);
  assert.deepEqual(harness({ stored: h.entries.get("jarvis-appearance") }).document.documentElement.dataset, defaultAppearance);
});

test("Overview starts with honest loading cards and an always-available real Console action", async () => {
  const h = harness(); h.initialize(); h.open();
  assert.equal(selectedId(h), "settings-tab-overview");
  assert.equal(h.$("settings-section-label").textContent, "Overview");
  assert.equal(classNodes(h, "overview-grid").length, 1);
  assert.equal(classNodes(h, "overview-card").length, 7);
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Unknown", "Unknown", "Unknown", "Unknown", "Unknown", "Unknown", "Browser-local"]);
  assert.match(h.$("settings-body").textContent, /Loading saved daemon configuration/);
  assert.match(h.$("settings-body").textContent, /Setup is unknown until it loads/);
  h.settings[0].reject({ status: 503 }); h.tools[0].reject({ status: 503 }); await flush();
  assert.match(h.$("settings-body").textContent, /Could not load daemon settings \(503\).*Configuration is unknown/);
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Unknown", "Unknown", "Unknown", "Unknown", "Unknown", "Unknown", "Browser-local"]);
  const console = h.$("settings-overview-console");
  assert.equal(console.tagName, "BUTTON");
  assert.equal(console.type, "button");
  assert.equal(console.disabled, false);
  console.click();
  assert.equal(selectedId(h), "settings-tab-console");
  assert.equal(h.document.activeElement, h.tab("console"));
  assert.ok(h.$("settings-console-accent"));
  assert.deepEqual(h.apiCalls, ["/settings", "/tools"]);
  assert.equal(h.putCalls.length, 0);
});

test("Overview derives only saved settings and key presence, hides values and never infers runtime health", async () => {
  const h = harness(); h.initialize(); h.open(); await loadFixture(h, configuredFixture);
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Model name saved", "2 folders saved", "Review saved connections", "2 saved channel entries", "1 saved override", "Optional setup", "Browser-local"]);
  const text = h.$("settings-body").textContent;
  const notice = classNodes(h, "overview-safety")[0];
  assert.equal(notice.textContent, "Saved setup only. Runtime availability has not been checked.");
  assert.equal(notice.getAttribute("role"), "status");
  assert.match(text, /Saved setup only/);
  assert.match(h.tab("overview").getAttribute("aria-label"), /Saved configuration overview; runtime availability has not been checked/);
  assert.match(text, /Runtime availability has not been checked/);
  assert.match(notice.getAttribute("title"), /do not prove a service is connected or healthy/);
  assert.equal(notice.getAttribute("aria-description"), notice.getAttribute("title"));
  assert.match(notice.getAttribute("aria-description"), /No provider tests run here/);
  assert.doesNotMatch(notice.textContent, /connected|healthy|provider tests/);
  assert.match(text, /account sign-in is unverified/);
  assert.match(text, /chat access is unverified/);
  assert.match(text, /calling is unverified/);
  assert.match(text, /Deterministic rules still decide/);
  assert.doesNotMatch(text, /key-value-must-stay-hidden|isolated-google-client|isolated-phone|isolated-owner|isolated-one|isolated-two|provider.invalid/);
  assert.deepEqual(h.apiCalls, ["/settings", "/tools"]);
  assert.equal(h.putCalls.length, 0);
  assert.equal(h.writes.length, 0);
  h.open(); h.settings[1].reject({ status: 503 }); await flush();
  assert.deepEqual(classNodes(h, "overview-state").slice(0, 4).map(node => node.textContent), ["Unknown", "Unknown", "Unknown", "Unknown"]);
  assert.doesNotMatch(h.$("settings-body").textContent, /Model name saved|3 saved setup entries/);
});

test("Overview distinguishes missing configuration from unreported settings", async () => {
  const h = harness(); h.initialize(); h.open(); await loadFixture(h, { settings: [], keys: {} });
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Not reported", "Not reported", "Review saved connections", "0 saved channel entries", "Not reported", "Optional setup", "Browser-local"]);
  assert.match(h.$("settings-body").textContent, /main model setting was not reported/);
  assert.match(h.$("settings-body").textContent, /Folder grants were not reported/);
  assert.doesNotMatch(h.$("settings-body").textContent, /No folder grants are saved/);
  h.open(); await loadFixture(h, { settings: [savedSetting("daemon.executor_model_name", null), savedSetting("daemon.tool_workspace_roots", null, "folders")], keys: {}, postures: {} });
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Choose a model", "No folders saved", "Review saved connections", "0 saved channel entries", "Default choices", "Optional setup", "Browser-local"]);
});

test("Overview buttons route through tab navigation, preserve search and focus sensibly when the target is filtered", () => {
  const h = harness(); h.initialize(); h.open();
  for (const id of ["brain", "files", "connections", "connect", "telegram", "phone", "alerts", "permissions", "voice", "search", "console"]) {
    h.select("overview");
    h.$("settings-overview-" + id).click();
    assert.equal(selectedId(h), "settings-tab-" + id);
    assert.equal(h.document.activeElement, h.tab(id));
    assert.equal(h.$("settings-body").getAttribute("aria-labelledby"), h.tab(id).id);
  }
  h.select("overview"); h.search("accent");
  h.$("settings-overview-files").click();
  assert.equal(selectedId(h), "settings-tab-files");
  assert.equal(h.$("settings-search").value, "accent");
  assert.equal(h.document.activeElement, h.$("settings-body"));
  assert.equal(h.tab("console").tabIndex, 0);
  assert.deepEqual(h.apiCalls, ["/settings", "/tools"]);
  assert.equal(h.putCalls.length, 0);
});

test("Overview refresh preserves an action's focus, navigation nodes, filter and scroll", async () => {
  const h = harness(); h.initialize(); h.open();
  const nav = h.tab("overview"); h.search("saved setup"); h.$("settings-body").scrollTop = 48;
  h.$("settings-overview-console").focus();
  await loadFixture(h, configuredFixture);
  assert.equal(h.document.activeElement, h.$("settings-overview-console"));
  assert.equal(h.tab("overview"), nav);
  assert.equal(h.$("settings-search").value, "saved setup");
  assert.equal(h.$("settings-body").scrollTop, 48);
});

test("local SVG navigation icons and one hero per tab keep accessible labels and stable navigation nodes", () => {
  const h = harness(); h.initialize(); h.open();
  const tabs = visibleTabs(h);
  for (const tab of tabs) {
    const [icon, label, dot] = tab.children;
    assert.equal(icon.tagName, "SVG");
    assert.equal(icon.namespaceURI, "http://www.w3.org/2000/svg");
    assert.equal(icon.className, "settings-nav-icon");
    assert.equal(icon.getAttribute("width"), "18");
    assert.equal(icon.getAttribute("height"), "18");
    assert.equal(icon.getAttribute("aria-hidden"), "true");
    assert.equal(icon.getAttribute("focusable"), "false");
    assert.equal(icon.children[0].tagName, "PATH");
    assert.equal(icon.children[0].namespaceURI, icon.namespaceURI);
    assert.match(icon.children[0].getAttribute("d"), /^M[\d ]/);
    assert.equal(label.className, "settings-nav-label");
    assert.ok(label.textContent);
    assert.equal(dot.getAttribute("aria-hidden"), "true");
    tab.click();
    assert.equal(classNodes(h, "settings-hero").length, 1);
    assert.equal(classNodes(h, "settings-hero-icon").length, 1);
    const copy = classNodes(h, "settings-hero-copy")[0];
    assert.equal(copy.children[0].tagName, "H3");
    assert.equal(copy.children[0].textContent, label.textContent);
    assert.ok(copy.children[1].textContent);
    assert.equal(classNodes(h, "tablead").length, 0);
    assert.equal(h.$("settings-section-label").textContent, label.textContent);
    assert.equal(h.$(tab.id), tab);
  }
  assert.doesNotMatch(source, /innerHTML|outerHTML|insertAdjacentHTML/);
  const iconDefinitions = source.slice(source.indexOf("var ICON_PATHS"), source.indexOf("function settingsIcon"));
  assert.ok(iconDefinitions.includes("connections:"));
  assert.doesNotMatch(iconDefinitions, /https?:\/\//, "icons stay local; provider OAuth links elsewhere are not assets");
});

test("dynamic text, choice, switch and permission controls have real visible labels and descriptions", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("alerts"); await loadFixture(h, configuredFixture);
  for (const field of descendants(h.$("settings-body")).filter(node => node.tagName === "INPUT")) {
    assert.ok(field.id);
    assert.ok(fieldLabel(h, field)?.textContent);
    assert.ok(h.$(field.getAttribute("aria-describedby"))?.textContent);
  }
  h.select("brain");
  for (const id of ["settings-model-main-provider", "settings-model-main-address", "settings-model-main-key", "settings-model-main-name", "settings-model-fallback-name"]) {
    const field = h.$(id);
    assert.ok(fieldLabel(h, field)?.textContent);
    if (id.endsWith("-key")) assert.match(h.$(field.getAttribute("aria-describedby")).textContent, /Write-only/);
  }
  const effort = descendants(h.$("settings-body")).find(node => node.tagName === "SELECT" && !node.id.startsWith("settings-model-"));
  assert.match(fieldLabel(h, effort).textContent, /Thinking effort/);
  h.select("permissions"); h.open();
  h.settings.at(-1).resolve(configuredFixture);
  h.tools.at(-1).resolve({ tools: [{ id: "mail.send", title: "Send mail", risk: "high", asks_first: true, effects: ["external_communication"] }] });
  await flush();
  const posture = descendants(h.$("settings-body")).find(node => node.tagName === "SELECT");
  assert.equal(fieldLabel(h, posture).textContent, "Send mail");
  assert.match(h.$(posture.getAttribute("aria-describedby")).textContent, /risk high/);
});

test("key rows remain write-only and clear submissions even when saving fails", async () => {
  const pending = deferred();
  const h = harness({ putHandler: () => pending.promise }); h.initialize(); h.open(); h.select("voice"); await loadFixture(h, configuredFixture);
  const key = h.$("settings-key-voice");
  assert.equal(key.type, "password");
  assert.equal(key.value, "");
  assert.match(fieldLabel(h, key).textContent, /ElevenLabs key/);
  assert.ok(h.$(key.getAttribute("aria-describedby")));
  key.value = "isolated-key-draft";
  buttonNamed(h, "Save key").click();
  assert.equal(key.value, "");
  assert.deepEqual(h.putCalls, [["/settings/keys/voice", "PUT", { key: "isolated-key-draft" }]]);
  pending.reject({ status: 400 }); await flush();
  assert.equal(key.value, "");
  assert.match(h.$("settings-note").textContent, /key was refused/);
  assert.equal(h.writes.length, 0);
});

for (const action of ["Load models", "Test", "Save"]) {
  test(`explicit isolated model ${action} submissions clear key drafts on failure`, async () => {
    const h = harness({ putHandler: () => Promise.reject({ status: 400 }) });
    h.initialize(); h.open(); h.select("brain"); await loadFixture(h, configuredFixture);
    assert.equal(h.putCalls.length, 0, "opening Brain must not call a provider automatically");
    const key = h.$("settings-model-main-key"); key.value = "isolated-model-key";
    buttonNamed(h, action).click();
    assert.equal(key.value, "");
    await flush();
    assert.equal(key.value, "");
    assert.equal(h.writes.length, 0);
    assert.equal(h.putCalls.length, 1);
    assert.equal(h.putCalls[0][0], action === "Save" ? "/settings" : "/settings/models");
    if (action !== "Save") assert.equal(h.putCalls[0][2].key, "isolated-model-key");
  });
}

test("restart is disabled without pending changes or configuration, guarded in flight, and keeps the existing force flow", async () => {
  const restarts = [];
  const h = harness({ putHandler(path) {
    if (path.startsWith("/restart")) { const request = deferred(); restarts.push(request); return request.promise; }
    return Promise.resolve({ applied: true });
  } });
  h.initialize();
  const restart = h.$("settings-restart");
  assert.equal(restart.disabled, true);
  restart.dispatch("click");
  assert.equal(h.putCalls.length, 0, "synthetic clicks must also fail closed");
  h.open(); h.select("alerts"); await loadFixture(h);
  assert.equal(restart.disabled, true);
  draftField(h).value = "updated-topic"; buttonNamed(h, "Save").click(); await flush();
  assert.equal(restart.disabled, false);
  assert.match(h.$("settings-hint").textContent, /Waiting for a restart: Push topic/);
  h.select("console");
  const beforeReset = h.putCalls.length, pendingHint = h.$("settings-hint").textContent;
  h.$("settings-console-reset").click();
  assert.equal(h.putCalls.length, beforeReset);
  assert.equal(h.$("settings-hint").textContent, pendingHint);
  assert.equal(restart.disabled, false);
  h.settings[1].reject({ status: 503 }); h.tools[1].reject({ status: 503 }); await flush();
  assert.equal(restart.disabled, true);
  restart.dispatch("click");
  assert.equal(restarts.length, 0);
  assert.equal(h.$("settings-hint").textContent, pendingHint);
  h.open(); await loadFixture(h);
  assert.equal(restart.disabled, false);
  restart.click();
  assert.equal(restart.disabled, true);
  assert.deepEqual(h.putCalls.at(-1), ["/restart", "POST", undefined]);
  restart.dispatch("click");
  assert.equal(restarts.length, 1);
  restarts[0].reject({ status: 409, body: "2 tasks are still working" }); await flush();
  assert.equal(restart.disabled, false);
  assert.equal(restart.textContent, "Restart anyway");
  assert.match(h.$("settings-note").textContent, /2 tasks are still working/);
  assert.equal(h.$("settings-hint").textContent, pendingHint);
  restart.click();
  assert.equal(restart.disabled, true);
  assert.deepEqual(h.putCalls.at(-1), ["/restart?force=true", "POST", undefined]);
  restarts[1].resolve({}); await flush();
  assert.equal(restart.disabled, true);
  assert.equal(restart.textContent, "Restart to apply");
  assert.equal(h.$("settings-hint").textContent, "Nothing is waiting for a restart.");
});

test("an ordinary restart failure re-enables the pending restart without adding force or losing the footer", async () => {
  const h = harness({ putHandler(path) { return path === "/restart" ? Promise.reject({ status: 503 }) : Promise.resolve({ applied: true }); } });
  h.initialize(); h.open(); h.select("alerts"); await loadFixture(h);
  draftField(h).value = "updated-topic"; buttonNamed(h, "Save").click(); await flush();
  const hint = h.$("settings-hint").textContent;
  h.$("settings-restart").click(); await flush();
  assert.equal(h.$("settings-restart").disabled, false);
  assert.equal(h.$("settings-hint").textContent, hint);
  assert.match(h.$("settings-note").textContent, /Could not restart/);
  h.$("settings-restart").click(); await flush();
  assert.equal(h.putCalls.at(-1)[0], "/restart");
});

const githubStatus = { configured: true, connected: false, client_id: "fixtureclient", client_secret_set: true, private_repositories: false, pending: false, callback_url: "http://127.0.0.1:8765/oauth/github/callback", restart_needed: true };

test("Connections exposes one-time GitHub app setup with a write-only secret and browser sign-in", async () => {
  const saveRequest = deferred();
  const h = harness({ apiResponses: { "/github": { ...githubStatus, configured: false, client_secret_set: false, client_id: null } }, putHandler: () => saveRequest.promise });
  h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
  assert.equal(h.$("github-client-secret").type, "password");
  assert.equal(h.$("github-client-secret").autocomplete, "off");
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, true);
  assert.match(h.$("settings-body").textContent, /http:\/\/127\.0\.0\.1:8765\/oauth\/github\/callback/);
  h.$("github-client-id").value = "fixtureclient"; h.$("github-client-id").dispatch("input");
  h.$("github-client-secret").value = "synthetic-app-secret-ui-8197"; h.$("github-client-secret").dispatch("input");
  buttonNamed(h, "Save GitHub app").click();
  assert.equal(h.$("github-client-secret").value, "", "clear before awaiting even a failed save");
  assert.deepEqual(h.putCalls[0], ["/github/app", "PUT", { client_id: "fixtureclient", client_secret: "synthetic-app-secret-ui-8197", private_repositories: false }]);
  saveRequest.resolve(githubStatus); await flush();
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, false);
  assert.equal(h.writes.length, 0);
  assert.ok(!h.$("settings-body").textContent.includes("synthetic-app-secret-ui-8197"));
  assert.ok(!h.putCalls.some(call => call[0] === "/restart"));
});

test("GitHub failed saves clear secrets and unsaved permissions block connect and test", async () => {
  const h = harness({ apiResponses: { "/github": { ...githubStatus, connected: true, login: "fixture-user" } }, putHandler: () => Promise.reject({ status: 422 }) });
  h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
  assert.equal(buttonNamed(h, "Test GitHub tools").disabled, false);
  h.$("github-private-repositories").checked = true; h.$("github-private-repositories").dispatch("change");
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, true);
  assert.equal(buttonNamed(h, "Test GitHub tools").disabled, true);
  h.$("github-client-secret").value = "synthetic-refused-secret-9721"; h.$("github-client-secret").dispatch("input");
  buttonNamed(h, "Save GitHub app").click(); await flush();
  assert.equal(h.$("github-client-secret").value, "");
  assert.match(h.$("settings-body").textContent, /App setup was not saved/);
  assert.equal(h.writes.length, 0);
  assert.ok(!h.putCalls.some(call => call[0] === "/github/connect"));
});

test("GitHub browser handoff is out-of-band and rejects non-GitHub addresses", async () => {
  for (const authUrl of ["https://github.com/login/oauth/authorize?state=synthetic-state", "https://untrusted.example.test/?state=synthetic-state"]) {
    const h = harness({ apiResponses: { "/github": githubStatus }, putHandler: () => Promise.resolve({ auth_url: authUrl }) });
    h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
    buttonNamed(h, "Connect GitHub").click(); await flush();
    assert.deepEqual(h.putCalls[0], ["/github/connect", "POST", {}]);
    const expected = authUrl.startsWith("https://github.com/") ? 1 : 0;
    assert.equal(h.openedWindows.length, expected);
    assert.equal(h.writes.length, 0);
    assert.ok(!h.putCalls.some(call => call[0] === "/runs" || call[0] === "/restart"));
    if (expected) assert.equal(h.openedWindows[0][2], "noopener,noreferrer");
    else assert.match(h.$("settings-body").textContent, /valid GitHub sign-in address/);
  }
});

test("late GitHub status preserves drafts and cannot undo a newer app save", async () => {
  const initial = deferred();
  const h = harness({ apiResponses: { "/github": initial.promise }, putHandler: () => Promise.resolve({ ...githubStatus, client_id: "editedclient" }) });
  h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
  h.$("github-client-id").value = "editedclient"; h.$("github-client-id").dispatch("input");
  h.$("github-client-secret").value = "synthetic-secret-newer-save"; h.$("github-client-secret").dispatch("input");
  buttonNamed(h, "Save GitHub app").click(); await flush();
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, false);
  initial.resolve({ ...githubStatus, configured: false, client_id: "oldclient" }); await flush(); await flush();
  assert.equal(h.$("github-client-id").value, "editedclient");
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, false);
  assert.equal(h.$("github-client-secret").value, "");
});

test("GitHub initial status does not overwrite unsaved drafts and polling stops off-page", async () => {
  const initial = deferred();
  const h = harness({ apiResponses: { "/github": initial.promise } });
  h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
  h.$("github-client-id").value = "draftclient"; h.$("github-client-id").dispatch("input");
  h.$("github-private-repositories").checked = true; h.$("github-private-repositories").dispatch("change");
  initial.resolve(githubStatus); await flush(); await flush();
  assert.equal(h.$("github-client-id").value, "draftclient");
  assert.equal(h.$("github-private-repositories").checked, true);
  assert.equal(buttonNamed(h, "Connect GitHub").disabled, true);
  assert.equal(h.putCalls.length, 0);
  const polling = harness({ apiResponses: { "/github": { ...githubStatus, pending: true } }, putHandler: () => Promise.resolve({ auth_url: "https://github.com/login/oauth/authorize?state=synthetic-state" }) });
  polling.initialize(); polling.open(); await loadFixture(polling); polling.select("connections"); await flush();
  buttonNamed(polling, "Connect GitHub").click(); await flush();
  const timer = [...polling.timers].find(([, timer]) => timer.delay === 2000);
  assert.ok(timer);
  polling.select("console"); const calls = polling.apiCalls.length;
  polling.runTimer(timer[0]); await flush();
  assert.equal(polling.apiCalls.length, calls);
});

test("GitHub tool test is explicit and Disconnect retains app setup without starting work", async () => {
  const h = harness({ apiResponses: { "/github": { ...githubStatus, connected: true, login: "fixture-user" } }, putHandler: path => Promise.resolve(path === "/github/test" ? { ok: true, tools: [{ id: "mcp.github.get_me" }] } : githubStatus) });
  h.initialize(); h.open(); await loadFixture(h); h.select("connections"); await flush();
  assert.match(h.$("settings-body").textContent, /fixture-user \(saved connection\)/);
  buttonNamed(h, "Test GitHub tools").click(); await flush();
  assert.match(h.$("settings-body").textContent, /GitHub answered: 1 tools/);
  buttonNamed(h, "Disconnect GitHub").click(); await flush();
  assert.equal(buttonNamed(h, "Test GitHub tools").disabled, true);
  assert.equal(h.$("github-client-id").value, "fixtureclient");
  assert.ok(!h.putCalls.some(call => call[0] === "/runs" || call[0] === "/restart"));
});

test("MCP credential status shows references only and Test preserves the configured mappings", async () => {
  const env = { GITHUB_PERSONAL_ACCESS_TOKEN: "mcp:github:token" };
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [{ name: "github", kind: "stdio", command: ["github-mcp-server", "stdio"], posture: "unclassified", state: "connected", tools: [], env, credentials: [{ name: "GITHUB_PERSONAL_ACCESS_TOKEN", reference: env.GITHUB_PERSONAL_ACCESS_TOKEN, state: "set" }] }] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
  h.initialize(); h.open(); await loadFixture(h);
  await openService(h, "mcp");
  assert.match(h.$("settings-body").textContent, /GITHUB_PERSONAL_ACCESS_TOKEN: set \(mcp:github:token\)/);
  buttonNamed(h, "Test").click(); await flush();
  const request = h.putCalls.find(call => call[0] === "/mcp/test");
  assert.ok(request);
  assert.deepEqual(request[2].env, env);
  assert.deepEqual(request[2].command, ["github-mcp-server", "stdio"]);
  assert.equal(request[2].endpoint, null);
  assert.equal(request[2].authorization, null);
  assert.equal(request[2].read_only, false);
  assert.equal(h.writes.length, 0, "credential data is not written to browser storage");
});

test("MCP remote form sends only a named bearer reference for Test and Add, then clears it", async () => {
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  const reference = h.$("mcp-bearer-ref");
  assert.equal(h.$("mcp-bearer-row").hidden, true);
  assert.equal(reference.disabled, true);
  h.$("mcp-program-name").value = "remote";
  h.$("mcp-program-command").value = "https://example.invalid/mcp";
  h.$("mcp-program-command").dispatch("input");
  assert.equal(h.$("mcp-bearer-row").hidden, false);
  assert.equal(reference.disabled, false);
  assert.equal(reference.placeholder, "mcp:remote:KEY");
  assert.match(fieldLabel(h, reference).textContent, /reference \(optional\)/);
  assert.equal(reference.getAttribute("aria-describedby"), "mcp-bearer-help");
  assert.match(h.$("mcp-bearer-help").textContent, /jarvis keys set mcp:remote:KEY/);
  assert.match(h.$("mcp-bearer-help").textContent, /prompt is hidden/);
  assert.match(h.$("mcp-bearer-help").textContent, /never the token/);
  const formFields = descendants(reference.parent.parent).filter(node => node.tagName === "INPUT");
  assert.deepEqual(formFields.map(node => node.type), ["text", "text", "text", "checkbox"], "no raw-token or password field is offered");
  reference.value = "mcp:remote:token";
  buttonNamed(h, "Test").click(); await flush();
  const expected = { name: "remote", command: [], endpoint: "https://example.invalid/mcp", read_only: false, authorization: "mcp:remote:token" };
  assert.deepEqual(h.putCalls, [["/mcp/test", "POST", expected]]);
  assert.equal(reference.value, "mcp:remote:token", "Test retains the reference for Add");
  buttonNamed(h, "Add").click(); await flush();
  assert.deepEqual(h.putCalls[1], ["/mcp/servers", "POST", expected]);
  assert.equal(reference.value, "");
  assert.equal(h.$("mcp-bearer-ref").value, "");
  assert.equal(h.$("mcp-bearer-row").hidden, true);
  assert.equal(h.$("mcp-program-name").value, "");
  assert.equal(h.$("mcp-program-command").value, "");
  assert.equal(h.writes.length, 0);
  assert.ok(!h.putCalls.some(call => call[0] === "/runs" || call[0] === "/restart"));
});

test("MCP remote form refuses literal and cross-server bearer references and query addresses before sending", async () => {
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [] } } });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  h.$("mcp-program-name").value = "remote";
  for (const [reference, endpoint, message] of [
    ["synthetic-literal-secret", "https://example.invalid/mcp", /never a token value/],
    ["Bearer synthetic-literal-secret", "https://example.invalid/mcp", /never a token value/],
    ["github", "https://example.invalid/mcp", /never a token value/],
    ["mcp:other:token", "https://example.invalid/mcp", /this server's name/],
    ["mcp:remote:", "https://example.invalid/mcp", /never a token value/],
    ["mcp:remote:token", "https://example.invalid/mcp?token=synthetic-query-secret", /without a query/]
  ]) {
    h.$("mcp-program-command").value = endpoint;
    h.$("mcp-program-command").dispatch("input");
    h.$("mcp-bearer-ref").value = reference;
    for (const action of ["Test", "Add"]) {
      buttonNamed(h, action).click(); await flush();
      assert.equal(h.putCalls.length, 0, `${action}: invalid binding stays off the API`);
      assert.equal(buttonNamed(h, action).disabled, false);
      assert.match(h.$("settings-body").textContent, message);
      assert.ok(!h.$("settings-body").textContent.includes("synthetic-literal-secret"));
      assert.ok(!h.$("settings-body").textContent.includes("synthetic-query-secret"));
    }
  }
  assert.equal(h.writes.length, 0);
});

test("MCP switching to a local command or Example clears remote references and anonymous HTTP stays optional", async () => {
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  h.$("mcp-program-name").value = "remote";
  const command = h.$("mcp-program-command"), reference = h.$("mcp-bearer-ref");
  command.value = "https://example.invalid/mcp"; command.dispatch("input");
  reference.value = "mcp:remote:token";
  command.value = 'local-program "a folder"'; command.dispatch("input");
  assert.equal(reference.value, "");
  assert.equal(reference.disabled, true);
  assert.equal(h.$("mcp-bearer-row").hidden, true);
  buttonNamed(h, "Test").click(); await flush();
  assert.deepEqual(h.putCalls[0], ["/mcp/test", "POST", { name: "remote", command: ["local-program", "a folder"], endpoint: null, read_only: false }]);
  command.value = "https://example.invalid/mcp"; command.dispatch("input");
  buttonNamed(h, "Test").click(); await flush();
  assert.equal(h.putCalls[1][2].authorization, null);
  reference.value = "mcp:remote:token";
  buttonNamed(h, "Example").click();
  assert.equal(reference.value, "");
  assert.equal(h.$("mcp-bearer-row").hidden, true);
  buttonNamed(h, "Test").click(); await flush();
  assert.equal(h.putCalls[2][2].authorization, undefined);
  assert.ok(h.putCalls[2][2].command.length > 0);
});

for (const state of ["set", "not set"]) {
  test(`MCP saved HTTP Test preserves authorization with ${state} reference status`, async () => {
    const reference = "mcp:remote:token";
    const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [{ name: "remote", kind: "http", endpoint: "https://example.invalid/mcp", command: [], authorization: reference, env: {}, posture: "unclassified", state: "connected", tools: [], credentials: [{ name: "Authorization", reference, state }] }] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
    h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
    assert.ok(h.$("settings-body").textContent.includes(`Authorization: ${state} (${reference})`));
    assert.match(h.$("settings-body").textContent, /jarvis keys set mcp:remote:token/);
    buttonNamed(h, "Test").click(); await flush();
    assert.deepEqual(h.putCalls, [["/mcp/test", "POST", { name: "remote", command: [], endpoint: "https://example.invalid/mcp", read_only: false, env: {}, authorization: reference }]]);
    assert.equal(h.writes.length, 0);
  });
}

test("MCP saved GitHub HTTP Test retains its fixed OAuth authorization", async () => {
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [{ name: "github", kind: "http", command: [], endpoint: "https://api.githubcopilot.com/mcp/", authorization: "github", posture: "read-only", tools: [] }] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  buttonNamed(h, "Test").click(); await flush();
  assert.equal(h.putCalls[0][2].authorization, "github");
  assert.equal(h.putCalls[0][2].read_only, true);
  assert.deepEqual(h.putCalls[0][2].env, {});
});

const mcpOAuthStatus = {
  configured: true, connected: false, pending: false, issuer: "https://issuer.example.test", client_id: "registered-client",
  client_secret_set: true, scopes: ["tools.read"], callback_url: "http://127.0.0.1:8765/oauth/mcp/remote/callback", restart_needed: false
};
const mcpOAuthBase = "/mcp/servers/remote/oauth";
const mcpOAuthEndpoint = "https://login.example.test/oauth/authorize";
const mcpOAuthReply = { auth_url: mcpOAuthEndpoint + "?state=synthetic-transaction&scope=tools.read", authorization_endpoint: mcpOAuthEndpoint, issuer: mcpOAuthStatus.issuer };
const oauthField = (h, suffix, name = "remote") => h.$(`mcp-oauth-${name}-${suffix}`);
const oauthButton = (h, text, name = "remote") => descendants(oauthField(h, "panel", name)).find(node => node.tagName === "BUTTON" && node.textContent === text);
const oauthLink = h => descendants(oauthField(h, "panel")).find(node => node.tagName === "A");
function oauthPopup() {
  const navigated = [];
  return { opener: { original: true }, closed: false, navigated, location: { replace(address) { navigated.push(address); } }, close() { this.closed = true; } };
}
async function openMcpOAuth({ status = {}, server = {}, apiResponses = {}, ...options } = {}) {
  const saved = { name: "remote", kind: "http", command: [], endpoint: "https://resource.example.test/mcp", authorization: "oauth:remote", env: {}, posture: "unclassified", state: "unreachable", reason: "No live connection.", tools: [], oauth: { ...mcpOAuthStatus, ...status }, ...server };
  const h = harness({ ...options, apiResponses: { "/mcp": { abilities: [], servers: [saved] }, ...apiResponses } });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  return h;
}

test("MCP browser OAuth is an explicit remote-only Add mode, mutually exclusive with bearer keys", async () => {
  const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [] } } });
  h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
  const auth = h.$("mcp-authorization"), bearer = h.$("mcp-bearer-ref");
  assert.equal(auth.disabled, true);
  assert.equal(h.$("mcp-authorization-row").hidden, true);
  assert.equal(fieldLabel(h, auth).textContent, "Remote authorization");
  assert.deepEqual(auth.children.map(node => node.textContent), ["Manual bearer (optional)", "Browser OAuth"]);
  h.$("mcp-program-name").value = " remote "; h.$("mcp-program-name").dispatch("input");
  h.$("mcp-program-command").value = "https://resource.example.test/mcp"; h.$("mcp-program-command").dispatch("input");
  bearer.value = "mcp:remote:token";
  auth.value = "oauth"; auth.dispatch("change");
  assert.equal(auth.disabled, false);
  assert.equal(bearer.value, ""); assert.equal(bearer.disabled, true);
  assert.equal(h.$("mcp-bearer-row").hidden, true);
  assert.equal(h.$("mcp-oauth-add-help").hidden, false);
  assert.match(h.$("mcp-oauth-add-help").textContent, /pre-registered app.*exact callback URL/);
  assert.equal(buttonNamed(h, "Test").disabled, true, "OAuth needs a saved setup and sign-in first");
  buttonNamed(h, "Test").click(); assert.equal(h.putCalls.length, 0);
  buttonNamed(h, "Add").click(); await flush();
  assert.deepEqual(h.putCalls, [["/mcp/servers", "POST", { name: "remote", command: [], endpoint: "https://resource.example.test/mcp", read_only: false, authorization: "oauth:remote" }]]);
  assert.match(h.$("settings-hint").textContent, /Waiting for a restart/);
  assert.equal(h.$("mcp-authorization").value, "bearer");
  assert.equal(h.writes.length, 0);
});

test("MCP OAuth mode needs a server name and changing to local or Example clears both auth modes", async () => {
  for (const localChoice of ["command", "Example"]) {
    const h = harness({ apiResponses: { "/mcp": { abilities: [], servers: [] } }, putHandler: () => Promise.resolve({ ok: true, tools: [] }) });
    h.initialize(); h.open(); await loadFixture(h); await openService(h, "mcp");
    const command = h.$("mcp-program-command"), auth = h.$("mcp-authorization");
    command.value = "https://resource.example.test/mcp"; command.dispatch("input");
    auth.value = "oauth"; auth.dispatch("change");
    buttonNamed(h, "Add").click(); await flush();
    assert.equal(h.putCalls.length, 0); assert.match(h.$("settings-body").textContent, /Enter a name/);
    h.$("mcp-program-name").value = "local";
    if (localChoice === "command") { command.value = 'local-program "a folder"'; command.dispatch("input"); }
    else buttonNamed(h, "Example").click();
    assert.equal(auth.value, "bearer"); assert.equal(auth.disabled, true);
    assert.equal(h.$("mcp-bearer-ref").value, ""); assert.equal(h.$("mcp-bearer-ref").disabled, true);
    assert.equal(buttonNamed(h, "Test").disabled, false);
    for (const action of ["Test", "Add"]) { buttonNamed(h, action).click(); await flush(); }
    assert.equal(h.putCalls.length, 2);
    for (const [, , body] of h.putCalls) { assert.equal(body.authorization, undefined); assert.equal(body.endpoint, null); assert.ok(body.command.length); }
  }
});

test("MCP one-time OAuth setup has exact callback, explicit minimal scopes and clears write-only secrets before saving", async () => {
  const request = deferred();
  const h = await openMcpOAuth({ status: { configured: false, issuer: null, client_id: null, client_secret_set: false, scopes: [] }, putHandler: () => request.promise });
  const panel = oauthField(h, "panel"), secret = oauthField(h, "client-secret");
  const setup = descendants(panel).find(node => node.tagName === "DETAILS");
  assert.equal(setup.open, true); assert.equal(setup.firstChild.textContent, "One-time OAuth app setup");
  assert.equal(oauthField(h, "callback").textContent, mcpOAuthStatus.callback_url);
  assert.equal(secret.type, "password"); assert.equal(secret.autocomplete, "off"); assert.equal(secret.value, "");
  assert.equal(fieldLabel(h, oauthField(h, "issuer")).textContent, "Expected issuer URL");
  assert.equal(oauthField(h, "issuer").placeholder, "Issuer URL from the provider's documentation");
  assert.equal(fieldLabel(h, oauthField(h, "client-id")).textContent, "Client ID");
  assert.equal(fieldLabel(h, secret).textContent, "Client secret (optional, write-only)");
  assert.equal(fieldLabel(h, oauthField(h, "scopes")).textContent, "Scopes (space-separated)");
  assert.equal(oauthField(h, "scopes").value, "");
  assert.match(panel.textContent, /expected issuer URL from the provider's documentation/);
  assert.match(panel.textContent, /only the minimal scopes/); assert.match(panel.textContent, /does not register an app automatically/);
  assert.equal(oauthButton(h, "Connect").disabled, true); assert.equal(oauthButton(h, "Test tools").disabled, true);
  for (const [suffix, value] of [["issuer", " https://issuer.example.test "], ["client-id", " registered-client "], ["client-secret", "synthetic-oauth-app-secret"], ["scopes", " tools.read  profile.read\t "]]) {
    oauthField(h, suffix).value = value; oauthField(h, suffix).dispatch("input");
  }
  oauthButton(h, "Save OAuth app").click();
  assert.equal(secret.value, "", "clear before the HTTP response"); assert.equal(secret.disabled, true);
  assert.deepEqual(h.putCalls, [[mcpOAuthBase + "/setup", "PUT", { issuer: mcpOAuthStatus.issuer, client_id: "registered-client", client_secret: "synthetic-oauth-app-secret", scopes: ["tools.read", "profile.read"] }]]);
  request.resolve({ ...mcpOAuthStatus, restart_needed: true }); await flush();
  assert.equal(oauthButton(h, "Connect").disabled, false); assert.equal(secret.disabled, false);
  assert.match(h.$("settings-hint").textContent, /OAuth tools: remote/);
  assert.equal(h.$("settings-restart").disabled, false);
  assert.equal(h.writes.length, 0); assert.ok(!panel.textContent.includes("synthetic-oauth-app-secret"));
  assert.ok(!h.putCalls.some(call => call[0] === "/restart"));
});

test("MCP failed setup clears the secret immediately, preserves drafts and never echoes API error details", async () => {
  const request = deferred();
  const h = await openMcpOAuth({ status: { connected: true }, putHandler: () => request.promise });
  oauthField(h, "client-id").value = "replacement-client"; oauthField(h, "client-id").dispatch("input");
  oauthField(h, "client-secret").value = "synthetic-refused-oauth-secret"; oauthField(h, "client-secret").dispatch("input");
  assert.equal(oauthButton(h, "Connect").disabled, true); assert.equal(oauthButton(h, "Test tools").disabled, true);
  oauthButton(h, "Save OAuth app").click();
  assert.equal(oauthField(h, "client-secret").value, "");
  request.reject(new Error("<img src=x> synthetic-refused-oauth-secret " + mcpOAuthReply.auth_url)); await flush();
  assert.match(oauthField(h, "panel").textContent, /OAuth app setup was not saved/);
  assert.equal(oauthField(h, "client-id").value, "replacement-client"); assert.equal(oauthField(h, "client-secret").value, "");
  assert.equal(oauthButton(h, "Connect").disabled, true); assert.equal(oauthButton(h, "Test tools").disabled, true);
  assert.equal(oauthButton(h, "Save OAuth app").disabled, false);
  assert.doesNotMatch(oauthField(h, "panel").textContent, /synthetic-refused-oauth-secret|synthetic-transaction|<img/);
  assert.equal(h.writes.length, 0);
});

test("MCP OAuth accepts public clients and omits empty secrets for unchanged or changed registrations", async () => {
  for (const change of [false, true]) {
    const h = await openMcpOAuth({ putHandler: () => Promise.resolve({ ...mcpOAuthStatus, client_secret_set: !change }) });
    assert.equal(descendants(oauthField(h, "panel")).find(node => node.tagName === "DETAILS").open, false);
    assert.equal(oauthField(h, "client-secret").value, "");
    if (change) { oauthField(h, "issuer").value = "https://other-issuer.example.test"; oauthField(h, "client-id").value = "public-client"; }
    oauthField(h, "scopes").value = "   "; oauthField(h, "scopes").dispatch("input");
    oauthButton(h, "Save OAuth app").click(); await flush();
    assert.deepEqual(h.putCalls[0], [mcpOAuthBase + "/setup", "PUT", { issuer: change ? "https://other-issuer.example.test" : mcpOAuthStatus.issuer, client_id: change ? "public-client" : mcpOAuthStatus.client_id, scopes: [] }]);
    assert.match(oauthField(h, "panel").textContent, /same issuer and client ID/);
    assert.match(oauthField(h, "panel").textContent, /re-enter the secret if the new registration requires one/);
    assert.equal(oauthField(h, "client-secret").value, ""); assert.equal(h.writes.length, 0);
  }
});

test("MCP missing setup fields clear secrets but send no setup or consent request", async () => {
  for (const missing of ["issuer", "client-id"]) {
    const h = await openMcpOAuth();
    oauthField(h, missing).value = " "; oauthField(h, missing).dispatch("input");
    oauthField(h, "client-secret").value = "synthetic-unsent-secret";
    oauthButton(h, "Save OAuth app").click();
    assert.equal(oauthField(h, "client-secret").value, ""); assert.equal(h.putCalls.length, 0);
    assert.match(oauthField(h, "panel").textContent, /Enter the expected issuer URL and registered client ID first/);
  }
});

test("MCP sign-in opens a blank window synchronously, severs opener and validates provider origin and path before navigation", async () => {
  const request = deferred(), popup = oauthPopup();
  const h = await openMcpOAuth({ popup, putHandler: () => {
    assert.equal(h.openedWindows.length, 1, "window opens before the request");
    assert.equal(popup.opener, null, "provider has no opener before navigation");
    assert.equal(popup.navigated.length, 0);
    return request.promise;
  } });
  oauthButton(h, "Connect").click();
  assert.deepEqual(h.openedWindows, [["about:blank", "_blank"]]);
  assert.deepEqual(h.putCalls, [[mcpOAuthBase + "/connect", "POST", {}]]);
  assert.equal(oauthButton(h, "Connect").disabled, true); assert.equal(oauthButton(h, "Check sign-in").disabled, true);
  request.resolve(mcpOAuthReply); await flush();
  assert.deepEqual(popup.navigated, [mcpOAuthReply.auth_url]); assert.equal(popup.closed, false);
  assert.equal(oauthLink(h), undefined); assert.match(oauthField(h, "panel").textContent, /Finish at the provider, then press Check sign-in/);
  assert.equal(oauthButton(h, "Disconnect").disabled, false);
  assert.ok(!oauthField(h, "panel").textContent.includes("synthetic-transaction")); assert.equal(h.writes.length, 0);
});

test("MCP blocked sign-in offers only a validated noopener link and removes it when Check finds a saved grant", async () => {
  const h = await openMcpOAuth({ apiResponses: { [mcpOAuthBase]: { ...mcpOAuthStatus, connected: true } }, putHandler: () => Promise.resolve(mcpOAuthReply) });
  oauthButton(h, "Connect").click(); await flush();
  const link = oauthLink(h);
  assert.equal(link.textContent, "Continue sign-in"); assert.equal(link.href, mcpOAuthReply.auth_url);
  assert.equal(link.target, "_blank"); assert.equal(link.rel, "noopener noreferrer");
  assert.equal(oauthButton(h, "Test tools").disabled, true);
  oauthButton(h, "Check sign-in").click(); await flush();
  assert.equal(oauthLink(h), undefined); assert.equal(oauthButton(h, "Test tools").disabled, false);
  assert.equal(h.apiCalls.filter(path => path === mcpOAuthBase).length, 1); assert.equal(h.writes.length, 0);
});

test("MCP provider handoff rejects mismatched, insecure, malformed or normalized-away authorities without leaking URLs", async () => {
  for (const reply of [
    { ...mcpOAuthReply, auth_url: "https://untrusted.example.test/oauth/authorize?state=synthetic-transaction" },
    { ...mcpOAuthReply, auth_url: "https://login.example.test/oauth/other?state=synthetic-transaction" },
    { ...mcpOAuthReply, auth_url: "https://login.example.test.untrusted.example/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "https://login.example.test:444/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "http://login.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: mcpOAuthReply.auth_url + "#fragment" },
    { ...mcpOAuthReply, auth_url: mcpOAuthReply.auth_url + "#" },
    { ...mcpOAuthReply, auth_url: "https://user:synthetic-secret@login.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "https://@login.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "https://login.example.test\\@untrusted.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "https://%6cogin.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "https://log\nin.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: " " + mcpOAuthReply.auth_url },
    { ...mcpOAuthReply, auth_url: "javascript:alert('synthetic-transaction')" },
    { ...mcpOAuthReply, auth_url: "//login.example.test/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: "/oauth/authorize" },
    { ...mcpOAuthReply, auth_url: {} },
    { ...mcpOAuthReply, authorization_endpoint: "https://untrusted.example.test/oauth/authorize" },
    { ...mcpOAuthReply, authorization_endpoint: mcpOAuthEndpoint + "#fragment" },
    { ...mcpOAuthReply, authorization_endpoint: "https://user:synthetic-secret@login.example.test/oauth/authorize" },
    { auth_url: mcpOAuthReply.auth_url }, {}, null
  ]) {
    const popup = oauthPopup();
    const h = await openMcpOAuth({ popup, putHandler: () => Promise.resolve(reply) });
    oauthButton(h, "Connect").click(); await flush();
    assert.deepEqual(popup.navigated, []); assert.equal(popup.closed, true); assert.equal(oauthLink(h), undefined);
    assert.match(oauthField(h, "panel").textContent, /valid provider sign-in address/);
    assert.doesNotMatch(oauthField(h, "panel").textContent, /synthetic-transaction|synthetic-secret|untrusted/);
    assert.equal(oauthButton(h, "Connect").disabled, false); assert.equal(h.writes.length, 0);
    assert.deepEqual(h.putCalls, [[mcpOAuthBase + "/connect", "POST", {}]]);
  }
});

test("MCP HTTP browser navigation is limited to explicit loopback development pages", async () => {
  for (const [pageLocation, endpoint, accepted] of [
    [{ protocol: "http:", hostname: "127.0.0.1" }, "http://127.0.0.1:9000/authorize", true],
    [{ protocol: "http:", hostname: "localhost" }, "http://localhost:9000/authorize", true],
    [{ protocol: "https:", hostname: "hud.example.test" }, "http://127.0.0.1:9000/authorize", false],
    [{ protocol: "http:", hostname: "hud.example.test" }, "http://127.0.0.1:9000/authorize", false],
    [{ protocol: "http:", hostname: "127.0.0.1" }, "http://login.example.test/authorize", false]
  ]) {
    const h = await openMcpOAuth({ pageLocation, putHandler: () => Promise.resolve({ ...mcpOAuthReply, auth_url: endpoint + "?state=synthetic-transaction", authorization_endpoint: endpoint }) });
    oauthButton(h, "Connect").click(); await flush();
    assert.equal(!!oauthLink(h), accepted);
    assert.equal(h.writes.length, 0);
  }
});

test("MCP sign-in closes its blank window on request failure or when Settings is no longer active", async () => {
  for (const outcome of ["failure", "tab", "close"]) {
    const request = deferred(), popup = oauthPopup();
    const h = await openMcpOAuth({ popup, putHandler: () => request.promise });
    oauthButton(h, "Connect").click();
    if (outcome === "failure") request.reject(new Error(mcpOAuthReply.auth_url));
    else { if (outcome === "tab") h.select("console"); else h.$("settings-close").click(); request.resolve(mcpOAuthReply); }
    await flush();
    assert.equal(popup.closed, true); assert.deepEqual(popup.navigated, []); assert.equal(h.writes.length, 0);
    if (outcome === "failure") {
      assert.match(oauthField(h, "panel").textContent, /Could not start browser sign-in/);
      assert.ok(!oauthField(h, "panel").textContent.includes("synthetic-transaction"));
      assert.equal(oauthButton(h, "Connect").disabled, false);
    }
  }
});

test("MCP Check reads saved-grant status only, preserves drafts and uses the normal explicit restart footer", async () => {
  const request = deferred();
  const h = await openMcpOAuth({ apiResponses: { [mcpOAuthBase]: request.promise } });
  oauthField(h, "client-id").value = "unsaved-client"; oauthField(h, "client-id").dispatch("input");
  oauthButton(h, "Check sign-in").click();
  assert.equal(oauthButton(h, "Check sign-in").disabled, true);
  request.resolve({ ...mcpOAuthStatus, connected: true, callback_url: "http://127.0.0.1:9999/oauth/mcp/remote/callback", restart_needed: true }); await flush();
  assert.match(oauthField(h, "panel").textContent, /OAuth grant saved on this computer/);
  assert.match(oauthField(h, "panel").textContent, /not a live health check or a verified account identity/);
  const badge = descendants(oauthField(h, "panel").parent).find(node => node.className.includes("badge"));
  assert.equal(badge.textContent, "not reachable", "saved grant must not replace actual transport state");
  assert.equal(oauthField(h, "callback").textContent, "http://127.0.0.1:9999/oauth/mcp/remote/callback");
  assert.equal(oauthField(h, "client-id").value, "unsaved-client"); assert.equal(oauthButton(h, "Test tools").disabled, true);
  assert.match(h.$("settings-hint").textContent, /OAuth tools: remote/); assert.equal(h.$("settings-restart").disabled, false);
  assert.equal(h.putCalls.length, 0); assert.equal(h.writes.length, 0);
});

test("MCP saved OAuth Test lists tools on its account route without effects or replacing authorization", async () => {
  const replies = [{ ok: true, tools: [{ id: "mcp.remote.read" }] }, { ok: false, message: "<b>Sign in again</b>" }];
  const h = await openMcpOAuth({ status: { connected: true }, putHandler: () => Promise.resolve(replies.shift()) });
  assert.equal(oauthButton(h, "Test tools").disabled, false);
  oauthButton(h, "Test tools").click(); await flush();
  assert.match(oauthField(h, "panel").textContent, /Reached it: 1 tools. No tool was executed/);
  oauthButton(h, "Test tools").click(); await flush();
  assert.ok(oauthField(h, "panel").textContent.includes("<b>Sign in again</b>"));
  assert.equal(descendants(oauthField(h, "panel")).some(node => node.tagName === "B"), false);
  assert.deepEqual(h.putCalls, [[mcpOAuthBase + "/test", "POST", {}], [mcpOAuthBase + "/test", "POST", {}]]);
  assert.equal(h.writes.length, 0);
});

for (const revoked of [false, true]) {
  test(`MCP Disconnect keeps local grant removed and reports provider revocation ${revoked}`, async () => {
    const h = await openMcpOAuth({ status: { connected: true }, putHandler: () => Promise.resolve({ ...mcpOAuthStatus, revoked, restart_needed: true }) });
    oauthButton(h, "Disconnect").click(); await flush();
    assert.deepEqual(h.putCalls, [[mcpOAuthBase + "/disconnect", "POST", {}]]);
    assert.match(oauthField(h, "panel").textContent, revoked ? /provider revocation confirmed/ : /Provider revocation was not confirmed; review the provider's authorized apps/);
    assert.equal(oauthButton(h, "Test tools").disabled, true); assert.equal(oauthButton(h, "Disconnect").disabled, true);
    assert.equal(oauthButton(h, "Connect").disabled, false); assert.equal(oauthField(h, "client-id").value, mcpOAuthStatus.client_id);
    assert.equal(oauthField(h, "client-secret").value, ""); assert.equal(oauthLink(h), undefined);
    assert.equal(h.apiCalls.filter(path => path === "/mcp").length, 1, "no stale overview reload can restore the disconnected grant");
    assert.match(h.$("settings-hint").textContent, /OAuth tools: remote/); assert.equal(h.writes.length, 0);
  });
}

test("MCP status and tool-list failures are safe, retryable and never expose raw errors", async () => {
  for (const label of ["Check sign-in", "Test tools", "Disconnect"]) {
    const request = deferred();
    const h = await openMcpOAuth({ status: { connected: true }, apiResponses: { [mcpOAuthBase]: request.promise }, putHandler: () => request.promise });
    oauthButton(h, label).click(); assert.equal(oauthButton(h, label).disabled, true);
    request.reject(new Error("<img> synthetic-sensitive-error " + mcpOAuthReply.auth_url)); await flush();
    assert.equal(oauthButton(h, label).disabled, false);
    assert.doesNotMatch(oauthField(h, "panel").textContent, /synthetic-sensitive-error|synthetic-transaction|<img/);
    assert.equal(oauthField(h, "client-secret").value, ""); assert.equal(h.writes.length, 0);
  }
});

test("MCP OAuth controls are selected by the exact saved authorization and isolated per server", async () => {
  for (const authorization of [null, "github", "mcp:remote:token", "oauth:other"]) {
    const h = await openMcpOAuth({ server: { authorization } });
    assert.equal(oauthField(h, "panel"), null, "status alone must not select generic OAuth");
  }
  const h = await openMcpOAuth({ server: { name: "other", authorization: "oauth:other" }, putHandler: () => Promise.resolve({ ok: true, tools: [] }), status: { connected: true } });
  assert.equal(oauthField(h, "panel"), null); assert.ok(oauthField(h, "panel", "other"));
  oauthButton(h, "Test tools", "other").click(); await flush();
  assert.deepEqual(h.putCalls, [["/mcp/servers/other/oauth/test", "POST", {}]]);
});

for (const [tab, path, response, pendingName] of [
  ["connect", "/mcp/access", { enabled: false, running: false, restart_needed: true }, "App access"],
  ["connections", "/mcp", { abilities: [], servers: [], restart_needed: true }, "Programs"]
]) {
  test(`existing ${tab} restart banners register saved pending changes and still restart explicitly`, async () => {
    const restartRequest = deferred();
    const h = harness({ apiResponses: { [path]: response }, putHandler: () => restartRequest.promise });
    h.initialize(); h.open(); await loadFixture(h);
    assert.equal(h.$("settings-restart").disabled, true);
    h.select(tab); await flush();
    assert.match(h.$("settings-hint").textContent, new RegExp("Waiting for a restart: " + pendingName));
    assert.equal(h.$("settings-restart").disabled, false);
    assert.equal(h.putCalls.length, 0);
    buttonNamed(h, "Restart now").click();
    assert.deepEqual(h.putCalls, [["/restart", "POST", undefined]]);
    assert.equal(h.$("settings-restart").disabled, true);
    restartRequest.resolve({}); await flush();
    assert.equal(h.$("settings-hint").textContent, "Nothing is waiting for a restart.");
    assert.equal(h.$("settings-restart").disabled, true);
  });
}

test("Overview incomplete checklist links use the same accessible tab navigation as card actions", async () => {
  const h = harness(); h.initialize(); h.open(); await loadFixture(h, { ...configuredFixture, settings: configuredFixture.settings.map(setting => setting.key === "daemon.executor_model_name" ? { ...setting, value: null } : setting) });
  const choose = buttonNamed(h, "Choose model");
  assert.equal(choose.type, "button");
  choose.click();
  assert.equal(selectedId(h), "settings-tab-brain");
  assert.equal(h.document.activeElement, h.tab("brain"));
  assert.equal(h.$("settings-body").getAttribute("aria-labelledby"), "settings-tab-brain");
  assert.equal(h.putCalls.length, 0);
});

test("successful model saves send a replacement key once and never retain it in appearance storage", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("brain"); await loadFixture(h, configuredFixture);
  const key = h.$("settings-model-main-key"); key.value = "isolated-replacement-key";
  buttonNamed(h, "Save").click();
  assert.equal(key.value, "");
  await flush(); await flush();
  assert.equal(h.putCalls.length, 2);
  assert.equal(h.putCalls[0][0], "/settings");
  assert.deepEqual(h.putCalls[1], ["/settings/keys/model", "PUT", { key: "isolated-replacement-key" }]);
  assert.equal(h.writes.length, 0);
  assert.match(h.$("settings-hint").textContent, /Waiting for a restart: Main model/);
  h.select("console");
  const accent = h.$("settings-console-accent"); accent.value = "ice"; accent.dispatch("change");
  assert.deepEqual(JSON.parse(h.writes[0][1]), { ...defaultAppearance, accent: "ice" });
  assert.doesNotMatch(h.writes[0][1], /isolated-replacement-key|model|provider|key/);
});
