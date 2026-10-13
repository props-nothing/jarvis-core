"use strict";
// The console's settings: the five tabs, the permissions and Google sign-in (split out of hud.js).
//
// `JarvisSettings(context)` is called once by hud.js at load. It takes the few helpers it shares with the console and wires the
// settings dialog itself; nothing else in the console calls into it. The same security rules as hud.js hold: every value from the
// daemon is inserted as text, never as markup, and a key is sent once from a password field and never read back.
window.JarvisSettings = function (context) {
  var $ = context.$, el = context.el, api = context.api, putJson = context.putJson, explain = context.explain;
  // Called when needed, not captured now: the tools list helpers are declared in hud.js after the point this is created.
  function families(tools) { return context.families(tools); }
  function familyLabel(group) { return context.familyLabel(group); }
    // ---- settings: five tabs, each saying what is set, what the default is, and why a setting is off -------------------------
    // The same checks as `jarvis config` and `jarvis keys` (one code path in the daemon). A key is typed into a password field,
    // sent once, and the field is cleared; the daemon never sends it back, only whether it is set.
    // The tabs are grouped in a side list. Each has a one-line lead and, where it can be told from the saved settings, a dot:
    // green when it is set up, amber when it is not.
    var NAV = [
      ["JARVIS", [
        ["brain", "Brain", "Which model JARVIS thinks with, and a second one for when the first is limited or down."],
        ["voice", "Voice & search", "How JARVIS sounds, and how it searches the web. Both are optional."],
        ["files", "Folders & code", "What JARVIS may touch on this computer. Both are off until you turn them on, so nothing is reachable by accident."],
        ["permissions", "Permissions", "For each tool: let the rules decide, always ask, run without asking, or switch it off."]
      ]],
      ["Accounts & channels", [
        ["google", "Google", "Sign in with your Google account so JARVIS can read your mail and calendar."],
        ["telegram", "Telegram", "Talk to JARVIS from your phone. It answers in a Telegram chat only you can use, and asks you there when it needs your approval."],
        ["phone", "Phone", "JARVIS rings you when it needs you, and you can ring JARVIS. A voice agent at ElevenLabs does the talking; the tools stay with JARVIS."]
      ]],
      ["Connections", [
        ["connect", "Connect apps", "Let your other AI apps (Claude Code, Cursor, VS Code, your own scripts) and cloud agents hand JARVIS a task. They connect with a key only you have."]
      ]],
      ["System", [
        ["alerts", "Alerts", "How JARVIS reaches you when the console is closed, and when it keeps quiet."],
        ["advanced", "Advanced", "Ports and other technical settings. The defaults are right for almost everyone."]
      ]]
    ];
    var TABS = NAV.reduce(function (all, group) { return all.concat(group[1]); }, []);
    var sview = { reply: null, tools: [], tab: "brain" };
    function settingsNote(text, bad) {
      var note = $("settings-note");
      note.textContent = text || "";
      note.className = bad ? "bad" : "ok";
    }
    function failure(error) {
      settingsNote("Could not do that (" + (error.status || "no reply") + ")", true);
    }
    // Changes that only take effect after a restart are listed in the footer, so it is always clear whether one is waiting.
    var pendingRestart = [];
    function paintRestart() {
      var hint = $("settings-hint"), button = $("settings-restart");
      var waiting = pendingRestart.length > 0;
      hint.textContent = waiting ? "Waiting for a restart: " + pendingRestart.join(", ") + "." : "Nothing is waiting for a restart.";
      hint.className = "dim" + (waiting ? " hot" : "");
      button.className = waiting ? "hot" : "";
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
    function goTab(id) { sview.tab = id; settingsNote(""); renderSettings(); }
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
      if (id === "brain") return settingIsSet("daemon.executor_model_name") ? "ok" : "todo";
      if (id === "voice") return keyIsSet("voice") || keyIsSet("search") ? "ok" : "";
      if (id === "files") return settingIsSet("daemon.tool_workspace_roots") || settingIsSet("daemon.code_sandbox_image") || settingIsSet("daemon.code_agent") ? "ok" : "";
      if (id === "google") return settingIsSet("daemon.google_client_id") && keyIsSet("google") ? "ok" : "todo";
      if (id === "telegram") return keyIsSet("telegram") ? "ok" : "todo";
      if (id === "phone") return settingIsSet("daemon.phone_agent_id") && settingIsSet("daemon.phone_number_id") && settingIsSet("daemon.phone_owner_numbers") ? "ok" : "todo";
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
      return el("span", "badge " + (set && !atDefault ? "on" : "off"), atDefault ? "default" : set ? "set" : setting.default ? "default" : "off");
    }
    // One setting: its field, Save (and Unset when set), and either its help (when set) or what "unset" means (when not).
    var LABELS = {
      "daemon.executor_model_name": "Model", "daemon.executor_base_url": "Model server", "daemon.executor_reasoning_effort": "Thinking effort",
      "daemon.executor_subagent_model_name": "Sub-agent model", "daemon.executor_background_model_name": "Scheduled-run model",
      "daemon.workspace_daily_token_limit": "Daily token limit",
      "daemon.tool_workspace_roots": "Folders", "daemon.code_sandbox_image": "Sandbox image", "daemon.code_sandbox_interpreter": "Interpreter",
      "daemon.speech_voice_id": "Voice", "daemon.speech_model": "Voice model", "daemon.http_port": "Console port",
      "daemon.mcp_serve_port": "Let other apps use JARVIS (port)", "daemon.notifications": "Desktop notifications", "daemon.google_client_id": "Client ID",
      "daemon.google_actions": "Send mail & add events", "policy.trust": "Runs without asking",
      "daemon.quiet_hours": "Quiet hours", "daemon.resume_interrupted": "Resume cut-off tasks", "daemon.push_topic": "Push topic (ntfy)", "daemon.push_server": "Push server (ntfy)",
      "daemon.phone_agent_id": "Agent id", "daemon.phone_number_id": "Phone number id", "daemon.phone_owner_numbers": "My numbers",
      "daemon.phone_approves": "My yes on the phone counts", "daemon.phone_opening_line": "JARVIS writes the opening line", "daemon.phone_call_anyone": "JARVIS may call other people", "daemon.phone_screen_callers": "Answer only my own numbers",
      "daemon.phone_calls_per_hour": "Calls per hour", "daemon.phone_calls_per_day": "Calls per day"
    };
    // A one-line statement of what an on/off switch does, shown instead of the long explanation (which is one click away under More).
    var SHORT = {
      "daemon.google_actions": "Let JARVIS send mail and add calendar events. It asks you every time.",
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
    function settingRow(setting, label, live) {
      var row = el("div", "set-row");
      var name = el("span", "name", labelFor(setting, label));
      name.title = setting.key;
      name.appendChild(stateBadge(setting));
      var field = el("input");
      field.type = "text";
      field.value = showFor(setting);
      field.placeholder = setting.default ? "default: " + setting.default : (setting.example ? "e.g. " + setting.example : "(not set)");
      var acts = el("div", "acts");
      var save = el("button", "", "Save");
      save.addEventListener("click", function () {
        var values = valuesFor(setting.kind, field.value);
        if (!values.length || !values[0]) { settingsNote("Enter a value, or use Unset.", true); return; }
        putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: values }).then(function () {
          afterSave(setting, live, labelFor(setting, label));
        }, function (error) { settingsNote(explain(error, "That value was refused."), true); });
      });
      acts.appendChild(save);
      if (isSetting(setting)) {
        var unset = el("button", "", "Unset");
        unset.addEventListener("click", function () {
          putJson("/settings/" + encodeURIComponent(setting.key), "DELETE").then(function () {
            afterSave(setting, live, labelFor(setting, label) + " removed");
          },
            function (error) { settingsNote(explain(error, "That cannot be removed on its own."), true); });
        });
        acts.appendChild(unset);
      }
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(acts);
      row.appendChild(helpNode(isSetting(setting) ? setting.help : setting.unset_means));
      return row;
    }
    // An on/off setting is a switch that saves as soon as it is flipped. The state shown is the one in force: what was saved, or the default.
    function toggleRow(setting, label, live) {
      var row = el("div", "set-row toggle");
      var name = el("span", "name", labelFor(setting, label));
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
      row.appendChild(help);
      return row;
    }
    // A setting with a few fixed answers is a list to pick from, not a box to type a word into.
    function choiceRow(setting, label, options, live) {
      var row = el("div", "set-row");
      var name = el("span", "name", labelFor(setting, label));
      name.title = setting.key;
      name.appendChild(stateBadge(setting));
      var select = el("select");
      options.forEach(function (option) { var node = el("option", "", option[1]); node.value = option[0]; select.appendChild(node); });
      select.value = isSetting(setting) ? String(setting.value) : "";
      if (select.value !== (isSetting(setting) ? String(setting.value) : "")) select.value = "";
      select.addEventListener("change", function () {
        var request = select.value ? putJson("/settings/" + encodeURIComponent(setting.key), "PUT", { values: [select.value] })
          : putJson("/settings/" + encodeURIComponent(setting.key), "DELETE");
        request.then(function () { afterSave(setting, live, labelFor(setting, label)); },
          function (error) { settingsNote(explain(error, "That was refused."), true); loadSettings(); });
      });
      row.appendChild(name);
      row.appendChild(select);
      row.appendChild(el("span"));
      row.appendChild(helpNode(isSetting(setting) ? setting.help : setting.unset_means));
      return row;
    }
    // Any setting in the right shape for its kind.
    function rowFor(setting, label, live) {
      return setting.kind === "flag" ? toggleRow(setting, label, live) : settingRow(setting, label, live);
    }
    function keyRow(which, label) {
      var state = sview.reply && sview.reply.keys && sview.reply.keys[which] ? sview.reply.keys[which].state : "unknown";
      var row = el("div", "set-row");
      var name = el("span", "name", label);
      name.appendChild(el("span", "badge " + (state === "set" ? "on" : "off"), state));
      var field = el("input");
      field.type = "password";
      field.autocomplete = "off";
      field.placeholder = state === "set" ? "set; paste a new key to replace it" : "paste a key";
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
      row.appendChild(helpNode(helps[which] || ""));
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
    function providerRow(label, control, acts, help) {
      var row = el("div", "set-row");
      row.appendChild(el("span", "name", label));
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
      var toggle = el("button", "combo-toggle", "\u25BE");
      toggle.type = "button";
      toggle.title = "Show the models";
      var menu = el("ul", "combo-list");
      menu.hidden = true;
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
        menu.replaceChildren();
        if (!models.length) {
          menu.appendChild(el("li", "note", "Press Load models to see what this provider offers, or type a model name."));
        } else if (!shown.length) {
          menu.appendChild(el("li", "note", "No model matches. Keep typing to use that name as it is."));
        }
        shown.forEach(function (m, index) {
          var item = el("li", (m.likely_chat ? "" : "dim ") + (m.id === input.value ? "current " : "") + (index === active ? "active" : ""), m.id + (m.likely_chat ? "" : "   (probably not a chat model)"));
          item.addEventListener("mousedown", function (event) { event.preventDefault(); choose(m.id); });
          menu.appendChild(item);
          if (index === active) item.scrollIntoView({ block: "nearest" });
        });
      }
      function open() {
        if (openPicker && openPicker !== api2) openPicker.close();
        openPicker = api2;
        menu.hidden = false;
        draw();
      }
      function close() {
        menu.hidden = true;
        active = -1;
        typed = false;
        if (openPicker === api2) openPicker = null;
      }
      function choose(id) { input.value = id; close(); input.dispatchEvent(new Event("change")); }
      toggle.addEventListener("mousedown", function (event) { event.preventDefault(); if (menu.hidden) { input.focus(); open(); } else close(); });
      input.addEventListener("focus", function () { typed = false; active = -1; open(); });
      input.addEventListener("click", function () { if (menu.hidden) open(); });
      input.addEventListener("input", function () { typed = true; active = -1; if (menu.hidden) open(); else draw(); });
      input.addEventListener("keydown", function (event) {
        var shown = visible();
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          if (menu.hidden) open();
          if (!shown.length) return;
          active = (active + (event.key === "ArrowDown" ? 1 : -1) + shown.length) % shown.length;
          draw();
        } else if (event.key === "Enter" && !menu.hidden && active >= 0 && shown[active]) {
          event.preventDefault();
          choose(shown[active].id);
        } else if (event.key === "Tab") {
          close();
        }
      });
      document.addEventListener("mousedown", function (event) { if (!menu.hidden && !wrap.contains(event.target)) close(); });
      var api2 = {
        element: wrap,
        close: close,
        // Sets the models on offer; `show` opens the list so the person sees it arrive.
        setModels: function (list, show) {
          models = list || [];
          if (show && models.length) { input.focus(); open(); } else if (!menu.hidden) draw();
        }
      };
      return api2;
    }    function providerCard(slot) {
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
        ? "Asked instead when the main model is rate limited, overloaded, down or out of credit. A different provider keeps JARVIS working through an outage; what a run is saying is then sent to that provider."
        : "The model JARVIS thinks with. Pick a provider, paste its key, load the list of models, and test before saving."));

      var select = el("select");
      presets.forEach(function (p) { var option = el("option", "", p.name); option.value = p.id; select.appendChild(option); });
      var address = el("input");
      address.type = "text";
      var keyBox = el("input");
      keyBox.type = "password";
      keyBox.autocomplete = "off";
      var model = el("input");
      model.type = "text";
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
        if (keyRowNode) keyRowNode.hidden = isSame();
      }
      var addressRow = null, keyRowNode = null;
      function say(text, bad) { status.textContent = text; status.className = "help " + (bad ? "bad" : ""); }
      select.addEventListener("change", function () {
        var p = chosen();
        if (p.id !== "custom") address.value = p.url;
        if (p.id === "custom" && !addressChanged()) address.value = "";
        picker.setModels([], false);
        say("");
        refreshFields();
      });
      address.addEventListener("input", function () {
        var p = providerFor(address.value);
        if (!isSame() && p) select.value = p.id;
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
      save.addEventListener("click", function () {
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
        var keyStep = function () {
          if (typed && !isSame() && p.needs_key) return putJson("/settings/keys/" + which, "PUT", { key: typed });
          if (!isSame() && !p.needs_key && (addressChanged() || keyState !== "set")) return putJson("/settings/keys/" + which, "PUT", { key: "ollama" });
          if (isSame() && fallback && keyState === "set") return putJson("/settings/keys/fallback", "DELETE");
          return Promise.resolve();
        };
        putJson("/settings", "PUT", { changes: changes }).then(keyStep).then(function () {
          keyBox.value = "";
          saved(fallback ? "Fallback model" : "Main model");
        }, function (error) { say(explain(error, "That was refused."), true); });
      });
      var acts = el("div", "acts");
      acts.appendChild(load); acts.appendChild(test); acts.appendChild(save);

      addressRow = providerRow("Address", address);
      keyRowNode = providerRow("Key" + (keyState === "set" ? " (set)" : ""), keyBox);
      card.appendChild(providerRow("Provider", select, null, providerHelp));
      card.appendChild(addressRow);
      card.appendChild(keyRowNode);
      card.appendChild(providerRow("Model", picker.element, acts, status));
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
      row.appendChild(el("span", "name", "Image"));
      var imageField = el("input");
      imageField.type = "text"; imageField.value = showFor(image); imageField.placeholder = "e.g. " + image.example;
      row.appendChild(imageField);
      row.appendChild(el("span"));
      var row2 = el("div", "set-row");
      row2.appendChild(el("span", "name", "Command"));
      var commandField = el("input");
      commandField.type = "text"; commandField.value = showFor(command); commandField.placeholder = "e.g. " + command.example;
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
      card.appendChild(el("p", "lead", "Hands a bigger coding task to an agent you already use (ChatGPT/Codex, Claude, Copilot, Gemini and others). It works in its own copy of your repository on a new branch, so your own files are not touched; JARVIS asks you first and never merges or pushes. It needs a folder above and git."));
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
          { done: info.needs.folder, text: "A folder is granted (above), with your repository in it." },
          { done: info.needs.git, text: info.needs.git ? "git is installed." : "git is not installed: install git first." },
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
        // Names of environment variables the agent may use (never values).
        var envRow = el("div", "set-row");
        envRow.appendChild(el("span", "name", "Environment names"));
        var envField = el("input");
        envField.type = "text"; envField.autocomplete = "off"; envField.spellcheck = false;
        envField.value = (info.env || []).join(" ");
        envField.placeholder = picked && picked.env.length ? picked.env.join(" ") : "OPENAI_API_KEY";
        envRow.appendChild(envField);
        var envActs = el("div", "acts");
        var envSave = el("button", "", "Save names");
        envSave.type = "button";
        envSave.addEventListener("click", function () {
          var names = envField.value.split(/[\s,]+/).filter(Boolean);
          store([{ key: "daemon.code_agent_env", values: names.length ? names : null }]);
        });
        envActs.appendChild(envSave);
        envRow.appendChild(envActs);
        holder.appendChild(envRow);
        holder.appendChild(el("p", "secthint", "Only the names of variables, never their values. The agent gets a small safe set (path, home, temp) and the names you list here, and nothing else of JARVIS's; most agents keep their own sign-in and need none."));
        // Any other agent that speaks ACP.
        var more = el("details");
        more.open = chosen === "custom";
        more.appendChild(el("summary", "", "Another agent (a command)"));
        var customRow = el("div", "set-row");
        customRow.appendChild(el("span", "name", "Command"));
        var customField = el("input");
        customField.type = "text"; customField.autocomplete = "off"; customField.spellcheck = false;
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
        more.appendChild(el("p", "secthint", "Any agent that speaks the Agent Client Protocol over standard input and output works. It is started with this command, no shell, in a copy of your repository."));
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
      var tasks = el("div");
      card.appendChild(tasks);
      function paintTasks(reply) {
        tasks.replaceChildren();
        var found = reply.tasks || [];
        if (!found.length) return;
        tasks.appendChild(el("h4", "", "Background coding tasks"));
        found.slice(0, 10).forEach(function (task) {
          var row = el("div", "set-row");
          row.appendChild(el("span", "name", task.repository));
          var middle = el("div");
          middle.appendChild(el("span", "help", task.task));
          if (task.branch) middle.appendChild(el("code", "", task.branch));
          if (task.error) middle.appendChild(el("span", "help bad", task.error));
          row.appendChild(middle);
          row.appendChild(el("span", "badge " + (task.status === "completed" ? "on" : "off"), task.status.replace("_", " ")));
          tasks.appendChild(row);
        });
        tasks.appendChild(el("p", "secthint", "Start a longer task by asking JARVIS to use the background coding agent. It returns a task id immediately; JARVIS notifies you when it ends. The branch and worktree are always kept for review."));
      }
      api("/code-agents/tasks").then(paintTasks, function () {});
      return card;
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
      label.appendChild(el("div", "tname", tool.title || tool.id));
      label.appendChild(el("div", "tid", tool.id + "  \u2022  risk " + tool.risk + (tool.asks_first ? "  \u2022  asks by default" : "  \u2022  runs by default")));
      var select = el("select");
      POSTURES.forEach(function (option) {
        var node = el("option", "", option[1]);
        node.value = option[0];
        select.appendChild(node);
      });
      select.value = postures[tool.id] || "default";
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
            + (status.actions && !status.can_act ? " Sending and calendar changes are switched on but not granted yet: sign in again." : "")
            + (status.actions && status.can_act ? " JARVIS may send mail and add events, asking you every time." : "");
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
      var name = el("span", "name", "Bot token");
      name.appendChild(el("span", "badge " + (keyState === "set" ? "on" : "off"), keyState));
      var field = el("input");
      field.type = "password";
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
      row.appendChild(el("span", "help", "Stored in a private file on this machine and never shown again."));
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
    function mcpServerCard(server, reload) {
      var card = el("div", "card");
      var head = el("h4", "", server.name + " ");
      var stateText = server.state === "connected" ? "connected" : server.state === "unreachable" ? "not reachable" : "starts after a restart";
      head.appendChild(el("span", "badge " + (server.state === "connected" ? "on" : "off"), stateText));
      card.appendChild(head);
      var how = server.kind === "http" ? server.endpoint : (server.command || []).join(" ");
      card.appendChild(el("code", "", how));
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
        putJson("/mcp/test", "POST", { name: server.name, command: server.command || [], endpoint: server.endpoint || null, read_only: server.posture === "read-only" }).then(function (reply) {
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
      acts.appendChild(testButton); acts.appendChild(remove); acts.appendChild(out);
      card.appendChild(acts);
      return card;
    }
    function mcpAddForm(reload) {
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Add a program"));
      var name = el("input"); name.type = "text"; name.placeholder = "A short name, for example files"; name.autocomplete = "off"; name.spellcheck = false;
      var command = el("input"); command.type = "text"; command.placeholder = "The command that starts it, or a web address"; command.autocomplete = "off"; command.spellcheck = false;
      var nameRow = el("div", "set-row");
      nameRow.appendChild(el("span", "name", "Name"));
      nameRow.appendChild(name);
      nameRow.appendChild(el("span"));
      var commandRow = el("div", "set-row");
      commandRow.appendChild(el("span", "name", "Command or address"));
      commandRow.appendChild(command);
      var example = el("button", "", "Example");
      example.type = "button";
      example.title = "Fill in a sample command";
      example.addEventListener("click", function () {
        if (!name.value.trim()) name.value = "files";
        command.value = "npx -y @modelcontextprotocol/server-filesystem C:\\Users\\you\\Documents";
        command.focus();
      });
      var exampleBox = el("div", "acts"); exampleBox.appendChild(example);
      commandRow.appendChild(exampleBox);
      commandRow.appendChild(el("span", "help", "A program on this computer is started with its command, for example npx -y followed by a package name. A remote server is its web address, starting with https."));
      var only = el("input"); only.type = "checkbox";
      var readRow = el("div", "set-row toggle");
      readRow.appendChild(el("span", "name", "It only reads"));
      var wrap = el("label", "switch");
      var track = el("span", "track");
      var state = el("span", "state", "No");
      wrap.appendChild(only); wrap.appendChild(track); wrap.appendChild(state);
      only.addEventListener("change", function () { state.textContent = only.checked ? "Yes" : "No"; });
      readRow.appendChild(wrap);
      readRow.appendChild(el("span"));
      readRow.appendChild(el("span", "help", "Yes lets its tools run without asking. Only say yes for a program that cannot change anything. By default every call asks you first."));
      var out = el("p", "help", "");
      var test = el("button", "", "Test");
      var add = el("button", "yes", "Add");
      function body() {
        var text = command.value.trim();
        var remote = /^https?:\/\//i.test(text);
        return { name: name.value.trim(), command: remote ? [] : splitCommand(text), endpoint: remote ? text : null, read_only: only.checked };
      }
      test.addEventListener("click", function () {
        test.disabled = true; out.className = "help"; out.textContent = "Connecting (a first start with npx may download the program)...";
        putJson("/mcp/test", "POST", body()).then(function (reply) {
          test.disabled = false;
          if (!reply.ok) { out.className = "help bad"; out.textContent = reply.message || "No answer."; return; }
          out.className = "help";
          out.textContent = "Reached it: " + (reply.tools || []).length + " tools ("
            + (reply.tools || []).slice(0, 4).map(function (tool) { return tool.id; }).join(", ") + ((reply.tools || []).length > 4 ? ", ..." : "") + ")."
            + ((reply.excluded || []).length ? " " + reply.excluded.length + " left out." : "");
        }, function (error) { test.disabled = false; out.className = "help bad"; out.textContent = explain(error, "The test failed."); });
      });
      add.addEventListener("click", function () {
        add.disabled = true;
        putJson("/mcp/servers", "POST", body()).then(function () {
          settingsNote("Program added. Restart to apply."); needsRestart("Tool program added"); name.value = ""; command.value = ""; only.checked = false; state.textContent = "No"; add.disabled = false; reload();
        }, function (error) { add.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That was refused."); });
      });
      var actionRow = el("div", "set-row");
      actionRow.appendChild(el("span"));
      var buttons = el("div", "acts"); buttons.appendChild(test); buttons.appendChild(add);
      actionRow.appendChild(buttons);
      actionRow.appendChild(el("span"));
      [nameRow, commandRow, readRow, actionRow].forEach(function (node) { card.appendChild(node); });
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
      var save = el("button", "yes", "Save and connect");
      save.addEventListener("click", function () {
        save.disabled = true; out.className = "help"; out.textContent = "";
        putJson("/mcp/access/public/named", "POST", { hostname: host.value, token: token.value }).then(function () { token.value = ""; reload(); },
          function (error) { save.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be saved."); });
      });
      var hostRow = el("div", "set-row");
      hostRow.appendChild(el("span", "name", "hostname"));
      hostRow.appendChild(host);
      hostRow.appendChild(el("span"));
      var tokenRow = el("div", "set-row");
      tokenRow.appendChild(el("span", "name", "tunnel token"));
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
            putJson("/mcp/access", "POST", { enabled: !info.enabled }).then(function () { needsRestart("Connect apps"); reload(); },
              function (error) { toggle.disabled = false; settingsNote(explain(error, "That could not be changed."), true); });
          });
          acts.appendChild(toggle);
          card.appendChild(acts);
          holder.appendChild(card);
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
    var lastTab = "";
    function renderSettings() {
      var nav = $("settings-tabs"), body = $("settings-body");
      // A switch or a save redraws the tab; stay where the person was scrolling instead of jumping to the top.
      var scrolled = lastTab === sview.tab ? body.scrollTop : 0;
      lastTab = sview.tab;
      nav.replaceChildren();
      body.replaceChildren();
      NAV.forEach(function (group) {
        nav.appendChild(el("div", "navgroup", group[0]));
        group[1].forEach(function (tab) {
          var button = el("button", sview.tab === tab[0] ? "on" : "");
          button.type = "button";
          button.setAttribute("role", "tab");
          button.setAttribute("aria-selected", sview.tab === tab[0] ? "true" : "false");
          button.appendChild(el("span", "", tab[1]));
          var state = sview.reply ? tabState(tab[0]) : "";
          var dot = el("span", "dot" + (state ? " " + state : ""));
          dot.title = state === "ok" ? "Set up" : state === "todo" ? "Not set up yet" : "";
          button.appendChild(dot);
          button.addEventListener("click", function () { goTab(tab[0]); });
          nav.appendChild(button);
        });
      });
      var current = TABS.filter(function (tab) { return tab[0] === sview.tab; })[0];
      body.appendChild(el("p", "lead tablead", current[2]));
      if (!sview.reply) return;
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
      } else if (sview.tab === "connect") {
        connectPanel(body);
      } else if (sview.tab === "permissions") {
        permissionsPanel(body);
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
        body.appendChild(section("For developers", "Letting other apps use JARVIS is under Connect apps."));
        programsPanel(body);
      }
      body.scrollTop = scrolled;
    }
    function loadSettings() {
      api("/settings").then(function (reply) { sview.reply = reply; renderSettings(); }, failure);
      api("/tools").then(function (reply) { sview.tools = reply.tools || []; if (sview.tab === "permissions") renderSettings(); }, function () { sview.tools = []; });
    }
    function openSettings() {
      $("settings").hidden = false;
      settingsNote("");
      paintRestart();
      renderSettings();
      loadSettings();
    }
    function closeSettings() { $("settings").hidden = true; }
    // Things that have stopped working while nobody is looking at Settings: the internet address, the phone door, Telegram. Each is
    // asked from the daemon's own status routes (no provider is called), and only a real problem is shown, with a way to the tab that fixes it.
    var attention = $("attention");
    function paintAttention(items) {
      attention.replaceChildren();
      attention.hidden = !items.length;
      items.forEach(function (item) {
        var line = el("div", "attline");
        line.appendChild(el("span", "", item.text));
        var fix = el("button", "link", "Open " + item.label);
        fix.type = "button";
        fix.addEventListener("click", function () { openSettings(); goTab(item.tab); });
        line.appendChild(fix);
        attention.appendChild(line);
      });
    }
    function checkAttention() {
      var items = [];
      var none = function () {};
      Promise.all([
        api("/mcp/access").then(function (info) {
          var pub = info.public || {};
          if (pub.wanted && pub.status && pub.status.state === "problem") {
            items.push({ tab: "connect", label: "Connect apps", text: "The internet address is not working: " + (pub.status.reason || "it could not start") + "." });
          }
        }, none),
        api("/phone").then(function (phone) {
          var door = phone.door || {};
          if (door.on && door.state === "problem") {
            items.push({ tab: "phone", label: "Phone", text: "The phone door is not working (" + (door.problem || "unknown reason") + "), so calls to JARVIS may get no answer." });
          }
        }, none),
        api("/telegram").then(function (telegram) {
          if (telegram.state === "error") items.push({ tab: "telegram", label: "Telegram", text: telegram.problem || "Telegram is not working." });
        }, none)
      ]).then(function () { paintAttention(items); });
    }
    setTimeout(checkAttention, 2500);
    setInterval(checkAttention, 20000);
    $("open-settings").addEventListener("click", openSettings);
    $("settings-close").addEventListener("click", closeSettings);
    $("settings").addEventListener("click", function (event) { if (event.target === $("settings")) closeSettings(); });
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
      settingsNote("Restarting. This page reconnects by itself in a few seconds.");
      putJson("/restart" + (restartForce ? "?force=true" : ""), "POST").then(function () {
        pendingRestart = [];
        paintRestart();
        restartForce = false;
        button.textContent = "Restart to apply";
        setTimeout(closeSettings, 1500);
      }, function (error) {
        if (error.status === 409) {
          restartForce = true;
          button.textContent = "Restart anyway";
          settingsNote((error.body || "Tasks are still working") + ". Wait for them to finish, or press Restart anyway to interrupt them.", true);
        } else {
          settingsNote(explain(error, "Could not restart from here; run `jarvis restart`."), true);
        }
      });
    });
};
