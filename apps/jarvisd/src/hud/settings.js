"use strict";
// The console's settings, permissions and Google sign-in (split out of hud.js).
//
// `JarvisSettings(context)` is called once by hud.js at load. It takes the few helpers it shares with the console and wires the
// settings dialog itself; nothing else in the console calls into it. The same security rules as hud.js hold: every value from the
// daemon is inserted as text, never as markup, and a key is sent once from a password field and never read back.
(function () {
  var appearanceDefaults = { density: "comfortable", motion: "system", accent: "mint", atmosphere: "cinematic" };
  var appearanceValues = { density: ["comfortable", "compact"], motion: ["system", "reduced"], accent: ["mint", "ice", "amethyst"], atmosphere: ["cinematic", "quiet"] };
  var appearance = Object.assign({}, appearanceDefaults);
  var appearanceNote = "Changes apply immediately; no daemon restart is needed.";
  var appearanceUnsaved = false;
  function validAppearance(key, value) {
    return Object.prototype.hasOwnProperty.call(appearanceValues, key) && appearanceValues[key].indexOf(value) >= 0;
  }
  try {
    var stored = JSON.parse(window.localStorage.getItem("jarvis-appearance"));
    if (stored && typeof stored === "object" && !Array.isArray(stored)) {
      Object.keys(appearanceDefaults).forEach(function (key) {
        if (validAppearance(key, stored[key])) appearance[key] = stored[key];
      });
    }
  } catch (error) {
    appearanceNote = "Saved appearance could not be read. Defaults apply for this page; changes apply immediately without a daemon restart.";
  }
  function applyAppearance() {
    Object.keys(appearanceDefaults).forEach(function (key) {
      document.documentElement.dataset[key] = appearance[key];
    });
  }
  function persistAppearance(reset) {
    // Persist only this validated, browser-local schema; never capture settings fields or key drafts.
    var preferences = {};
    Object.keys(appearanceDefaults).forEach(function (key) {
      preferences[key] = validAppearance(key, appearance[key]) ? appearance[key] : appearanceDefaults[key];
    });
    appearance = preferences;
    applyAppearance();
    try {
      window.localStorage.setItem("jarvis-appearance", JSON.stringify(preferences));
      appearanceUnsaved = false;
      appearanceNote = (reset ? "Appearance reset to defaults in this browser." : "Appearance saved in this browser.") + " Applied immediately; no daemon restart is needed.";
    } catch (error) {
      appearanceUnsaved = true;
      appearanceNote = "Appearance applied for this page, but browser storage is unavailable. It will not be remembered after a reload.";
    }
  }
  // Apply before the settings dialog is wired, and leave system motion to the browser's media preference.
  applyAppearance();
window.JarvisSettings = function (context) {
  var $ = context.$, el = context.el, api = context.api, putJson = context.putJson, explain = context.explain;
  // Called when needed, not captured now: the tools list helpers are declared in hud.js after the point this is created.
  function families(tools) { return context.families(tools); }
  function familyLabel(group) { return context.familyLabel(group); }
    // ---- settings: saved configuration and browser-local appearance ------------------------------------------------------
    // The same checks as `jarvis config` and `jarvis keys` (one code path in the daemon). A key is typed into a password field,
    // sent once, and the field is cleared; the daemon never sends it back, only whether it is set.
    // Sidebar dots describe saved configuration, not runtime availability.
    var NAV = [
      ["JARVIS", [
        ["overview", "Overview", "Your model, connected services, permissions and appearance."],
        ["brain", "Brain", "Which model JARVIS thinks with, and a second one for when the first is limited or down."],
        ["voice", "Voice & search", "How JARVIS sounds, and how it searches the web. Both are optional."],
        ["files", "Folders & code", "What JARVIS may touch on this computer. Both are off until you turn them on, so nothing is reachable by accident."],
        ["permissions", "Permissions", "For each tool: let the rules decide, always ask, run without asking, or switch it off."]
      ]],
      ["Apps & tools", [
        ["apps", "Apps & tools", "Choose the services and tools JARVIS can use."],
        ["connections", "GitHub", "Connect accounts with browser sign-in. GitHub setup, saved account, testing and disconnect; no token or server command for ordinary sign-in."],
        ["google", "Google", "Sign in with your Google account so JARVIS can read your mail and calendar."],
        ["programs", "Custom tools (MCP)", "Add a local program or remote MCP server. Advanced tool connections, OAuth, bearer authentication and approval behavior."]
      ]],
      ["Channels", [
        ["telegram", "Telegram", "Talk to JARVIS from your phone. It answers in a Telegram chat only you can use, and asks you there when it needs your approval."],
        ["phone", "Phone", "JARVIS rings you when it needs you, and you can ring JARVIS. A voice agent at ElevenLabs does the talking; the tools stay with JARVIS."]
      ]],
      ["Access", [
        ["connect", "External access", "Let your other AI apps (Claude Code, Cursor, VS Code, your own scripts) and cloud agents hand JARVIS a task. They connect with a key only you have."]
      ]],
      ["System", [
        ["console", "Console", "Make this console yours: accent, cinematic or quiet atmosphere, comfortable or compact density, and system or reduced motion. Browser-local, immediate and available offline."],
        ["alerts", "Alerts", "How JARVIS reaches you when the console is closed, and when it keeps quiet."],
        ["advanced", "Advanced", "Ports and other technical settings. The defaults are right for almost everyone."]
      ]]
    ];
    var TABS = NAV.reduce(function (all, group) { return all.concat(group[1]); }, []);
    var sview = { reply: null, tools: [], tab: "overview", error: "", toolsError: "", toolsLoaded: false };
    var navGroups = [], navButtons = {}, settingsLoad = 0;
    // Non-secret form drafts belong to this dialog instance, never localStorage.
    var drafts = {}, catalogUnmount = null;
    function draftKey(field) { return (field.dataset.section || sview.tab) + ":" + field.id; }
    function paintDrafts() { var button = $("settings-discard"); if (button) { button.disabled = !Object.keys(drafts).length; button.title = "Unsaved fields stay in this page across sections. Reloading discards them."; } }
    function forgetDraft(field) { delete drafts[draftKey(field)]; paintDrafts(); }
    function acknowledgeDraft(field, value) { if (drafts[draftKey(field)] === value) forgetDraft(field); }
    function walkFields(node, visit) {
      Array.from(node.children || []).forEach(function (child) {
        if (/^(INPUT|SELECT|TEXTAREA)$/.test(child.tagName)) visit(child);
        walkFields(child, visit);
      });
    }
    function rememberDraft(event) {
      var field = event.target;
      if (!field.id || field.type === "password" || field.type === "checkbox" || field.dataset.autosave) return;
      drafts[draftKey(field)] = field.value;
      paintDrafts();
    }
    $("settings-body").addEventListener("input", rememberDraft);
    $("settings-body").addEventListener("change", rememberDraft);
    function restoreDrafts(body) {
      walkFields(body, function (field) {
        var key = draftKey(field);
        if (Object.prototype.hasOwnProperty.call(drafts, key)) field.value = drafts[key];
      });
      walkFields(body, function (field) { if (field.restoreDraft) field.restoreDraft(); });
    }
    function clearSecrets() { walkFields($("settings-body"), function (field) { if (field.type === "password") field.value = ""; }); }
    function appsPanel(body) {
      var entries = [
        // The tone only colours what the saved settings already say; it is not a live connection check.
        { id: "connections", name: "GitHub", description: "Repositories, issues and pull requests. Sign in, check access or disconnect.", status: "Open to check account", tone: "info" },
        { id: "google", name: "Google", description: "Gmail and Calendar. Configure sign-in and allowed actions.", status: keyIsSet("google") ? "Client configured" : "Setup needed", tone: keyIsSet("google") ? "ok" : "todo" },
        { id: "voice", name: "Voice & web search", description: "ElevenLabs voice and web search credentials.", status: keyIsSet("voice") || keyIsSet("search") ? "Credentials saved" : "Optional setup", tone: keyIsSet("voice") || keyIsSet("search") ? "ok" : "info" },
        { id: "files", name: "Files & coding", description: "Folder access, coding agents and sandbox configuration.", status: tabState("files") === "ok" ? "Configured" : "Off until configured", tone: tabState("files") === "ok" ? "ok" : "info" },
        { id: "permissions", name: "Browser & documentation", description: "Built-in tools. Inspect available tools and choose when JARVIS asks.", status: "Built in · managed by JARVIS", tone: "info" },
        { id: "programs", name: "Custom tools (MCP)", description: "Connect a local program or remote tool server. For advanced integrations.", status: "Open to inspect servers", tone: "info" }
      ];
      if (window.JarvisControls) catalogUnmount = window.JarvisControls.mountCatalog(body, entries, goTab);
      else entries.forEach(function (entry) {
        var card = section(entry.name, entry.description + " " + entry.status);
        var open = el("button", "", "Open " + entry.name);
        open.addEventListener("click", function () { goTab(entry.id); });
        card.appendChild(open); body.appendChild(card);
      });
    }
    function settingsNote(text, bad) {
      var note = $("settings-note");
      note.textContent = text || "";
      note.className = bad ? "bad" : "ok";
    }
    function failure(error) {
      settingsNote("Could not do that (" + (error.status || "no reply") + ")", true);
    }
    // Changes that only take effect after a restart are listed in the footer, so it is always clear whether one is waiting.
    var pendingRestart = [], restartInFlight = false, restartHints = {};
    function integrationRestart(name, needed) {
      if (needed) restartHints[name] = true; else delete restartHints[name];
      pendingRestart = pendingRestart.filter(function (item) { return item !== name; });
      if (needed) pendingRestart.push(name);
      paintRestart();
    }
    function paintRestart() {
      var hint = $("settings-hint"), button = $("settings-restart");
      var waiting = pendingRestart.length > 0;
      hint.textContent = sview.reply && sview.reply.restart_state_known === false ? "Saved changes could not be compared with startup. Restart status is unknown." : waiting ? "Waiting for a restart: " + pendingRestart.join(", ") + "." : "Nothing is waiting for a restart.";
      hint.className = "dim" + (waiting ? " hot" : "");
      button.className = waiting ? "hot" : "";
      button.disabled = !waiting || !sview.reply || !!sview.error || restartInFlight;
    }
    function needsRestart(what) {
      var name = String(what || "A change").replace(/[\s:;]+$/, "").replace(/ (removed|turned off)$/, "");
      if (pendingRestart.indexOf(name) < 0) pendingRestart.push(name);
      paintRestart();
    }
    function saved(what) {
      settingsNote(String(what).replace(/[\s:;]+$/, "") + " saved. Restart to apply.");
      needsRestart(what);
      loadSettings();
    }
    function focusSettingsTab() {
      var picker = $("settings-section-pick");
      if (picker && picker.getClientRects && picker.getClientRects().length) { picker.focus(); return; }
      var button = navButtons[sview.tab];
      if (button && !button.hidden) button.focus(); else $("settings-body").focus();
    }
    function goTab(id) {
      if (!TABS.some(function (tab) { return tab[0] === id; })) return;
      if (sview.tab !== id) {
        clearSecrets();
        sview.tab = id;
        settingsNote("");
        renderSettings();
      } else settingsNav();
      focusSettingsTab();
    }
    // The first sentence of a long explanation is shown; the rest is one click away. A person reads what a setting does at a glance and
    // opens the details only when they want them.
    function helpNode(text, extraClass) {
      var full = String(text || "").trim();
      var span = el("span", "help" + (extraClass ? " " + extraClass : ""));
      var cut = full.search(/[.!?]\s/);
      if (full.length <= 150 || cut < 0 || cut > 200) { span.textContent = full; return span; }
      var rest = full.slice(cut + 1).trim();
      span.appendChild(document.createTextNode(full.slice(0, cut + 1) + " "));
      var more = el("button", "link", "More");
      more.type = "button";
      var box = el("span", "morebox", rest);
      box.hidden = true;
      more.addEventListener("click", function () { box.hidden = !box.hidden; more.textContent = box.hidden ? "More" : "Less"; });
      span.appendChild(more);
      span.appendChild(box);
      return span;
    }
    function section(title, hint) {
      var wrap = el("div");
      wrap.appendChild(el("div", "secthead", title));
      if (hint) wrap.appendChild(el("p", "secthint", hint));
      return wrap;
    }
    // A short list of what is done and what is still to do, each item optionally leading to the tab where it is done.
    function checklist(items) {
      var list = el("ul", "checklist");
      items.forEach(function (item) {
        var line = el("li", item.done ? "done" : "");
        line.appendChild(el("span", "mark", item.done ? "\u2713" : "\u25CB"));
        var what = el("span", "what", item.text);
        if (item.tab && !item.done) {
          what.appendChild(document.createTextNode(" "));
          var go = el("button", "link", item.tabLabel || "Open");
          go.type = "button";
          go.addEventListener("click", function () { goTab(item.tab); });
          what.appendChild(go);
        }
        if (item.action) {
          what.appendChild(document.createTextNode(" "));
          var act = el("button", "link", item.action.label);
          act.type = "button";
          act.addEventListener("click", function () { act.disabled = true; item.action.run(function () { act.disabled = false; }); });
          what.appendChild(act);
        }
        line.appendChild(what);
        list.appendChild(line);
      });
      return list;
    }
    // Setup steps are open until the thing is set up, then fold away.
    function stepsCard(title, steps, done, footer) {
      var card = el("div", "card" + (done ? " quiet" : ""));
      var details = el("details");
      if (!done) details.open = true;
      details.appendChild(el("summary", "", title));
      var list = el("ol");
      steps.forEach(function (text) { list.appendChild(el("li", "", text)); });
      details.appendChild(list);
      if (footer) details.appendChild(el("p", "lead", footer));
      card.appendChild(details);
      return card;
    }
    function keyIsSet(which) {
      return !!(sview.reply && sview.reply.keys && sview.reply.keys[which] && sview.reply.keys[which].state === "set");
    }
    function settingIsSet(key) {
      var s = byKey(key);
      return !!(s && s.value !== null && s.value !== undefined && String(s.value) !== "");
    }
    // The dot beside a tab. Only what the saved settings can tell is used; nothing here calls out.
    function tabState(id) {
      if (id === "brain") return byKey("daemon.executor_model_name") ? (settingIsSet("daemon.executor_model_name") ? "ok" : "todo") : "";
      if (id === "voice") return keyIsSet("voice") || keyIsSet("search") ? "ok" : "";
      if (id === "files") return settingIsSet("daemon.tool_workspace_roots") || settingIsSet("daemon.code_sandbox_image") || settingIsSet("daemon.code_agent") ? "ok" : "";
      var keys = (sview.reply && sview.reply.keys) || {};
      if (id === "google") {
        if (settingIsSet("daemon.google_client_id") && keyIsSet("google")) return "ok";
        return byKey("daemon.google_client_id") && keys.google && keys.google.state === "unset" ? "todo" : "";
      }
      if (id === "telegram") return keyIsSet("telegram") ? "ok" : keys.telegram && keys.telegram.state === "unset" ? "todo" : "";
      if (id === "phone") {
        var phoneKeys = ["daemon.phone_agent_id", "daemon.phone_number_id", "daemon.phone_owner_numbers"];
        if (phoneKeys.every(settingIsSet)) return "ok";
        return phoneKeys.every(function (key) { return !!byKey(key); }) ? "todo" : "";
      }
      if (id === "connect") return settingIsSet("daemon.mcp_serve_port") ? "ok" : "";
      return "";
    }
    function valuesFor(kind, text) {
      var t = text.trim();
      if (kind === "folders") return t.split(";").map(function (part) { return part.trim(); }).filter(Boolean);
      if (kind === "tools") return t.split(/[\s,]+/).filter(Boolean);
      return [t];
    }
    function showFor(setting) {
      if (setting.value === null || setting.value === undefined) return "";
      // The daemon lists a list value with ", "; folders are shown separated by ";" and a command by plain spaces.
      if (setting.kind === "folders") return String(setting.value).split(", ").join("; ");
      if (setting.kind === "words") return String(setting.value).split(", ").join(" ");
      return String(setting.value);
    }
    function byKey(name) {
      return ((sview.reply && sview.reply.settings) || []).filter(function (s) { return s.key === name; })[0];
    }
    function stateBadge(setting) {
      var set = setting.value !== null && setting.value !== undefined;
      var atDefault = set && setting.default && String(setting.value) === String(setting.default);
      var badge = el("span", "badge " + (set && !atDefault ? "on" : "off"), atDefault ? "default" : set ? "set" : setting.default ? "default" : "off");
      badge.setAttribute("aria-hidden", "true");
      return badge;
    }
    // One setting: its field, Save (and Unset when set), and either its help (when set) or what "unset" means (when not).
    var LABELS = {
      "daemon.executor_model_name": "Model", "daemon.executor_base_url": "Model server", "daemon.executor_reasoning_effort": "Thinking effort",
      "daemon.executor_subagent_model_name": "Sub-agent model", "daemon.executor_background_model_name": "Scheduled-run model",
      "daemon.workspace_daily_token_limit": "Daily token limit",
      "daemon.tool_workspace_roots": "Folders", "daemon.code_sandbox_image": "Sandbox image", "daemon.code_sandbox_interpreter": "Interpreter",
      "daemon.speech_voice_id": "Voice", "daemon.speech_model": "Voice model", "daemon.http_port": "Console port",
      "daemon.mcp_serve_port": "Let other apps use JARVIS (port)", "daemon.notifications": "Desktop notifications", "daemon.google_client_id": "Client ID",
      "daemon.google_actions": "Send mail, trash mail & change events", "policy.trust": "Runs without asking",
      "daemon.quiet_hours": "Quiet hours", "daemon.resume_interrupted": "Resume cut-off tasks", "daemon.push_topic": "Push topic (ntfy)", "daemon.push_server": "Push server (ntfy)",
      "daemon.phone_agent_id": "Agent id", "daemon.phone_number_id": "Phone number id", "daemon.phone_owner_numbers": "My numbers",
      "daemon.phone_approves": "My yes on the phone counts", "daemon.phone_opening_line": "JARVIS writes the opening line", "daemon.phone_call_anyone": "JARVIS may call other people", "daemon.phone_screen_callers": "Answer only my own numbers",
      "daemon.phone_calls_per_hour": "Calls per hour", "daemon.phone_calls_per_day": "Calls per day"
    };
    // A one-line statement of what an on/off switch does, shown instead of the long explanation (which is one click away under More).
    var SHORT = {
      "daemon.google_actions": "Allow sending, drafts, moving mail to Gmail Trash and calendar changes. This requests Google's broader mailbox-change permission; sign in again to grant it. Deletion asks by default unless you explicitly trust the tool.",
      "daemon.notifications": "Show a desktop notification when a scheduled task finishes or a reminder is due while the console is closed.",
      "daemon.resume_interrupted": "After a restart, continue a task that was cut off, once.",
      "daemon.phone_approves": "Count my spoken yes, on a call JARVIS placed to me, as the approval.",
      "daemon.phone_opening_line": "JARVIS writes the first sentence of a call to me.",
      "daemon.phone_call_anyone": "Let JARVIS call numbers that are not mine. Every call asks me first.",
      "daemon.phone_screen_callers": "Only answer calls to my phone number when they come from my own numbers."
    };
    function labelFor(setting, label) {
      return label && label !== "folders" ? label : LABELS[setting.key] || setting.key.replace(/^(daemon|policy)\./, "").replace(/_/g, " ");
    }
    function isSetting(setting) { return setting.value !== null && setting.value !== undefined; }
    function afterSave(setting, live, what) {
      if (live) { settingsNote(what + " saved."); loadSettings(); } else saved(what);
    }
    var controlNumber = 0;
    function describeControl(field, label, help) {
      if (!field.id) field.id = "settings-field-" + sview.tab + "-" + (++controlNumber);
      field.dataset.section = sview.tab;
      label.setAttribute("for", field.id);
      if (help) {
        help.id = field.id + "-help";
        field.setAttribute("aria-describedby", help.id);
      }
    }
    function settingRow(setting, label, live) {
      var row = el("div", "set-row");
      var name = el("label", "name", labelFor(setting, label));
      name.title = setting.key;
      name.appendChild(stateBadge(setting));
      var field = el("input");
      field.type = "text";
      field.id = "setting-" + setting.key;
      field.value = showFor(setting);
      field.placeholder = setting.default ? "default: " + setting.default : (setting.example ? "e.g. " + setting.example : "(not set)");
      var acts = el("div", "acts");
      var save = el("button", "", "Save");
      save.addEventListener("click", function () {
        var values = valuesFor(setting.kind, field.value);
        if (!values.length || !values[0]) { settingsNote("Enter a value, or use Unset.", true); return; }
        var submitted = field.value;
        save.disabled = true;
        putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: values }).then(function () {
          acknowledgeDraft(field, submitted);
          afterSave(setting, live, labelFor(setting, label));
        }, function (error) { save.disabled = false; settingsNote(explain(error, "That value was refused."), true); });
      });
      acts.appendChild(save);
      if (isSetting(setting)) {
        var unset = el("button", "", "Unset");
        unset.addEventListener("click", function () {
          putJson("/settings/" + encodeURIComponent(setting.key), "DELETE").then(function () {
            forgetDraft(field);
            afterSave(setting, live, labelFor(setting, label) + " removed");
          },
            function (error) { settingsNote(explain(error, "That cannot be removed on its own."), true); });
        });
        acts.appendChild(unset);
      }
      var help = helpNode(isSetting(setting) ? setting.help : setting.unset_means);
      describeControl(field, name, help);
      save.setAttribute("aria-label", "Save " + labelFor(setting, label));
      save.id = field.id + "-save";
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(acts);
      row.appendChild(help);
      return row;
    }
    // An on/off setting is a switch that saves as soon as it is flipped. The state shown is the one in force: what was saved, or the default.
    function toggleRow(setting, label, live) {
      var row = el("div", "set-row toggle");
      var name = el("label", "name", labelFor(setting, label));
      name.title = setting.key;
      var on = isSetting(setting) ? /^(on|true|yes)$/i.test(String(setting.value)) : /^(on|true|yes)$/i.test(String(setting.default || "off"));
      var wrap = el("label", "switch");
      var box = el("input");
      box.type = "checkbox";
      box.checked = on;
      var track = el("span", "track");
      var state = el("span", "state", on ? "On" : "Off");
      wrap.appendChild(box); wrap.appendChild(track); wrap.appendChild(state);
      box.addEventListener("change", function () {
        var want = box.checked;
        box.disabled = true;
        putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: [want ? "on" : "off"] }).then(function () {
          if (live) { settingsNote(labelFor(setting, label) + (want ? " is on." : " is off.")); loadSettings(); }
          else saved(labelFor(setting, label));
        }, function (error) {
          box.checked = !want; box.disabled = false;
          settingsNote(explain(error, "That was refused."), true);
        });
      });
      row.appendChild(name);
      row.appendChild(wrap);
      row.appendChild(el("span"));
      var short = SHORT[setting.key];
      var long = isSetting(setting) ? setting.help : setting.unset_means;
      var help = helpNode(short ? short + " " + long : long);
      describeControl(box, name, help);
      row.appendChild(help);
      return row;
    }
    // A setting with a few fixed answers is a list to pick from, not a box to type a word into.
    function choiceRow(setting, label, options, live) {
      var row = el("div", "set-row");
      var name = el("label", "name", labelFor(setting, label));
      name.title = setting.key;
      name.appendChild(stateBadge(setting));
      var select = el("select");
      select.dataset.autosave = "true";
      options.forEach(function (option) { var node = el("option", "", option[1]); node.value = option[0]; select.appendChild(node); });
      select.value = isSetting(setting) ? String(setting.value) : "";
      if (select.value !== (isSetting(setting) ? String(setting.value) : "")) select.value = "";
      select.addEventListener("change", function () {
        var request = select.value ? putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: [select.value] })
          : putJson("/settings/" + encodeURIComponent(setting.key), "DELETE");
        request.then(function () { afterSave(setting, live, labelFor(setting, label)); },
          function (error) { settingsNote(explain(error, "That was refused."), true); loadSettings(); });
      });
      var help = helpNode(isSetting(setting) ? setting.help : setting.unset_means);
      describeControl(select, name, help);
      row.appendChild(name);
      row.appendChild(select);
      row.appendChild(el("span"));
      row.appendChild(help);
      return row;
    }
    // Any setting in the right shape for its kind.
    function rowFor(setting, label, live) {
      return setting.kind === "flag" ? toggleRow(setting, label, live) : settingRow(setting, label, live);
    }
    function keyRow(which, label) {
      var state = sview.reply && sview.reply.keys && sview.reply.keys[which] ? sview.reply.keys[which].state : "unknown";
      var row = el("div", "set-row");
      var name = el("label", "name", label);
      name.appendChild(el("span", "badge " + (state === "set" ? "on" : "off"), state));
      var field = el("input");
      field.type = "password";
      field.id = "settings-key-" + which;
      field.autocomplete = "off";
      field.spellcheck = false;
      field.setAttribute("autocapitalize", "none");
      field.placeholder = state === "set" ? "Saved key hidden; paste a replacement" : "Paste a key; never shown after saving";
      var acts = el("div", "acts");
      var save = el("button", "", "Save key");
      save.addEventListener("click", function () {
        var value = field.value;
        field.value = "";
        if (!value.trim()) { settingsNote("Paste a key first.", true); return; }
        putJson("/settings/keys/" + which, "PUT", { key: value }).then(function () { saved(label); },
          function (error) { settingsNote(explain(error, "That key was refused."), true); });
      });
      acts.appendChild(save);
      var result = el("span", "help", "");
      // A key that is saved can be checked against its provider at once, without waiting for the restart that applies it.
      if ((which === "voice" || which === "search") && state === "set") {
        var check = el("button", "", "Test");
        check.addEventListener("click", function () {
          check.disabled = true; result.className = "help"; result.textContent = "Asking the provider...";
          putJson("/settings/keys/" + which + "/test", "POST").then(function (reply) {
            check.disabled = false;
            result.className = "help " + (reply.ok ? "good" : "bad");
            result.textContent = reply.message;
          }, function (error) { check.disabled = false; result.className = "help bad"; result.textContent = explain(error, "The test could not be run."); });
        });
        acts.appendChild(check);
      }
      if ((which === "voice" || which === "search" || which === "fallback") && state === "set") {
        var remove = el("button", "", "Remove");
        remove.addEventListener("click", function () {
          putJson("/settings/keys/" + which, "DELETE").then(function () { saved(label + " removed;"); },
            function (error) { settingsNote(explain(error, "Could not remove it."), true); });
        });
        acts.appendChild(remove);
      }
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(acts);
      var helps = {
        model: "Stored in a private file on this machine and never shown again. Local Ollama needs no real key.",
        google: "The client secret of your Google OAuth Desktop client. Stored in a private file and never shown again.",
        voice: "An ElevenLabs API key, from your ElevenLabs account. It also lets the Phone tab place calls, so give it access to the agents platform as well as speech. Stored in a private file and never shown again; what JARVIS says is sent to ElevenLabs to be spoken. Press Test to check it.",
        fallback: "The key of the different provider you gave above for the fallback model. Stored in a private file and never shown again; when the main model is limited, what the run is saying is sent to that provider. Check it with `jarvis keys test fallback`.",
        search: "An Ollama API key; a free account at ollama.com can make one. It turns on web search; what JARVIS searches for is sent to ollama.com. The key is stored in a private file and never shown again. Press Test to check it."
      };
      var help = helpNode(helps[which] || "Keys are write-only and never displayed after saving.");
      describeControl(field, name, help);
      save.setAttribute("aria-label", "Save " + label);
      row.appendChild(help);
      row.appendChild(result);
      return row;
    }
    // Two settings that only make sense together are saved together.
    // ---- the model providers: pick one, load its models, test it, save ---------------------------------------------------------
    // The presets come from the daemon (with the settings list) and only fill in an address. Every request to a provider is made by the
    // daemon, so a stored key never reaches this page; the key typed here is sent once, with the request that needs it.
    function normUrl(url) { return String(url || "").trim().replace(/\/+$/, "").toLowerCase(); }
    function providerFor(url) {
      var all = (sview.reply && sview.reply.providers) || [];
      var found = all.filter(function (p) { return p.url && normUrl(p.url) === normUrl(url); })[0];
      return found || all.filter(function (p) { return p.id === "custom"; })[0];
    }
    function providerRow(label, control, acts, help, field) {
      var row = el("div", "set-row");
      var name = el("label", "name", label);
      describeControl(field || control, name, help);
      row.appendChild(name);
      row.appendChild(control);
      row.appendChild(acts || el("span"));
      if (help) row.appendChild(help);
      return row;
    }
    // A filterable dropdown for the model box. The browser's own <datalist> only offers what matches the text already in the box (so a
    // saved model name hides every other choice) and cannot be styled, so this draws its own: it opens on click or with the arrow,
    // lists every model, narrows as you type, and takes the keyboard. Typing any name that is not listed is still allowed.
    var openPicker = null;
    function modelPicker(input) {
      var wrap = el("div", "combo");
      var toggle = el("button", "combo-toggle");
      toggle.type = "button";
      toggle.tabIndex = -1;
      toggle.disabled = true;
      toggle.title = "Load models to see suggestions; you can also type a name.";
      toggle.setAttribute("aria-label", "Show model choices");
      var chevron = el("span", "chevron");
      chevron.setAttribute("aria-hidden", "true");
      toggle.appendChild(chevron);
      var menu = el("ul", "combo-list");
      menu.id = input.id + "-options";
      menu.setAttribute("role", "listbox");
      menu.setAttribute("aria-label", "Model suggestions");
      menu.hidden = true;
      input.setAttribute("role", "combobox");
      input.setAttribute("aria-autocomplete", "list");
      input.setAttribute("aria-controls", menu.id);
      input.setAttribute("aria-expanded", "false");
      toggle.setAttribute("aria-controls", menu.id);
      toggle.setAttribute("aria-expanded", "false");
      toggle.setAttribute("aria-haspopup", "listbox");
      wrap.appendChild(input);
      wrap.appendChild(toggle);
      wrap.appendChild(menu);
      var models = [], active = -1, typed = false;
      function visible() {
        var needle = typed ? input.value.trim().toLowerCase() : "";
        return models.filter(function (m) { return !needle || m.id.toLowerCase().indexOf(needle) >= 0; }).slice(0, 300);
      }
      function draw() {
        var shown = visible();
        if (active >= shown.length) active = -1;
        input.removeAttribute("aria-activedescendant");
        menu.replaceChildren();
        if (!shown.length) {
          var empty = el("li", "note", "No matching models. You can still use the name you typed.");
          empty.setAttribute("role", "presentation");
          menu.appendChild(empty);
        }
        shown.forEach(function (m, index) {
          var item = el("li", (m.likely_chat ? "" : "dim ") + (m.id === input.value ? "current " : "") + (index === active ? "active" : ""), m.id + (m.likely_chat ? "" : "   (probably not a chat model)"));
          item.id = menu.id + "-" + index;
          item.setAttribute("role", "option");
          item.setAttribute("aria-selected", index === active || (active < 0 && m.id === input.value) ? "true" : "false");
          item.addEventListener("mousedown", function (event) { event.preventDefault(); });
          item.addEventListener("click", function () { choose(m.id); });
          menu.appendChild(item);
          if (index === active) { input.setAttribute("aria-activedescendant", item.id); item.scrollIntoView({ block: "nearest" }); }
        });
      }
      function open() {
        if (!models.length) return;
        if (openPicker && openPicker !== api2) openPicker.close();
        openPicker = api2;
        menu.hidden = false;
        input.setAttribute("aria-expanded", "true");
        toggle.setAttribute("aria-expanded", "true");
        draw();
      }
      function close() {
        menu.hidden = true;
        input.setAttribute("aria-expanded", "false");
        toggle.setAttribute("aria-expanded", "false");
        input.removeAttribute("aria-activedescendant");
        active = -1;
        typed = false;
        if (openPicker === api2) openPicker = null;
      }
      function choose(id) { input.value = id; close(); input.focus(); input.dispatchEvent(new Event("change", { bubbles: true })); }
      toggle.addEventListener("mousedown", function (event) { event.preventDefault(); });
      toggle.addEventListener("click", function () { if (menu.hidden) { input.focus(); open(); } else close(); });
      input.addEventListener("focus", function () { typed = false; active = -1; });
      input.addEventListener("click", function () { if (menu.hidden) open(); });
      input.addEventListener("input", function () { typed = true; active = -1; if (menu.hidden) open(); else draw(); });
      input.addEventListener("keydown", function (event) {
        if (event.isComposing) return;
        var shown = visible();
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          if (menu.hidden) open();
          if (!shown.length) return;
          active = active < 0 ? (event.key === "ArrowDown" ? 0 : shown.length - 1)
            : Math.max(0, Math.min(shown.length - 1, active + (event.key === "ArrowDown" ? 1 : -1)));
          draw();
        } else if (event.key === "Enter" && !menu.hidden && active >= 0 && shown[active]) {
          event.preventDefault();
          choose(shown[active].id);
        } else if (event.key === "Tab") {
          close();
        }
      });
      var api2 = {
        element: wrap,
        close: close,
        contains: function (target) { return wrap.contains(target); },
        // Sets the models on offer; `show` opens the list so the person sees it arrive.
        setModels: function (list, show) {
          models = list || [];
          active = -1;
          toggle.disabled = !models.length;
          toggle.title = models.length ? "Show model choices" : "Load models to see suggestions; you can also type a name.";
          if (!models.length) { close(); return; }
          if (show && models.length) { input.focus(); open(); } else if (!menu.hidden) draw();
        }
      };
      return api2;
    }
    function providerCard(slot) {
      var fallback = slot === "fallback";
      var urlKey = fallback ? "daemon.executor_fallback_base_url" : "daemon.executor_base_url";
      var modelKey = fallback ? "daemon.executor_fallback_model_name" : "daemon.executor_model_name";
      var which = fallback ? "fallback" : "model";
      var urlSetting = byKey(urlKey), modelSetting = byKey(modelKey);
      var keyState = sview.reply.keys && sview.reply.keys[which] ? sview.reply.keys[which].state : "unknown";
      var savedUrl = urlSetting && urlSetting.value ? String(urlSetting.value) : "";
      var presets = (sview.reply.providers || []).slice();
      if (fallback) presets.unshift({ id: "same", name: "Same provider as the main model", url: "", needs_key: false,
        help: "The fallback model is asked at the main provider: a different model there, for when this one is limited." });
      var card = el("div", "card");
      var title = el("h4", "", fallback ? "Fallback model" : "Main model");
      var isSet = modelSetting && modelSetting.value;
      title.appendChild(el("span", "badge " + (isSet ? "on" : "off"), isSet ? "set" : "off"));
      card.appendChild(title);
      card.appendChild(el("p", "lead", fallback
        ? "Used when the main model is unavailable or limited. A different provider can keep JARVIS working during an outage; conversation content is sent to that provider."
        : "Choose the provider and model JARVIS uses for conversations. Save & test saves the setup, then checks the model."));

      var select = el("select");
      select.id = "settings-model-" + slot + "-provider";
      presets.forEach(function (p) { var option = el("option", "", p.name); option.value = p.id; select.appendChild(option); });
      var address = el("input");
      address.id = "settings-model-" + slot + "-address";
      address.type = "text";
      address.spellcheck = false;
      var keyBox = el("input");
      keyBox.id = "settings-model-" + slot + "-key";
      keyBox.type = "password";
      keyBox.autocomplete = "off";
      keyBox.spellcheck = false;
      keyBox.setAttribute("autocapitalize", "none");
      var model = el("input");
      model.id = "settings-model-" + slot + "-name";
      model.type = "text";
      model.autocomplete = "off";
      model.spellcheck = false;
      model.value = modelSetting && modelSetting.value ? String(modelSetting.value) : "";
      model.placeholder = "choose from the list, or type a model name";
      var picker = modelPicker(model);
      var providerHelp = el("span", "help");
      var status = el("div", "help");

      var current = savedUrl ? providerFor(savedUrl) : (fallback ? presets[0] : providerFor(""));
      select.value = current.id;
      address.value = savedUrl;
      function chosen() { return presets.filter(function (p) { return p.id === select.value; })[0]; }
      function isSame() { return select.value === "same"; }
      function addressChanged() { return normUrl(address.value) !== normUrl(savedUrl); }
      function refreshFields() {
        var p = chosen();
        address.disabled = isSame();
        address.placeholder = p.id === "custom" ? "the provider's address, ending in /v1" : "";
        keyBox.disabled = isSame() || !p.needs_key;
        keyBox.placeholder = isSame() ? "uses the main key" : !p.needs_key ? "no key needed" :
          (keyState === "set" && !addressChanged() ? "set; paste a new key to replace it" : "paste the key for this provider");
        providerHelp.textContent = p.help || "";
        // With "same provider" there is no address or key of its own to show.
        if (addressRow) addressRow.hidden = isSame();
        if (keyRowNode) keyRowNode.hidden = isSame() || !p.needs_key;
      }
      var addressRow = null, keyRowNode = null;
      function say(text, bad) { status.textContent = text; status.className = "help " + (bad ? "bad" : ""); }
      select.addEventListener("change", function () {
        var p = chosen();
        keyBox.value = "";
        if (p.id !== "custom") address.value = p.url;
        if (p.id === "custom" && !addressChanged()) address.value = "";
        drafts[draftKey(address)] = address.value;
        picker.setModels([], false);
        say("");
        refreshFields();
      });
      address.addEventListener("input", function () {
        var p = providerFor(address.value);
        if (!isSame() && p) select.value = p.id;
        drafts[draftKey(select)] = select.value;
        refreshFields();
      });
      refreshFields();

      function ask(test) {
        var body = { slot: slot };
        if (!isSame()) body.base_url = address.value.trim();
        if (keyBox.value.trim()) body.key = keyBox.value.trim();
        if (test) {
          if (!model.value.trim()) { say("Choose or type a model to test.", true); return; }
          body.test_model = model.value.trim();
        }
        keyBox.value = "";
        say(test ? "Asking the model..." : "Asking the provider for its models...");
        putJson("/settings/models", "POST", body).then(function (reply) {
          picker.setModels(reply.models || [], true);
          var parts = [];
          if ((reply.models || []).length) parts.push((reply.models.length) + " models found: choose one from the list in the model box.");
          else if (reply.list_problem) parts.push("No list: " + reply.list_problem + " You can still type a model name.");
          if (reply.test) parts.push((reply.test.ok ? "Test passed in " + (reply.test.millis / 1000).toFixed(1) + " s. " : "Test failed: ") + reply.test.message);
          say(parts.join(" "), (reply.test && !reply.test.ok) || (!reply.models.length && !!reply.list_problem && !reply.test));
        }, function (error) { say(explain(error, "Could not ask the provider."), true); });
      }
      var load = el("button", "", "Load models");
      load.addEventListener("click", function () { ask(false); });
      var test = el("button", "", "Test");
      test.addEventListener("click", function () { ask(true); });
      var save = el("button", "", "Save");
      function saveProvider(check) {
        var p = chosen(), typed = keyBox.value.trim(), name = model.value.trim();
        if (!name) { say("Choose or type a model first.", true); return; }
        var changes = [{ key: modelKey, values: [name] }];
        if (isSame()) { if (savedUrl) changes.push({ key: urlKey, values: null }); }
        else {
          if (!address.value.trim()) { say("Give the provider's address first.", true); return; }
          changes.push({ key: urlKey, values: [address.value.trim()] });
          // The key on file belongs to the address it was saved for: never leave it standing for a different provider.
          if (p.needs_key && !typed && (addressChanged() || keyState !== "set")) { say("Paste the key for " + p.name + " first.", true); return; }
        }
        var payload = { slot: slot, model: name, base_url: isSame() ? null : address.value.trim() };
        if (!isSame() && typed && p.needs_key) payload.key = typed;
        else if (!isSame() && !p.needs_key && (addressChanged() || keyState !== "set")) payload.key = "ollama";
        keyBox.value = "";
        save.disabled = true;
        var submittedDrafts = [[select, select.value], [address, address.value], [model, model.value]];
        putJson("/settings/provider", "PUT", payload).then(function () {
          submittedDrafts.forEach(function (entry) { acknowledgeDraft(entry[0], entry[1]); });
          saved(fallback ? "Fallback model" : "Main model");
          if (check) putJson("/settings/models", "POST", { slot: slot, test_model: name }).then(function (reply) {
            settingsNote("Provider saved. " + (reply.test && reply.test.ok ? "Model check passed. Restart to use it for conversation." : "Model check failed: " + (reply.test ? reply.test.message : "No test result.")), !(reply.test && reply.test.ok));
          }, function (error) { settingsNote(explain(error, "Provider saved, but its model check failed. Open Brain to retry."), true); });
        }, function (error) { save.disabled = false; say(explain(error, "That was refused; your previous provider is unchanged."), true); });
      }
      save.addEventListener("click", function () { saveProvider(false); });
      var saveTest = el("button", "yes", "Save & test");
      saveTest.addEventListener("click", function () { if (!save.disabled) saveProvider(true); });
      var acts = el("div", "acts provider-actions");
      acts.appendChild(load); acts.appendChild(test); acts.appendChild(save); acts.appendChild(saveTest);

      addressRow = providerRow("Provider address", address);
      keyRowNode = providerRow("API key" + (keyState === "set" ? " (saved, hidden)" : ""), keyBox, null,
        helpNode("Write-only. Loading or testing clears a typed key; enter it again before saving. Never stored in browser preferences."));
      card.appendChild(providerRow("Provider", select, null, providerHelp));
      card.appendChild(addressRow);
      card.appendChild(keyRowNode);
      card.appendChild(providerRow("Model name", picker.element, null, status, model));
      card.appendChild(acts);
      select.restoreDraft = refreshFields;
      refreshFields();
      return card;
    }
    // The folders JARVIS may touch: a list to add to and remove from, each change saved as the whole list.
    function foldersCard() {
      var setting = byKey("daemon.tool_workspace_roots");
      var roots = isSetting(setting) ? String(setting.value).split(", ").filter(Boolean) : [];
      var card = el("div", "card");
      var title = el("h4", "", "Folders");
      title.appendChild(el("span", "badge " + (roots.length ? "on" : "off"), roots.length ? roots.length + (roots.length === 1 ? " folder" : " folders") : "off"));
      card.appendChild(title);
      card.appendChild(el("p", "lead", roots.length
        ? "JARVIS may read and write files inside these folders, and nowhere else. It asks before it changes or deletes anything."
        : "Off. With no folder granted JARVIS has no file tools at all, so nothing outside a folder you choose is ever reachable. Add the folders you want it to work in, for example your notes or a project."));
      function store(list) {
        var request = list.length ? putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: list })
          : putJson("/settings/" + encodeURIComponent(setting.key), "DELETE");
        request.then(function () { saved("Folders"); }, function (error) { settingsNote(explain(error, "That folder was refused."), true); });
      }
      roots.forEach(function (root) {
        var line = el("div", "pathrow");
        line.appendChild(el("code", "", root));
        var remove = el("button", "danger", "Remove");
        remove.addEventListener("click", function () { store(roots.filter(function (other) { return other !== root; })); });
        line.appendChild(remove);
        card.appendChild(line);
      });
      var add = el("div", "pathrow");
      var field = el("input");
      field.type = "text"; field.autocomplete = "off"; field.spellcheck = false;
      field.placeholder = "Full path, e.g. " + (setting.example || "C:/Users/me/notes").split(";")[0].trim();
      var pathLabel = el("label", "help", "Folder path");
      describeControl(field, pathLabel);
      card.appendChild(pathLabel);
      var go = el("button", "yes", "Add folder");
      function addIt() {
        var path = field.value.trim();
        if (!path) { settingsNote("Type the full path of a folder first.", true); return; }
        if (roots.indexOf(path) >= 0) { settingsNote("That folder is already on the list.", true); return; }
        store(roots.concat([path]));
      }
      go.addEventListener("click", addIt);
      field.addEventListener("keydown", function (event) { if (event.key === "Enter") addIt(); });
      add.appendChild(field); add.appendChild(go);
      card.appendChild(add);
      return card;
    }
    function codeCard() {
      var image = byKey("daemon.code_sandbox_image"), command = byKey("daemon.code_sandbox_interpreter");
      var card = el("div", "card");
      var title = el("h4", "", "Code sandbox");
      title.appendChild(el("span", "badge " + (image && image.value ? "on" : "off"), image && image.value ? "on" : "off"));
      card.appendChild(title);
      card.appendChild(el("p", "lead", image && image.value ? "JARVIS can run code in a disposable container with no network."
        : "Off. Lets JARVIS run small pieces of code to calculate or transform things, in a throwaway container with no network and no access to your files."));
      var needs = el("details");
      if (!(image && image.value)) needs.open = true;
      needs.appendChild(el("summary", "", "What it needs"));
      var steps = el("ol");
      ["Docker running on this computer (Docker Desktop on Windows and macOS).",
       "An image you pulled yourself, for example by running docker pull node:22-alpine in a terminal. JARVIS never pulls one for you.",
       "Name the image and the command that runs code inside it, below, or press a preset."].forEach(function (text) { steps.appendChild(el("li", "", text)); });
      needs.appendChild(steps);
      card.appendChild(needs);
      var presets = el("div", "acts");
      var row = el("div", "set-row");
      var imageLabel = el("label", "name", "Sandbox image");
      row.appendChild(imageLabel);
      var imageField = el("input");
      imageField.type = "text"; imageField.value = showFor(image); imageField.placeholder = "e.g. " + image.example;
      describeControl(imageField, imageLabel);
      row.appendChild(imageField);
      row.appendChild(el("span"));
      var row2 = el("div", "set-row");
      var commandLabel = el("label", "name", "Interpreter command");
      row2.appendChild(commandLabel);
      var commandField = el("input");
      commandField.type = "text"; commandField.value = showFor(command); commandField.placeholder = "e.g. " + command.example;
      describeControl(commandField, commandLabel);
      [["Node", "node:22-alpine", "node -e"], ["Python", "python:3.13-alpine", "python -c"]].forEach(function (preset) {
        var button = el("button", "", "Use " + preset[0]);
        button.addEventListener("click", function () { imageField.value = preset[1]; commandField.value = preset[2]; });
        presets.appendChild(button);
      });
      card.appendChild(presets);
      row2.appendChild(commandField);
      var acts = el("div", "acts");
      var save = el("button", "", "Save sandbox");
      save.addEventListener("click", function () {
        if (!imageField.value.trim() || !commandField.value.trim()) { settingsNote("Give both the image and the command.", true); return; }
        putJson("/settings", "PUT", { changes: [
          { key: "daemon.code_sandbox_image", values: [imageField.value.trim()] },
          { key: "daemon.code_sandbox_interpreter", values: [commandField.value.trim()] }
        ] }).then(function () { saved("Code sandbox"); }, function (error) { settingsNote(explain(error, "That was refused."), true); });
      });
      acts.appendChild(save);
      if (image && image.value) {
        var off = el("button", "", "Turn off");
        off.addEventListener("click", function () {
          putJson("/settings", "PUT", { changes: [
            { key: "daemon.code_sandbox_image", values: null }, { key: "daemon.code_sandbox_interpreter", values: null }
          ] }).then(function () { saved("Code sandbox turned off;"); }, function (error) { settingsNote(explain(error, "Could not turn it off."), true); });
        });
        acts.appendChild(off);
      }
      row2.appendChild(acts);
      card.appendChild(row);
      card.appendChild(row2);
      return card;
    }
    // The coding agent: which one JARVIS hands a bigger coding task to, tested from here. Saves apply at once (no restart): the daemon reads
    // the setting each time a task starts. Nothing here runs a task; Test only starts the agent in an empty folder and says what it found.
    function codingAgentCard() {
      var card = el("div", "card");
      var title = el("h4", "", "Coding agent");
      var badge = el("span", "badge off", "off");
      title.appendChild(badge);
      card.appendChild(title);
      card.appendChild(el("p", "lead", "Hands a bigger coding task to an agent you already use (ChatGPT/Codex, Claude, Copilot, Gemini and others). It edits the folder permitted above in Settings, using the first folder by default. No Git repository, branch or automatic commit is needed. Commands and project dependency setup are enabled for the approved task. Jobs queue without an overall time limit unless you select one; Stop remains available. Isolated worktrees are an explicit option, not the default."));
      var holder = el("div");
      card.appendChild(holder);
      var out = el("p", "help", "");
      function say(text, bad) { out.className = "help" + (bad ? " bad" : ""); out.textContent = text; }
      function store(changes, done) {
        putJson("/settings", "PUT", { changes: changes }).then(function () { settingsNote("Coding agent saved. It applies at once."); loadSettings(); },
          function (error) { say(explain(error, "That was refused."), true); });
      }
      function paint(info) {
        holder.replaceChildren();
        var chosen = info.selected || (info.custom ? "custom" : null);
        var ready = !!(chosen || info.detected);
        badge.className = "badge " + (ready ? "on" : "off");
        badge.textContent = ready ? "ready to choose" : "off";
        if (chosen === "custom") badge.textContent = "custom";
        else if (chosen) badge.textContent = "chosen";
        holder.appendChild(checklist([
          { done: info.needs.folder, text: "A working folder is permitted above; the first is the default." },
          { done: true, text: info.needs.git ? "Git is available for optional worktrees." : "Git is optional; normal in-place coding does not need it." },
          { done: ready, text: chosen ? "An agent is chosen." : info.detected ? "No agent is chosen; JARVIS will use the installed " + info.detected + "." : "Choose an agent below." }
        ]));
        var list = el("div");
        info.presets.forEach(function (preset) {
          var row = el("div", "set-row");
          var name = el("span", "name", preset.name);
          row.appendChild(name);
          var middle = el("div");
          middle.appendChild(el("span", "help", preset.summary));
          middle.appendChild(el("span", "help" + (preset.available ? "" : " bad"), " " + (preset.via === "installed" ? "Found on this computer."
            : preset.via === "npx" ? "Not installed; it will be downloaded with npx the first time (needs Node)."
            : "Not found: install it" + (info.needs.node ? "" : ", or install Node so it can be run with npx") + ".")));
          row.appendChild(middle);
          var acts = el("div", "acts");
          var use = el("button", preset.selected ? "" : "yes", preset.selected ? "In use" : "Use");
          use.type = "button";
          use.disabled = !!preset.selected || !preset.available;
          use.addEventListener("click", function () { use.disabled = true; store([{ key: "daemon.code_agent", values: preset.command }]); });
          acts.appendChild(use);
          row.appendChild(acts);
          list.appendChild(row);
        });
        holder.appendChild(list);
        var picked = info.presets.filter(function (preset) { return preset.selected; })[0];
        if (picked) holder.appendChild(el("p", "lead", "To sign in: " + picked.sign_in));
        // Any other agent that speaks ACP.
        var more = el("details");
        more.open = chosen === "custom";
        more.appendChild(el("summary", "", "Another agent (a command)"));
        var customRow = el("div", "set-row");
        customRow.appendChild(el("span", "name", "Command"));
        var customField = el("input");
        customField.type = "text"; customField.autocomplete = "off"; customField.spellcheck = false;
        customField.setAttribute("aria-label", "Coding agent command");
        customField.value = chosen === "custom" ? (info.command || []).join(" ") : "";
        customField.placeholder = "e.g. npx -y @github/copilot@1.0.96 --acp";
        customRow.appendChild(customField);
        var customActs = el("div", "acts");
        var customSave = el("button", "", "Save command");
        customSave.type = "button";
        customSave.addEventListener("click", function () {
          var words = splitCommand(customField.value.trim());
          if (!words.length) { say("Type the command that starts the agent in ACP mode.", true); return; }
          store([{ key: "daemon.code_agent", values: words }]);
        });
        customActs.appendChild(customSave);
        customRow.appendChild(customActs);
        more.appendChild(customRow);
        more.appendChild(el("p", "secthint", "Any agent that speaks the Agent Client Protocol over standard input and output works. It is started with this command, no shell, in your Settings-permitted working folder."));
        holder.appendChild(more);
        var buttons = el("div", "acts");
        var test = el("button", "yes", "Test");
        test.type = "button";
        var signIns = el("div", "acts");
        test.addEventListener("click", function () {
          test.disabled = true; signIns.replaceChildren();
          say("Starting the agent (a first start through npx may download it)...");
          putJson("/code-agents/test", "POST", {}).then(function (reply) {
            test.disabled = false;
            say((reply.ok ? "\u2713 " : "") + (reply.message || "No answer."), !reply.ok);
            if (!(reply.methods || []).length) return;
            (reply.methods || []).forEach(function (method) {
              var button = el("button", "", "Sign in with " + method.name);
              button.type = "button";
              if (method.description) button.title = method.description;
              button.addEventListener("click", function () {
                signIns.querySelectorAll("button").forEach(function (other) { other.disabled = true; });
                say("Signing in. A page may open in your browser; this waits up to 5 minutes...");
                putJson("/code-agents/sign-in", "POST", { method_id: method.id }).then(function (done) {
                  say((done.ok ? "\u2713 " : "") + (done.message || "No answer."), !done.ok);
                  signIns.querySelectorAll("button").forEach(function (other) { other.disabled = false; });
                  if (done.ok) { signIns.replaceChildren(); loadSettings(); }
                }, function (error) { signIns.querySelectorAll("button").forEach(function (other) { other.disabled = false; }); say(explain(error, "The sign-in failed."), true); });
              });
              signIns.appendChild(button);
            });
          }, function (error) { test.disabled = false; say(explain(error, "The test failed."), true); });
        });
        buttons.appendChild(test);
        if (chosen) {
          var automatic = el("button", "", "Choose automatically");
          automatic.type = "button";
          automatic.title = "Stop using the saved choice; JARVIS then looks for an installed Copilot or Gemini.";
          automatic.addEventListener("click", function () { store([{ key: "daemon.code_agent", values: null }, { key: "daemon.code_agent_auth", values: null }]); });
          buttons.appendChild(automatic);
        }
        holder.appendChild(buttons);
        holder.appendChild(signIns);
        holder.appendChild(out);
      }
      api("/code-agents").then(paint, function () { holder.appendChild(el("p", "help bad", "The coding agents could not be listed.")); });
      var activity = el("button", "", "View coding tasks in Activity");
      activity.addEventListener("click", function () { closeSettings(); if (context.openActivity) context.openActivity(); });
      card.appendChild(activity);
      return card;
    }
    function mountCodingTasks(parent, isVisible) {
      var card = el("div", "coding-tasks");
      var taskHeader = el("div", "code-task-header");
      taskHeader.appendChild(el("h4", "", "Background coding tasks"));
      var refreshTasks = el("button", "", "\u21bb");
      refreshTasks.type = "button";
      refreshTasks.title = "Refresh coding tasks";
      refreshTasks.setAttribute("aria-label", "Refresh coding tasks");
      taskHeader.appendChild(refreshTasks);
      card.appendChild(taskHeader);
      var taskNotice = el("p", "help");
      taskNotice.setAttribute("role", "status");
      card.appendChild(taskNotice);
      var tasks = el("div");
      card.appendChild(tasks);
      var emptyTasks = el("p", "help", "No background coding tasks.");
      card.appendChild(emptyTasks);
      var taskViews = Object.create(null);
      function taskReview(task) {
        var review = el("details", "code-task");
        var summary = el("summary");
        var name = el("span", "name", task.repository);
        var status = el("span", "badge");
        var objective = el("span", "code-task-objective", task.task);
        summary.appendChild(name);
        summary.appendChild(document.createTextNode(" "));
        summary.appendChild(status);
        summary.appendChild(objective);
        review.appendChild(summary);
        var body = el("div", "code-task-body");
        body.appendChild(el("h5", "", "Agent-reported activity"));
        var activity = el("ol", "code-task-activity");
        activity.setAttribute("aria-label", "Agent-reported activity");
        activity.setAttribute("aria-live", "polite");
        activity.setAttribute("aria-relevant", "additions");
        var activityNotice = el("p", "help");
        body.appendChild(activity);
        body.appendChild(activityNotice);
        var resultBody = el("div");
        body.appendChild(resultBody);
        review.appendChild(body);
        var activityViews = Object.create(null), resultSignature = null;
        function paintActivity(task) {
          var entries = task.activity || [], present = Object.create(null);
          entries.forEach(function (entry) {
            var key = String(entry.sequence);
            present[key] = true;
            var row = activityViews[key];
            if (!row) {
              var line = el("li", "code-task-event");
              var time = el("time");
              var text = el("pre");
              line.appendChild(time); line.appendChild(text);
              activity.appendChild(line);
              row = activityViews[key] = { node: line, time: time, text: text };
            }
            if (row.text.textContent !== entry.summary) row.text.textContent = entry.summary;
            if (row.time.getAttribute("datetime") !== entry.created_at) {
              var created = new Date(entry.created_at);
              row.time.textContent = isNaN(created.getTime()) ? "" : created.toLocaleTimeString();
              row.time.setAttribute("datetime", entry.created_at || "");
              row.time.title = entry.created_at || "";
            }
          });
          Object.keys(activityViews).forEach(function (key) {
            if (!present[key]) { activityViews[key].node.remove(); delete activityViews[key]; }
          });
          activity.hidden = !entries.length;
          activityNotice.className = "help" + (task.activity_limited ? " bad" : "");
          activityNotice.textContent = task.activity_limited ? "Some activity was not retained." : entries.length ? "" : "No tool activity reported yet.";
        }
        function paintResult(task) {
          resultBody.replaceChildren();
          var metadata = el("dl", "code-task-meta");
          resultBody.appendChild(metadata);
          function field(name, value) {
            if (value === null || value === undefined || value === "") return;
            metadata.appendChild(el("dt", "", name));
            metadata.appendChild(el("dd", "", String(value)));
          }
          function evidence(name, value) {
            var text = Array.isArray(value) ? value.join("\n") : typeof value === "string" ? value : "";
            if (!text) return;
            var section = el("details", "code-evidence");
            section.appendChild(el("summary", "", name));
            section.appendChild(el("pre", "", text));
            resultBody.appendChild(section);
          }
          var result = task.result || {};
          field("Task", task.task_id);
          field("Working folder", result.directory || task.repository);
          field("Mode", task.mode || result.mode || "Legacy task");
          field("Overall deadline", task.minutes ? task.minutes + " minutes" : "None (owner can stop)");
          field("Branch", task.branch || result.branch);
          field("Worktree", task.worktree || result.worktree);
          field("Created", task.created_at);
          field("Finished", task.finished_at);
          field("Commands", task.allow_commands === undefined ? "Not recorded" : task.allow_commands ? "Allowed" : "Not allowed");
          field("Agent outcome", result.outcome);
          field("Delivery", result.delivery_status || (task.result ? "Not independently verified" : ""));
          field("Base commit", result.base_commit);
          field("Commit", result.commit);
          field("Files changed", result.files_changed);
          if (task.result) field("Verification", result.verification ? result.verification.status : "Not independently verified");
          if (task.result) field("Website browser check", "Separate desktop/mobile inspection required");
          if (result.verification) evidence("Independent checks", JSON.stringify(result.verification, null, 2));
          if (result.permissions) field("Permissions", result.permissions.allowed + " allowed, " + result.permissions.refused + " refused");
          [task.error, result.detail, result.result_unreadable, result.activity_error].concat(result.notes || []).forEach(function (note) {
            if (note) resultBody.appendChild(el("p", "help bad", note));
          });
          if (!task.result) resultBody.appendChild(el("p", "help", task.status === "interrupted" ? "Interrupted; no final result was recorded." : "No result has been recorded yet."));
          evidence("Agent report", result.agent_message);
          evidence("Agent activity", result.agent_tool_calls);
          evidence("Permission decisions", result.agent_requests);
          evidence("Changed files", result.stat);
          if (result.patch_was_cut) resultBody.appendChild(el("p", "help bad", "Diff preview truncated."));
          evidence("Diff", result.patch);
          if (result.next) resultBody.appendChild(el("p", "help", result.next));
        }
        function update(task) {
          if (name.textContent !== task.repository) name.textContent = task.repository;
          if (objective.textContent !== task.task) objective.textContent = task.task;
          status.className = "badge " + (task.status === "completed" && task.result && task.result.verification && task.result.verification.status === "passed" ? "on" : "off");
          status.textContent = task.status === "completed" && (!task.result || !task.result.verification || task.result.verification.status !== "passed")
            ? "Finished, unverified" : String(task.status || "unknown").replace(/_/g, " ");
          paintActivity(task);
          var signature = JSON.stringify([task.task_id, task.branch, task.worktree, task.created_at, task.finished_at, task.allow_commands, task.status, task.error, task.result]);
          if (signature !== resultSignature) { paintResult(task); resultSignature = signature; }
        }
        return { node: review, update: update };
      }
      function paintTasks(reply) {
        var found = reply.tasks || [], present = Object.create(null);
        found.forEach(function (task) { present[task.task_id] = true; });
        Object.keys(taskViews).forEach(function (key) {
          if (!present[key]) { taskViews[key].node.remove(); delete taskViews[key]; }
        });
        emptyTasks.hidden = !!found.length;
        found.forEach(function (task, index) {
          var view = taskViews[task.task_id];
          if (!view) {
            view = taskViews[task.task_id] = taskReview(task);
            tasks.insertBefore(view.node, tasks.children[index] || null);
          }
          view.update(task);
        });
        activeTasks = found.some(function (task) { return task.status === "queued" || task.status === "running"; });
      }
      var pollTimer = null, activeTasks = false;
      function tasksVisible() {
        return isVisible() && parent.contains(card);
      }
      function scheduleTasks() {
        if (pollTimer !== null) clearTimeout(pollTimer);
        pollTimer = null;
        if (activeTasks && tasksVisible()) pollTimer = setTimeout(function () {
          pollTimer = null;
          if (tasksVisible()) loadTasks(true);
        }, 2000);
      }
      function loadTasks(automatic) {
        if (refreshTasks.disabled) return;
        if (pollTimer !== null) clearTimeout(pollTimer);
        pollTimer = null;
        refreshTasks.disabled = true;
        taskNotice.className = "help";
        if (!automatic) taskNotice.textContent = "Loading coding tasks...";
        api("/code-agents/tasks").then(function (reply) {
          refreshTasks.disabled = false;
          if (!tasksVisible()) return;
          paintTasks(reply);
          taskNotice.textContent = "";
          scheduleTasks();
        }, function () {
          refreshTasks.disabled = false;
          if (!tasksVisible()) return;
          taskNotice.className = "help bad";
          taskNotice.textContent = "Coding tasks could not be loaded.";
          scheduleTasks();
        });
      }
      refreshTasks.addEventListener("click", function () { loadTasks(false); });
      parent.appendChild(card);
      loadTasks(false);
      return function () { if (pollTimer !== null) clearTimeout(pollTimer); card.remove(); };
    }
    var POSTURES = [["default", "Default (the rules decide)"], ["ask", "Always ask first"], ["trusted", "Run without asking"], ["off", "Off"]];
    var permFilter = { text: "", changedOnly: false };
    function permissionsPanel(into) {
      var postures = (sview.reply && sview.reply.postures) || {};
      if (!sview.tools.length) { into.appendChild(el("p", "lead", "No tools are available to configure yet.")); return; }
      var changedCount = sview.tools.filter(function (tool) { return postures[tool.id] && postures[tool.id] !== "default"; }).length;
      var bar = el("div", "permtools");
      var search = el("input");
      search.type = "search"; search.placeholder = "Find a tool, for example mail or browser"; search.value = permFilter.text; search.autocomplete = "off";
      var searchLabel = el("label", "help", "Find a tool");
      describeControl(search, searchLabel);
      bar.appendChild(searchLabel);
      var only = el("input"); only.type = "checkbox"; only.checked = permFilter.changedOnly;
      var onlyLabel = el("label", "", " Only the ones I changed (" + changedCount + ")");
      onlyLabel.insertBefore(only, onlyLabel.firstChild);
      bar.appendChild(search); bar.appendChild(onlyLabel);
      into.appendChild(bar);
      var list = el("div");
      into.appendChild(list);
      into.appendChild(el("p", "lead", "Every tool is yours to set. Tools that send things to other people ask by default, and choosing \u201CRun without asking\u201D for one of them asks you to confirm once. Nothing runs above the workspace ceiling, whatever is chosen here."));
      function matches(tool) {
        var needle = permFilter.text.trim().toLowerCase();
        if (permFilter.changedOnly && !(postures[tool.id] && postures[tool.id] !== "default")) return false;
        if (!needle) return true;
        return ((tool.title || "") + " " + tool.id + " " + (tool.group || "")).toLowerCase().indexOf(needle) >= 0;
      }
      function draw() {
        list.replaceChildren();
        var shown = 0;
        families(sview.tools).forEach(function (group) {
          var tools = group.tools.filter(matches);
          if (!tools.length) return;
          shown += tools.length;
          var changed = tools.filter(function (tool) { return postures[tool.id] && postures[tool.id] !== "default"; }).length;
          var details = el("details", "permgroup");
          // A group is open while searching or when it holds something changed; otherwise it is folded to keep the list short.
          details.open = !!permFilter.text.trim() || permFilter.changedOnly || changed > 0 || sview.tools.length <= 12;
          var summary = el("summary");
          var label = familyLabel(group);
          var count = el("span", "count" + (changed ? " changed" : ""), tools.length + (tools.length === 1 ? " tool" : " tools") + (changed ? ", " + changed + " changed" : ""));
          label.appendChild(count);
          summary.appendChild(label);
          details.appendChild(summary);
          tools.forEach(function (tool) { details.appendChild(permissionRow(tool, postures)); });
          list.appendChild(details);
        });
        if (!shown) list.appendChild(el("p", "help", "No tool matches."));
      }
      search.addEventListener("input", function () { permFilter.text = search.value; draw(); });
      only.addEventListener("change", function () { permFilter.changedOnly = only.checked; draw(); });
      draw();
    }
    function permissionRow(tool, postures) {
      var row = el("div", "perm");
      var label = el("div");
      var name = el("label", "tname", tool.title || tool.id);
      var detail = el("div", "tid", tool.id + "  \u2022  risk " + tool.risk + (tool.asks_first ? "  \u2022  asks by default" : "  \u2022  runs by default"));
      label.appendChild(name);
      label.appendChild(detail);
      var select = el("select");
      POSTURES.forEach(function (option) {
        var node = el("option", "", option[1]);
        node.value = option[0];
        select.appendChild(node);
      });
      select.value = postures[tool.id] || "default";
      describeControl(select, name, detail);
      // A saved "run without asking" that the rules still hold must say so, rather than look ignored.
      if (select.value === "trusted" && tool.asks_first) label.appendChild(el("div", "tid bad", "still asks: a stricter rule applies to it"));
      var outward = (tool.effects || []).indexOf("external_communication") >= 0;
      select.addEventListener("change", function () {
        // Letting a tool that reaches other people run unasked is allowed, but it is the one choice worth a second look: a hostile
        // page or email can try to steer it.
        if (select.value === "trusted" && outward && !window.confirm("Let \"" + (tool.title || tool.id) + "\" run without asking you?\n\nIt sends things to other people or the web. A hostile page or email could try to make JARVIS use it. You can set it back to \"Always ask first\" at any time.")) {
          select.value = postures[tool.id] || "default";
          return;
        }
        putJson("/settings/tools/" + encodeURIComponent(tool.id), "PUT", { posture: select.value }).then(function (reply) { if (reply && reply.applied) { settingsNote(tool.title + ": saved and applied now."); loadSettings(); } else saved(tool.title + ":"); },
          function (error) { settingsNote(explain(error, "That was refused."), true); loadSettings(); });
      });
      row.appendChild(label);
      row.appendChild(select);
      return row;
    }
    // ---- Google: set up once, then one button -----------------------------------------------------------------------------------
    // Google needs an OAuth client the owner makes in their own Google Cloud project (a few minutes, once). After that, signing in is
    // a button: the browser goes to Google, Google returns to this daemon, and the status below turns to "Connected".
    var googlePoll = 0;
    function googlePanel(body) {
      var idSetting = byKey("daemon.google_client_id");
      var configured = settingIsSet("daemon.google_client_id") && keyIsSet("google");
      body.appendChild(section("Account"));
      var card = el("div", "card");
      var line = el("p", "lead", "Checking...");
      var acts = el("div", "acts");
      card.appendChild(line);
      card.appendChild(acts);
      body.appendChild(card);
      body.appendChild(section("Your Google client", "JARVIS signs in through an OAuth client you make yourself in Google Cloud, so nothing about your account passes through anyone else."));
      body.appendChild(stepsCard("One-time setup in Google Cloud (about 10 minutes)", [
        "Open console.cloud.google.com, create a project, and enable the Gmail API and the Google Calendar API.",
        "In Google Auth platform, set up the consent screen (External is fine), add your own Google address as a test user, and press Publish app (otherwise Google ends the sign-in after 7 days).",
        "Create credentials, OAuth client ID, application type Desktop app. Copy the client ID and the client secret into the two fields below.",
        "Press Sign in with Google above. Google warns that the app is unverified: that is expected for your own client; choose Continue."
      ], configured, "Details and the reasoning are in docs/user/google.md. Nothing is sent to Google until you press Sign in."));
      if (idSetting) body.appendChild(settingRow(idSetting));
      body.appendChild(keyRow("google", "Client secret"));
      var actionsSetting = byKey("daemon.google_actions");
      if (actionsSetting) {
        body.appendChild(section("What JARVIS may do", "Reading mail and calendar is on once you sign in. Saving a draft never asks."));
        body.appendChild(toggleRow(actionsSetting));
      }
      function paint(status) {
        acts.replaceChildren();
        if (status.connected) {
          line.textContent = "Connected as " + (status.email || "your Google account") + ". Restart once if the mail and calendar tools are not listed yet."
            + (status.actions && !status.can_act ? " Some action permissions (including moving mail to Trash) are not granted yet: sign in again. Existing granted actions still work." : "")
            + (status.actions && status.can_act ? " JARVIS may send mail, move mail to Trash and change events. Policy asks first unless you explicitly trust the tool. Drafts send nothing and do not ask." : "");
          if (status.actions && !status.can_act) {
            var again = el("button", "yes", "Sign in again to grant it");
            again.addEventListener("click", function () {
              again.disabled = true;
              putJson("/google/connect", "POST").then(function (reply) { window.open(reply.auth_url, "_blank", "noopener"); waitForGoogle(); },
                function (error) { settingsNote(explain(error, "Could not start the sign-in."), true); again.disabled = false; });
            });
            acts.appendChild(again);
          }
          var out = el("button", "danger", "Disconnect");
          out.addEventListener("click", function () {
            out.disabled = true;
            putJson("/google/disconnect", "POST").then(function () { settingsNote("Google disconnected."); loadGoogle(); },
              function (error) { settingsNote(explain(error, "Could not disconnect."), true); out.disabled = false; });
          });
          acts.appendChild(out);
        } else if (!status.configured) {
          line.textContent = "Not set up yet: save the client ID and client secret below first.";
        } else {
          line.textContent = status.pending ? "Waiting for you to finish in the Google tab..." : "Not connected.";
          var go = el("button", "yes", status.pending ? "Open Google again" : "Sign in with Google");
          go.addEventListener("click", function () {
            go.disabled = true;
            putJson("/google/connect", "POST").then(function (reply) {
              window.open(reply.auth_url, "_blank", "noopener");
              settingsNote("Finish signing in in the Google tab. This page notices by itself.");
              waitForGoogle();
            }, function (error) { settingsNote(explain(error, "Could not start the sign-in."), true); go.disabled = false; });
          });
          acts.appendChild(go);
        }
      }
      function loadGoogle() {
        api("/google").then(paint, function () { line.textContent = "Google sign-in is not available on this daemon."; });
      }
      function waitForGoogle() {
        clearInterval(googlePoll);
        var tries = 0;
        googlePoll = setInterval(function () {
          tries += 1;
          if (tries > 90 || $("settings").hidden || sview.tab !== "google") { clearInterval(googlePoll); return; }
          api("/google").then(function (status) {
            if (status.connected) { clearInterval(googlePoll); settingsNote("Google is connected."); paint(status); }
            else paint(status);
          }, function () {});
        }, 2000);
      }
      loadGoogle();
    }
    function connectionsPanel(body) {
      var card = el("div", "card"), current = {}, dirty = false, pending = false, loaded = false, polls = 0, statusEpoch = 0;
      card.appendChild(el("h4", "", "GitHub"));
      card.appendChild(el("p", "help", "Sign in in your browser; your connection is saved on this computer. No PAT, Docker or server command is needed. This connection exposes read-only GitHub tools."));
      var line = el("p", "lead", "Checking the saved connection..."); line.setAttribute("role", "status"); card.appendChild(line);
      var acts = el("div", "acts"), connect = el("button", "yes", "Connect GitHub"), test = el("button", "", "Test GitHub tools"), disconnect = el("button", "danger", "Disconnect GitHub"), refresh = el("button", "", "Check connection");
      [connect, test, disconnect, refresh].forEach(function (button) { button.type = "button"; acts.appendChild(button); }); card.appendChild(acts);
      var signIn = el("div", "acts"), out = el("p", "help", ""); out.setAttribute("role", "status"); card.appendChild(signIn); card.appendChild(out);
      var setup = el("details"); setup.appendChild(el("summary", "", "One-time JARVIS app setup"));
      var appHelp = el("p", "help", "Register one GitHub OAuth app for JARVIS, then save its client ID and secret here once. Later, just press Connect GitHub. The secret is write-only; it is never returned to this page."); appHelp.id = "github-app-description"; setup.appendChild(appHelp);
      var register = el("a", "", "Register a GitHub OAuth app"); register.href = "https://github.com/settings/applications/new"; register.target = "_blank"; register.rel = "noopener noreferrer"; setup.appendChild(register);
      var callback = el("code", "", "Loading callback address..."); setup.appendChild(el("p", "help", "Use this exact Authorization callback URL when registering the app:")); setup.appendChild(callback);
      function field(label, id, type) {
        var row = el("div", "set-row"), name = el("label", "name", label), input = el("input");
        name.setAttribute("for", id); input.id = id; input.type = type; input.autocomplete = "off"; input.spellcheck = false; input.setAttribute("aria-describedby", "github-app-description");
        row.appendChild(name); row.appendChild(input); setup.appendChild(row); return input;
      }
      var client = field("Client ID", "github-client-id", "text"), secret = field("Client secret", "github-client-secret", "password"), privateRepos = field("Private repositories", "github-private-repositories", "checkbox");
      client.maxLength = 128; secret.maxLength = 4096;
      setup.appendChild(el("p", "help", "Private repositories requests GitHub's broad repo scope, which also grants write capabilities at GitHub. JARVIS still exposes only read-only tools here; signing in never approves an action. Off requests only the basic persistent sign-in."));
      var save = el("button", "", "Save GitHub app"); save.type = "button"; setup.appendChild(save); card.appendChild(setup); body.appendChild(card);
      var google = el("button", "", "Connect Google"); google.type = "button"; google.addEventListener("click", function () { goTab("google"); }); body.appendChild(google);
      function controls() {
        connect.disabled = pending || dirty || !current.configured;
        test.disabled = pending || dirty || !current.connected;
        disconnect.disabled = pending || (!current.connected && !current.pending);
        refresh.disabled = pending; save.disabled = pending;
        client.disabled = pending; secret.disabled = pending; privateRepos.disabled = pending;
      }
      function paint(status) {
        current = status;
        if (!loaded) { if (!dirty) { client.value = status.client_id || ""; privateRepos.checked = !!status.private_repositories; } setup.open = !status.configured || dirty; loaded = true; }
        secret.placeholder = status.client_secret_set ? "Saved; enter a replacement only to change it" : "Paste the app client secret once";
        callback.textContent = status.callback_url || "The daemon has not reported a callback address.";
        line.textContent = status.pending ? "Waiting for GitHub sign-in..." : status.connected ? "Signed in as " + (status.login || "your GitHub account") + " (saved connection). " + (status.restart_needed ? "Restart JARVIS to register its tools." : "Tools were registered at startup; Test checks the connection now.") : status.configured ? "Ready to sign in. Press Connect GitHub." : "Complete the one-time app setup below, then press Connect GitHub.";
        integrationRestart("GitHub tools", !!(status.connected && status.restart_needed));
        controls();
      }
      function active() { return !$("settings").hidden && sview.tab === "connections" && body.contains(card); }
      function load() {
        var epoch = ++statusEpoch;
        return api("/github").then(function (status) { if (active() && epoch === statusEpoch && !pending) paint(status); }, function () { if (active() && epoch === statusEpoch && !pending) { line.textContent = "GitHub sign-in is unavailable. Update the daemon or check its connection."; controls(); } });
      }
      [client, secret].forEach(function (input) { input.addEventListener("input", function () { dirty = true; controls(); }); });
      privateRepos.addEventListener("change", function () { dirty = true; controls(); });
      save.addEventListener("click", function () {
        var value = secret.value; secret.value = "";
        if (!client.value.trim() || (!current.client_secret_set && !value.trim())) { out.textContent = "Enter the app client ID and client secret first."; return; }
        statusEpoch += 1; pending = true; controls(); out.className = "help"; out.textContent = "Saving app setup...";
        putJson("/github/app", "PUT", { client_id: client.value.trim(), client_secret: value.trim() || null, private_repositories: !!privateRepos.checked }).then(function (status) {
          pending = false; dirty = false; paint(status); out.textContent = "App setup saved. Press Connect GitHub to authorize your account.";
        }, function (error) { pending = false; out.className = "help bad"; out.textContent = explain(error, "App setup was not saved. Enter the secret again to retry."); controls(); });
      });
      function poll() {
        if (!active() || pending || dirty || polls++ >= 90) return;
        var epoch = statusEpoch;
        api("/github").then(function (status) { if (!active() || epoch !== statusEpoch || pending || dirty) return; paint(status); if (status.pending) setTimeout(poll, 2000); else if (status.connected) signIn.replaceChildren(); }, function () {});
      }
      connect.addEventListener("click", function () {
        statusEpoch += 1; pending = true; controls(); out.className = "help"; out.textContent = "Starting browser sign-in...";
        putJson("/github/connect", "POST", {}).then(function (reply) {
          pending = false;
          if (!/^https:\/\/github\.com\/login\/oauth\/authorize\?/.test(reply.auth_url || "")) { out.textContent = "The daemon did not provide a valid GitHub sign-in address."; controls(); return; }
          signIn.replaceChildren(); var link = el("a", "", "Continue on GitHub"); link.href = reply.auth_url; link.target = "_blank"; link.rel = "noopener noreferrer"; signIn.appendChild(link);
          if (typeof window.open === "function") window.open(reply.auth_url, "_blank", "noopener,noreferrer");
          out.textContent = "Complete sign-in on GitHub. If no page opened, use Continue on GitHub. Your password stays with GitHub.";
          polls = 0; controls(); poll();
        }, function (error) { pending = false; out.className = "help bad"; out.textContent = explain(error, "Could not start GitHub sign-in."); controls(); });
      });
      test.addEventListener("click", function () {
        statusEpoch += 1; pending = true; controls(); out.className = "help"; out.textContent = "Testing the read-only GitHub connection...";
        putJson("/github/test", "POST", {}).then(function (reply) { pending = false; out.textContent = reply.ok ? "GitHub answered: " + (reply.tools || []).length + " tools. This test does not execute a tool or restart JARVIS." : reply.message || "GitHub did not complete the test."; out.className = reply.ok ? "help" : "help bad"; controls(); }, function (error) { pending = false; out.className = "help bad"; out.textContent = explain(error, "GitHub test failed; check the connection or sign in again."); controls(); });
      });
      disconnect.addEventListener("click", function () {
        statusEpoch += 1; pending = true; controls(); signIn.replaceChildren();
        putJson("/github/disconnect", "POST", {}).then(function (status) { pending = false; paint(status); out.textContent = "GitHub disconnected. The app setup is kept for a later sign-in."; }, function (error) { pending = false; load().then(function () { out.className = "help bad"; out.textContent = explain(error, "Local access was removed, but GitHub revocation may need checking in your authorized apps."); }); });
      });
      refresh.addEventListener("click", load); controls(); load();
    }
    // ---- Telegram: a bot token, a first message to the bot, and the code it answers with --------------------------------------
    // No restart: the daemon reads the token fresh, so the status below turns by itself as each step is done.
    var telegramPoll = 0;
    function telegramPanel(body) {
      var keyState = sview.reply && sview.reply.keys && sview.reply.keys.telegram ? sview.reply.keys.telegram.state : "unknown";
      // The status comes first: it is what a person opens this tab to see. The steps fold away once the bot token is saved.
      body.appendChild(section("Status"));
      var card = el("div", "card");
      var line = el("p", "lead", "Checking...");
      var more = el("div", "acts");
      card.appendChild(line);
      card.appendChild(more);
      body.appendChild(card);
      body.appendChild(section("Your bot"));
      body.appendChild(stepsCard("Setup, once (about 2 minutes)", [
        "In Telegram, open @BotFather and send /newbot. Pick a name; it gives you a token.",
        "Paste the token below and press Save token.",
        "Open your new bot in Telegram and send it any message. It replies with a six digit code.",
        "Enter the code in the Status box above. From then on only your chat is answered; everyone else is ignored."
      ], keyState === "set", "Messages go through Telegram's servers, so treat this chat like any cloud chat. Nothing needs to be opened on your network."));
      var row = el("div", "set-row");
      var name = el("label", "name", "Bot token");
      name.appendChild(el("span", "badge " + (keyState === "set" ? "on" : "off"), keyState));
      var field = el("input");
      field.type = "password";
      field.id = "settings-telegram-token";
      field.autocomplete = "off";
      field.placeholder = keyState === "set" ? "set; paste a new token to replace it (this unpairs the chat)" : "123456:ABC-DEF...";
      var acts = el("div", "acts");
      var save = el("button", "", "Save token");
      save.addEventListener("click", function () {
        var value = field.value;
        field.value = "";
        if (!value.trim()) { settingsNote("Paste the token first.", true); return; }
        putJson("/settings/keys/telegram", "PUT", { key: value }).then(function () {
          settingsNote("Token saved. Now send your bot a message in Telegram.");
          loadSettings();
        }, function (error) { settingsNote(explain(error, "That token was refused."), true); });
      });
      acts.appendChild(save);
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(acts);
      var tokenHelp = el("span", "help", "Stored in a private file on this machine and never shown again.");
      describeControl(field, name, tokenHelp);
      row.appendChild(tokenHelp);
      body.appendChild(row);
      function paint(status) {
        more.replaceChildren();
        var bot = status.bot ? "@" + status.bot : "your bot";
        if (status.state === "paired") {
          line.textContent = "Paired with " + (status.paired_as || "your Telegram account") + " through " + bot + ". Write to the bot as you would here; approvals arrive as buttons.";
          var out = el("button", "danger", "Disconnect this chat");
          out.addEventListener("click", function () {
            out.disabled = true;
            putJson("/telegram/unpair", "POST").then(function () { settingsNote("Chat disconnected."); poll(); },
              function (error) { settingsNote(explain(error, "Could not disconnect."), true); out.disabled = false; });
          });
          more.appendChild(out);
        } else if (status.state === "waiting_for_code") {
          line.textContent = "Your bot sent a six digit code to Telegram. Enter it to finish.";
          var code = el("input");
          code.type = "text";
          code.inputMode = "numeric";
          code.maxLength = 6;
          code.placeholder = "123456";
          code.setAttribute("aria-label", "Telegram pairing code");
          code.autocomplete = "one-time-code";
          var go = el("button", "yes", "Pair");
          function pair() {
            go.disabled = true;
            putJson("/telegram/pair", "POST", { code: code.value }).then(function (reply) {
              settingsNote("Paired with " + reply.paired_as + ".");
              poll();
            }, function (error) { settingsNote(explain(error, "That code was refused."), true); go.disabled = false; });
          }
          go.addEventListener("click", pair);
          code.addEventListener("keydown", function (event) { if (event.key === "Enter") pair(); });
          more.appendChild(code);
          more.appendChild(go);
        } else if (status.state === "waiting_for_message") {
          line.textContent = "Connected to " + bot + ". Now send it a message in Telegram; it replies with a code.";
        } else if (status.state === "error") {
          line.textContent = status.problem || "Telegram is not working.";
        } else {
          line.textContent = keyState === "set" ? "Starting..." : "Not connected. Paste the bot token below.";
        }
        if (keyState === "set" && status.state !== "paired") {
          var remove = el("button", "", "Remove the bot");
          remove.addEventListener("click", function () {
            putJson("/settings/keys/telegram", "DELETE").then(function () { settingsNote("Telegram removed."); loadSettings(); },
              function (error) { settingsNote(explain(error, "Could not remove it."), true); });
          });
          more.appendChild(remove);
        }
      }
      function poll() {
        api("/telegram").then(paint, function () { line.textContent = "Telegram is not available on this daemon."; });
      }
      clearInterval(telegramPoll);
      poll();
      telegramPoll = setInterval(function () {
        if ($("settings").hidden || sview.tab !== "telegram") { clearInterval(telegramPoll); return; }
        poll();
      }, 2000);
    }
    // ---- Phone: the ElevenLabs agent and number, the numbers JARVIS may ring, a test call and the recent calls ---------------
    // These settings apply at once (the daemon reads them on every call), so nothing here asks for a restart.
    function phonePanel(body) {
      var voiceKey = keyIsSet("voice") ? "set" : "unknown";
      // Status first, then the connection, then one section per kind of call, so each switch sits beside what it affects.
      body.appendChild(section("Status"));
      var card = el("div", "card");
      var line = el("p", "lead", "Checking...");
      var checks = el("div");
      var more = el("div", "acts");
      var numbers = el("ul", "help");
      card.appendChild(line);
      card.appendChild(checks);
      card.appendChild(more);
      card.appendChild(numbers);
      body.appendChild(card);
      body.appendChild(section("Connection", "Which ElevenLabs agent talks, and which phone number it uses. Both ids are in your ElevenLabs account; once the key is set, the numbers it finds are listed below with a button that fills both in."));
      ["daemon.phone_agent_id", "daemon.phone_number_id"].forEach(function (key) {
        var setting = byKey(key);
        if (setting) body.appendChild(settingRow(setting, null, true));
      });
      var found = el("div");
      body.appendChild(found);
      body.appendChild(section("Calls to you", "JARVIS rings only the numbers you list here, and a model can never name another."));
      ["daemon.phone_owner_numbers", "daemon.phone_approves", "daemon.phone_opening_line"].forEach(function (key) {
        var setting = byKey(key);
        if (setting) body.appendChild(rowFor(setting, null, true));
      });
      body.appendChild(section("Calls to other people", "Off by default. When on, JARVIS can ring a restaurant or an office for you; it says it is an AI, is limited to 5 calls a day and never calls in quiet hours."));
      var anyone = byKey("daemon.phone_call_anyone");
      if (anyone) body.appendChild(rowFor(anyone, null, true));
      body.appendChild(section("Calls to JARVIS", "What happens when someone rings the number you imported."));
      var screen = byKey("daemon.phone_screen_callers");
      if (screen) body.appendChild(rowFor(screen, null, true));
      body.appendChild(section("How many calls", "A guard against a mistaken or manipulated model ringing a phone over and over. Calls to other people have their own, lower limits."));
      ["daemon.phone_calls_per_hour", "daemon.phone_calls_per_day"].forEach(function (key) {
        var setting = byKey(key);
        if (setting) body.appendChild(settingRow(setting, null, true));
      });
      var ready = settingIsSet("daemon.phone_agent_id") && settingIsSet("daemon.phone_number_id") && settingIsSet("daemon.phone_owner_numbers");
      body.appendChild(section("Setup"));
      body.appendChild(stepsCard("Setup, once (about 15 minutes)", [
        "At elevenlabs.io, create an agent and import a phone number (a Twilio number or a SIP trunk) for it. Assign the agent to the number.",
        "In Connect apps, switch on Let other apps use JARVIS and From the internet. For a phone number use your own Cloudflare tunnel there: the free temporary address changes on every restart and the agent loses its tools.",
        "In ElevenLabs the agent needs JARVIS's tools. Press Connect them now in the Status box above: JARVIS adds an MCP server for the internet address, signing in with your key, and attaches it to the agent. It asks the caller before each tool, and never touches the agent's other servers.",
        "Put your ElevenLabs key in Voice & search. It needs access to the agents platform, not only speech.",
        "Press Use this number below (it fills in the agent and the number), and add your own number under Calls to you.",
        "Press Call me now. To ring JARVIS, call the number you imported; the agent answers and uses JARVIS's tools."
      ], ready, "The call and what is said on it pass through ElevenLabs (and Twilio or your SIP provider). JARVIS keeps a short record of each call it placed: who, why, how long and a summary. It does not keep the transcript or the audio."));
      body.appendChild(section("Recent calls"));
      var callsCard = el("div", "card");
      var calls = el("ul", "help");
      callsCard.appendChild(calls);
      body.appendChild(callsCard);
      var access = null;
      var tools = null;
      // Adds JARVIS's tools to the ElevenLabs agent (or sends the current key again), then asks the provider how things stand.
      function connectTools(refresh, finished) {
        settingsNote(refresh ? "Sending the key to ElevenLabs..." : "Connecting JARVIS's tools to the agent...");
        putJson("/phone/tools/connect", "POST", { refresh_key: refresh }).then(function (done) {
          settingsNote(done.created ? "Done: JARVIS's tools are now connected to the agent." : done.rekeyed ? "Done: the server at ElevenLabs has the current key." : done.attached_now ? "Done: the agent now uses JARVIS's tools." : "Already connected; nothing needed changing.");
          finished(); poll(true);
        }, function (error) { settingsNote(explain(error, "That could not be done."), true); finished(); });
      }
      // Removes the unused temporary addresses of earlier runs from the ElevenLabs account, after the owner has seen their names.
      function removeUnused(finished) {
        var names = (tools && tools.unused_names) || [];
        if (!window.confirm("Delete " + tools.unused_temporary + " unused temporary tool server" + (tools.unused_temporary === 1 ? "" : "s") + " from your ElevenLabs account?\n\n" + names.join("\n") + (tools.unused_temporary > names.length ? "\n...and more" : "") + "\n\nNo agent uses them. JARVIS only deletes servers on a temporary trycloudflare.com address that nothing uses.")) { finished(); return; }
        settingsNote("Deleting the unused temporary addresses...");
        putJson("/phone/tools/unused", "DELETE").then(function (done) {
          settingsNote("Deleted " + done.removed + " unused temporary address" + (done.removed === 1 ? "" : "es") + ".");
          finished(); poll(true);
        }, function (error) { settingsNote(explain(error, "That could not be done."), true); finished(); });
      }
      // What is true of each step, from the saved settings and, once they have answered, the internet address.
      function drawChecks() {
        var items = [
          { done: keyIsSet("voice"), text: "ElevenLabs key saved.", tab: "voice", tabLabel: "Add it in Voice & search" },
          { done: settingIsSet("daemon.phone_agent_id") && settingIsSet("daemon.phone_number_id"), text: "Agent and phone number chosen (Connection, below)." },
          { done: settingIsSet("daemon.phone_owner_numbers"), text: "Your own number added (Calls to you, below)." }
        ];
        if (access) {
          var state = access.public && access.public.status ? access.public.status.state : "off";
          var stable = access.public && access.public.stable && access.public.stable.configured;
          items.push({
            done: state === "on",
            text: state === "on" ? (stable ? "Internet address is on and stays the same (" + access.public.stable.host + ")."
              : "Internet address is on, but it changes on every restart: use your own Cloudflare tunnel in Connect apps for a number people call.")
              : "Internet address is off, so the agent cannot reach JARVIS's tools or the inbound door.",
            tab: "connect", tabLabel: "Open Connect apps"
          });
        }
        checks.replaceChildren();
        if (tools && tools.address) {
          var joined = !!(tools.registered && tools.attached);
          items.push({
            done: joined,
            text: joined ? "JARVIS's tools are connected to the agent at ElevenLabs."
              : tools.problem ? "ElevenLabs would not show its tool servers: " + tools.problem
              : tools.registered ? "The tools server exists at ElevenLabs but this agent does not use it yet."
              : "JARVIS's tools are not connected to the agent at ElevenLabs yet.",
            action: tools.problem ? null : joined ? { label: "Send the current key again", run: function (finished) { connectTools(true, finished); } }
              : { label: "Connect them now", run: function (finished) { connectTools(false, finished); } }
          });
          if (tools.unused_temporary > 0) {
            items.push({
              done: false,
              text: tools.unused_temporary + " old temporary address" + (tools.unused_temporary === 1 ? " is" : "es are") + " still listed in your ElevenLabs account and no agent uses " + (tools.unused_temporary === 1 ? "it" : "them") + ". They are harmless; you can tidy them away.",
              action: { label: "Delete them", run: removeUnused }
            });
          }
        }
        checks.appendChild(checklist(items));
      }
      api("/mcp/access").then(function (info) { access = info; drawChecks(); }, function () { drawChecks(); });
      drawChecks();
      function paint(status) {
        tools = status.tools || null;
        drawChecks();
        more.replaceChildren();
        numbers.replaceChildren();
        calls.replaceChildren();
        var check = status.check;
        if (!status.ready) {
          line.textContent = status.missing || "Phone calls are not set up.";
        } else if (check && check.ok === false) {
          line.textContent = "Set up, but ElevenLabs says: " + (check.problem || "something is wrong") + ".";
        } else if (check && check.answers_with_this_agent === false) {
          line.textContent = "Ready, but this phone number is assigned to another agent at ElevenLabs, so calling it will not reach this one.";
        } else {
          line.textContent = "Ready. JARVIS may ring " + (status.numbers || []).join(", ") + ", at most " + status.limits.per_hour + " times an hour.";
        }
        var test = el("button", "yes", "Call me now");
        test.disabled = !status.ready;
        test.addEventListener("click", function () {
          test.disabled = true;
          putJson("/phone/test", "POST").then(function () { settingsNote("Calling you now."); poll(); },
            function (error) { settingsNote(explain(error, "The call was not placed."), true); test.disabled = false; });
        });
        more.appendChild(test);
        var again = el("button", "", "Check with ElevenLabs");
        again.addEventListener("click", function () { poll(true); });
        more.appendChild(again);
        var clear = el("button", "danger", "Delete call records");
        clear.addEventListener("click", function () {
          putJson("/phone/calls", "DELETE").then(function () { settingsNote("Call records deleted."); poll(); },
            function (error) { settingsNote(explain(error, "Could not delete them."), true); });
        });
        more.appendChild(clear);
        var door = status.door || {};
        if (door.on) {
          var doorLine = el("li", "", door.state === "guarding" ? "Inbound door: guarding. Only your own numbers are answered." :
            door.state === "problem" ? "Inbound door: not working yet (" + (door.problem || "unknown") + "). Until it works, inbound calls may get no answer." : "Inbound door: starting...");
          numbers.appendChild(doorLine);
        }
        (door.turned_away || []).forEach(function (away) {
          numbers.appendChild(el("li", "", "Turned away: " + away.from + " at " + new Date(away.at).toLocaleString()));
        });
        // The numbers ElevenLabs reports, each with a button that fills in the number and its agent (what the person would otherwise copy by hand).
        found.replaceChildren();
        var listed = (check && check.numbers) || [];
        if (listed.length) {
          var foundCard = el("div", "card");
          foundCard.appendChild(el("p", "help", "Phone numbers in your ElevenLabs account:"));
          listed.forEach(function (number) {
            var row = el("div", "numberrow");
            row.appendChild(el("code", "", number.number));
            row.appendChild(el("span", "help", (number.label || "number") + " (" + number.provider + ")" + (number.agent_id ? ", agent " + number.agent_id : ", no agent assigned")));
            var current = byKey("daemon.phone_number_id");
            var inUse = current && String(current.value) === number.id;
            var use = el("button", inUse ? "" : "yes", inUse ? "In use" : "Use this number");
            use.disabled = !!inUse;
            use.addEventListener("click", function () {
              var changes = [{ key: "daemon.phone_number_id", values: [number.id] }];
              if (number.agent_id) changes.push({ key: "daemon.phone_agent_id", values: [number.agent_id] });
              use.disabled = true;
              putJson("/settings", "PUT", { changes: changes }).then(function () { settingsNote("Phone number saved."); loadSettings(); },
                function (error) { use.disabled = false; settingsNote(explain(error, "That was refused."), true); });
            });
            row.appendChild(use);
            foundCard.appendChild(row);
          });
          found.appendChild(foundCard);
        }
        (status.calls || []).forEach(function (call) {
          var text = new Date(call.placed_at).toLocaleString() + ": " + call.status.replace("_", " ") + ", " + call.to +
            (call.duration_seconds !== null && call.duration_seconds !== undefined ? ", " + call.duration_seconds + " s" : "") + ". " + call.reason +
            (call.summary ? " Summary: " + call.summary : "") + (call.error ? " (" + call.error + ")" : "");
          calls.appendChild(el("li", "", text));
        });
        if (!(status.calls || []).length) calls.appendChild(el("li", "", "No calls yet."));
      }
      function poll(check) {
        api("/phone" + (check || voiceKey === "set" ? "?check=1" : "")).then(paint, function () { line.textContent = "Phone calls are not available on this daemon."; });
      }
      poll();
    }
    // ---- MCP servers: list what each one became, test one before adding it, add, remove (P11-001) -------------------------    // A command is typed on one line and split here into a program and its arguments (quotes keep a path with spaces together).
    // The daemon resolves the program and checks the whole entry before it writes anything; a change applies on restart.
    function splitCommand(text) {
      var parts = [], current = "", quote = "", started = false;
      for (var i = 0; i < text.length; i++) {
        var c = text.charAt(i);
        if (quote) { if (c === quote) quote = ""; else current += c; }
        else if (c === '"' || c === "'") { quote = c; started = true; }
        else if (/\s/.test(c)) { if (started || current) { parts.push(current); current = ""; started = false; } }
        else { current += c; started = true; }
      }
      if (started || current) parts.push(current);
      return parts;
    }
    function mcpOAuthAddress(reply) {
      function parse(value) {
        if (typeof value !== "string" || /[\u0000-\u0020\u007f\\]/.test(value)) return null;
        var original = /^(https?):\/\/([^/?#]+)(?:[/?#]|$)/i.exec(value);
        if (!original || original[2].indexOf("@") >= 0 || value.indexOf("#") >= 0) return null;
        var parsed = new URL(value);
        // Do not let URL normalization replace the authority supplied by the daemon.
        if (parsed.username || parsed.password || parsed.origin.toLowerCase() !== (original[1] + "://" + original[2]).toLowerCase()) return null;
        return parsed;
      }
      try {
        var address = parse(reply.auth_url), endpoint = parse(reply.authorization_endpoint);
        if (!address || !endpoint || address.origin !== endpoint.origin || address.pathname !== endpoint.pathname) return null;
        if (address.protocol !== "https:") {
          var local = /^(127\.0\.0\.1|localhost|\[::1\])$/;
          if (!window.location || window.location.protocol !== "http:" || !local.test(window.location.hostname) || !local.test(address.hostname)) return null;
        }
        return address.href;
      } catch (_) { return null; }
    }
    function mcpOAuthPanel(server) {
      var panel = el("div"), current = server.oauth || {}, dirty = false, pending = false;
      var base = "/mcp/servers/" + encodeURIComponent(server.name) + "/oauth", prefix = "mcp-oauth-" + server.name + "-";
      panel.id = prefix + "panel";
      var line = el("p", "lead"), out = el("p", "help"), signIn = el("div", "acts");
      line.setAttribute("role", "status"); out.setAttribute("role", "status"); panel.appendChild(line);
      panel.appendChild(el("p", "help", "A saved OAuth grant is not a live health check or a verified account identity. Test tools only lists tools; it executes none. Signing in never approves a tool action."));
      var acts = el("div", "acts"), connect = el("button", "yes", "Connect"), check = el("button", "", "Check sign-in"), test = el("button", "", "Test tools"), disconnect = el("button", "danger", "Disconnect");
      [connect, check, test, disconnect].forEach(function (button) { button.type = "button"; acts.appendChild(button); });
      panel.appendChild(acts); panel.appendChild(signIn); panel.appendChild(out);
      var setup = el("details"); setup.open = !current.configured;
      setup.appendChild(el("summary", "", "One-time OAuth app setup"));
      var help = el("p", "help", "Register an OAuth app at your provider once using the exact callback below. Copy the expected issuer URL from the provider's documentation, enter the registered client ID and request only the minimal scopes needed. JARVIS does not register an app automatically. Later, press Connect; sign-in is saved on this computer.");
      help.id = prefix + "help"; setup.appendChild(help);
      setup.appendChild(el("p", "help", "Register this exact callback URL:"));
      var callback = el("code"); callback.id = prefix + "callback"; setup.appendChild(callback);
      function field(label, suffix, type) {
        var row = el("div", "set-row"), labelNode = el("label", "name", label), input = el("input");
        input.id = prefix + suffix; input.type = type; input.autocomplete = "off"; input.spellcheck = false; input.setAttribute("aria-describedby", help.id);
        labelNode.setAttribute("for", input.id); row.appendChild(labelNode); row.appendChild(input); setup.appendChild(row); return input;
      }
      var issuer = field("Expected issuer URL", "issuer", "text"), client = field("Client ID", "client-id", "text"), secret = field("Client secret (optional, write-only)", "client-secret", "password"), scopes = field("Scopes (space-separated)", "scopes", "text");
      issuer.value = current.issuer || ""; client.value = current.client_id || ""; scopes.value = (current.scopes || []).join(" ");
      issuer.placeholder = "Issuer URL from the provider's documentation";
      setup.appendChild(el("p", "help", "Public clients need no secret. An empty secret preserves the saved secret only for the same issuer and client ID. If either changes, re-enter the secret if the new registration requires one. Secrets are write-only and never saved in browser storage. Leave scopes blank if none are required; do not copy a provider's entire scope catalog."));
      var save = el("button", "", "Save OAuth app"); save.type = "button"; setup.appendChild(save); panel.appendChild(setup);
      function controls() {
        connect.disabled = pending || dirty || !current.configured;
        test.disabled = pending || dirty || !current.connected;
        disconnect.disabled = pending || (!current.connected && !current.pending);
        check.disabled = pending; save.disabled = pending;
        [issuer, client, secret, scopes].forEach(function (input) { input.disabled = pending; });
      }
      function paint(status) {
        current = Object.assign({}, current, status);
        callback.textContent = current.callback_url || "The daemon has not reported a callback address.";
        secret.placeholder = current.client_secret_set ? "Saved for this client; leave empty to keep it" : "Optional for public clients";
        line.textContent = current.pending ? "Waiting for browser sign-in. Finish at the provider, then press Check sign-in." : current.connected ? "OAuth grant saved on this computer. Test tools checks reachability now." : current.configured ? "App setup saved. Press Connect to sign in." : "Complete the one-time app setup, then press Connect.";
        if (!current.pending) signIn.replaceChildren();
        integrationRestart("OAuth tools: " + server.name, !!current.restart_needed);
        controls();
      }
      function fail(message) { pending = false; out.className = "help bad"; out.textContent = message; controls(); }
      [issuer, client, secret, scopes].forEach(function (input) { input.addEventListener("input", function () { dirty = true; controls(); }); });
      save.addEventListener("click", function () {
        var value = secret.value; secret.value = "";
        if (!issuer.value.trim() || !client.value.trim()) { fail("Enter the expected issuer URL and registered client ID first."); return; }
        var payload = { issuer: issuer.value.trim(), client_id: client.value.trim(), scopes: scopes.value.trim() ? scopes.value.trim().split(/\s+/) : [] };
        if (value) payload.client_secret = value;
        pending = true; controls(); signIn.replaceChildren(); out.className = "help"; out.textContent = "Saving OAuth app setup...";
        putJson(base + "/setup", "PUT", payload).then(function (status) {
          pending = false; dirty = false; paint(status); out.textContent = "OAuth app setup saved. Press Connect to sign in.";
        }, function () { fail("OAuth app setup was not saved. Re-enter the secret if needed to retry."); });
      });
      connect.addEventListener("click", function () {
        var popup = null;
        function closeBlank() { if (popup) { try { popup.close(); } catch (_) {} popup = null; } }
        try { popup = window.open("about:blank", "_blank"); if (popup) popup.opener = null; } catch (_) { closeBlank(); }
        pending = true; controls(); signIn.replaceChildren(); out.className = "help"; out.textContent = "Starting browser sign-in...";
        putJson(base + "/connect", "POST", {}).then(function (reply) {
          pending = false;
          if ($("settings").hidden || sview.tab !== "programs" || !$("settings-body").contains(panel)) { closeBlank(); return; }
          var address = mcpOAuthAddress(reply);
          if (!address) { closeBlank(); fail("The daemon did not provide a valid provider sign-in address. No provider page was opened."); return; }
          var opened = false;
          if (popup) { try { if (!popup.closed) { popup.location.replace(address); opened = true; } } catch (_) {} }
          if (!opened) {
            closeBlank(); var link = el("a", "", "Continue sign-in"); link.href = address; link.target = "_blank"; link.rel = "noopener noreferrer"; signIn.appendChild(link);
          }
          paint({ pending: true });
          out.textContent = opened ? "Complete sign-in at the provider, then press Check sign-in. Your password stays with the provider." : "The sign-in window could not open. Use Continue sign-in, then press Check sign-in.";
        }, function () { closeBlank(); fail("Could not start browser sign-in. Check the saved app setup and try again."); });
      });
      check.addEventListener("click", function () {
        pending = true; controls(); out.className = "help"; out.textContent = "Checking saved sign-in...";
        api(base).then(function (status) { pending = false; paint(status); out.textContent = "Saved sign-in checked; no tool was executed."; }, function () { fail("Could not check saved sign-in. Check the daemon connection and try again."); });
      });
      test.addEventListener("click", function () {
        pending = true; controls(); out.className = "help"; out.textContent = "Listing tools to test reachability...";
        putJson(base + "/test", "POST", {}).then(function (reply) {
          pending = false; out.className = reply.ok ? "help" : "help bad";
          out.textContent = reply.ok ? "Reached it: " + (reply.tools || []).length + " tools. No tool was executed." : reply.message || "The provider did not complete the test. Check sign-in or connect again."; controls();
        }, function () { fail("The tool-list test failed. Check sign-in or connect again."); });
      });
      disconnect.addEventListener("click", function () {
        pending = true; controls(); signIn.replaceChildren(); out.className = "help"; out.textContent = "Disconnecting...";
        putJson(base + "/disconnect", "POST", {}).then(function (status) {
          pending = false; paint(Object.assign({}, status, { connected: false, pending: false }));
          out.textContent = status.revoked ? "Disconnected locally and provider revocation confirmed. App setup is kept for later sign-in." : "Disconnected locally. Provider revocation was not confirmed; review the provider's authorized apps. App setup is kept for later sign-in.";
        }, function () { fail("Disconnect was not confirmed. Press Check sign-in before relying on removal, and review the provider's authorized apps."); });
      });
      paint(current);
      return panel;
    }
    function mcpServerCard(server, reload) {
      var card = el("div", "card");
      var browserOAuth = server.kind === "http" && server.authorization === "oauth:" + server.name;
      var head = el("h4", "", server.name + " ");
      var stateText = server.state === "connected" ? "connected" : server.state === "unreachable" ? "not reachable" : "starts after a restart";
      head.appendChild(el("span", "badge " + (server.state === "connected" ? "on" : "off"), stateText));
      card.appendChild(head);
      var how = server.kind === "http" ? server.endpoint : (server.command || []).join(" ");
      card.appendChild(el("code", "", how));
      (server.credentials || []).forEach(function (credential) {
        card.appendChild(el("p", "help", credential.name + ": " + credential.state + " (" + credential.reference + ")"));
        if (credential.name === "Authorization") card.appendChild(el("p", "help", "Set or replace this key with jarvis keys set " + credential.reference + ". The terminal prompts privately; no token value belongs here."));
      });
      if (server.state === "unreachable") card.appendChild(el("p", "help bad", server.reason || "It did not answer."));
      if (server.state === "connected") {
        var tools = server.tools || [];
        card.appendChild(el("p", "lead", tools.length + " tool" + (tools.length === 1 ? "" : "s") + ", "
          + (server.posture === "read-only" ? "run without asking (you marked it read-only)." : "each call asks you first.")
          + (server.era ? " Speaks the " + server.era + " protocol." : "")));
        var shown = el("details");
        shown.appendChild(el("summary", "", "Show the tools"));
        var list = el("ul", "help");
        tools.slice(0, 8).forEach(function (tool) { list.appendChild(el("li", "", tool.id + (tool.description ? ": " + tool.description : ""))); });
        if (tools.length > 8) list.appendChild(el("li", "", "and " + (tools.length - 8) + " more (see Permissions)."));
        shown.appendChild(list);
        card.appendChild(shown);
        (server.excluded || []).slice(0, 5).forEach(function (left) { card.appendChild(el("p", "help bad", "Left out: " + left.tool + ": " + left.reason)); });
      }
      var acts = el("div", "acts");
      var testButton = el("button", "", "Test");
      var out = el("span", "help", "");
      testButton.addEventListener("click", function () {
        testButton.disabled = true; out.className = "help"; out.textContent = "Connecting (a first start with npx may download the program)...";
        putJson("/mcp/test", "POST", { name: server.name, command: server.command || [], endpoint: server.endpoint || null, read_only: server.posture === "read-only", env: server.env || {}, authorization: server.authorization || null }).then(function (reply) {
          testButton.disabled = false;
          out.className = reply.ok ? "help" : "help bad";
          out.textContent = reply.ok ? "Reached it: " + (reply.tools || []).length + " tools." : (reply.message || "No answer.");
        }, function (error) { testButton.disabled = false; out.className = "help bad"; out.textContent = explain(error, "The test failed."); });
      });
      var remove = el("button", "danger", "Remove");
      remove.addEventListener("click", function () {
        remove.disabled = true;
        putJson("/mcp/servers/" + encodeURIComponent(server.name), "DELETE").then(function () {
          settingsNote(server.name + " removed. Restart to apply."); needsRestart("Tool program removed"); reload();
        }, function (error) { settingsNote(explain(error, "Could not remove it."), true); remove.disabled = false; });
      });
      if (browserOAuth) card.appendChild(mcpOAuthPanel(server));
      else acts.appendChild(testButton);
      acts.appendChild(remove); acts.appendChild(out);
      card.appendChild(acts);
      return card;
    }
    function mcpAddForm(reload) {
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Add a program"));
      var name = el("input"); name.id = "mcp-program-name"; name.type = "text"; name.placeholder = "A short name, for example files"; name.autocomplete = "off"; name.spellcheck = false;
      var command = el("input"); command.id = "mcp-program-command"; command.type = "text"; command.placeholder = "The command that starts it, or a web address"; command.autocomplete = "off"; command.spellcheck = false;
      var nameRow = el("div", "set-row");
      var nameLabel = el("label", "name", "Name"); nameLabel.setAttribute("for", name.id);
      nameRow.appendChild(nameLabel);
      nameRow.appendChild(name);
      nameRow.appendChild(el("span"));
      var commandRow = el("div", "set-row");
      var commandLabel = el("label", "name", "Command or address"); commandLabel.setAttribute("for", command.id);
      commandRow.appendChild(commandLabel);
      commandRow.appendChild(command);
      var example = el("button", "", "Example");
      example.type = "button";
      example.title = "Fill in a sample command";
      example.addEventListener("click", function () {
        if (!name.value.trim()) name.value = "files";
        command.value = "npx -y @modelcontextprotocol/server-filesystem C:\\Users\\you\\Documents";
        updateBearer();
        command.focus();
      });
      var exampleBox = el("div", "acts"); exampleBox.appendChild(example);
      commandRow.appendChild(exampleBox);
      commandRow.appendChild(el("span", "help", "A program on this computer is started with its command, for example npx -y followed by a package name. A remote server is its web address, starting with https."));
      var auth = el("select"); auth.id = "mcp-authorization";
      [["bearer", "Manual bearer (optional)"], ["oauth", "Browser OAuth"]].forEach(function (choice) { var option = el("option", "", choice[1]); option.value = choice[0]; auth.appendChild(option); }); auth.value = "bearer";
      var authRow = el("div", "set-row"); authRow.id = "mcp-authorization-row";
      var authLabel = el("label", "name", "Remote authorization"); authLabel.setAttribute("for", auth.id); authRow.appendChild(authLabel); authRow.appendChild(auth);
      var oauthHelp = el("p", "help", "Add this server first, then complete One-time OAuth app setup on its saved card using your pre-registered app and the exact callback URL. After that, use Connect for persistent browser sign-in. No automatic app registration. Test tools becomes available after sign-in."); oauthHelp.id = "mcp-oauth-add-help"; authRow.appendChild(oauthHelp); auth.setAttribute("aria-describedby", oauthHelp.id);
      var bearer = el("input"); bearer.id = "mcp-bearer-ref"; bearer.type = "text"; bearer.autocomplete = "off"; bearer.spellcheck = false;
      bearer.setAttribute("aria-describedby", "mcp-bearer-help");
      var bearerRow = el("div", "set-row"); bearerRow.id = "mcp-bearer-row";
      var bearerLabel = el("label", "name", "Bearer key reference (optional)"); bearerLabel.setAttribute("for", bearer.id);
      var bearerHelp = el("span", "help"); bearerHelp.id = "mcp-bearer-help";
      bearerRow.appendChild(bearerLabel); bearerRow.appendChild(bearer); bearerRow.appendChild(el("span")); bearerRow.appendChild(bearerHelp);
      function updateBearer() {
        var remote = /^https?:\/\//i.test(command.value.trim());
        if (!remote) auth.value = "bearer";
        var oauth = remote && auth.value === "oauth";
        authRow.hidden = !remote; auth.disabled = !remote; oauthHelp.hidden = !oauth;
        bearerRow.hidden = !remote || oauth; bearer.disabled = !remote || oauth;
        if (!remote || oauth) bearer.value = "";
        if (test) test.disabled = oauth;
        bearer.placeholder = "mcp:" + (name.value.trim() || "NAME") + ":KEY";
        bearerHelp.textContent = "If this remote server needs a bearer token, first run jarvis keys set " + bearer.placeholder + " in a terminal. Its prompt is hidden. Enter only that reference here, never the token. Use this server's name and an address without a query. Leave blank for no authorization. GitHub sign-in is under Apps & tools → GitHub.";
      }
      command.addEventListener("input", updateBearer); name.addEventListener("input", updateBearer); auth.addEventListener("change", updateBearer);
      updateBearer();
      var only = el("input"); only.type = "checkbox";
      var readRow = el("div", "set-row toggle");
      var approvalLabel = el("label", "name", "Trust this server’s tools as read-only");
      only.id = "mcp-trust-read-only";
      approvalLabel.setAttribute("for", only.id);
      only.setAttribute("aria-label", "Trust this server’s tools as read-only");
      readRow.appendChild(approvalLabel);
      var wrap = el("label", "switch");
      var track = el("span", "track");
      var state = el("span", "state", "No");
      wrap.appendChild(only); wrap.appendChild(track); wrap.appendChild(state);
      only.addEventListener("change", function () { state.textContent = only.checked ? "Yes" : "No"; });
      readRow.appendChild(wrap);
      readRow.appendChild(el("span"));
      readRow.appendChild(el("span", "help", "Off: ask before using this server. On: classify its tools as read-only so they can run without asking. This is your trust decision; it does not prevent the server from changing data."));
      var out = el("p", "help", "");
      var test = el("button", "", "Test");
      var add = el("button", "yes", "Add");
      function body() {
        updateBearer();
        var text = command.value.trim();
        var remote = /^https?:\/\//i.test(text);
        var payload = { name: name.value.trim(), command: remote ? [] : splitCommand(text), endpoint: remote ? text : null, read_only: only.checked };
        if (remote && auth.value === "oauth") {
          if (!payload.name) { out.className = "help bad"; out.textContent = "Enter a name for this OAuth server first."; return null; }
          payload.authorization = "oauth:" + payload.name;
        } else if (remote) {
          var reference = bearer.value.trim();
          var parts = /^mcp:([^:\s]+):([^:\s]+)$/.exec(reference);
          if (reference && (!parts || parts[1] !== payload.name)) {
            out.className = "help bad"; out.textContent = "Use a key reference mcp:NAME:KEY with this server's name, never a token value. Set the key first with jarvis keys set mcp:NAME:KEY.";
            return null;
          }
          if (reference && text.indexOf("?") >= 0) {
            out.className = "help bad"; out.textContent = "A remote server using a bearer key needs an address without a query.";
            return null;
          }
          payload.authorization = reference || null;
        }
        return payload;
      }
      test.addEventListener("click", function () {
        var payload = body(); if (!payload) return;
        test.disabled = true; out.className = "help"; out.textContent = "Connecting (a first start with npx may download the program)...";
        putJson("/mcp/test", "POST", payload).then(function (reply) {
          test.disabled = false;
          if (!reply.ok) { out.className = "help bad"; out.textContent = reply.message || "No answer."; return; }
          out.className = "help";
          out.textContent = "Reached it: " + (reply.tools || []).length + " tools ("
            + (reply.tools || []).slice(0, 4).map(function (tool) { return tool.id; }).join(", ") + ((reply.tools || []).length > 4 ? ", ..." : "") + ")."
            + ((reply.excluded || []).length ? " " + reply.excluded.length + " left out." : "");
        }, function (error) { test.disabled = false; out.className = "help bad"; out.textContent = explain(error, "The test failed."); });
      });
      add.addEventListener("click", function () {
        var payload = body(); if (!payload) return;
        add.disabled = true;
        putJson("/mcp/servers", "POST", payload).then(function () {
          settingsNote("Program added. Restart to apply."); needsRestart("Tool program added"); name.value = ""; command.value = ""; bearer.value = ""; updateBearer(); only.checked = false; state.textContent = "No"; add.disabled = false; reload();
        }, function (error) { add.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That was refused."); });
      });
      var actionRow = el("div", "set-row");
      actionRow.appendChild(el("span"));
      var buttons = el("div", "acts"); buttons.appendChild(test); buttons.appendChild(add);
      actionRow.appendChild(buttons);
      actionRow.appendChild(el("span"));
      [nameRow, commandRow, authRow, bearerRow, readRow, actionRow].forEach(function (node) { card.appendChild(node); });
      card.appendChild(out);
      return card;
    }    // Connect apps: let other AI apps (Claude Code, Cursor, VS Code, a script) hand JARVIS a task. One switch, one key, copy-paste lines.
    // Programs a developer adds by hand. The browser and documentation lookup are ordinary tools that come with JARVIS (they are
    // listed and controlled under Permissions like every other tool), so this is the only MCP-specific screen, and it lives in Advanced.
    function programsPanel(body) {
      var holder = el("div");
      body.appendChild(holder);
      function reload() {
        api("/mcp").then(function (reply) {
          holder.replaceChildren();
          var builtIn = (reply.abilities || []).map(function (a) { return a.id; });
          var custom = (reply.servers || []).filter(function (server) { return builtIn.indexOf(server.name) < 0; });
          var intro = el("div", "card");
          intro.appendChild(el("p", "lead", "JARVIS can use any program that speaks MCP, the Model Context Protocol. The browser and documentation lookup already come with JARVIS and are under Permissions; add your own here."));
          intro.appendChild(el("p", "help warn", "Adding a program runs it as you, so only add programs you trust. A change applies after a restart."));
          holder.appendChild(intro);
          if (custom.length) {
            holder.appendChild(el("div", "secthead", "Your programs"));
            custom.forEach(function (server) { holder.appendChild(mcpServerCard(server, reload)); });
          }
          holder.appendChild(mcpAddForm(reload));
          integrationRestart("Programs", !!reply.restart_needed);
          if (reply.restart_needed) {
            var banner = el("div", "card");
            banner.appendChild(el("p", "lead", "A restart switches your changes on."));
            var go = el("button", "yes", "Restart now");
            go.addEventListener("click", function () { go.disabled = true; $("settings-restart").click(); });
            banner.appendChild(go);
            holder.appendChild(banner);
          }
        }, function () { holder.replaceChildren(); });
      }
      reload();
    }    // Connect apps: let other AI apps (Claude Code, Cursor, VS Code, a script) hand JARVIS a task. One switch, one key, copy-paste lines.
    // The key is the owner's own to copy, so it is shown here (and only here); a new key stops the old one at once.
    function copyButton(getText, label) {
      var button = el("button", "", label || "Copy");
      button.addEventListener("click", function () {
        var text = getText();
        var done = function () { button.textContent = "Copied"; setTimeout(function () { button.textContent = label || "Copy"; }, 1500); };
        if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, function () { button.textContent = "Select and copy by hand"; });
        else button.textContent = "Select and copy by hand";
      });
      return button;
    }
    function snippetBox(title, getText) {
      var wrap = el("div");
      wrap.appendChild(el("p", "help", title));
      var box = el("textarea"); box.readOnly = true; box.rows = 4; box.value = getText(); box.spellcheck = false;
      box.setAttribute("aria-label", title);
      box.addEventListener("focus", function () { box.select(); });
      wrap.appendChild(box);
      var acts = el("div", "acts"); acts.appendChild(copyButton(function () { return box.value; })); wrap.appendChild(acts);
      return { node: wrap, refresh: function () { box.value = getText(); } };
    }
    // An address that never changes: the owner's own Cloudflare tunnel. They make it once in their Cloudflare account and paste its token and the
    // hostname here. The token is saved privately and never shown again. With none saved, the free temporary address is used as before.
    function stableBlock(pub, reload) {
      var stable = pub.stable || {};
      var box = el("div", "stable");
      var out = el("p", "help", "");
      if (stable.configured) {
        box.appendChild(el("p", "help", "Stable address: your own Cloudflare tunnel, at " + stable.host + ". Its token is saved privately and is not shown."));
        var forget = el("button", "", "Go back to the temporary address");
        forget.addEventListener("click", function () {
          forget.disabled = true;
          putJson("/mcp/access/public/named", "DELETE", {}).then(function () { reload(); },
            function (error) { forget.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be removed."); });
        });
        var row = el("div", "acts"); row.appendChild(forget); box.appendChild(row);
        box.appendChild(out);
        return box;
      }
      var details = el("details");
      details.appendChild(el("summary", "", "Recommended for a phone number: an address that never changes (your own Cloudflare tunnel)"));
      var service = stable.service_url || "the address under How to connect, without the /mcp at the end";
      [
        "You need a free Cloudflare account with a domain on it.",
        "In the Cloudflare dashboard open Networking, Tunnels, Create tunnel and name it (for example jarvis). Pick any operating system: you do not install anything, JARVIS runs the connector itself.",
        "On that page copy the command under Install and Run (cloudflared.exe service install eyJ...). Do not run it.",
        "Choose the name you want, such as jarvis.yourdomain.com, and paste both below. Press Save and connect: within a few seconds Cloudflare's Connection Status turns to connected.",
        "Back in Cloudflare continue to the route: add a published application with that same name and the Service URL " + service + "."
      ].forEach(function (text, index) { details.appendChild(el("p", "help", (index + 1) + ". " + text)); });
      var host = el("input"); host.type = "text"; host.placeholder = "jarvis.yourdomain.com"; host.autocomplete = "off"; host.spellcheck = false;
      var token = el("input"); token.type = "password"; token.placeholder = "cloudflared.exe service install eyJ... (or just the eyJ... token)"; token.autocomplete = "off"; token.spellcheck = false;
      host.setAttribute("aria-label", "Hostname");
      token.setAttribute("aria-label", "Tunnel token");
      var save = el("button", "yes", "Save and connect");
      save.addEventListener("click", function () {
        save.disabled = true; out.className = "help"; out.textContent = "";
        putJson("/mcp/access/public/named", "POST", { hostname: host.value, token: token.value }).then(function () { token.value = ""; reload(); },
          function (error) { save.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be saved."); });
      });
      var hostRow = el("div", "set-row");
      hostRow.appendChild(el("span", "name", "Hostname"));
      hostRow.appendChild(host);
      hostRow.appendChild(el("span"));
      var tokenRow = el("div", "set-row");
      tokenRow.appendChild(el("span", "name", "Tunnel token"));
      tokenRow.appendChild(token);
      var acts = el("div", "acts"); acts.appendChild(save); tokenRow.appendChild(acts);
      details.appendChild(hostRow);
      details.appendChild(tokenRow);
      details.appendChild(out);
      box.appendChild(details);
      return box;
    }
    // The internet address: a free Cloudflare quick tunnel that JARVIS starts and stops. Cloud agents (a phone agent, a hosted assistant) connect
    // from the provider's servers, so they cannot reach an address that only works on this computer. The key is still required on every request.
    function publicCard(info, current, reload) {
      var pub = info.public || {};
      var status = pub.status || { state: "off" };
      var card = el("div", "card");
      var head = el("h4", "", "From the internet ");
      var badge = status.state === "on" ? "on" : status.state === "problem" ? "problem" : status.state === "getting" ? "downloading" : status.state === "starting" ? "starting" : "off";
      head.appendChild(el("span", "badge " + (status.state === "on" ? "on" : "off"), badge));
      card.appendChild(head);
      card.appendChild(el("p", "lead", "JARVIS opens a web address that leads to this endpoint. Every request still needs your key."));
      card.appendChild(stableBlock(pub, reload));
      var out = el("p", "help", "");
      var acts = el("div", "acts");
      function busy(button) { button.disabled = true; }
      if (status.state === "off" || status.state === "problem") {
        if (status.state === "problem") card.appendChild(el("p", "help bad", status.reason || "It could not start."));
        var on = el("button", "yes", status.state === "problem" ? "Try again" : "Make it reachable");
        if (!pub.available) { on.disabled = true; out.className = "help bad"; out.textContent = "This computer needs Cloudflare's free cloudflared program: install it (macOS: brew install cloudflared), then come back."; }
        on.addEventListener("click", function () {
          busy(on);
          putJson("/mcp/access/public", "POST", { enabled: true }).then(function () { reload(); },
            function (error) { on.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be started."); });
        });
        acts.appendChild(on);
        if (status.state === "problem" && pub.wanted) {
          var off = el("button", "", "Stop");
          off.addEventListener("click", function () { putJson("/mcp/access/public", "POST", { enabled: false }).then(function () { reload(); }); });
          acts.appendChild(off);
        }
        if (pub.download_megabytes) card.appendChild(el("p", "help", "JARVIS will download Cloudflare's cloudflared once (about " + pub.download_megabytes + " MB), checked against a fixed checksum."));
      } else {
        var stop = el("button", "", "Stop");
        stop.addEventListener("click", function () {
          busy(stop);
          putJson("/mcp/access/public", "POST", { enabled: false }).then(function () { reload(); },
            function (error) { stop.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be stopped."); });
        });
        acts.appendChild(stop);
      }
      card.appendChild(acts);
      card.appendChild(out);
      if (status.state === "getting") card.appendChild(el("p", "help", "Downloading the program. This happens once."));
      if (status.state === "starting") card.appendChild(el("p", "help", "Starting. The address appears in a few seconds."));
      if (status.state === "on" && pub.url) {
        card.appendChild(el("p", "help", "Server URL"));
        var row = el("div", "acts"); row.appendChild(el("code", "", pub.url)); row.appendChild(copyButton(function () { return pub.url; })); card.appendChild(row);
        card.appendChild(el("p", "help", "In ElevenLabs: Agents, Integrations, Add custom MCP server. Server URL: the address above. Secret token: your key (shown above). Then attach it to your agent. Other cloud apps take the same two values."));
        card.appendChild(el("p", "help", pub.stable && pub.stable.configured
          ? "This is your own Cloudflare tunnel, so the address stays the same after a restart. Anyone with the address and the key can ask JARVIS to work for them, so keep both private."
          : "The address changes every time JARVIS starts, so paste the new one into the app after a restart (or use your own Cloudflare tunnel above for one that never changes). Anyone with the address and the key can ask JARVIS to work for them, so keep both private."));
      }
      if (status.state === "getting" || status.state === "starting") {
        setTimeout(function () { if (!$("settings").hidden && sview.tab === "connect") reload(); }, 2000);
      }
      return card;
    }
    function connectPanel(body) {
      var holder = el("div");
      body.appendChild(holder);
      function reload() {
        api("/mcp/access").then(function (info) {
          holder.replaceChildren();
          holder.appendChild(section("On this computer"));
          var card = el("div", "card");
          var head = el("h4", "", "Let other apps use JARVIS ");
          head.appendChild(el("span", "badge " + (info.running ? "on" : "off"), info.running ? "on" : info.enabled ? "on after restart" : "off"));
          card.appendChild(head);
          card.appendChild(el("p", "lead", info.enabled
            ? "Other AI apps on this computer can hand JARVIS a task and use its tools, if they have the key below."
            : "Off. Nothing outside JARVIS can reach it. Switch it on to let your other AI apps hand JARVIS tasks."));
          var acts = el("div", "acts");
          var toggle = el("button", info.enabled ? "" : "yes", info.enabled ? "Switch off" : "Switch on");
          toggle.addEventListener("click", function () {
            toggle.disabled = true;
            putJson("/mcp/access", "POST", { enabled: !info.enabled }).then(function () { needsRestart("External access"); reload(); },
              function (error) { toggle.disabled = false; settingsNote(explain(error, "That could not be changed."), true); });
          });
          acts.appendChild(toggle);
          card.appendChild(acts);
          holder.appendChild(card);
          integrationRestart("External access", !!info.restart_needed);
          if (info.restart_needed) {
            var banner = el("div", "card");
            banner.appendChild(el("p", "lead", "One restart switches this on or off."));
            var go = el("button", "yes", "Restart now");
            go.addEventListener("click", function () { go.disabled = true; $("settings-restart").click(); });
            banner.appendChild(go);
            holder.appendChild(banner);
          }
          if (!info.enabled || !info.key) return;
          var current = { key: info.key, shown: false };
          var url = info.address;
          var access = el("div", "card");
          access.appendChild(el("h4", "", "How to connect"));
          access.appendChild(el("p", "help", "Address"));
          var addressRow = el("div", "acts"); var addressText = el("code", "", url); addressRow.appendChild(addressText); addressRow.appendChild(copyButton(function () { return url; })); access.appendChild(addressRow);
          access.appendChild(el("p", "help", "Key (keep it secret: anyone with it can ask JARVIS to work for them)"));
          var keyRow = el("div", "acts"); var keyText = el("code", "", "") ;
          function paintKey() { keyText.textContent = current.shown ? current.key : "\u2022".repeat(24); }
          paintKey();
          var show = el("button", "", "Show"); show.addEventListener("click", function () { current.shown = !current.shown; show.textContent = current.shown ? "Hide" : "Show"; paintKey(); });
          keyRow.appendChild(keyText); keyRow.appendChild(show); keyRow.appendChild(copyButton(function () { return current.key; }));
          access.appendChild(keyRow);
          var boxes = [
            snippetBox("Claude Code: paste this in a terminal", function () { return "claude mcp add --transport http jarvis " + url + " --header \"Authorization: Bearer " + current.key + "\""; }),
            snippetBox("Cursor: add this to your MCP settings (mcp.json)", function () { return JSON.stringify({ mcpServers: { jarvis: { url: url, headers: { Authorization: "Bearer " + current.key } } } }, null, 2); }),
            snippetBox("VS Code: add this to mcp.json", function () { return JSON.stringify({ servers: { jarvis: { type: "http", url: url, headers: { Authorization: "Bearer " + current.key } } } }, null, 2); })
          ];
          var snippets = el("details");
          snippets.appendChild(el("summary", "", "Connect Claude Code, Cursor or VS Code"));
          boxes.forEach(function (box) { snippets.appendChild(box.node); });
          access.appendChild(snippets);
          var renew = el("button", "danger", "Make a new key");
          var renewNote = el("span", "help", "");
          renew.addEventListener("click", function () {
            if (!window.confirm("Make a new key? Apps using the old key stop working until you give them the new one.")) return;
            renew.disabled = true;
            putJson("/mcp/access/key", "POST").then(function (reply) {
              current.key = reply.key; paintKey(); boxes.forEach(function (box) { box.refresh(); }); renew.disabled = false;
              renewNote.textContent = "New key made. The old one no longer works.";
            }, function (error) { renew.disabled = false; renewNote.className = "help bad"; renewNote.textContent = explain(error, "That could not be done."); });
          });
          var renewRow = el("div", "acts"); renewRow.appendChild(renew); renewRow.appendChild(renewNote); access.appendChild(renewRow);
          holder.appendChild(access);
          holder.appendChild(section("For cloud agents", "A phone agent (ElevenLabs) or a hosted assistant connects from the provider's servers, so it needs an address that works from the internet."));
          holder.appendChild(publicCard(info, current, reload));
          var safeCard = el("div", "card quiet");
          var safe = el("details");
          safe.appendChild(el("summary", "", "What connected apps can and cannot do"));
          ["They can hand JARVIS a task and read the answer, and see and use JARVIS's tools that run without asking (memory, projects, files you allowed, reading web pages, documentation lookup).",
           "Anything that would ask you (clicking or typing in the browser, sending mail, changing your schedule, running commands) waits for your answer, which appears with your other approvals like any other question. They wait up to two minutes, then the call is refused. They cannot approve things themselves.",
           "Only this computer can connect, unless you switch on the internet address above.",
           "Tasks they hand JARVIS appear in your runs. A single tool they call directly follows the same rules as your own requests but is not yet recorded in your history."].forEach(function (text) { safe.appendChild(el("p", "help", text)); });
          safeCard.appendChild(safe);
          holder.appendChild(safeCard);
        }, function () { holder.replaceChildren(); holder.appendChild(el("p", "help bad", "This is not available on this daemon.")); });
      }
      reload();
    }
    // Original local line icons. No markup or external assets enter the settings DOM.
    var SVG = "http:" + "//www.w3.org/2000/svg";
    var ICON_PATHS = {
      overview: "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
      brain: "M12 3v18 M12 5C8 1 4 4 5 8C1 10 2 15 5 16C4 20 9 23 12 19 M12 5C16 1 20 4 19 8C23 10 22 15 19 16C20 20 15 23 12 19 M5 8l3 2 M19 8l-3 2 M5 16l3-2 M19 16l-3-2",
      voice: "M4 9v6 M8 5v14 M12 2v20 M16 5v14 M20 9v6",
      files: "M3 6h7l2 3h9v12H3z M3 6V3h7l2 3h6v3",
      permissions: "M12 2l8 3v7c0 5-8 10-8 10S4 17 4 12V5z M8 12l3 3 5-6",
      connections: "M9 15l6-6 M8 12l-2 2a3 3 0 0 0 4 4l3-3 M16 12l2-2a3 3 0 0 0-4-4l-3 3",
      google: "M16 7a4 4 0 1 1-8 0a4 4 0 1 1 8 0 M4 21v-3c0-5 16-5 16 0v3 M17 4l3 3 2-3",
      telegram: "M3 5h18v12H11l-6 4v-4H3z M7 9h10 M7 13h6",
      phone: "M5 3h4l2 5-3 2c2 4 3 5 6 6l2-3 5 2v4c-9 4-22-9-16-16z",
      connect: "M8 4H3v7h5z M21 13h-5v7h5z M8 7h5v10h3 M10 4l3 3-3 3 M14 14l3 3-3 3",
      console: "M3 3h18v14H3z M9 21h6 M12 17v4 M7 7l3 3-3 3 M12 13h5",
      alerts: "M5 17h14l-2-4V8a5 5 0 0 0-10 0v5z M9 20c1 2 5 2 6 0 M12 1v2",
      advanced: "M3 5h18 M3 12h18 M3 19h18 M7 2v6 M17 9v6 M10 16v6"
    };
    function settingsIcon(id, className) {
      var svg = document.createElementNS(SVG, "svg");
      svg.setAttribute("class", className);
      svg.setAttribute("viewBox", "0 0 24 24");
      svg.setAttribute("width", "18");
      svg.setAttribute("height", "18");
      svg.setAttribute("fill", "none");
      svg.setAttribute("stroke", "currentColor");
      svg.setAttribute("stroke-width", "1.6");
      svg.setAttribute("stroke-linecap", "round");
      svg.setAttribute("stroke-linejoin", "round");
      svg.setAttribute("aria-hidden", "true");
      svg.setAttribute("focusable", "false");
      var path = document.createElementNS(SVG, "path");
      path.setAttribute("d", ICON_PATHS[id] || ICON_PATHS.overview);
      svg.appendChild(path);
      return svg;
    }
    function settingsHero(tab) {
      var hero = el("div", "settings-hero");
      hero.appendChild(settingsIcon(tab[0], "settings-hero-icon"));
      var copy = el("div", "settings-hero-copy");
      copy.appendChild(el("h3", "", tab[1]));
      copy.appendChild(el("p", "", tab[2]));
      hero.appendChild(copy);
      return hero;
    }
    function overviewPanel(body) {
      var known = !!sview.reply && !sview.error;
      var summary = el("p", "help overview-safety" + (sview.error ? " bad" : ""), known
        ? "Saved setup only. Runtime availability has not been checked."
        : sview.error || "Loading saved daemon configuration. Setup is unknown until it loads; Console appearance remains available in this browser.");
      summary.setAttribute("role", "status");
      if (known) {
        var explanation = "Saved settings and key presence do not prove a service is connected or healthy. No provider tests run here.";
        summary.setAttribute("title", explanation);
        summary.setAttribute("aria-description", explanation);
      }
      body.appendChild(summary);
      var grid = el("div", "overview-grid");
      function card(title, state, items, actions) {
        var node = el("section", "overview-card");
        node.appendChild(el("h4", "", title));
        node.appendChild(el("span", "overview-state", state));
        node.appendChild(checklist(items));
        var buttons = el("div", "overview-actions");
        actions.forEach(function (action) {
          var button = el("button", "", action[1]);
          button.id = "settings-overview-" + action[0];
          button.type = "button";
          button.addEventListener("click", function () { goTab(action[0]); });
          buttons.appendChild(button);
        });
        node.appendChild(buttons);
        grid.appendChild(node);
      }
      function unknown(text) { return [{ done: false, text: text + " Saved configuration is unavailable." }]; }
      var model = known && byKey("daemon.executor_model_name"), modelSaved = known && settingIsSet("daemon.executor_model_name");
      card("Model", !known ? "Unknown" : modelSaved ? "Model name saved" : model ? "Choose a model" : "Not reported", known ? [
        { done: modelSaved, text: modelSaved ? "Main model name is saved." : model ? "Choose the model JARVIS should think with." : "The main model setting was not reported.", tab: "brain", tabLabel: "Choose model" },
        { done: keyIsSet("model"), text: keyIsSet("model") ? "Main provider key is saved; its value stays hidden." : "No saved main provider key is reported. Local providers may not need one." }
      ] : unknown("Model and provider key presence are unknown."), [["brain", "Model settings"]]);
      var folders = known && byKey("daemon.tool_workspace_roots");
      var roots = folders && settingIsSet(folders.key) ? String(folders.value).split(", ").filter(Boolean).length : 0;
      var codeSaved = known && (settingIsSet("daemon.code_agent") || settingIsSet("daemon.code_sandbox_image"));
      card("Folders & code", !known ? "Unknown" : roots ? roots + (roots === 1 ? " folder saved" : " folders saved") : folders ? "No folders saved" : "Not reported", known ? [
        { done: roots > 0, text: roots ? roots + (roots === 1 ? " folder grant is saved;" : " folder grants are saved;") + " no paths are displayed here." : folders ? "No folder grants are saved. Add only folders you want JARVIS to use." : "Folder grants were not reported.", tab: "files", tabLabel: "Review folders" },
        { done: codeSaved, text: codeSaved ? "A coding agent or sandbox image is saved; runtime availability is unverified." : "Coding agents and code sandboxes are optional." }
      ] : unknown("Folder grants and code configuration are unknown."), [["files", "Folders & code"]]);
      var googleSaved = known && settingIsSet("daemon.google_client_id") && keyIsSet("google");
      var telegramSaved = known && keyIsSet("telegram");
      var phoneSaved = known && ["daemon.phone_agent_id", "daemon.phone_number_id", "daemon.phone_owner_numbers"].every(settingIsSet);
      var accountCount = Number(googleSaved) + Number(telegramSaved) + Number(phoneSaved);
      card("Accounts & channels", known ? accountCount + (accountCount === 1 ? " saved setup entry" : " saved setup entries") : "Unknown", known ? [
        { done: googleSaved, text: googleSaved ? "Google client ID and secret are saved; account sign-in is unverified." : "Google client setup is incomplete or not reported.", tab: "google", tabLabel: "Google setup" },
        { done: telegramSaved, text: telegramSaved ? "Telegram key is saved; chat access is unverified." : "No saved Telegram key is reported.", tab: "telegram", tabLabel: "Telegram setup" },
        { done: phoneSaved, text: phoneSaved ? "Phone IDs and owner numbers are saved; calling is unverified." : "Phone IDs or owner numbers are missing or not reported.", tab: "phone", tabLabel: "Phone setup" }
      ] : unknown("Optional account and channel setup is unknown."), [["google", "Google"], ["telegram", "Telegram"], ["phone", "Phone"]]);
      var postures = known && sview.reply.postures;
      var overrides = postures ? Object.keys(postures).filter(function (key) { return postures[key] !== "default"; }).length : 0;
      card("Permissions", !known ? "Unknown" : !postures ? "Not reported" : overrides ? overrides + (overrides === 1 ? " saved override" : " saved overrides") : "Default choices", known ? [
        { done: !!postures, text: postures ? overrides + (overrides === 1 ? " tool posture override is saved." : " tool posture overrides are saved.") + " Deterministic rules still decide what may run." : "Saved tool posture choices were not reported." },
        { done: false, text: "Review tool-level choices and approval behavior before widening access.", tab: "permissions", tabLabel: "Review permissions" }
      ] : unknown("Saved permission choices are unknown; no permissions are changed here."), [["permissions", "Permissions"]]);
      card("Console", "Browser-local", [
        { done: true, text: "Accent, atmosphere, density and motion apply immediately, including offline." },
        { done: !appearanceUnsaved, text: appearanceUnsaved ? "Current appearance applies to this page only; browser storage is unavailable." : "Appearance stays in this browser, separate from daemon configuration." }
      ], [["console", "Customize Console"]]);
      body.appendChild(grid);
    }
    function settingsNav() {
      var nav = $("settings-tabs"), search = $("settings-search"), empty = $("settings-search-empty");
      if (!navGroups.length) {
        nav.replaceChildren();
        nav.setAttribute("role", "tablist");
        nav.setAttribute("aria-orientation", "vertical");
        if (!nav.getAttribute("aria-label")) nav.setAttribute("aria-label", "Settings sections");
        NAV.forEach(function (group) {
          var heading = el("div", "navgroup", group[0]);
          var entries = [];
          nav.appendChild(heading);
          group[1].forEach(function (tab) {
            var button = el("button");
            button.id = "settings-tab-" + tab[0];
            button.type = "button";
            button.setAttribute("role", "tab");
            button.setAttribute("aria-controls", "settings-body");
            button.appendChild(settingsIcon(tab[0], "settings-nav-icon"));
            button.appendChild(el("span", "settings-nav-label", tab[1]));
            var dot = el("span", "dot");
            dot.setAttribute("aria-hidden", "true");
            button.appendChild(dot);
            button.addEventListener("click", function () { goTab(tab[0]); });
            nav.appendChild(button);
            navButtons[tab[0]] = button;
            entries.push({ tab: tab, button: button, dot: dot });
          });
          navGroups.push({ heading: heading, entries: entries });
        });
        nav.addEventListener("keydown", function (event) {
          if (["ArrowUp", "ArrowDown", "Home", "End"].indexOf(event.key) < 0) return;
          var visible = settingsNav(), index = visible.indexOf(event.target);
          if (index < 0) return;
          event.preventDefault();
          if (event.key === "Home") index = 0;
          else if (event.key === "End") index = visible.length - 1;
          else index = (index + (event.key === "ArrowDown" ? 1 : -1) + visible.length) % visible.length;
          goTab(visible[index].id.replace("settings-tab-", ""));
        });
      }
      var mobilePick = $("settings-section-pick");
      if (mobilePick && !mobilePick.children.length) {
        TABS.forEach(function (tab) { var option = el("option", "", tab[1]); option.value = tab[0]; mobilePick.appendChild(option); });
        mobilePick.addEventListener("change", function () { goTab(mobilePick.value); });
      }
      if (mobilePick) mobilePick.value = sview.tab;
      var terms = ((search && search.value) || "").trim().toLowerCase().split(/\s+/).filter(Boolean);
      var visible = [];
      navGroups.forEach(function (group) {
        var shown = false;
        group.entries.forEach(function (entry) {
          var tab = entry.tab, button = entry.button;
          var groupForTab = { brain: "brain", voice: "voice", files: "files", alerts: "advanced", advanced: "advanced", phone: "phone", google: "google", telegram: "telegram" };
          var fields = ((sview.reply && sview.reply.settings) || []).filter(function (setting) { return setting.group === groupForTab[tab[0]]; }).map(function (setting) { return setting.key + " " + labelFor(setting) + " " + setting.help; }).join(" ");
          var text = (tab[1] + " " + tab[2] + " " + fields).toLowerCase();
          button.hidden = !terms.every(function (term) { return text.indexOf(term) >= 0; });
          if (!button.hidden) { visible.push(button); shown = true; }
          var selected = sview.tab === tab[0];
          button.className = selected ? "on" : "";
          button.setAttribute("aria-selected", selected ? "true" : "false");
          var state = sview.reply && !sview.error ? tabState(tab[0]) : "";
          var status = tab[0] === "console" ? "Local appearance preferences; no daemon connection required."
            : sview.error ? "Configuration unknown: daemon settings could not be loaded."
            : !sview.reply ? "Configuration unknown: daemon settings have not loaded."
            : tab[0] === "overview" ? "Saved configuration overview; runtime availability has not been checked."
            : state === "ok" ? "Configured in saved settings; runtime availability has not been checked."
            : state === "todo" ? "Not fully configured in saved settings; runtime availability has not been checked."
            : "Configuration status unknown; saved settings do not establish runtime availability.";
          entry.dot.className = "dot" + (state ? " " + state : "");
          entry.dot.title = status;
          button.title = tab[2] + " " + status;
          button.setAttribute("aria-label", tab[1] + ". " + status);
        });
        group.heading.hidden = !shown;
      });
      var stop = visible.indexOf(navButtons[sview.tab]) >= 0 ? navButtons[sview.tab] : visible[0];
      TABS.forEach(function (tab) { navButtons[tab[0]].tabIndex = navButtons[tab[0]] === stop ? 0 : -1; });
      if (empty) {
        empty.hidden = visible.length > 0;
        empty.textContent = visible.length ? "" : "No settings sections match your search.";
      }
      return visible;
    }
    function consolePanel(body) {
      var card = el("div", "card console-options");
      card.appendChild(el("div", "secthead", "Appearance"));
      card.appendChild(el("p", "help", "Stored only in this browser. These preferences do not change providers, permissions or daemon settings, and work while JARVIS is offline."));
      var preview = el("div", "appearance-preview");
      preview.setAttribute("role", "group");
      preview.setAttribute("aria-label", "Live appearance preview");
      var orbit = el("div", "preview-orbit"), core = el("div", "preview-core"), caption = el("span", "preview-caption");
      orbit.setAttribute("aria-hidden", "true");
      core.setAttribute("aria-hidden", "true");
      preview.appendChild(orbit); preview.appendChild(core); preview.appendChild(caption);
      card.appendChild(preview);
      card.appendChild(el("p", "help", "The preview follows your choices. Approval, listening and error colors keep their own meaning."));
      var note = el("p", "help" + (appearanceUnsaved ? " bad" : ""), appearanceNote);
      note.setAttribute("role", "status");
      var controls = {};
      function updatePreview() {
        var names = { mint: "Mint", ice: "Ice", amethyst: "Amethyst", cinematic: "Cinematic", quiet: "Quiet", comfortable: "Comfortable", compact: "Compact", system: "System motion", reduced: "Reduced motion" };
        caption.textContent = [appearance.accent, appearance.atmosphere, appearance.density, appearance.motion].map(function (key) { return names[key]; }).join(" / ");
        note.className = "help" + (appearanceUnsaved ? " bad" : "");
        note.textContent = appearanceNote;
      }
      function optionRow(key, title, options, hint) {
        var row = el("div", "set-row"), id = "settings-console-" + key;
        var label = el("label", "name", title);
        label.setAttribute("for", id);
        var select = el("select");
        select.id = id;
        select.dataset.autosave = "true";
        controls[key] = select;
        options.forEach(function (option) {
          var item = el("option", "", option[1]);
          item.value = option[0];
          select.appendChild(item);
        });
        select.value = appearance[key];
        var help = el("span", "help", hint);
        help.id = id + "-help";
        select.setAttribute("aria-describedby", help.id);
        select.addEventListener("change", function () {
          if (!validAppearance(key, select.value)) {
            select.value = appearance[key];
            note.textContent = "That appearance choice is not supported. The previous preference is unchanged.";
            note.className = "help bad";
            return;
          }
          appearance[key] = select.value;
          persistAppearance(false);
          updatePreview();
        });
        row.appendChild(label);
        row.appendChild(select);
        row.appendChild(help);
        return row;
      }
      card.appendChild(optionRow("accent", "Accent", [["mint", "Mint"], ["ice", "Ice"], ["amethyst", "Amethyst"]], "Tint the console and hologram. Status and approval colors are independent."));
      card.appendChild(optionRow("atmosphere", "Atmosphere", [["cinematic", "Cinematic"], ["quiet", "Quiet"]], "Cinematic adds ambient detail; quiet softens the background without changing functionality."));
      card.appendChild(optionRow("density", "Density", [["comfortable", "Comfortable"], ["compact", "Compact"]], "Comfortable gives controls more space; compact fits more on screen."));
      card.appendChild(optionRow("motion", "Motion", [["system", "Use system preference"], ["reduced", "Reduce motion"]], "System follows your device's reduced-motion preference. Reduce motion always minimizes animation."));
      var actions = el("div", "acts"), reset = el("button", "", "Reset appearance");
      reset.id = "settings-console-reset";
      reset.type = "button";
      reset.addEventListener("click", function () {
        appearance = Object.assign({}, appearanceDefaults);
        persistAppearance(true);
        Object.keys(controls).forEach(function (key) { controls[key].value = appearance[key]; });
        updatePreview();
      });
      actions.appendChild(reset);
      card.appendChild(actions);
      card.appendChild(note);
      updatePreview();
      body.appendChild(card);
    }
    var lastTab = "";
    // ---- layout: every section is one panel -----------------------------------------------------------------------------
    // The tab builders append headings, rows, notes and cards straight to the body. In a browser this pass groups them, after every render
    // and whenever something loads in later: a heading (section()) opens a panel, the rows after it go inside, a card stands as a panel of
    // its own, and loose rows before any heading share one untitled panel. It only moves the nodes the builders made (listeners and ids go
    // with them), never creates content, and does nothing a second time, so it is safe to run whenever the body changes.
    function isHeading(node) {
      return node.tagName === "DIV" && !node.className && node.firstElementChild && node.firstElementChild.classList.contains("secthead");
    }
    function standsAlone(node) {
      var cls = node.classList;
      return cls.contains("settings-hero") || cls.contains("spanel") || cls.contains("overview-grid") || cls.contains("card") ||
        (node.getAttribute("role") === "status") || !!(node.querySelector && node.querySelector(".integration-list, .overview-grid"));
    }
    function shapeSettingsBody(body) {
      var panel = null;
      Array.prototype.slice.call(body.children).forEach(function (node) {
        if (node.classList.contains("spanel") && node.dataset.shaped) { panel = node.querySelector(".spanel-body"); return; }
        if (isHeading(node)) {
          var wrap = el("section", "spanel");
          wrap.dataset.shaped = "1";
          var head = el("header", "spanel-head");
          var inner = el("div", "spanel-body");
          body.insertBefore(wrap, node);
          head.appendChild(node);
          wrap.appendChild(head);
          wrap.appendChild(inner);
          panel = inner;
          return;
        }
        if (node.classList.contains("card")) {
          // A card after a heading belongs to that section. On its own it becomes a section: its title (and the sentence under it) move
          // to the section's side column, and the card with everything else stays together beside it.
          if (panel) { panel.appendChild(node); return; }
          var section = el("section", "spanel");
          section.dataset.shaped = "1";
          var side = el("header", "spanel-head");
          var main = el("div", "spanel-body");
          body.insertBefore(section, node);
          var first = node.firstElementChild;
          var title = first && (first.tagName === "H4" || first.classList.contains("secthead") || isHeading(first)) ? first : null;
          if (title) {
            side.appendChild(title);
            var intro = node.firstElementChild;
            if (intro && intro.tagName === "P" && (intro.classList.contains("lead") || intro.classList.contains("secthint") || intro.classList.contains("help"))) side.appendChild(intro);
            section.appendChild(side);
          }
          main.appendChild(node);
          section.appendChild(main);
          panel = null;
          return;
        }
        if (standsAlone(node)) { panel = null; return; }
        if (!panel) {
          var loose = el("section", "spanel");
          loose.dataset.shaped = "1";
          panel = el("div", "spanel-body");
          loose.appendChild(panel);
          body.insertBefore(loose, node);
        }
        panel.appendChild(node);
      });
    }
    if (typeof MutationObserver === "function") {
      var shaping = false;
      new MutationObserver(function () {
        if (shaping) return;
        shaping = true;
        var body = $("settings-body"), top = body.scrollTop;
        shapeSettingsBody(body);
        body.scrollTop = top;
        Promise.resolve().then(function () { shaping = false; });
      }).observe($("settings-body"), { childList: true });
    }

    function renderSettings() {
      var body = $("settings-body");
      if (openPicker) openPicker.close();
      // A switch or a save redraws the tab; stay where the person was scrolling instead of jumping to the top.
      var scrolled = lastTab === sview.tab ? body.scrollTop : 0;
      var focused = document.activeElement;
      var focusId = lastTab === sview.tab && focused && body.contains(focused) ? focused.id : "";
      var selection = focused && typeof focused.selectionStart === "number" ? [focused.selectionStart, focused.selectionEnd] : null;
      clearSecrets();
      paintDrafts();
      if (catalogUnmount) { catalogUnmount(); catalogUnmount = null; }
      controlNumber = 0;
      lastTab = sview.tab;
      settingsNav();
      body.replaceChildren();
      body.setAttribute("role", "tabpanel");
      body.setAttribute("aria-labelledby", "settings-tab-" + sview.tab);
      body.tabIndex = 0;
      var current = TABS.filter(function (tab) { return tab[0] === sview.tab; })[0];
      var sectionLabel = $("settings-section-label");
      if (sectionLabel) sectionLabel.textContent = current[1];
      body.appendChild(settingsHero(current));
      if (sview.tab === "overview") {
        overviewPanel(body);
        body.scrollTop = scrolled;
        if (focusId.indexOf("settings-overview-") === 0 && $(focusId)) $(focusId).focus();
        return;
      }
      if (sview.tab === "console") {
        consolePanel(body);
        body.scrollTop = scrolled;
        return;
      }
      if (!sview.reply || sview.error) {
        var unavailable = el("p", "help" + (sview.error ? " bad" : ""), sview.error || "Loading daemon settings. Console appearance preferences are available without a daemon connection.");
        unavailable.setAttribute("role", "status");
        body.appendChild(unavailable);
        body.scrollTop = scrolled;
        return;
      }
      var settings = sview.reply.settings || [];
      var inGroup = function (name) { return settings.filter(function (s) { return s.group === name && !/(_api_key_ref|_secret_ref)$/.test(s.key); }); };
      if (sview.tab === "brain") {
        body.appendChild(providerCard("main"));
        body.appendChild(providerCard("fallback"));
        body.appendChild(section("Behaviour and limits"));
        var effort = byKey("daemon.executor_reasoning_effort");
        if (effort) body.appendChild(choiceRow(effort, "Thinking effort", [["", "Let the model decide"], ["none", "None"], ["low", "Low (faster, cheaper)"], ["medium", "Medium"], ["high", "High (slower, more thorough)"]]));
        var tokenLimit = byKey("daemon.workspace_daily_token_limit");
        if (tokenLimit) body.appendChild(settingRow(tokenLimit, "Daily token limit"));
        var special = [byKey("daemon.executor_subagent_model_name"), byKey("daemon.executor_background_model_name")].filter(Boolean);
        if (special.length) {
          var more = el("details");
          more.open = special.some(isSetting);
          more.appendChild(el("summary", "", "Different models for sub-agents and scheduled runs (optional)"));
          more.appendChild(el("p", "secthint", "By default every run uses the main model. A smaller, cheaper one is often enough for background work. These are models at the main provider."));
          special.forEach(function (s) { more.appendChild(settingRow(s)); });
          body.appendChild(more);
        }
      } else if (sview.tab === "voice") {
        var voiceOn = keyIsSet("voice");
        body.appendChild(section("Voice", voiceOn ? "Using the ElevenLabs voice below (after a restart). The same key lets the Phone tab place calls."
          : "Using your browser's own voice. Add an ElevenLabs key for a natural one; it is also what the Phone tab needs."));
        body.appendChild(keyRow("voice", "ElevenLabs key"));
        var voiceSettings = inGroup("voice");
        if (voiceSettings.length) {
          var voiceMore = el("details");
          voiceMore.open = voiceSettings.some(isSetting);
          voiceMore.appendChild(el("summary", "", "Choose another voice or model (optional)"));
          voiceMore.appendChild(el("p", "secthint", "The defaults are right unless you want a different voice. Copy a voice id from the voice library in your ElevenLabs account. They only apply once a key is set."));
          voiceSettings.forEach(function (s) { voiceMore.appendChild(settingRow(s)); });
          body.appendChild(voiceMore);
        }
        body.appendChild(section("Web search", keyIsSet("search") ? "JARVIS can search the web." : "Off. Lets JARVIS search the web when you ask a question it cannot answer from memory. Uses an Ollama key: a free account at ollama.com makes one."));
        body.appendChild(keyRow("search", "Search key"));
      } else if (sview.tab === "files") {
        body.appendChild(foldersCard());
        if (byKey("daemon.code_agent")) body.appendChild(codingAgentCard());
        if (byKey("daemon.code_sandbox_image")) body.appendChild(codeCard());
      } else if (sview.tab === "programs") {
        programsPanel(body);
      } else if (sview.tab === "apps") {
        appsPanel(body);
      } else if (sview.tab === "connect") {
        connectPanel(body);
      } else if (sview.tab === "permissions") {
        if (sview.toolsError || !sview.toolsLoaded) {
          var toolsNote = el("p", "help" + (sview.toolsError ? " bad" : ""), sview.toolsError || "Loading tool permissions.");
          toolsNote.setAttribute("role", "status");
          body.appendChild(toolsNote);
        } else permissionsPanel(body);
      } else if (sview.tab === "connections") {
        connectionsPanel(body);
      } else if (sview.tab === "google") {
        googlePanel(body);
      } else if (sview.tab === "telegram") {
        telegramPanel(body);
      } else if (sview.tab === "phone") {
        phonePanel(body);
      } else if (sview.tab === "alerts") {
        body.appendChild(section("When the console is closed", "JARVIS can tell you a task finished or that it needs your approval. Telegram and the phone are set up in their own tabs."));
        ["daemon.notifications", "daemon.push_topic", "daemon.push_server"].forEach(function (key) {
          var s = byKey(key);
          if (s) body.appendChild(rowFor(s));
        });
        body.appendChild(section("Quiet hours", "A daily window in which finished tasks and approvals are held back. Work still goes on; everything held is announced when the window ends."));
        var quiet = byKey("daemon.quiet_hours");
        if (quiet) body.appendChild(settingRow(quiet));
      } else {
        var claimed = ["daemon.mcp_serve_port", "daemon.notifications", "daemon.push_topic", "daemon.push_server", "daemon.quiet_hours"];
        body.appendChild(section("JARVIS itself"));
        inGroup("advanced").filter(function (s) { return claimed.indexOf(s.key) < 0; }).forEach(function (s) { body.appendChild(rowFor(s)); });
        body.appendChild(section("For developers", "Let other apps call JARVIS under External access. Add services JARVIS can call under Apps & tools."));
      }
      restoreDrafts(body);
      body.scrollTop = scrolled;
      if (focusId && $(focusId)) { $(focusId).focus(); if (selection && $(focusId).setSelectionRange) $(focusId).setSelectionRange(selection[0], selection[1]); }
    }
    function loadSettings() {
      var request = ++settingsLoad;
      sview.error = "";
      sview.toolsError = "";
      function refresh() {
        paintRestart();
        // Daemon replies must not replace a local control while the person is changing it.
        if (sview.tab === "console") settingsNav(); else renderSettings();
      }
      api("/settings").then(function (reply) {
        if (request !== settingsLoad) return;
        sview.reply = reply;
        if (Array.isArray(reply.restart_changes)) pendingRestart = reply.restart_changes.map(function (key) { return LABELS[key] || key.replace(/^daemon\./, "").replace(/_/g, " "); }).concat(Object.keys(restartHints));
        sview.error = "";
        refresh();
      }, function (error) {
        if (request !== settingsLoad) return;
        sview.reply = null;
        sview.error = "Could not load daemon settings (" + ((error && error.status) || "no reply") + "). Configuration is unknown. Console appearance preferences remain available locally.";
        if (sview.tab !== "console") settingsNote(sview.error, true);
        refresh();
      });
      api("/tools").then(function (reply) {
        if (request !== settingsLoad) return;
        sview.tools = reply.tools || [];
        sview.toolsLoaded = true;
        if (sview.tab === "permissions") renderSettings();
      }, function (error) {
        if (request !== settingsLoad) return;
        sview.tools = [];
        sview.toolsLoaded = false;
        sview.toolsError = "Could not load tool permissions (" + ((error && error.status) || "no reply") + "). Their current configuration is unknown.";
        if (sview.tab === "permissions") renderSettings();
      });
    }
    function openSettings() {
      $("settings").hidden = false;
      settingsNote("");
      paintRestart();
      renderSettings();
      focusSettingsTab();
      loadSettings();
    }
    function closeSettings() { if (openPicker) openPicker.close(); clearSecrets(); $("settings").hidden = true; if (typeof checkAttention === "function") checkAttention(); }
    // Things that have stopped working while nobody is looking at Settings: the internet address, the phone door, Telegram. Each is
    // asked from the daemon's own status routes (no provider is called). Only a real breakage is raised, and quietly: a pill in the
    // header ("1 issue") that opens a small panel with Check again, the tab that fixes it, and Hide. A step that is simply not set up
    // yet (the door waiting for an internet address that is switched off) is not an alarm; the Phone tab shows it where it is set up.
    // The daemon retries these by itself, so the page asks again every 20 seconds, when the tab is shown again and when Settings closes.
    var attention = $("attention"), issuesPill = $("pill-issues");
    var issues = [], hiddenIssues = {}, attentionOpen = false, attentionNote = "", checking = null;
    function issueKey(item) { return item.tab + "|" + item.text; }
    function setAttentionOpen(open) {
      attentionOpen = open && issues.length > 0;
      if (issuesPill) issuesPill.setAttribute("aria-expanded", attentionOpen ? "true" : "false");
      paintAttention();
    }
    function paintAttention() {
      var visible = issues.filter(function (item) { return !hiddenIssues[issueKey(item)]; });
      attention.replaceChildren();
      if (issuesPill) {
        issuesPill.hidden = !visible.length && !attentionNote;
        issuesPill.lastChild.textContent = visible.length ? visible.length + (visible.length === 1 ? " issue" : " issues") : "all clear";
        issuesPill.className = "pill " + (visible.length ? "wait issues" : "on");
        attention.hidden = !attentionOpen || (!visible.length && !attentionNote);
      } else {
        attention.hidden = !visible.length;
      }
      visible.forEach(function (item) {
        var line = el("div", "attline");
        line.appendChild(el("span", "", item.text));
        var acts = el("span", "attacts");
        var again = el("button", "link", checking ? "Checking\u2026" : "Check again");
        again.type = "button";
        again.disabled = !!checking;
        again.addEventListener("click", function () { checkAttention(true); });
        var fix = el("button", "link", "Open " + item.label);
        fix.type = "button";
        fix.addEventListener("click", function () { setAttentionOpen(false); openSettings(); goTab(item.tab); });
        var hide = el("button", "link", "Hide");
        hide.type = "button";
        hide.title = "Hide this until it changes or the page is reloaded";
        hide.addEventListener("click", function () { hiddenIssues[issueKey(item)] = true; attentionNote = ""; paintAttention(); if (issuesPill && !issuesPill.hidden) issuesPill.focus(); });
        acts.appendChild(again);
        acts.appendChild(fix);
        acts.appendChild(hide);
        line.appendChild(acts);
        attention.appendChild(line);
      });
      if (attentionNote) attention.appendChild(el("div", "attnote", attentionNote));
    }
    function checkAttention(manual) {
      if (checking) return checking;
      var items = [];
      var none = function () {};
      var before = {};
      issues.forEach(function (item) { before[issueKey(item)] = item.label; });
      if (manual) { attentionNote = ""; checking = true; paintAttention(); }
      checking = Promise.all([
        api("/mcp/access").then(function (info) {
          var pub = info.public || {};
          if (pub.wanted && pub.status && pub.status.state === "problem") {
            items.push({ tab: "connect", label: "External access", text: "The internet address is not working: " + (pub.status.reason || "it could not start") + "." });
          }
        }, none),
        api("/phone").then(function (phone) {
          var door = phone.door || {};
          var settingUp = /internet address/i.test(door.problem || "");
          if (door.on && door.state === "problem" && !settingUp) {
            items.push({ tab: "phone", label: "Phone", text: "The phone door is not working (" + (door.problem || "unknown reason") + "), so calls to JARVIS may get no answer." });
          }
        }, none),
        api("/telegram").then(function (telegram) {
          if (telegram.state === "error") items.push({ tab: "telegram", label: "Telegram", text: telegram.problem || "Telegram is not working." });
        }, none)
      ]).then(function () {
        checking = null;
        var now = {};
        items.forEach(function (item) { now[issueKey(item)] = true; });
        // A hidden issue comes back only if it changes, so forget the ones that went away.
        Object.keys(hiddenIssues).forEach(function (key) { if (!now[key]) delete hiddenIssues[key]; });
        var fixed = Object.keys(before).filter(function (key) { return !now[key]; }).map(function (key) { return before[key]; });
        issues = items;
        if (manual) attentionNote = fixed.length ? fixed.join(", ") + ": working again." : items.length ? "Checked just now: still not working. JARVIS keeps retrying by itself." : "Checked just now: everything is working.";
        else if (fixed.length && attentionOpen) attentionNote = fixed.join(", ") + ": working again.";
        if (!items.length && attentionNote) setTimeout(function () { if (!issues.length) { attentionNote = ""; setAttentionOpen(false); } }, 4000);
        paintAttention();
      });
      return checking;
    }
    if (issuesPill) {
      issuesPill.addEventListener("click", function () { setAttentionOpen(!attentionOpen); });
      document.addEventListener("click", function (event) {
        // A button inside the panel may have been redrawn by the time this runs; a detached target was inside, not outside.
        if (!event.target || event.target.isConnected === false) return;
        if (attentionOpen && !attention.contains(event.target) && !issuesPill.contains(event.target)) setAttentionOpen(false);
      });
      document.addEventListener("keydown", function (event) {
        if (event.key === "Escape" && attentionOpen) { setAttentionOpen(false); issuesPill.focus(); event.stopPropagation(); }
      }, true);
    }
    if (document.addEventListener) document.addEventListener("visibilitychange", function () { if (!document.hidden) checkAttention(); });
    setTimeout(checkAttention, 2500);
    setInterval(checkAttention, 20000);
    $("open-settings").addEventListener("click", openSettings);
    $("settings-close").addEventListener("click", closeSettings);
    if ($("settings-discard")) $("settings-discard").addEventListener("click", function () {
      drafts = {}; clearSecrets(); renderSettings(); settingsNote("Unsaved drafts discarded. Saved settings are unchanged.");
    });
    var settingsSearch = $("settings-search"), settingsEmpty = $("settings-search-empty");
    if (settingsSearch) {
      settingsSearch.setAttribute("aria-controls", "settings-tabs");
      settingsSearch.addEventListener("input", settingsNav);
    }
    if (settingsEmpty) settingsEmpty.setAttribute("role", "status");
    $("settings").addEventListener("click", function (event) { if (event.target === $("settings")) closeSettings(); });
    function dismissPickerOutside(event) { if (openPicker && !openPicker.contains(event.target)) openPicker.close(); }
    document.addEventListener("mousedown", dismissPickerOutside);
    document.addEventListener("focusin", dismissPickerOutside);
    document.addEventListener("keydown", function (event) {
      if (event.key === "Escape" && !$("settings").hidden) {
        // Escape closes an open model list first, and the dialog only when none is open.
        if (openPicker) openPicker.close(); else closeSettings();
        event.stopPropagation();
      }
    }, true);
    // A restart that would interrupt working tasks is refused by the daemon, which says how many; the button then offers to do it anyway.
    var restartForce = false;
    $("settings-restart").addEventListener("click", function () {
      var button = $("settings-restart");
      if (!pendingRestart.length || !sview.reply || sview.error || restartInFlight) return;
      restartInFlight = true;
      paintRestart();
      settingsNote("Restarting. This page reconnects by itself in a few seconds.");
      putJson("/restart" + (restartForce ? "?force=true" : ""), "POST").then(function () {
        settingsNote("Restart requested. Reopen settings after reconnection to verify that the saved changes are applied.");
        restartInFlight = false;
        paintRestart();
        restartForce = false;
        button.textContent = "Restart to apply";
        setTimeout(closeSettings, 1500);
      }, function (error) {
        restartInFlight = false;
        paintRestart();
        if (error.status === 409) {
          restartForce = true;
          button.textContent = "Restart anyway";
          settingsNote((error.body || "Tasks are still working") + ". Wait for them to finish, or press Restart anyway to interrupt them.", true);
        } else {
          settingsNote(explain(error, "Could not restart from here; run `jarvis restart`."), true);
        }
      });
    });
    paintRestart();
    return { mountCodingTasks: mountCodingTasks };
};
}());
