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
    var TABS = [
      ["brain", "Brain", "Which model JARVIS thinks with, and a second one for when the first is limited or down."],
      ["search", "Web search", "Lets JARVIS search the web for you. Uses an Ollama key (a free ollama.com account makes one)."],
      ["voice", "Voice", "How JARVIS sounds. Without a voice key it speaks with your browser's own voice."],
      ["files", "Folders & code", "What JARVIS may touch. Both are off until you turn them on, so nothing is reachable by accident."],
      ["connect", "Connect apps", "Let your other AI apps (Claude Code, Cursor, VS Code, your own scripts) hand JARVIS a task. They connect with a key only you have."],
      ["permissions", "Permissions", "For each tool: let the rules decide, always ask, run without asking, or switch it off."],
      ["google", "Google", "Sign in with your Google account so JARVIS can read your mail and calendar. Reading is on by default. Sending mail and adding events need the option below and ask you every time; drafts are saved without asking."],
      ["telegram", "Telegram", "Talk to JARVIS from your phone. It answers in a Telegram chat only you can use, and asks you there when it needs your approval."],
      ["phone", "Phone", "JARVIS rings you when it needs you, and you can ring JARVIS. A voice agent at ElevenLabs does the talking; the tools stay with JARVIS."],
      ["advanced", "Advanced", "Ports and other technical settings. The defaults are right for almost everyone. Letting other apps use JARVIS is under Connect apps."]
    ];
    var sview = { reply: null, tools: [], tab: "brain" };
    function settingsNote(text, bad) {
      var note = $("settings-note");
      note.textContent = text || "";
      note.className = bad ? "bad" : "ok";
    }
    function failure(error) {
      settingsNote("Could not do that (" + (error.status || "no reply") + ")", true);
    }
    function saved(what) {
      settingsNote(what + " saved. Restart to apply.");
      $("settings-restart").className = "hot";
      loadSettings();
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
      "daemon.workspace_daily_token_limit": "Daily token limit",
      "daemon.tool_workspace_roots": "Folders", "daemon.code_sandbox_image": "Sandbox image", "daemon.code_sandbox_interpreter": "Interpreter",
      "daemon.speech_voice_id": "Voice", "daemon.speech_model": "Voice model", "daemon.http_port": "Console port",
      "daemon.mcp_serve_port": "Let other apps use JARVIS (port)", "daemon.notifications": "Notifications", "daemon.google_client_id": "Client ID",
      "daemon.google_actions": "Send mail & events", "policy.trust": "Runs without asking",
      "daemon.phone_agent_id": "Agent id", "daemon.phone_number_id": "Phone number id", "daemon.phone_owner_numbers": "Numbers JARVIS may call",
      "daemon.phone_approves": "Your yes on the phone counts", "daemon.phone_opening_line": "JARVIS writes the opening line", "daemon.phone_call_anyone": "JARVIS may call other people", "daemon.phone_screen_callers": "Answer only my own numbers"
    };
    function settingRow(setting, label, live) {
      var row = el("div", "set-row");
      var name = el("span", "name", label && label !== "folders" ? label : LABELS[setting.key] || setting.key.replace(/^(daemon|policy)\./, ""));
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
          if (live) { settingsNote(setting.key + " saved."); loadSettings(); } else { saved(setting.key); }
        }, function (error) { settingsNote(explain(error, "That value was refused."), true); });
      });
      acts.appendChild(save);
      if (setting.value !== null && setting.value !== undefined) {
        var unset = el("button", "", "Unset");
        unset.addEventListener("click", function () {
          putJson("/settings/" + encodeURIComponent(setting.key), "DELETE").then(function () {
            if (live) { settingsNote(setting.key + " removed."); loadSettings(); } else { saved(setting.key + " removed;"); }
          },
            function (error) { settingsNote(explain(error, "That cannot be removed on its own."), true); });
        });
        acts.appendChild(unset);
      }
      row.appendChild(name);
      row.appendChild(field);
      row.appendChild(acts);
      var set = setting.value !== null && setting.value !== undefined;
      row.appendChild(el("span", "help", set ? setting.help : setting.unset_means));
      return row;
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
        voice: "An ElevenLabs API key. Stored in a private file and never shown again; what JARVIS says is sent to ElevenLabs to be spoken.",
        fallback: "The key of the different provider you gave above for the fallback model. Stored in a private file and never shown again; when the main model is limited, what the run is saying is sent to that provider. Check it with `jarvis keys test fallback`.",
        search: "An Ollama API key (a free ollama.com account makes one) that turns on web search. Stored in a private file and never shown again; what JARVIS searches for is sent to ollama.com. Check it with `jarvis keys test search`."
      };
      row.appendChild(el("span", "help", helps[which] || ""));
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
      }
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

      card.appendChild(providerRow("Provider", select, null, providerHelp));
      card.appendChild(providerRow("Address", address));
      card.appendChild(providerRow("Key" + (keyState === "set" ? " (set)" : ""), keyBox));
      card.appendChild(providerRow("Model", picker.element, acts, status));
      return card;
    }
    function codeCard() {
      var image = byKey("daemon.code_sandbox_image"), command = byKey("daemon.code_sandbox_interpreter");
      var card = el("div", "card");
      var title = el("h4", "", "Code sandbox");
      title.appendChild(el("span", "badge " + (image && image.value ? "on" : "off"), image && image.value ? "on" : "off"));
      card.appendChild(title);
      card.appendChild(el("p", "lead", image && image.value ? "JARVIS can run code in a disposable container with no network."
        : image.unset_means));
      var row = el("div", "set-row");
      row.appendChild(el("span", "name", "image"));
      var imageField = el("input");
      imageField.type = "text"; imageField.value = showFor(image); imageField.placeholder = "e.g. " + image.example;
      row.appendChild(imageField);
      row.appendChild(el("span"));
      var row2 = el("div", "set-row");
      row2.appendChild(el("span", "name", "command"));
      var commandField = el("input");
      commandField.type = "text"; commandField.value = showFor(command); commandField.placeholder = "e.g. " + command.example;
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
    var POSTURES = [["default", "Default (the rules decide)"], ["ask", "Always ask first"], ["trusted", "Run without asking"], ["off", "Off"]];
    function permissionsPanel(into) {
      var postures = (sview.reply && sview.reply.postures) || {};
      if (!sview.tools.length) { into.appendChild(el("p", "lead", "No tools are available to configure yet.")); return; }
      families(sview.tools).forEach(function (group) {
        into.appendChild(familyLabel(group));
        group.tools.forEach(function (tool) { into.appendChild(permissionRow(tool, postures)); });
      });
      into.appendChild(el("p", "lead", "Every tool is yours to set. Tools that send things to other people ask by default, and choosing \u201CRun without asking\u201D for one of them asks you to confirm once. Nothing runs above the workspace ceiling, whatever is chosen here."));
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
      var help = el("div", "card");
      help.appendChild(el("h4", "", "One-time setup"));
      var steps = el("ol");
      ["Open console.cloud.google.com, create a project, and enable the Gmail API and the Google Calendar API.",
       "In Google Auth platform, set up the consent screen (External is fine), add your own Google address as a test user, and press Publish app (otherwise Google ends the sign-in after 7 days).",
       "Create credentials, OAuth client ID, application type Desktop app. Copy the client ID and the client secret into the two fields below.",
       "Press Sign in with Google. Google warns that the app is unverified: that is expected for your own client; choose Continue."].forEach(function (text) {
        steps.appendChild(el("li", "", text));
      });
      help.appendChild(steps);
      help.appendChild(el("p", "lead", "Details and the reasoning are in docs/user/google.md. Nothing is sent to Google until you press Sign in."));
      body.appendChild(help);
      if (idSetting) body.appendChild(settingRow(idSetting));
      body.appendChild(keyRow("google", "Client secret"));
      var actionsSetting = byKey("daemon.google_actions");
      if (actionsSetting) body.appendChild(settingRow(actionsSetting));
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Account"));
      var line = el("p", "lead", "Checking...");
      var acts = el("div", "acts");
      card.appendChild(line);
      card.appendChild(acts);
      body.appendChild(card);
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
          line.textContent = "Not set up yet: save the client ID and client secret above first.";
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
      var help = el("div", "card");
      help.appendChild(el("h4", "", "Setup, once"));
      var steps = el("ol");
      ["In Telegram, open @BotFather and send /newbot. Pick a name; it gives you a token.",
       "Paste the token below and press Save token.",
       "Open your new bot in Telegram and send it any message. It replies with a six digit code.",
       "Enter the code here. From then on only your chat is answered; everyone else is ignored."].forEach(function (text) {
        steps.appendChild(el("li", "", text));
      });
      help.appendChild(steps);
      help.appendChild(el("p", "lead", "Messages go through Telegram's servers, so treat this chat like any cloud chat. Nothing needs to be opened on your network."));
      body.appendChild(help);
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
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Status"));
      var line = el("p", "lead", "Checking...");
      var more = el("div", "acts");
      card.appendChild(line);
      card.appendChild(more);
      body.appendChild(card);
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
          line.textContent = keyState === "set" ? "Starting..." : "Not connected. Paste the bot token above.";
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
      var voiceKey = sview.reply && sview.reply.keys && sview.reply.keys.voice ? sview.reply.keys.voice.state : "unknown";
      var help = el("div", "card");
      help.appendChild(el("h4", "", "Setup, once"));
      var steps = el("ol");
      ["At elevenlabs.io, create an agent and import a phone number (a Twilio number or a SIP trunk) for it. Assign the agent to the number.",
       "Add the agent as an MCP server pointing at JARVIS (Connect apps shows the address and key), with tool approval set to ask you for acting tools.",
       "Put your ElevenLabs key in the Voice tab. It needs access to the agents platform, not only speech.",
       "Fill in the agent id, the phone number id (listed below once the key is set) and your own number. JARVIS rings only numbers you list here.",
       "Press Call me now. To ring JARVIS, call the number you imported; the agent answers and uses JARVIS's tools."].forEach(function (text) {
        steps.appendChild(el("li", "", text));
      });
      help.appendChild(steps);
      help.appendChild(el("p", "lead", "The call and what is said on it pass through ElevenLabs (and Twilio or your SIP provider). JARVIS keeps a short record of each call it placed: who, why, how long and a summary. It does not keep the transcript or the audio."));
      body.appendChild(help);
      var keyNote = el("p", "lead", voiceKey === "set" ? "ElevenLabs key: set (Voice tab)." : "ElevenLabs key: not set. Add it in the Voice tab first.");
      body.appendChild(keyNote);
      ["daemon.phone_agent_id", "daemon.phone_number_id", "daemon.phone_owner_numbers", "daemon.phone_approves", "daemon.phone_opening_line", "daemon.phone_call_anyone", "daemon.phone_screen_callers"].forEach(function (key) {
        var setting = byKey(key);
        if (setting) body.appendChild(settingRow(setting, null, true));
      });
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Status"));
      var line = el("p", "lead", "Checking...");
      var more = el("div", "acts");
      var numbers = el("ul", "help");
      var calls = el("ul", "help");
      card.appendChild(line);
      card.appendChild(more);
      card.appendChild(numbers);
      card.appendChild(el("h4", "", "Recent calls"));
      card.appendChild(calls);
      body.appendChild(card);
      function paint(status) {
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
        ((check && check.numbers) || []).forEach(function (number) {
          var text = (number.label || "number") + " " + number.number + " (" + number.provider + "), id " + number.id +
            (number.agent_id ? ", agent " + number.agent_id : ", no agent");
          numbers.appendChild(el("li", "", text));
        });
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
      card.appendChild(el("p", "help", how));
      if (server.state === "unreachable") card.appendChild(el("p", "help bad", server.reason || "It did not answer."));
      if (server.state === "connected") {
        var tools = server.tools || [];
        card.appendChild(el("p", "lead", tools.length + " tool" + (tools.length === 1 ? "" : "s") + ", "
          + (server.posture === "read-only" ? "run without asking (you marked it read-only)." : "each call asks you first.")
          + (server.era ? " Speaks the " + server.era + " protocol." : "")));
        var list = el("ul", "help");
        tools.slice(0, 8).forEach(function (tool) { list.appendChild(el("li", "", tool.id + (tool.description ? ": " + tool.description : ""))); });
        if (tools.length > 8) list.appendChild(el("li", "", "and " + (tools.length - 8) + " more (see Permissions)."));
        card.appendChild(list);
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
          settingsNote(server.name + " removed. Restart to apply."); $("settings-restart").className = "hot"; reload();
        }, function (error) { settingsNote(explain(error, "Could not remove it."), true); remove.disabled = false; });
      });
      acts.appendChild(testButton); acts.appendChild(remove); acts.appendChild(out);
      card.appendChild(acts);
      return card;
    }
    function mcpAddForm(reload) {
      var card = el("div", "card");
      card.appendChild(el("h4", "", "Add a tool program"));
      card.appendChild(el("p", "help", "Type the command that starts it, for example: npx -y @modelcontextprotocol/server-filesystem C:\\Users\\you\\Documents. For a remote server, type its https address instead."));
      var name = el("input"); name.placeholder = "A short name, for example files"; name.autocomplete = "off";
      var command = el("input"); command.placeholder = "Command or https address"; command.autocomplete = "off";
      var only = el("input"); only.type = "checkbox";
      var label = el("label", "help", " It only reads: let its tools run without asking.");
      label.insertBefore(only, label.firstChild);
      var out = el("p", "help", "");
      var acts = el("div", "acts");
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
          settingsNote("Server added. Restart to apply."); $("settings-restart").className = "hot"; name.value = ""; command.value = ""; only.checked = false; add.disabled = false; reload();
        }, function (error) { add.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That was refused."); });
      });
      acts.appendChild(test); acts.appendChild(add);
      [name, command, label, acts, out].forEach(function (node) { card.appendChild(node); });
      return card;
    }
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
          var more = el("details");
          more.appendChild(el("summary", "", "Add your own tool program (for developers)"));
          more.appendChild(el("p", "help", "JARVIS can use any program that speaks MCP. Adding one runs that program as you, so only add programs you trust. Its tools ask before every call. A change applies after a restart."));
          custom.forEach(function (server) { more.appendChild(mcpServerCard(server, reload)); });
          more.appendChild(mcpAddForm(reload));
          if (custom.length || reply.restart_needed) more.open = true;
          holder.appendChild(more);
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
        box.appendChild(el("p", "help", "Stable address: your own Cloudflare tunnel at https://" + stable.host + ". Its token is saved privately and is not shown."));
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
      details.appendChild(el("summary", "", "Want an address that never changes? Use your own Cloudflare tunnel"));
      var service = stable.service_url || "http://127.0.0.1:8766";
      [
        "You need a free Cloudflare account with a domain on it.",
        "In the Cloudflare dashboard open Networking, Tunnels, Create tunnel. Name it (for example jarvis).",
        "Add a published application route: pick a name such as jarvis.yourdomain.com, and set the Service URL to " + service + " (this is the endpoint on this computer).",
        "Open the tunnel, choose Add a replica, and copy the install command (do not run it). Paste it below, with the name from step 3."
      ].forEach(function (text, index) { details.appendChild(el("p", "help", (index + 1) + ". " + text)); });
      var host = el("input"); host.type = "text"; host.placeholder = "jarvis.yourdomain.com"; host.autocomplete = "off";
      var token = el("input"); token.type = "password"; token.placeholder = "the install command, or the token that starts with eyJ"; token.autocomplete = "off";
      var save = el("button", "yes", "Save and connect");
      save.addEventListener("click", function () {
        save.disabled = true; out.className = "help"; out.textContent = "";
        putJson("/mcp/access/public/named", "POST", { hostname: host.value, token: token.value }).then(function () { token.value = ""; reload(); },
          function (error) { save.disabled = false; out.className = "help bad"; out.textContent = explain(error, "That could not be saved."); });
      });
      var fields = el("div", "acts"); fields.appendChild(host); fields.appendChild(token); fields.appendChild(save);
      details.appendChild(fields);
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
      card.appendChild(el("p", "lead", "For apps that run in the cloud, such as a phone agent (ElevenLabs) or a hosted assistant. JARVIS opens a private-looking web address that leads to this endpoint. Every request still needs your key."));
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
            putJson("/mcp/access", "POST", { enabled: !info.enabled }).then(function () { $("settings-restart").className = "hot"; reload(); },
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
          boxes.forEach(function (box) { access.appendChild(box.node); });
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
          holder.appendChild(publicCard(info, current, reload));
          var safe = el("div", "card");
          safe.appendChild(el("h4", "", "What connected apps can and cannot do"));
          ["They can hand JARVIS a task and read the answer, and see and use JARVIS's tools that run without asking (memory, projects, files you allowed, reading web pages, documentation lookup).",
           "Anything that would ask you (clicking or typing in the browser, sending mail, changing your schedule, running commands) waits for your answer, which appears with your other approvals like any other question. They wait up to two minutes, then the call is refused. They cannot approve things themselves.",
           "Only this computer can connect, unless you switch on the internet address above.",
           "Tasks they hand JARVIS appear in your runs. A single tool they call directly follows the same rules as your own requests but is not yet recorded in your history."].forEach(function (text) { safe.appendChild(el("p", "help", text)); });
          holder.appendChild(safe);
        }, function () { holder.replaceChildren(); holder.appendChild(el("p", "help bad", "This is not available on this daemon.")); });
      }
      reload();
    }
    function renderSettings() {
      var tabs = $("settings-tabs"), body = $("settings-body");
      tabs.replaceChildren();
      body.replaceChildren();
      TABS.forEach(function (tab) {
        var button = el("button", sview.tab === tab[0] ? "on" : "", tab[1]);
        button.addEventListener("click", function () { sview.tab = tab[0]; settingsNote(""); renderSettings(); });
        tabs.appendChild(button);
      });
      var current = TABS.filter(function (tab) { return tab[0] === sview.tab; })[0];
      body.appendChild(el("p", "lead", current[2]));
      if (!sview.reply) return;
      var settings = sview.reply.settings || [];
      var inGroup = function (name) { return settings.filter(function (s) { return s.group === name && !/(_api_key_ref|_secret_ref)$/.test(s.key); }); };
      if (sview.tab === "brain") {
        body.appendChild(providerCard("main"));
        body.appendChild(providerCard("fallback"));
        var effort = byKey("daemon.executor_reasoning_effort");
        if (effort) body.appendChild(settingRow(effort, "Thinking effort"));
        var tokenLimit = byKey("daemon.workspace_daily_token_limit");
        if (tokenLimit) body.appendChild(settingRow(tokenLimit, "Daily token limit"));
      } else if (sview.tab === "search") {
        body.appendChild(keyRow("search", "Search key"));
      } else if (sview.tab === "voice") {
        var voiceKey = sview.reply.keys && sview.reply.keys.voice ? sview.reply.keys.voice.state : "";
        body.appendChild(el("p", "lead", voiceKey === "set" ? "Using the ElevenLabs voice below (after a restart)." : "Using your browser's own voice. Add an ElevenLabs key for a natural one."));
        body.appendChild(keyRow("voice", "Voice key"));
        inGroup("voice").forEach(function (s) { body.appendChild(settingRow(s)); });
      } else if (sview.tab === "files") {
        inGroup("files").filter(function (s) { return !/code_sandbox/.test(s.key); }).forEach(function (s) { body.appendChild(settingRow(s, "folders")); });
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
      } else {
        inGroup("advanced").filter(function (s) { return s.key !== "daemon.mcp_serve_port"; }).forEach(function (s) { body.appendChild(settingRow(s)); });
        programsPanel(body);
      }
    }
    function loadSettings() {
      api("/settings").then(function (reply) { sview.reply = reply; renderSettings(); }, failure);
      api("/tools").then(function (reply) { sview.tools = reply.tools || []; if (sview.tab === "permissions") renderSettings(); }, function () { sview.tools = []; });
    }
    function openSettings() {
      $("settings").hidden = false;
      settingsNote("");
      renderSettings();
      loadSettings();
    }
    function closeSettings() { $("settings").hidden = true; }
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
        button.className = "";
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
