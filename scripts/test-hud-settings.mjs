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

function harness({ stored = null, readThrows = false, writeThrows = false, storageThrows = false, storageEntries = {}, putHandler = null, apiResponses = {} } = {}) {
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
  const window = { confirm(text) { confirmations.push(text); return false; } };
  Object.defineProperty(window, "localStorage", { get() {
    if (storageThrows) throw new Error("Storage access blocked");
    return storage;
  } });
  runInNewContext(source, { window, document, Event, setTimeout() {}, setInterval() {} }, { filename: "settings.js" });
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
  return { document, $, initialize, writes, entries, apiCalls, putCalls, settings, tools, confirmations, search, tab, open, select };
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
  assert.equal(visibleTabs(h).length, 12);
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
  assert.equal(h.$("settings-section-label").textContent, "Voice & search");
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
  assert.equal(h.$("settings-section-label").textContent, "Alerts");
  assert.equal(h.$("settings-body").scrollTop, 64);
  assert.match(h.$("settings-hint").textContent, /Waiting for a restart: Push topic/);
});

test("configuration summaries distinguish saved configuration from unknown availability", async () => {
  const h = harness(); h.initialize(); h.open(); h.select("console");
  assert.match(h.tab("brain").getAttribute("aria-label"), /Configuration unknown/);
  h.settings[0].resolve({ settings: [{ key: "daemon.executor_model_name", value: "model" }], keys: {} });
  await flush();
  assert.match(h.tab("brain").getAttribute("aria-label"), /Configured in saved settings; runtime availability has not been checked/);
  assert.match(h.tab("google").getAttribute("aria-label"), /status unknown/);
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
  assert.equal(classNodes(h, "overview-card").length, 5);
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Unknown", "Unknown", "Unknown", "Unknown", "Browser-local"]);
  assert.match(h.$("settings-body").textContent, /Loading saved daemon configuration/);
  assert.match(h.$("settings-body").textContent, /Setup is unknown until it loads/);
  h.settings[0].reject({ status: 503 }); h.tools[0].reject({ status: 503 }); await flush();
  assert.match(h.$("settings-body").textContent, /Could not load daemon settings \(503\).*Configuration is unknown/);
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Unknown", "Unknown", "Unknown", "Unknown", "Browser-local"]);
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
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Model name saved", "2 folders saved", "3 saved setup entries", "1 saved override", "Browser-local"]);
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
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Not reported", "Not reported", "0 saved setup entries", "Not reported", "Browser-local"]);
  assert.match(h.$("settings-body").textContent, /main model setting was not reported/);
  assert.match(h.$("settings-body").textContent, /Folder grants were not reported/);
  assert.doesNotMatch(h.$("settings-body").textContent, /No folder grants are saved/);
  h.open(); await loadFixture(h, { settings: [savedSetting("daemon.executor_model_name", null), savedSetting("daemon.tool_workspace_roots", null, "folders")], keys: {}, postures: {} });
  assert.deepEqual(classNodes(h, "overview-state").map(node => node.textContent), ["Choose a model", "No folders saved", "0 saved setup entries", "Default choices", "Browser-local"]);
});

test("Overview buttons route through tab navigation, preserve search and focus sensibly when the target is filtered", () => {
  const h = harness(); h.initialize(); h.open();
  for (const id of ["brain", "files", "google", "telegram", "phone", "permissions", "console"]) {
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
  assert.doesNotMatch(source, /innerHTML|outerHTML|insertAdjacentHTML|https?:\/\//);
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

for (const [tab, path, response, pendingName] of [
  ["connect", "/mcp/access", { enabled: false, running: false, restart_needed: true }, "Connect apps"],
  ["advanced", "/mcp", { abilities: [], servers: [], restart_needed: true }, "Programs"]
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
