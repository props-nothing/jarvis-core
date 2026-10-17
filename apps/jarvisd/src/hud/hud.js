"use strict";
// The JARVIS console: talk to the assistant, watch it work, stop it. No dependencies.
//
// Security notes that the rest of the file relies on:
//  - The credential arrives in the URL fragment (never sent to a server), moves to this tab's session storage and is
//    removed from the address bar at once.
//  - Every value that came from the daemon or the model is inserted with textContent / text nodes, never as markup.
//  - Speech recognition and synthesis are the browser's own: nothing here sends audio anywhere itself. In Chrome and
//    Edge the browser's recognizer may use its vendor's cloud service; the page says so.
(function () {
  var fragment = new URLSearchParams(location.hash.slice(1));
  var token = fragment.get("token");
  try {
    if (token) sessionStorage.setItem("jarvis-token", token);
    else token = sessionStorage.getItem("jarvis-token");
  } catch (ignored) { /* This tab can still use its fragment credential without browser storage. */ }
  if (fragment.get("token")) history.replaceState(null, "", location.pathname);
  function storedPreference(key) { try { return localStorage.getItem(key); } catch (ignored) { return null; } }


  function $(id) { return document.getElementById(id); }
  function el(tag, className, text) {
    var node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }
  function sleep(ms) { return new Promise(function (resolve) { setTimeout(resolve, ms); }); }
  // A one-line plain rendering of model text for the side panels: markdown marks are dropped, not interpreted.
  function plainLine(text) {
    return String(text || "").replace(/```[\s\S]*?```/g, " code ").replace(/[`*_#>]/g, "").replace(/^\s*-\s+/gm, "");
  }
  function clip(text, limit) {
    var flat = String(text || "").split(/\s+/).join(" ");
    return flat.length > limit ? flat.slice(0, limit) + "..." : flat;
  }

  // ---- state --------------------------------------------------------------------------------------------------
  var S = {
    session: null, activity: "",
    offline: true, needYou: 0, working: 0, thinking: 0,
    listening: false, speaking: false, voiceBusy: false, preparing: false, voiceKind: "browser", level: 0, voiceLevel: 0,
    speak: storedPreference("jarvis-speak") === "1",
    wake: false,
    pending: [], announced: null
  };
  var projects = [], projectsAt = 0;

  // ---- api ----------------------------------------------------------------------------------------------------
  function api(path, method, body) {
    var headers = { Authorization: "Bearer " + token };
    var init = { method: method || "GET", headers: headers, cache: "no-store" };
    if (body !== undefined) {
      headers["Content-Type"] = "application/json";
      init.body = JSON.stringify(body);
    }
    if (typeof AbortSignal !== "undefined" && AbortSignal.timeout) init.signal = AbortSignal.timeout(15000);
    return fetch("/api/v1" + path, init).then(function (response) {
      if (!response.ok) {
        var error = new Error(String(response.status));
        error.status = response.status;
        throw error;
      }
      return response.status === 204 ? null : response.json();
    });
  }
  function explain(error, fallback) {
    // The daemon explains a refusal in its error body; show that rather than a status number.
    return error && error.body ? error.body : fallback;
  }
  function putJson(path, method, body) {
    var init = { method: method, cache: "no-store", headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" } };
    if (body !== undefined) init.body = JSON.stringify(body);
    return fetch("/api/v1" + path, init).then(function (response) {
      if (response.ok) return response.json();
      return response.json().catch(function () { return {}; }).then(function (reply) {
        var error = new Error(String(response.status));
        error.status = response.status;
        error.body = reply && reply.message ? reply.message : null;
        throw error;
      });
    });
  }

  // ---- markdown-lite, as DOM nodes ------------------------------------------------------------------------------
  function inline(text, parent) {
    var re = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]]+\]\(https?:\/\/[^\s)]+\)|https?:\/\/[^\s<>)\]]+)/g, last = 0, match;
    while ((match = re.exec(text))) {
      if (match.index > last) parent.appendChild(document.createTextNode(text.slice(last, match.index)));
      var piece = match[0];
      if (piece[0] === "`") parent.appendChild(el("code", "", piece.slice(1, -1)));
      else if (piece[0] === "*") parent.appendChild(el("strong", "", piece.slice(2, -2)));
      else {
        // A link in an answer is model text: only a plain web address is ever offered, in a new tab, with no opener or referrer.
        var inner = piece[0] === "[" ? /^\[([^\]]+)\]\((.*)\)$/.exec(piece) : null;
        var href = window.JarvisMission ? JarvisMission.safeHref(inner ? inner[2] : piece.replace(/[.,;:!?]+$/, "")) : null;
        if (href) {
          var anchor = el("a", "alink", inner ? inner[1] : piece.replace(/[.,;:!?]+$/, ""));
          anchor.href = href;
          anchor.target = "_blank";
          anchor.rel = "noopener noreferrer";
          parent.appendChild(anchor);
          if (!inner) parent.appendChild(document.createTextNode(piece.slice(piece.replace(/[.,;:!?]+$/, "").length)));
        } else parent.appendChild(document.createTextNode(piece));
      }
      last = match.index + piece.length;
    }
    if (last < text.length) parent.appendChild(document.createTextNode(text.slice(last)));
  }
  function blocks(text, into) {
    text.split(/\n{2,}/).forEach(function (paragraph) {
      if (!paragraph.trim()) return;
      var lines = paragraph.split("\n");
      var bullet = /^\s*([-*]|\d+\.)\s+/;
      if (lines.every(function (line) { return bullet.test(line); })) {
        var list = el("ul");
        lines.forEach(function (line) {
          var item = el("li");
          inline(line.replace(bullet, ""), item);
          list.appendChild(item);
        });
        into.appendChild(list);
      } else {
        var p = el("p");
        lines.forEach(function (line, index) {
          if (index) p.appendChild(document.createElement("br"));
          inline(line, p);
        });
        into.appendChild(p);
      }
    });
  }
  function markdown(text) {
    var into = document.createDocumentFragment();
    String(text).split("```").forEach(function (part, index) {
      if (index % 2 === 1) {
        var pre = el("pre");
        pre.appendChild(el("code", "", part.replace(/^[a-z0-9+#-]*\n/i, "").replace(/\n$/, "")));
        into.appendChild(pre);
      } else {
        blocks(part, into);
      }
    });
    return into;
  }

  // ---- chat ---------------------------------------------------------------------------------------------------
  var chat = $("chat");
  // Conversations are kept in this browser (not in a tab), so closing the page and coming back continues where you were, and an
  // earlier conversation can be reopened. Each one remembers its daemon session, so reopening it carries on with the same context.
  var historyScope = null, historyTouched = false;
  var CHATS_KEY = null, CURRENT_KEY = null, MAX_CHATS = 30, chats = [];
  function historyNotice(problem) {
    $("history-status").textContent = problem || "Browser history: up to 30 chats, latest 60 messages each. Daemon records are retained separately.";
    $("history-status").dataset.warning = problem && !problem.startsWith("Loading") ? "true" : "false";
  }
  historyNotice("Loading this profile’s browser history…");
  $("clear-history").disabled = true;
  function loadProfileHistory() {
    return (token ? api("/settings") : Promise.resolve(null)).then(function (profile) {
      if (!profile || !/^[a-f0-9]{64}$/.test(profile.profile_id)) throw new Error("Profile unavailable");
      historyScope = profile.profile_id;
      CHATS_KEY = "jarvis-chats:" + historyScope;
      CURRENT_KEY = "jarvis-chat-current:" + historyScope;
      var stored;
      try { stored = JSON.parse(localStorage.getItem(CHATS_KEY) || "[]"); }
      catch (ignored) { historyNotice("Browser history could not be read. This page still works."); return; }
      if (!Array.isArray(stored)) stored = [];
      stored = stored.filter(function (item) { return item && typeof item.id === "string" && Array.isArray(item.transcript); }).slice(0, MAX_CHATS);
      stored.forEach(function (item) { item.transcript = item.transcript.filter(function (entry) { return entry && typeof entry.text === "string" && ["user", "jarvis", "system"].indexOf(entry.role) >= 0; }).slice(-60); });
      if (!historyTouched && stored.length) {
        chats = stored;
        currentChat = chats.find(function (item) { return item.id === storedPreference(CURRENT_KEY); }) || chats[0];
        transcript = currentChat.transcript; S.session = currentChat.session || null;
        renderChat(); paintProjectPick();
      } else {
        chats = chats.concat(stored.filter(function (item) { return !chats.some(function (current) { return current.id === item.id; }); }));
        if (historyTouched) saveChats();
      }
      $("clear-history").disabled = false;
      paintUnread();
      historyNotice();
    }).catch(function () { historyNotice("Profile unavailable: this conversation will only be kept in this page. Reload after reconnecting to restore browser history."); });
  }
  function newChatRecord() {
    return { id: Date.now().toString(36) + Math.random().toString(36).slice(2, 6), session: null, project: null, title: "", updated: Date.now(), transcript: [] };
  }
  var currentChat = chats.filter(function (item) { return item.id === (CURRENT_KEY && storedPreference(CURRENT_KEY)); })[0] || chats[0] || null;
  if (!currentChat) { currentChat = newChatRecord(); chats.unshift(currentChat); }
  var transcript = currentChat.transcript || [];
  S.session = currentChat.session || null;
  function saveChats() {
    if (!historyScope) { historyTouched = true; return; }
    try {
      chats = chats.filter(function (item) { return item === currentChat || (item.transcript && item.transcript.length); })
        .sort(function (a, b) { return b.updated - a.updated; }).slice(0, MAX_CHATS);
      localStorage.setItem(CHATS_KEY, JSON.stringify(chats));
      localStorage.setItem(CURRENT_KEY, currentChat.id);
    } catch (ignored) { historyNotice("Browser storage is full or blocked. New messages are not being kept after this page closes."); }
  }
  $("clear-history").addEventListener("click", function () {
    if (!window.confirm("Clear this profile’s browser history? Daemon records and running work are retained.")) return;
    try {
      if (CHATS_KEY) localStorage.removeItem(CHATS_KEY);
      if (CURRENT_KEY) localStorage.removeItem(CURRENT_KEY);
      chats = [currentChat]; currentChat.transcript = []; transcript = [];
      chat.replaceChildren(); closeChats(); historyNotice("Browser history cleared. Daemon records are retained separately.");
    } catch (ignored) { historyNotice("Browser history could not be cleared. Storage is unavailable."); }
  });
  function remember(role, text, pictures) {
    var entry = { role: role, text: text };
    if (pictures && pictures.length) entry.pictures = pictures;
    transcript.push(entry);
    transcript = transcript.slice(-60);
    currentChat.transcript = transcript;
    if (role === "user" && !currentChat.title) currentChat.title = clip(text, 56);
    currentChat.updated = Date.now();
    saveChats();
  }
  function setSession(id) {
    S.session = id;
    currentChat.session = id;
    saveChats();
  }
  // What scheduled tasks found goes into its own conversation, never into the one you are in the middle of. The History button
  // says how many results are unread.
  function scheduledInbox() {
    var found = chats.filter(function (item) { return item.kind === "scheduled"; })[0];
    if (!found) {
      found = newChatRecord();
      found.kind = "scheduled";
      found.title = "Scheduled results";
      chats.push(found);
    }
    return found;
  }
  function paintUnread() {
    var total = chats.reduce(function (sum, item) { return sum + (item.unread || 0); }, 0);
    $("chats-btn").textContent = total ? "History (" + total + ")" : "History";
  }
  function scroll() { chat.scrollTop = chat.scrollHeight; }
  function clearEmpty() {
    var empty = chat.querySelector(".empty");
    if (empty) empty.remove();
  }
  function bubble(role, label) {
    clearEmpty();
    var node = el("div", "msg " + role);
    if (label) node.appendChild(el("div", "who", label));
    var chips = el("div", "chips");
    chips.hidden = true;
    var body = el("div", "body");
    node.appendChild(chips);
    node.appendChild(body);
    chat.appendChild(node);
    scroll();
    return { node: node, body: body, chips: chips };
  }
  // A picture a tool made (a screenshot). It is fetched with the credential and shown from a blob, because an <img> cannot send a header.
  // One the daemon has since cleared away simply does not appear.
  function addPicture(view, id) {
    var frame = el("div", "apics", "");
    var image = document.createElement("img");
    image.alt = "a picture from a tool";
    image.loading = "lazy";
    frame.appendChild(image);
    view.node.insertBefore(frame, view.chips);
    fetch("/api/v1/images/" + encodeURIComponent(id), { headers: { Authorization: "Bearer " + token }, cache: "force-cache" })
      .then(function (response) { if (!response.ok) throw new Error(String(response.status)); return response.blob(); })
      .then(function (blob) {
        var url = URL.createObjectURL(blob);
        image.src = url;
        image.addEventListener("click", function () { window.open(url, "_blank", "noopener"); });
        image.addEventListener("load", scroll);
      })
      .catch(function () { frame.remove(); });
  }
  function show(role, text, pictures) {
    var view = bubble(role, role === "user" ? "you" : role === "jarvis" ? "jarvis" : "");
    view.body.appendChild(role === "jarvis" ? markdown(text) : document.createTextNode(text));
    (pictures || []).forEach(function (id) { addPicture(view, id); });
    return view;
  }
  function renderEmpty() {
    var empty = el("div", "empty");
    empty.appendChild(el("h2", "", currentChat.project ? "What’s next for this project?" : "Start a conversation"));
    empty.appendChild(el("p", "", "Ask anything, or choose a starting point."));
    var ideas = currentChat.project
      ? [["Check project progress", "Where does this project stand?"], ["Plan the next steps", "Plan the next steps and start on the first"], ["Find sources", "Research this and give me sources"]]
      : [["Explore your capabilities", "What can you do for me?"], ["Research a topic", "Research the latest news on AI agents and give me sources"], ["Plan a project", "Help me plan a project"]];
    var row = el("div", "suggest");
    ideas.forEach(function (idea) {
      var button = el("button");
      button.type = "button";
      button.title = "Add this idea to your message";
      button.appendChild(el("span", "", idea[0]));
      button.addEventListener("click", function () { $("input").value = idea[1]; $("input").focus(); });
      row.appendChild(button);
    });
    empty.appendChild(row);
    chat.appendChild(empty);
  }
  function renderChat() {
    chat.replaceChildren();
    if (transcript.length === 0) renderEmpty();
    else transcript.forEach(function (entry) { show(entry.role, entry.text, entry.pictures); });
    scroll();
  }
  renderChat();
  paintUnread();

  // ---- new chat, and the earlier ones ------------------------------------------------------------------------------
  function switchChat(target) {
    // The answer on screen belongs to the conversation that asked for it, so wait for it (or Stop) before moving away.
    if (busy) { say("one moment: it is still answering. Press Stop to cut it short."); return false; }
    interruptSpeech();
    currentChat = target;
    transcript = target.transcript || [];
    S.session = target.session || null;
    target.unread = 0;
    paintUnread();
    saveChats();
    renderChat();
    closeChats();
    paintProjectPick();
    return true;
  }
  function newChat() {
    historyTouched = true;
    // An empty conversation is already a new one.
    if (!transcript.length) { closeChats(); $("input").focus(); return; }
    var fresh = newChatRecord();
    chats.unshift(fresh);
    if (!switchChat(fresh)) { chats.shift(); return; }
    feel("attentive", 1.4, "nod");
    $("input").focus();
  }
  function closeChats() { $("chats").hidden = true; $("chats-btn").setAttribute("aria-expanded", "false"); }
  function renderChats() {
    var host = $("chats");
    host.replaceChildren();
    var shown = chats.filter(function (item) { return item.transcript && item.transcript.length; });
    if (!shown.length) host.appendChild(el("div", "none", "no earlier conversations"));
    paintUnread();
    shown.forEach(function (item) {
      var row = el("div", "chatrow" + (item === currentChat ? " on" : ""));
      var open = el("button", "chatopen", (item.project ? "[" + item.project + "] " : "") + (item.title || "(untitled)"));
      open.addEventListener("click", function () { switchChat(item); });
      row.appendChild(open);
      row.appendChild(el("span", "dim", age(new Date(item.updated).toISOString())));
      var drop = el("button", "chatdrop", "\u00D7");
      drop.title = "Remove this conversation from this browser";
      drop.setAttribute("aria-label", "Remove " + (item.title || "untitled conversation") + " from browser history");
      drop.addEventListener("click", function () {
        if (item === currentChat) { if (busy) { say("one moment: it is still answering."); return; } }
        chats = chats.filter(function (other) { return other !== item; });
        if (item === currentChat) { var fresh = newChatRecord(); chats.unshift(fresh); currentChat = fresh; transcript = []; S.session = null; renderChat(); paintProjectPick(); }
        saveChats();
        renderChats();
      });
      row.appendChild(drop);
      host.appendChild(row);
    });
  }
  $("new-chat").addEventListener("click", newChat);
  $("chats-btn").addEventListener("click", function () {
    var host = $("chats");
    if (host.hidden) { renderChats(); host.hidden = false; $("chats-btn").setAttribute("aria-expanded", "true"); } else closeChats();
  });

  // What a tool call means in words, for the progress line: "writing app/page.tsx" says more than "Requested jarvis.files.write".
  var VERBS = {
    "jarvis.files.write": "writing", "jarvis.files.edit": "editing", "jarvis.files.move": "moving a file", "jarvis.files.read": "reading", "jarvis.files.list": "listing", "jarvis.files.search": "searching files",
    "jarvis.web.fetch": "fetching a page", "jarvis.agent.delegate": "handing off a task", "jarvis.agent.result": "collecting a result",
    "jarvis.memory.propose": "noting a memory", "jarvis.skill.propose": "writing down a procedure", "jarvis.code.run": "running code", "jarvis.command.run": "running",
    "jarvis.web.search": "searching the web", "jarvis.memory.search": "checking memory", "jarvis.gmail.search": "searching your mail", "jarvis.gmail.read": "reading an email", "jarvis.calendar.events": "checking your calendar", "jarvis.gmail.send": "sending an email", "jarvis.calendar.create": "adding a calendar event", "jarvis.calendar.delete": "removing a calendar event", "jarvis.project.note": "writing a project note", "jarvis.project.list": "checking the projects", "jarvis.memory.correct": "correcting a memory", "jarvis.memory.forget": "forgetting a memory", "jarvis.project.use": "joining a project", "jarvis.project.create": "creating a project", "jarvis.project.update": "updating a project", "jarvis.project.assign_schedule": "filing a schedule under a project", "jarvis.schedule.pause": "pausing a scheduled task", "jarvis.schedule.resume": "resuming a scheduled task", "jarvis.schedule.add": "scheduling a task", "jarvis.schedule.list": "checking the schedule", "jarvis.schedule.remove": "removing a scheduled task", "jarvis.files.read_document": "reading a document", "jarvis.knowledge.search": "searching your files", "jarvis.code.delegate": "handing a task to the coding agent", "jarvis.gmail.save_attachment": "saving an email attachment", "jarvis.gmail.draft": "drafting an email", "jarvis.contacts.save": "saving a contact", "jarvis.contacts.search": "checking the contacts", "jarvis.contacts.stats": "counting the contacts", "jarvis.contacts.duplicates": "looking for duplicate contacts", "jarvis.contacts.merge": "merging two contacts"
  };
  function describeCall(tool, target) {
    var verb = VERBS[tool] || String(tool || "working").replace(/^jarvis\./, "");
    return clip(target ? verb + " " + target : verb, 48);
  }
  function duration(seconds) {
    if (seconds < 60) return seconds + "s";
    return Math.floor(seconds / 60) + "m " + (seconds % 60) + "s";
  }
  // Why a run stopped, in words a person can act on. The daemon sends a code and a message; the message alone ("the run
  // failed") says nothing.
  function explainFailure(code, message) {
    if (code === "too_many_tool_calls" || code === "too_many_model_calls") return "It reached its safety ceiling for one task (a very large number of steps). Say \u201Ccontinue\u201D and it picks up where it stopped.";
    if (code === "repeating_the_same_call") return "It got stuck repeating the same step, so I stopped it. Say \u201Ccontinue\u201D to try a different way.";
    return message || "the run failed";
  }

  function pendingAnswer() {
    var view = bubble("jarvis", "jarvis");
    view.body.classList.add("cursor");
    var text = "";
    var queued = false;
    var done = false;
    // A line that keeps you informed during a long task: the step it is on, what it is doing, and how long it has been.
    var status = el("div", "progress", "");
    status.hidden = true;
    view.node.insertBefore(status, view.chips);
    var began = Date.now(), steps = 0, doing = "thinking";
    var shownPictures = {};
    // A long quiet stretch is spoken about (when the voice is on), so you are not left listening to nothing: at most once a minute.
    var lastSignal = began, lastSpoken = 0, quietHook = null;
    function tick() {
      if (done) return;
      var seconds = Math.round((Date.now() - began) / 1000);
      var quiet = Date.now() - Math.max(lastSignal, lastSpoken);
      if (quietHook && quiet > 45000) { lastSpoken = Date.now(); quietHook(seconds); }
      status.hidden = steps === 0 && seconds < 8;
      status.textContent = (steps ? "step " + steps + " \u00B7 " : "") + doing + " \u00B7 " + duration(seconds);
    }
    var timer = setInterval(function () { if (done) clearInterval(timer); else tick(); }, 1000);
    function paint() {
      queued = false;
      if (done) return;
      view.body.replaceChildren(markdown(text));
      view.body.classList.add("cursor");
      scroll();
    }
    return {
      onQuiet: function (hook) { quietHook = hook; },
      add: function (piece) {
        lastSignal = Date.now();
        text += piece;
        if (!queued) { queued = true; requestAnimationFrame(paint); }
      },
      activity: function (label, isStep) {
        if (isStep) { steps += 1; lastSignal = Date.now(); }
        doing = label;
        S.activity = label;
        tick();
      },
      chip: function (label, waiting) {
        // The same label twice (a stream can repeat it) is one chip.
        var existing = view.chips.children;
        for (var i = 0; i < existing.length; i += 1) { if (existing[i].textContent === label) return; }
        view.chips.hidden = false;
        var chipNode = el("span", "chip" + (waiting ? " wait" : ""), label);
        view.chips.appendChild(chipNode);
        launchPacket(chipNode, waiting);
        // A big task makes dozens of these; the latest few are shown and the rest are counted.
        var all = [].filter.call(view.chips.children, function (chip) { return !chip.classList.contains("more"); });
        var hidden = Math.max(0, all.length - 6);
        all.forEach(function (chip, index) { chip.hidden = index < hidden; });
        var more = view.chips.querySelector(".more");
        if (hidden > 0) {
          if (!more) { more = el("span", "chip more", ""); view.chips.insertBefore(more, view.chips.firstChild); }
          more.textContent = "+" + hidden + " earlier";
        }
      },
      // Pictures a tool made (a screenshot): fetched with the credential, so shown from a blob rather than a bare address.
      pictures: function (ids) {
        ids.forEach(function (id) {
          if (shownPictures[id]) return;
          shownPictures[id] = true;
          addPicture(view, id);
        });
      },
      sources: function (links) {
        if (!links.length) return;
        var strip = el("div", "asources");
        strip.appendChild(el("span", "dim", "sources"));
        links.slice(0, 8).forEach(function (link) {
          var node = JarvisMission.chip(link, "mchip");
          if (node) strip.appendChild(node);
        });
        view.node.appendChild(strip);
        scroll();
      },
      finish: function (final) {
        done = true;
        clearInterval(timer);
        S.activity = "";
        if (steps > 0) { status.hidden = false; status.textContent = "done \u00B7 " + steps + " step" + (steps === 1 ? "" : "s") + " \u00B7 " + duration(Math.round((Date.now() - began) / 1000)); }
        else status.hidden = true;
        if (typeof final === "string" && final.length) text = final;
        view.body.classList.remove("cursor");
        view.body.replaceChildren(markdown(text || "(no answer)"));
        remember("jarvis", text || "(no answer)", Object.keys(shownPictures));
        scroll();
        return text;
      },
      fail: function (message, onContinue) {
        done = true;
        clearInterval(timer);
        S.activity = "";
        status.hidden = steps === 0;
        if (steps > 0) status.textContent = "stopped after " + steps + " step" + (steps === 1 ? "" : "s") + " \u00B7 " + duration(Math.round((Date.now() - began) / 1000));
        view.node.classList.add("error");
        view.body.classList.remove("cursor");
        view.body.replaceChildren(document.createTextNode(message));
        if (onContinue) {
          var again = el("button", "yes", "Continue");
          again.addEventListener("click", function () { again.disabled = true; onContinue(); });
          view.body.appendChild(document.createElement("br"));
          view.body.appendChild(again);
        }
        remember("system", message);
      }
    };
  }

  // ---- following a run's stream (server-sent events over fetch, so the credential can be a header) -------------
  function parseBlock(block) {
    var frame = { id: null, event: null, data: "" };
    block.split("\n").forEach(function (line) {
      if (line.indexOf("id:") === 0) frame.id = line.slice(3).trim();
      else if (line.indexOf("event:") === 0) frame.event = line.slice(6).trim();
      else if (line.indexOf("data:") === 0) frame.data += line.slice(5).trim();
    });
    if (!frame.event) return null;
    try { frame.body = JSON.parse(frame.data); } catch (ignored) { frame.body = {}; }
    return frame;
  }

  var TERMINAL = { run_completed: 1, run_failed: 1, run_cancelled: 1 };

  async function follow(runId, view, talker) {
    var last = 0, finished = false, attempts = 0, finalText = null, failure = null, failedCode = "";
    while (!finished && attempts < 4) {
      try {
        var headers = { Authorization: "Bearer " + token };
        if (last) headers["Last-Event-ID"] = String(last);
        var response = await fetch("/api/v1/runs/" + encodeURIComponent(runId) + "/stream", { headers: headers, cache: "no-store" });
        if (!response.ok) throw new Error(String(response.status));
        var reader = response.body.getReader();
        var decoder = new TextDecoder();
        var buffer = "";
        for (;;) {
          var chunk = await reader.read();
          if (chunk.done) break;
          buffer += decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, "\n");
          var cut;
          while ((cut = buffer.indexOf("\n\n")) >= 0) {
            var frame = parseBlock(buffer.slice(0, cut));
            buffer = buffer.slice(cut + 2);
            if (!frame) continue;
            if (frame.id) last = Number(frame.id) || last;
            var payload = (frame.body && frame.body.payload) || {};
            var summary = (frame.body && frame.body.summary) || "";
            if (window.JarvisMission) JarvisMission.event(runId, "", frame);
            if (frame.event === "output_delta") { view.add(payload.text || ""); if (talker) feed(talker, payload.text || ""); }
            else if (frame.event === "output_completed") finalText = payload.text;
            else if (frame.event === "tool_requested") {
              var what = describeCall(payload.tool, payload.target);
              view.chip(payload.target ? what : (summary || "using a tool"));
              view.activity(what, true);
            }
            else if (frame.event === "activity_updated" && payload.phase) {
              // The model is working but has said nothing yet (reasoning, or writing a large tool call): show that it is alive.
              if (payload.phase === "tool_result") { if (payload.images && payload.images.length) view.pictures(payload.images); continue; }
              if (payload.phase === "reasoning_limit") { view.activity("it was thinking too long, so I asked it to act", false); continue; }
              var size = Math.round((payload.chars || 0) / 4);
              var amount = size >= 1000 ? (size / 1000).toFixed(1) + "k" : String(size);
              view.activity((payload.phase === "reasoning" ? "thinking" : "writing a large call") + " (~" + amount + " tokens)", false);
            }
            else if (frame.event === "state_changed" && summary === "interpreting the tool result") view.activity("checking the result", false);
            else if (frame.event === "state_changed" && summary === "calling the model") view.activity("thinking", false);
            else if (frame.event === "approval_requested") view.chip("waiting for your approval", true);
            else if (frame.event === "run_failed") { failure = explainFailure(payload.error_code, payload.message || summary); failedCode = payload.error_code || ""; }
            else if (frame.event === "run_cancelled") failure = "stopped";
            if (TERMINAL[frame.event]) finished = true;
          }
          if (finished) break;
        }
      } catch (error) {
        await sleep(700);
      }
      if (!finished) attempts += 1;
    }
    if (failure) {
      var resumable = failure !== "stopped";
      view.fail(failure === "stopped" ? "Stopped." : "That did not complete: " + failure,
        resumable ? function () { send("Continue where you left off."); } : null);
      if (failure !== "stopped") feel("concerned", 3.5);
      if (talker) interruptSpeech();
      return null;
    }
    var finished = view.finish(finalText);
    if (window.JarvisMission) view.sources(JarvisMission.sourcesOf(runId));
    return finished;
  }

  var busy = false;
  async function send(text) {
    text = String(text || "").trim();
    if (!text || busy) return;
    if (S.offline) { say(token ? "Reconnect to JARVIS before sending. Your draft stays here." : "Open the console with `jarvis hud` before sending."); return; }
    interruptSpeech();
    clearEmpty();
    show("user", text);
    remember("user", text);
    var view = pendingAnswer();
    // With the neural voice, speech starts while the answer is still being written (see "the neural voice, streamed").
    var talker = S.speak && neural.enabled ? neuralSession() : null;
    if (talker) {
      view.onQuiet(function (seconds) {
        var minutes = Math.floor(seconds / 60);
        var said = minutes < 1 ? "Still working on it." : "Still working. About " + minutes + (minutes > 1 ? " minutes" : " minute") + " in.";
        // Newlines on both sides make it a sentence of its own even if the answer was mid-sentence.
        feed(talker, "\n" + said + "\n");
      });
    }
    feel("attentive", 1.2, "nod");
    if (window.JarvisMission) JarvisMission.begin();
    busy = true;
    S.thinking = 1;
    paintHeader();
    try {
      var body = { objective: text };
      if (S.session) body.session_id = S.session;
      if (currentChat.project) body.project_id = currentChat.project;
      var reply;
      try {
        reply = await api("/runs", "POST", body);
      } catch (error) {
        // A conversation that no longer exists (a fresh profile, a cleared database) starts a new one rather than failing.
        if (error.status !== 404 || !(S.session || body.project_id)) throw error;
        if (S.session) { setSession(null); delete body.session_id; }
        try {
          reply = await api("/runs", "POST", body);
        } catch (second) {
          // A project that was deleted: carry on without it rather than failing.
          if (second.status !== 404 || !body.project_id) throw second;
          currentChat.project = null;
          delete body.project_id;
          saveChats();
          paintProjectPick();
          notice("That project no longer exists, so this conversation continues without it.");
          reply = await api("/runs", "POST", body);
        }
      }
      setSession(reply.session_id);
      S.runId = reply.run_id;
      var answer = await follow(reply.run_id, view, talker);
      if (answer) react(answer, !!talker);
      if (talker) { if (answer) endFeed(talker, answer); else interruptSpeech(); }
      else if (answer && S.speak) speak(answer);
    } catch (error) {
      if (talker) interruptSpeech();
      view.fail(error.status === 401 ? "The daemon rejected the credential. Open this page with `jarvis hud`."
        : error.status === 409 ? "This conversation already belongs to another project. Start a new chat to use a different one."
        : "Could not reach the assistant.");
    } finally {
      busy = false;
      S.thinking = 0;
      paintHeader();
      refresh();
    }
  }

  $("composer").addEventListener("submit", function (event) {
    event.preventDefault();
    var input = $("input");
    var text = input.value;
    if (!text.trim() || busy || S.offline) return;
    input.value = "";
    send(text);
  });
  $("input").addEventListener("keydown", function (event) {
    if (event.key === "Enter" && !event.isComposing) {
      event.preventDefault();
      $("composer").requestSubmit();
    }
  });

  // ---- voice: speaking -------------------------------------------------------------------------------------------
  // Two voices. When the daemon has a speech provider configured (ADR-0138) the answer is spoken in that neural voice:
  // the page asks the daemon for audio (the provider's key never leaves the daemon) and plays it, making the next
  // sentence group while the current one plays. Otherwise, or if that fails, the browser's own synthesis is used,
  // choosing the best voice it has. Either way the head's mouth follows what is actually being said.
  var synth = window.speechSynthesis;
  var neural = { enabled: false, voice: "", warnedBasic: false };
  var playing = { token: 0, element: null, controller: null, session: null, context: null, analyser: null, data: null };

  function plain(text) {
    return String(text).replace(/```[\s\S]*?```/g, " code omitted. ").replace(/[`*_#>]/g, "").replace(/\s+/g, " ").trim();
  }

  // Browser voices: a "Natural" or "Online" voice (Edge ships good ones) beats a Google voice, which beats the legacy
  // desktop voices that sound robotic; British male first, because that is the voice this assistant is meant to have.
  function rankVoice(voice) {
    if (!/^en/i.test(voice.lang)) return -1;
    var score = 0;
    if (/natural|neural|online/i.test(voice.name)) score += 100;
    if (/Google/i.test(voice.name)) score += 30;
    if (/en[-_]GB|UK|British/i.test(voice.name + " " + voice.lang)) score += 20;
    if (/Ryan|George|Thomas|Daniel|Oliver|Guy|Davis|Andrew|Brian|Christopher|Eric|Male/i.test(voice.name)) score += 15;
    return score;
  }
  function pickVoice() {
    if (!synth) return null;
    var best = null, top = -1;
    synth.getVoices().forEach(function (voice) {
      var score = rankVoice(voice);
      if (score > top) { top = score; best = voice; }
    });
    if (best && top < 100 && !neural.warnedBasic) {
      neural.warnedBasic = true;
      say("This browser only has a basic voice. For a natural one: use Edge, or add an ElevenLabs key (see getting started).");
    }
    return best;
  }

  // Sentences grouped so the first group is short (it starts playing sooner) and later ones are longer (fewer requests).
  function groups(text) {
    var sentences = text.match(/[^.!?]+[.!?]*\s*/g) || [text];
    var out = [], current = "";
    sentences.forEach(function (sentence) {
      var limit = out.length === 0 ? 140 : 500;
      if (current && (current + sentence).length > limit) { out.push(current.trim()); current = ""; }
      current += sentence;
    });
    if (current.trim()) out.push(current.trim());
    return out;
  }

  function finishSpeaking(after) {
    S.speaking = false;
    resumeWake();
    if (after) after();
  }

  function speakBrowser(text, after) {
    if (!synth) { finishSpeaking(after); return; }
    var sentences = text.slice(0, 1800).match(/[^.!?]+[.!?]*/g) || [];
    if (!sentences.length) { S.speaking = false; return; }
    S.voiceKind = "browser";
    var voice = pickVoice();
    sentences.forEach(function (sentence, index) {
      var utterance = new SpeechSynthesisUtterance(sentence.trim());
      if (voice) { utterance.voice = voice; utterance.lang = voice.lang; }
      utterance.rate = 1;
      utterance.pitch = 1;
      if (index === 0) utterance.onstart = function () { S.speaking = true; };
      if (index === sentences.length - 1) {
        utterance.onend = utterance.onerror = function () { finishSpeaking(after); };
      }
      synth.speak(utterance);
    });
  }

  // ---- the neural voice, streamed ----------------------------------------------------------------------------------
  // Speech starts while the answer is still being written, not after it: each sentence is sent to the daemon as soon as it is
  // complete, the daemon streams the provider's audio straight back, and the page plays it as it arrives (Media Source), with
  // the next sentence already on its way. So the first words are heard about a quarter of a second after the first sentence
  // exists, instead of after the whole answer and the whole first group have been made. S.speaking is true only while sound is
  // actually playing, so the face is not "speaking" while it is still waiting for its voice (S.preparing says that instead).
  var HAS_MSE = !!(window.MediaSource && MediaSource.isTypeSupported && MediaSource.isTypeSupported("audio/mpeg"));
  var MIN_PART = 25, MAX_PART = 1400, BATCH_CHARS = 160, GAP_GRACE_MS = 450, LOOKAHEAD = 2;

  function attachAnalyser(element) {
    try {
      var Context = window.AudioContext || window.webkitAudioContext;
      if (!playing.context) playing.context = new Context();
      playing.analyser = playing.context.createAnalyser();
      playing.analyser.fftSize = 256;
      playing.data = new Uint8Array(playing.analyser.fftSize);
      var source = playing.context.createMediaElementSource(element);
      source.connect(playing.analyser);
      playing.analyser.connect(playing.context.destination);
      if (playing.context.state === "suspended") playing.context.resume();
    } catch (ignored) { playing.analyser = null; }
  }

  // Feeds a streaming response into an <audio> element, resolving once the first audio is buffered (so it can start at once).
  function streamInto(response, element) {
    return new Promise(function (resolve, reject) {
      var source = new MediaSource();
      var url = URL.createObjectURL(source);
      var reader = response.body.getReader();
      var buffer = null, queue = [], finished = false, started = false;
      function pump() {
        if (!buffer || buffer.updating) return;
        if (queue.length) {
          try { buffer.appendBuffer(queue.shift()); } catch (error) { fail(error); }
          return;
        }
        if (finished && source.readyState === "open") { try { source.endOfStream(); } catch (ignored) { /* already ended */ } }
      }
      function fail(error) {
        URL.revokeObjectURL(url);
        if (!started) reject(error); else element.dispatchEvent(new Event("error"));
      }
      source.addEventListener("sourceopen", function () {
        try {
          buffer = source.addSourceBuffer("audio/mpeg");
        } catch (error) { reject(error); return; }
        buffer.addEventListener("updateend", function () {
          if (!started) { started = true; resolve(url); }
          pump();
        });
        (function read() {
          reader.read().then(function (chunk) {
            if (chunk.done) { finished = true; pump(); return; }
            queue.push(chunk.value);
            pump();
            read();
          }, fail);
        })();
      }, { once: true });
      element.src = url;
    });
  }

  // One request for one piece of speech; resolves with an element that is ready to play.
  function openSpeech(session, item) {
    var body = { text: item.text };
    if (item.previous) body.previous_text = item.previous;
    return fetch("/api/v1/speech", {
      method: "POST", cache: "no-store", signal: session.controller.signal,
      headers: { Authorization: "Bearer " + token, "Content-Type": "application/json" },
      body: JSON.stringify(body)
    }).then(function (response) {
      if (!response.ok) { var error = new Error(String(response.status)); error.status = response.status; throw error; }
      var element = new Audio();
      element.preload = "auto";
      if (HAS_MSE && response.body && response.body.getReader) {
        return streamInto(response, element).then(function (url) { return { element: element, url: url }; });
      }
      return response.blob().then(function (blob) {
        var url = URL.createObjectURL(blob);
        element.src = url;
        return { element: element, url: url };
      });
    });
  }

  function alive(session) { return session.turn === playing.token; }

  function neuralSession(after) {
    interruptSpeech();
    stopListening();
    var session = {
      turn: playing.token, controller: new AbortController(), items: [], current: -1, playingItem: null,
      fed: "", consumed: 0, ended: false, after: after, gap: null
    };
    playing.controller = session.controller;
    playing.session = session;
    S.voiceBusy = true;
    S.preparing = true;
    S.voiceKind = "neural";
    return session;
  }

  function endSession(session) {
    if (!alive(session)) return;
    playing.session = null;
    S.voiceBusy = false;
    S.preparing = false;
    S.speaking = false;
    resumeWake();
    if (session.after) session.after();
  }

  // The neural voice failed (key, quota, network, playback): say the rest with the browser's voice, so the answer is still heard.
  function fallbackToBrowser(session, error, rest) {
    if (!alive(session)) return;
    if (error && error.status === 404) neural.enabled = false;
    say("The neural voice is unavailable (" + ((error && error.status) || "no reply") + "), using the browser's voice.");
    var after = session.after;
    playing.token += 1;
    session.controller.abort();
    playing.session = null;
    S.voiceBusy = false;
    S.preparing = false;
    S.voiceKind = "browser";
    var text = plain(rest);
    if (text) speakBrowser(text, after); else { S.speaking = false; if (after) after(); }
  }

  function remainingText(session, from) {
    var parts = session.items.slice(from).map(function (item) { return item.text; });
    parts.push(plain(session.fed.slice(session.consumed)));
    return parts.join(" ");
  }

  // Keeps the next few pieces loading, and starts the next one playing when its turn comes.
  function drive(session) {
    if (!alive(session)) return;
    var i;
    for (i = Math.max(0, session.current + 1); i < session.items.length && i <= session.current + 1 + LOOKAHEAD; i += 1) {
      (function (item, index) {
        if (item.state !== "idle") return;
        item.state = "loading";
        openSpeech(session, item).then(function (handle) {
          if (!alive(session)) { URL.revokeObjectURL(handle.url); return; }
          item.handle = handle;
          item.state = "ready";
          drive(session);
        }, function (error) {
          if (!alive(session)) return;
          // A first piece that never arrives is a failure to speak at all; later ones fall back from where they were.
          item.state = "failed";
          item.error = error;
          drive(session);
        });
      })(session.items[i], i);
    }
    if (session.playingItem !== null) return;
    var next = session.items[session.current + 1];
    if (!next) {
      if (session.ended) endSession(session);
      return;
    }
    if (next.state === "failed") { fallbackToBrowser(session, next.error, remainingText(session, session.current + 1)); return; }
    if (next.state !== "ready") return;
    play(session, session.current + 1);
  }

  function play(session, index) {
    var item = session.items[index];
    var element = item.handle.element;
    clearTimeout(session.gap);
    session.current = index;
    session.playingItem = index;
    item.state = "playing";
    playing.element = element;
    attachAnalyser(element);
    function done(error) {
      URL.revokeObjectURL(item.handle.url);
      if (playing.element === element) playing.element = null;
      if (!alive(session)) return;
      session.playingItem = null;
      item.state = "done";
      if (error) { fallbackToBrowser(session, null, remainingText(session, index + 1)); return; }
      // A short silence between pieces is not "stopped speaking": the face holds its pose for a moment while the next arrives.
      session.gap = setTimeout(function () {
        if (!alive(session) || session.playingItem !== null) return;
        S.speaking = false;
        S.preparing = !session.ended || session.items.length > session.current + 1;
      }, GAP_GRACE_MS);
      drive(session);
    }
    element.onplaying = function () { if (alive(session)) { S.speaking = true; S.preparing = false; } };
    element.onended = function () { done(null); };
    element.onerror = function () { done(new Error("playback")); };
    var started = element.play();
    if (started && started.catch) started.catch(function () { done(new Error("playback")); });
  }

  function enqueue(session, raw) {
    var spoken = plain(raw);
    if (!spoken) return;
    // The face takes the feeling of what it is about to say (worry for an apology, pleasure for good news).
    if (window.JarvisHead) JarvisHead.speakMood(JarvisHead.mood(spoken));
    var pieces = spoken.length > MAX_PART ? groups(spoken) : [spoken];
    pieces.forEach(function (piece) {
      var last = session.items[session.items.length - 1];
      session.items.push({ text: piece, previous: last ? last.text : "", state: "idle", handle: null, error: null });
    });
    drive(session);
  }

  // Text arrives as the answer is written. A sentence is spoken as soon as it is complete (a full stop followed by a space or
  // a line break), unless it is inside a code block, which is not read out.
  function feed(session, delta) {
    if (!alive(session) || !delta) return;
    session.fed += delta;
    for (;;) {
      var pending = session.fed.slice(session.consumed);
      if ((pending.match(/```/g) || []).length % 2 === 1) return;
      var match = /([.!?]+["')\]]*\s+|\n+)/.exec(pending);
      if (!match) return;
      var cut = match.index + match[0].length;
      var candidate = pending.slice(0, cut);
      var queuedAhead = session.items.length - 1 - session.current;
      // Speak at once when nothing is waiting to be played; otherwise let a few sentences gather into one request.
      if (plain(candidate).length < MIN_PART) {
        var more = /([.!?]+["')\]]*\s+|\n+)/.exec(pending.slice(cut));
        if (!more) return;
        cut += more.index + more[0].length;
        candidate = pending.slice(0, cut);
      }
      if (queuedAhead > 0 && plain(candidate).length < BATCH_CHARS) {
        var next = /([.!?]+["')\]]*\s+|\n+)/.exec(pending.slice(cut));
        if (!next) return;
        cut += next.index + next[0].length;
        candidate = pending.slice(0, cut);
      }
      session.consumed += cut;
      enqueue(session, candidate);
    }
  }

  // The answer is complete: speak whatever is left, and finish when it has been played.
  function endFeed(session, finalText) {
    if (!alive(session)) return;
    var head = session.fed.slice(0, session.consumed);
    var tail = typeof finalText === "string" && finalText.slice(0, session.consumed) === head
      ? finalText.slice(session.consumed) : session.fed.slice(session.consumed);
    session.fed = head + tail;
    session.consumed = session.fed.length;
    session.ended = true;
    if (tail.trim()) enqueue(session, tail); else drive(session);
  }

  function speakNeural(text, after) {
    var session = neuralSession(after);
    feed(session, text);
    endFeed(session, null);
  }
  function speak(text, after) {
    interruptSpeech();
    var spoken = plain(text);
    if (!spoken) return;
    stopListening();
    if (neural.enabled) speakNeural(spoken, after); else speakBrowser(spoken, after);
  }

  // How loud the voice is right now, from the audio actually playing; the head's mouth follows it.
  function readVoiceLevel() {
    var level = 0;
    if (playing.analyser && playing.element && !playing.element.paused) {
      playing.analyser.getByteTimeDomainData(playing.data);
      var sum = 0;
      for (var i = 0; i < playing.data.length; i += 1) { var v = (playing.data[i] - 128) / 128; sum += v * v; }
      level = Math.min(1, Math.sqrt(sum / playing.data.length) * 5);
    }
    S.voiceLevel += (level - S.voiceLevel) * 0.5;
  }

  // The "you can interrupt it" half of voice: the microphone, Escape, the orb, or a new message silences it at once.
  function interruptSpeech() {
    var was = S.speaking || S.voiceBusy;
    playing.token += 1;
    if (playing.session) {
      clearTimeout(playing.session.gap);
      playing.session.items.forEach(function (item) { if (item.handle) URL.revokeObjectURL(item.handle.url); });
      playing.session = null;
    }
    if (playing.controller) { playing.controller.abort(); playing.controller = null; }
    if (playing.element) { playing.element.onended = playing.element.onerror = playing.element.onplaying = null; playing.element.pause(); playing.element = null; }
    if (synth && (synth.speaking || synth.pending)) synth.cancel();
    S.speaking = false;
    S.voiceBusy = false;
    S.preparing = false;
    if (was) resumeWake();
  }
  // Whether the daemon has a speech provider; asked once the credential is known.
  function learnVoice() {
    if (!token) return;
    api("/speech").then(function (reply) {
      neural.enabled = !!(reply && reply.enabled);
      if (neural.enabled) neural.voice = reply.voice || "";
      $("speak").disabled = !(neural.enabled || synth);
    }, function () {});
  }  $("speak").checked = S.speak;
  $("speak").addEventListener("change", function () {
    S.speak = $("speak").checked;
    localStorage.setItem("jarvis-speak", S.speak ? "1" : "0");
    if (!S.speak) interruptSpeech();
  });
  if (!synth) { $("speak").disabled = true; }
  learnVoice();

  // ---- voice: listening (push-to-talk and wake word) -----------------------------------------------------------------
  var Recognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  var recognizer = null;
  var wakeArmedUntil = 0;
  var audio = { context: null, analyser: null, stream: null, data: null };

  function startMeter() {
    if (audio.context || !navigator.mediaDevices) return;
    navigator.mediaDevices.getUserMedia({ audio: true }).then(function (stream) {
      var Context = window.AudioContext || window.webkitAudioContext;
      audio.stream = stream;
      audio.context = new Context();
      audio.analyser = audio.context.createAnalyser();
      audio.analyser.fftSize = 256;
      audio.data = new Uint8Array(audio.analyser.fftSize);
      audio.context.createMediaStreamSource(stream).connect(audio.analyser);
    }).catch(function () {});
  }
  function stopMeter() {
    if (audio.stream) audio.stream.getTracks().forEach(function (track) { track.stop(); });
    if (audio.context) audio.context.close();
    audio = { context: null, analyser: null, stream: null, data: null };
  }
  function readLevel() {
    if (!audio.analyser) { S.level *= 0.9; return; }
    audio.analyser.getByteTimeDomainData(audio.data);
    var sum = 0;
    for (var i = 0; i < audio.data.length; i += 1) { var v = (audio.data[i] - 128) / 128; sum += v * v; }
    S.level += (Math.min(1, Math.sqrt(sum / audio.data.length) * 4) - S.level) * 0.35;
  }

  function say(hint) { $("hint").textContent = hint || ""; }
  // Tell the face what just happened; it decides how to look about it. Absent when the face script did not load.
  function feel(name, seconds, gesture) {
    if (!window.JarvisHead) return;
    if (name) JarvisHead.emote(name, seconds);
    if (gesture === "nod") JarvisHead.nod();
    else if (gesture === "shake") JarvisHead.shake();
  }
  // An answer carries a feeling (sorry, glad, a question), which the face holds while it speaks and shows briefly if it does not.
  function react(text, streamed) {
    if (!window.JarvisHead) return;
    var mood = JarvisHead.mood(text);
    // A streamed answer sets the mood sentence by sentence as it is spoken; this is for a voice that speaks it whole.
    if (!streamed) JarvisHead.speakMood(mood);
    if (!S.speak) JarvisHead.emote(mood, 3);
    if (mood === "pleased" || mood === "amused") JarvisHead.nod();
  }
  // A line in the conversation itself, because the hint under the box is easy to miss and an answer you gave (a yes, a
  // Keep) should visibly land. Kept in the transcript so it survives a reload.
  function notice(text) {
    show("system", text);
    remember("system", text);
    scroll();
  }

  // Answering a question: a plain yes or no is an answer, and only while something is actually waiting.
  var YES = /^(yes|yeah|yep|yup|sure|ok|okay|approve|approved|allow|confirm|proceed|go ahead|do it)( please| jarvis)?$/i;
  var NEWCHAT = /^(new (chat|conversation|session)|start (a )?new (chat|conversation|session)|start over|clear (the )?chat)( please| jarvis)?$/i;
  var NO = /^(no|nope|nah|deny|denied|refuse|reject|don't|do not|don't do it)( please| jarvis)?$/i;

  // One click or one word: record the owner's answer and let the run carry on.
  function decide(approvalId, approve, via) {
    var asked = S.pending.filter(function (approval) { return approval.approval_id === approvalId; })[0];
    var what = asked && asked.tool ? asked.tool : "that";
    return api("/approvals/" + encodeURIComponent(approvalId) + "/decision", "POST",
      { decision: approve ? "approve" : "deny", resume: true, channel: via || "desktop" })
      .then(function () {
        say(approve ? "approved" : "denied");
        // Only when this was the one thing waiting, so a second pending question keeps its own chip.
        if (S.pending.length <= 1) document.querySelectorAll(".chip.wait").forEach(function (chip) {
          chip.textContent = approve ? "approved" : "denied";
          chip.className = "chip " + (approve ? "done" : "refused");
        });
        notice(approve ? "Approved " + what + ". Carrying on." : "Denied " + what + ". It was not done.");
        if (approve) feel("pleased", 2.2, "nod"); else feel("neutral", 1.8, "shake");
        celebrate(approvalId, approve);
      },
      function (error) {
        if (error.status === 409) {
          // Already answered, or no longer waiting (for example the daemon restarted while the call was running). Offering the same
          // button again would only repeat the refusal, so the list is refreshed and the question drops out of it.
          say("that question is no longer waiting");
          notice("That question was already answered or is no longer waiting, so it was cleared. If the work did not finish, ask again.");
          return;
        }
        say("could not record that (" + (error.status || "no reply") + ")");
        notice("Could not record that answer (" + (error.status || "no reply") + "). It is still waiting.");
      })
      .then(refresh);
  }

  function answerPending(approve) {
    // A spoken yes cannot approve what becomes a standing instruction or changes memory: its text is not heard, only read. A no is always fine.
    if (S.pending.length === 1 && approve && NEVER_TRUSTED[S.pending[0].tool]) {
      var screenOnly = "That one changes what I am told to do or remember, so please read it and press Approve on the screen.";
      say(screenOnly);
      if (S.speak) speak(screenOnly);
      return;
    }
    if (S.pending.length === 1) { decide(S.pending[0].approval_id, approve, "voice"); return; }
    if (S.pending.length === 0) { say("nothing is waiting for an answer"); return; }
    var message = S.pending.length + " things are waiting, so use the buttons to say which.";
    say(message);
    if (S.speak) speak(message);
  }

  // What a spoken utterance means. "stop" is the hands-free kill switch, a yes or no answers a waiting question, and
  // everything else is a message.
  function command(text) {
    var spoken = text.trim().replace(/[.!?]+$/, "");
    if (!spoken) return;
    if (NEWCHAT.test(spoken)) { newChat(); return; }
    if (YES.test(spoken)) { answerPending(true); return; }
    if (NO.test(spoken)) { answerPending(false); return; }
    if (/^(stop|cancel|abort|kill)( that| it| everything| all| now)?$/i.test(spoken)) { say("stopping everything"); stopEverything(); return; }
    if (/^(quiet|be quiet|silence|shut up|enough)$/i.test(spoken)) { interruptSpeech(); return; }
    send(spoken);
  }

  function onResult(event) {
    var heard = "";
    for (var i = event.resultIndex; i < event.results.length; i += 1) {
      var result = event.results[i];
      if (!result.isFinal) { say("\u201C" + clip(result[0].transcript, 60) + "\u201D"); continue; }
      heard = result[0].transcript;
    }
    if (!heard) return;
    say("");
    if (!S.wake) { stopListening(); command(heard); return; }
    // While something waits for an answer, a bare yes or no needs no wake word.
    if (S.pending.length && (YES.test(heard.trim().replace(/[.!?]+$/, "")) || NO.test(heard.trim().replace(/[.!?]+$/, "")))) {
      command(heard);
      return;
    }
    // Wake-word mode: only speech that names JARVIS (or follows it within a few seconds) is acted on.
    var match = /\bjarvis\b[\s,.:;!-]*(.*)$/i.exec(heard);
    if (match) {
      wakeFlare();
      if (match[1].trim()) command(match[1]);
      else wakeArmedUntil = Date.now() + 8000;
    } else if (Date.now() < wakeArmedUntil) {
      wakeArmedUntil = 0;
      command(heard);
    }
  }

  function startListening(continuous) {
    if (!Recognition) { say("this browser has no speech recognition (use Chrome or Edge)"); return; }
    if (recognizer) return;
    interruptSpeech();
    var made = new Recognition();
    recognizer = made;
    made.lang = navigator.language || "en-US";
    made.continuous = !!continuous;
    made.interimResults = true;
    made.onresult = onResult;
    made.onerror = function (event) {
      if (event.error === "not-allowed" || event.error === "service-not-allowed") {
        say("microphone permission was refused");
        S.wake = false;
        $("wake").checked = false;
        stopListening();
      }
    };
    made.onend = function () {
      if (recognizer !== made) return;
      recognizer = null;
      S.listening = false;
      $("mic").classList.remove("on");
      if (S.wake && !S.speaking) setTimeout(function () { startListening(true); }, 250);
      else if (!S.wake) stopMeter();
    };
    try {
      made.start();
      S.listening = true;
      $("mic").classList.add("on");
      startMeter();
    } catch (error) { recognizer = null; }
  }
  function stopListening() {
    var active = recognizer;
    recognizer = null;
    if (active) { active.onend = null; try { active.abort(); } catch (ignored) {} }
    S.listening = false;
    $("mic").classList.remove("on");
    if (!S.wake) stopMeter();
  }
  // Wake-word listening pauses while JARVIS speaks (its own voice would be heard as the user's) and resumes after.
  function resumeWake() {
    if (!S.wake) return;
    setTimeout(function () { if (S.wake && !recognizer && !S.speaking) startListening(true); }, 300);
  }

  $("mic").addEventListener("click", function () {
    if (S.speaking) { interruptSpeech(); return; }
    if (recognizer && !S.wake) { stopListening(); return; }
    if (!recognizer) startListening(false);
  });
  $("wake").addEventListener("change", function () {
    S.wake = $("wake").checked;
    if (S.wake) { stopListening(); startListening(true); say("listening for \u201CJarvis\u201D"); }
    else { stopListening(); say(""); }
  });
  if (!Recognition) {
    $("mic").classList.add("unsupported");
    $("wake").disabled = true;
    say("voice input needs Chrome or Edge");
  } else {
    $("mic").title = "Talk (M). The browser's recognizer may send audio to its vendor's service.";
  }

  // ---- stopping ------------------------------------------------------------------------------------------------
  function stopEverything() {
    interruptSpeech();
    feel("alert", 1.6, "shake");
    return api("/runs?limit=50").then(function (list) {
      return Promise.all(list.runs.filter(function (run) { return !run.outcome; }).map(function (run) {
        return api("/runs/" + encodeURIComponent(run.run_id) + "/cancel", "POST").catch(function () {});
      }));
    }).then(refresh, refresh);
  }
  $("stop").addEventListener("click", function () {
    $("stop").disabled = true;
    stopEverything().then(function () { $("stop").disabled = false; });
  });

  document.addEventListener("keydown", function (event) {
    var focused = document.activeElement;
    var typing = focused && (/^(INPUT|TEXTAREA|SELECT)$/.test(focused.tagName) || focused.isContentEditable);
    if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey || document.querySelector(".overlay:not([hidden])")) return;
    if (event.key === "Escape") {
      if ($("voice-options").open) { $("voice-options").open = false; $("voice-options-toggle").focus(); return; }
      if (!$("ops").hidden) { setOps(false); return; }
      if (!$("chats").hidden) { closeChats(); return; }
      interruptSpeech(); if (recognizer && !S.wake) stopListening(); return;
    }
    if (typing) return;
    if (event.key === "o" || event.key === "O") { event.preventDefault(); setOps($("ops").hidden); }
    else if (event.key === "n" || event.key === "N") { event.preventDefault(); newChat(); }
    else if (event.key === "/") { event.preventDefault(); $("input").focus(); }
    else if (event.key === "m" || event.key === "M") { event.preventDefault(); $("mic").click(); }
  });

  // ---- the Ops page: everything that is not the face or the conversation --------------------------------------------
  document.addEventListener("click", function (event) {
    var options = $("voice-options");
    if (options.open && !options.contains(event.target)) options.open = false;
  });
  function mountActivityTasks() {
    if (!settingsControls || !$("coding-tasks")) return;
    if (codingTasksDispose) codingTasksDispose();
    codingTasksDispose = settingsControls.mountCodingTasks($("coding-tasks"), function () { return !$("ops").hidden && !$("coding-tasks-frame").hidden; });
  }
  function setOps(open) {
    $("ops").hidden = !open;
    if (open) mountActivityTasks();
    else if (codingTasksDispose) { codingTasksDispose(); codingTasksDispose = null; }
    if (open) { abilitiesAt = 0; if (typeof refreshAbilities === "function") refreshAbilities(); loadProjects(); }
    if (!memoryLoaded && !$("ops").hidden) loadMemory(true);
  }
  var codingTasksDispose = null;
  function selectOpsSection(section) {
    if (section === "activity") mountActivityTasks();
    document.querySelectorAll("[data-ops-section]").forEach(function (panel) { panel.hidden = panel.dataset.opsSection !== section; });
    ["activity", "automations", "knowledge", "tools"].forEach(function (id) { $("ops-" + id).setAttribute("aria-pressed", id === section ? "true" : "false"); });
    if (section === "knowledge" && !memoryLoaded) loadMemory(true);
  }
  var OPS_SECTIONS = ["activity", "automations", "knowledge", "tools"];
  OPS_SECTIONS.forEach(function (id) { $("ops-" + id).addEventListener("click", function () { selectOpsSection(id); }); });
  // Inside Operations: 1-4 jump to a section, and the arrow keys move along the section buttons.
  $("ops").addEventListener("keydown", function (event) {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    var focused = document.activeElement;
    if (focused && (/^(INPUT|TEXTAREA|SELECT)$/.test(focused.tagName) || focused.isContentEditable)) return;
    var at = OPS_SECTIONS.indexOf(focused && focused.id ? focused.id.replace(/^ops-/, "") : "");
    var to = /^[1-4]$/.test(event.key) ? Number(event.key) - 1
      : at >= 0 && (event.key === "ArrowRight" || event.key === "ArrowLeft") ? (at + (event.key === "ArrowRight" ? 1 : OPS_SECTIONS.length - 1)) % OPS_SECTIONS.length : -1;
    if (to < 0) return;
    event.preventDefault();
    selectOpsSection(OPS_SECTIONS[to]);
    $("ops-" + OPS_SECTIONS[to]).focus();
  });
  $("open-ops").addEventListener("click", function () { setOps($("ops").hidden); });
  $("ops-close").addEventListener("click", function () { setOps(false); });
  $("ops").addEventListener("click", function (event) { if (event.target === $("ops")) setOps(false); });

  // ---- the side panels -----------------------------------------------------------------------------------------
  function age(iso) {
    var seconds = Math.max(0, (Date.now() - Date.parse(iso)) / 1000);
    if (seconds < 60) return Math.floor(seconds) + "s";
    if (seconds < 3600) return Math.floor(seconds / 60) + "m";
    if (seconds < 86400) return Math.floor(seconds / 3600) + "h";
    return Math.floor(seconds / 86400) + "d";
  }
  function ageSpan(iso, suffix) {
    var span = el("span", "dim", age(iso) + (suffix || ""));
    span.dataset.since = iso;
    span.dataset.suffix = suffix || "";
    return span;
  }
  function tickAges() {
    document.querySelectorAll("[data-since]").forEach(function (span) {
      span.textContent = age(span.dataset.since) + span.dataset.suffix;
    });
    document.querySelectorAll("[data-until]").forEach(function (span) { span.textContent = until(span.dataset.until); });
    document.querySelectorAll("[data-now]").forEach(function (line) {
      var doing = window.JarvisMission ? JarvisMission.nowFor(line.dataset.now) : "";
      line.textContent = doing;
      line.hidden = !doing;
    });
  }
  function fill(id, nodes, empty) {
    var host = $(id);
    host.replaceChildren();
    if (nodes.length === 0) host.appendChild(el("div", "none", empty));
    nodes.forEach(function (node) { host.appendChild(node); });
  }
  function stopButton(runId) {
    var button = el("button", "danger", "Stop");
    button.addEventListener("click", function () {
      button.disabled = true;
      api("/runs/" + encodeURIComponent(runId) + "/cancel", "POST").then(refresh, refresh);
    });
    return button;
  }
  // Tools that write standing instructions or change memory: the daemon never lets them run without a yes, so the card does not offer it.
  var NEVER_TRUSTED = { "jarvis.project.create": 1, "jarvis.project.update": 1, "jarvis.project.assign_schedule": 1, "jarvis.memory.correct": 1, "jarvis.memory.forget": 1 };
  function answerButtons(approvalId, tool) {
    var row = el("div", "row cmd");
    var yes = el("button", "yes", "Approve");
    var no = el("button", "danger", "Deny");
    function answer(approve) {
      yes.disabled = true;
      no.disabled = true;
      if (always) always.disabled = true;
      decide(approvalId, approve, "desktop");
    }
    // "Always": the owner's own click trusts this tool from now on (the same setting as Settings, Permissions), then approves.
    // It applies at once; anything that talks to other people still asks, whatever is chosen.
    var always = tool && !NEVER_TRUSTED[tool] ? el("button", "", "Always allow") : null;
    if (always) {
      always.title = "Approve, and stop asking for " + tool + ". Change it any time in Settings, Permissions.";
      always.addEventListener("click", function () {
        yes.disabled = true;
        no.disabled = true;
        always.disabled = true;
        putJson("/settings/tools/" + encodeURIComponent(tool), "PUT", { posture: "trusted" }).then(function (reply) {
          notice(tool + " will no longer ask" + (reply.applied ? "." : " after a restart.") + " Change it in Settings, Permissions.");
          decide(approvalId, true, "desktop");
        }, function () {
          notice("Could not change the permission for " + tool + ". Nothing was approved.");
          yes.disabled = false;
          no.disabled = false;
          always.disabled = false;
        });
      });
    }
    yes.addEventListener("click", function () { answer(true); });
    no.addEventListener("click", function () { answer(false); });
    row.appendChild(yes);
    if (always) row.appendChild(always);
    row.appendChild(no);
    return row;
  }

  function paintStats(runs, approvals, schedules) {
    var done = runs.filter(function (run) { return run.outcome === "succeeded"; }).length;
    var failed = runs.filter(function (run) { return run.outcome === "failed"; }).length;
    var rows = [
      ["Active runs", runs.filter(function (run) { return !run.outcome; }).length, false],
      ["Needs you", Math.max(approvals.length, runs.filter(function (run) { return run.state === "awaiting_approval"; }).length), approvals.length > 0],
      ["Scheduled", schedules.filter(function (schedule) { return schedule.enabled; }).length, false],
      ["Projects", projects.filter(function (project) { return project.status === "active"; }).length, false],
      ["Completed", done, false],
      ["Failed", failed, failed > 0]
    ];
    var host = $("stats");
    host.replaceChildren();
    rows.forEach(function (row) {
      var line = el("div", "stat" + (row[2] ? " hot" : ""));
      line.appendChild(el("span", "", row[0]));
      line.appendChild(el("b", "", String(row[1])));
      host.appendChild(line);
    });
  }
  // ---- the cards on the Ops page ------------------------------------------------------------------------------------
  function plainObjective(text) { return String(text || "").replace(/^(\[(scheduled|sub-agent|resumed|mail trigger|file trigger|webhook trigger)\]\s*)+/, ""); }
  function span(from, to) {
    var seconds = Math.max(0, Math.round((Date.parse(to) - Date.parse(from)) / 1000));
    return duration(seconds);
  }
  function until(iso) {
    var seconds = Math.round((Date.parse(iso) - Date.now()) / 1000);
    if (seconds <= 0) return "due now";
    if (seconds < 90) return "in " + seconds + "s";
    if (seconds < 5400) return "in " + Math.round(seconds / 60) + "m";
    if (seconds < 172800) return "in " + Math.round(seconds / 3600) + "h";
    return "in " + Math.round(seconds / 86400) + "d";
  }
  function every(seconds) {
    if (seconds % 86400 === 0) return "every " + seconds / 86400 + "d";
    if (seconds % 3600 === 0) return "every " + seconds / 3600 + "h";
    if (seconds % 60 === 0) return "every " + seconds / 60 + "m";
    return "every " + seconds + "s";
  }
  function cadenceLabel(schedule) {
    if (schedule.interval_seconds) return every(schedule.interval_seconds);
    if (schedule.cadence === "daily") return schedule.at_time + " " + schedule.days + " (" + schedule.time_zone + ")";
    return schedule.cadence;
  }
  function scheduleCard(schedule) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag" + (schedule.enabled ? " work" : ""), cadenceLabel(schedule)));
    if (!schedule.enabled) row.appendChild(el("span", "tag", schedule.fire_count > 0 && schedule.cadence === "once" ? "done" : "paused"));
    if (schedule.project) row.appendChild(el("span", "tag ok", schedule.project));
    if (schedule.next_run_at) {
      var next = el("span", "dim", until(schedule.next_run_at));
      next.dataset.until = schedule.next_run_at;
      row.appendChild(next);
    }
    item.appendChild(row);
    item.appendChild(el("div", "obj", clip(schedule.objective, 220)));
    var meta = el("div", "meta");
    meta.appendChild(el("span", "dim", "ran " + schedule.fire_count + (schedule.fire_count === 1 ? " time" : " times")
      + (schedule.skipped_count ? ", skipped " + schedule.skipped_count : "")));
    if (schedule.last_fired_at) meta.appendChild(ageSpan(schedule.last_fired_at, " ago"));
    item.appendChild(meta);
    var acts = el("div", "sched-act");
    var filed = projects.filter(function (project) { return project.status !== "done" || project.name === schedule.project; });
    if (filed.length) {
      var pick = el("select", "sched-project");
      pick.title = "The project this task belongs to: its runs are told the project's goal and guidance";
      var none = el("option", "", "No project");
      none.value = "";
      pick.appendChild(none);
      filed.forEach(function (project) {
        var option = el("option", "", project.name);
        option.value = project.name;
        pick.appendChild(option);
      });
      pick.value = schedule.project || "";
      pick.addEventListener("change", function () {
        pick.disabled = true;
        putJson("/schedules/" + encodeURIComponent(schedule.schedule_id) + "/project", "POST", pick.value ? { project_id: pick.value } : {}).then(function () { lastSignature = ""; refresh(); },
          function () { pick.disabled = false; pick.value = schedule.project || ""; notice("Could not move that task to the project."); });
      });
      acts.appendChild(pick);
    }
    if (schedule.enabled || schedule.cadence !== "once") {
      var toggle = el("button", "", schedule.enabled ? "Pause" : "Resume");
      toggle.addEventListener("click", function () {
        toggle.disabled = true;
        api("/schedules/" + encodeURIComponent(schedule.schedule_id) + "/" + (schedule.enabled ? "pause" : "resume"), "POST").then(refresh, refresh);
      });
      acts.appendChild(toggle);
    }
    var drop = el("button", "danger", "Remove");
    drop.addEventListener("click", function () {
      if (drop.textContent === "Remove") { drop.textContent = "Really remove?"; setTimeout(function () { drop.textContent = "Remove"; }, 4000); return; }
      drop.disabled = true;
      api("/schedules/" + encodeURIComponent(schedule.schedule_id), "DELETE").then(refresh, refresh);
    });
    acts.appendChild(drop);
    item.appendChild(acts);
    return item;
  }
  function triggerLabel(trigger) {
    if (trigger.kind === "file") return "new file in " + trigger.query;
    if (trigger.kind === "webhook") return "webhook";
    return "new mail: " + trigger.query;
  }
  // A trigger starts an ordinary run when something happens (ADR-0193, ADR-0195). Made with `jarvis trigger add`; the console shows and stops them.
  function triggerCard(trigger) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag" + (trigger.enabled ? " work" : ""), clip(triggerLabel(trigger), 60)));
    if (!trigger.enabled) row.appendChild(el("span", "tag", "paused"));
    if (trigger.project) row.appendChild(el("span", "tag ok", trigger.project));
    if (trigger.kind !== "webhook") row.appendChild(el("span", "dim", every(trigger.interval_seconds)));
    if (trigger.next_poll_at) {
      var next = el("span", "dim", "next look " + until(trigger.next_poll_at));
      next.dataset.until = trigger.next_poll_at;
      row.appendChild(next);
    }
    item.appendChild(row);
    item.appendChild(el("div", "obj", clip(trigger.objective, 220)));
    var meta = el("div", "meta");
    meta.appendChild(el("span", "dim", "started " + trigger.fire_count + (trigger.fire_count === 1 ? " task" : " tasks")));
    if (trigger.last_polled_at) meta.appendChild(ageSpan(trigger.last_polled_at, trigger.kind === "webhook" ? " since the last request" : " since the last look"));
    item.appendChild(meta);
    if (trigger.last_error) item.appendChild(el("div", "dim", "last look: " + clip(trigger.last_error, 200)));
    var acts = el("div", "sched-act");
    var toggle = el("button", "", trigger.enabled ? "Pause" : "Resume");
    toggle.addEventListener("click", function () {
      toggle.disabled = true;
      api("/triggers/" + encodeURIComponent(trigger.trigger_id) + "/" + (trigger.enabled ? "pause" : "resume"), "POST").then(refresh, refresh);
    });
    acts.appendChild(toggle);
    var drop = el("button", "danger", "Remove");
    drop.addEventListener("click", function () {
      if (drop.textContent === "Remove") { drop.textContent = "Really remove?"; setTimeout(function () { drop.textContent = "Remove"; }, 4000); return; }
      drop.disabled = true;
      api("/triggers/" + encodeURIComponent(trigger.trigger_id), "DELETE").then(refresh, refresh);
    });
    acts.appendChild(drop);
    item.appendChild(acts);
    return item;
  }
  var openRuns = {}, showFinished = false;
  function recentCard(run) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag " + (run.outcome === "succeeded" ? "ok" : run.outcome === "cancelled" ? "" : "bad"), run.outcome));
    if (tagOf(run.objective)) row.appendChild(el("span", "tag", tagOf(run.objective)));
    row.appendChild(ageSpan(run.started_at, " ago"));
    if (run.completed_at) row.appendChild(el("span", "dim", "took " + span(run.started_at, run.completed_at)));
    item.appendChild(row);
    item.appendChild(el("div", "obj", clip(plainObjective(run.objective), 140)));
    if (!run.answer) return item;
    var preview = el("div", "dim", "=> " + clip(plainLine(run.answer), 300));
    item.appendChild(preview);
    item.dataset.open = "1";
    item.title = "Click to read the whole answer";
    var full = null;
    function toggle(open) {
      if (open && !full) {
        full = el("div", "full");
        full.appendChild(markdown(run.answer));
        item.appendChild(full);
      } else if (!open && full) {
        full.remove();
        full = null;
      }
      preview.hidden = open;
      item.classList.toggle("open", open);
      openRuns[run.run_id] = open;
    }
    item.addEventListener("click", function (event) {
      if (event.target.closest("a, button")) return;
      toggle(!full);
    });
    if (openRuns[run.run_id]) toggle(true);
    return item;
  }
  function renderPanels(runs, approvals, schedules, triggers) {
    var working = runs.filter(function (run) { return !run.outcome && run.state !== "awaiting_approval"; });
    var parked = runs.filter(function (run) { return run.state === "awaiting_approval"; });
    var finished = runs.filter(function (run) { return run.outcome; }).slice(0, 8);
    S.needYou = Math.max(approvals.length, parked.length);
    S.working = working.length;
    S.pending = approvals;
    paintStats(runs, approvals, schedules);

    function approvalCard(approval) {
      var item = el("div", "item");
      item.dataset.approval = approval.approval_id;
      var row = el("div", "row");
      row.appendChild(el("span", "tag wait", "risk " + approval.risk_level));
      row.appendChild(el("strong", "", approval.tool));
      item.appendChild(row);
      if (NEVER_TRUSTED[approval.tool] && approval.arguments) {
        // What is approved here becomes a standing instruction or changes memory, so every field is shown whole, one under its name:
        // a long harmless-looking field must not push the one that matters out of sight.
        var whole = el("div", "fields");
        Object.keys(approval.arguments).forEach(function (key) {
          var value = approval.arguments[key];
          var field = el("div", "field");
          field.appendChild(el("span", "dim", key));
          field.appendChild(el("div", "fieldtext", typeof value === "string" ? value : JSON.stringify(value)));
          whole.appendChild(field);
        });
        item.appendChild(whole);
      } else {
        item.appendChild(el("div", "obj", clip(approval.arguments ? JSON.stringify(approval.arguments) : approval.preview, 400)));
      }
      item.appendChild(answerButtons(approval.approval_id, approval.tool));
      return item;
    }
    fill("waiting", approvals.map(approvalCard), "nothing needs you");
    // The same questions sit on the face page too, so nothing waits on you out of sight.
    $("tray-waiting").replaceChildren.apply($("tray-waiting"), approvals.map(approvalCard));
    var needs = Math.max(approvals.length, parked.length);
    var badge = $("ops-count");
    badge.hidden = needs === 0;
    badge.textContent = String(needs);

    fill("working", working.concat(parked).map(function (run) {
      var item = el("div", "item");
      var row = el("div", "row");
      row.appendChild(el("span", "tag " + (run.state === "awaiting_approval" ? "wait" : "work"), run.state === "awaiting_approval" ? "needs you" : "working"));
      if (tagOf(run.objective)) row.appendChild(el("span", "tag", tagOf(run.objective)));
      row.appendChild(ageSpan(run.started_at));
      row.appendChild(el("span", "spacer"));
      row.appendChild(stopButton(run.run_id));
      item.appendChild(row);
      item.appendChild(el("div", "obj", clip(plainObjective(run.objective), 220)));
      // What it is doing this second, kept fresh by the page (see tickAges).
      var now = el("div", "nowline", "");
      now.dataset.now = run.run_id;
      item.appendChild(now);
      return item;
    }), "nothing running");

    // Tasks that will run come first; one-offs that already ran are put away behind a toggle so they do not push the rest down.
    var recurring = schedules.filter(function (schedule) { return schedule.enabled || schedule.cadence !== "once"; })
      .sort(function (a, b) { return (b.enabled ? 1 : 0) - (a.enabled ? 1 : 0); });
    var finishedOnce = schedules.filter(function (schedule) { return !schedule.enabled && schedule.cadence === "once"; });
    fill("scheduled", recurring.concat(showFinished ? finishedOnce : []).map(scheduleCard), "nothing scheduled: ask JARVIS to \u201Cdo this every morning\u201D");
    if (finishedOnce.length) {
      var more = el("button", "", showFinished ? "Hide finished one-off tasks" : "Show " + finishedOnce.length + " finished one-off task" + (finishedOnce.length === 1 ? "" : "s"));
      more.addEventListener("click", function () { showFinished = !showFinished; lastSignature = ""; refresh(); });
      $("scheduled").appendChild(more);
    }

    fill("triggers", (triggers || []).map(triggerCard), "no triggers: make one in a terminal with `jarvis trigger add --mail \"is:unread\" note each subject`");

    fill("recent", finished.map(recentCard), "nothing yet");
  }

  // ---- what happened lately (ADR-0155) ----------------------------------------------------------------------------------
  var digestAt = 0;
  function loadDigest() {
    digestAt = Date.now();
    api("/digest").then(paintDigest, function () { digestAt = 0; });
  }
  function tokenText(count) {
    var n = Number(count) || 0;
    if (n >= 1000000) return (n / 1000000).toFixed(1) + "M";
    if (n >= 1000) return (n / 1000).toFixed(1) + "k";
    return String(n);
  }
  function paintDigest(digest) {
    var host = $("digest");
    host.replaceChildren();
    function line(label, value, hot) {
      var row = el("div", "stat" + (hot ? " hot" : ""));
      row.appendChild(el("span", "", label));
      row.appendChild(el("b", "", String(value)));
      host.appendChild(row);
    }
    line("Runs", digest.runs, false);
    line("Finished", digest.succeeded, false);
    line("Failed", digest.failed, digest.failed > 0);
    line("Waiting for you", digest.waiting, digest.waiting > 0);
    // Not every provider reports usage, so zero means unknown rather than free.
    line("Tokens", digest.input_tokens + digest.output_tokens === 0 ? "not reported" : tokenText(digest.input_tokens) + " in / " + tokenText(digest.output_tokens) + " out", false);
    (digest.projects || []).forEach(function (project) {
      line(project.name, project.runs + " run" + (project.runs === 1 ? "" : "s"), false);
      (project.highlights || []).forEach(function (note) { host.appendChild(el("div", "dim", note.kind + ": " + clip(note.text, 140))); });
    });
    (digest.problems || []).forEach(function (problem) {
      host.appendChild(el("div", "dim", "failed: " + clip(plainObjective(problem.objective), 70) + " (" + problem.error_code + ")"));
    });
  }

  // ---- contacts (ADR-0155) ---------------------------------------------------------------------------------------------
  // The people and companies JARVIS is working with. You can change where one stands here, which is also how a do-not-contact is lifted:
  // JARVIS itself cannot do that.
  var contactsAt = 0;
  var CONTACT_STATUSES = ["new", "contacted", "replied", "meeting", "won", "lost", "do_not_contact"];
  function loadContacts() {
    contactsAt = Date.now();
    api("/contacts?limit=40").then(paintContacts, function () { contactsAt = 0; });
  }
  function contactRow(contact) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("strong", "", contact.company));
    if (contact.person) row.appendChild(el("span", "dim", contact.person));
    row.appendChild(el("span", "spacer"));
    var status = el("select");
    CONTACT_STATUSES.forEach(function (name) {
      var option = el("option", "", name.replace("_", " "));
      option.value = name;
      status.appendChild(option);
    });
    status.value = contact.status;
    status.addEventListener("change", function () {
      var change = { company: contact.company, person: contact.person, status: status.value };
      if (contact.email) change.email = contact.email;
      putJson("/contacts", "POST", change).then(function () { loadContacts(); }, function () { status.value = contact.status; });
    });
    row.appendChild(status);
    item.appendChild(row);
    var detail = [contact.email, contact.role].filter(Boolean).join(" \u00B7 ");
    if (detail) item.appendChild(el("div", "dim", detail));
    if (contact.notes) item.appendChild(el("div", "obj", clip(contact.notes, 140)));
    return item;
  }
  function paintContacts(reply) {
    var total = (reply.counts || []).reduce(function (sum, entry) { return sum + entry.count; }, 0);
    var host = $("contacts");
    if (!total) {
      host.replaceChildren(el("div", "none", "no contacts yet: JARVIS saves the leads it finds, or add one with `jarvis contacts add`"));
      return;
    }
    var counts = (reply.counts || []).filter(function (entry) { return entry.count > 0; })
      .map(function (entry) { return entry.status.replace("_", " ") + " " + entry.count; }).join(" \u00B7 ");
    var nodes = [el("div", "dim", total + " in all: " + counts)];
    (reply.contacts || []).forEach(function (contact) { nodes.push(contactRow(contact)); });
    if (total > (reply.contacts || []).length) nodes.push(el("div", "dim", "showing the first " + reply.contacts.length + "; `jarvis contacts export` has them all"));
    host.replaceChildren.apply(host, nodes);
  }

  // ---- projects (ADR-0151) ----------------------------------------------------------------------------------------------
  // A project is long-running work with a goal, the owner's standing guidance, a working folder and a journal. A conversation that
  // belongs to one is told all of that on every run, so it is written once here instead of pasted into each request. A project
  // changes what JARVIS is told, never what it may do: approvals and permissions are unchanged.
  function projectNamed(name) {
    return projects.filter(function (project) { return project.name === name || project.project_id === name; })[0] || null;
  }
  function loadProjects() {
    projectsAt = Date.now();
    return api("/projects").then(function (reply) {
      projects = reply.projects || [];
      lastSignature = "";
      paintProjects();
      paintProjectPick();
    }, function () {});
  }
  function paintProjectPick() {
    var pick = $("project-pick");
    var wanted = currentChat.project || "";
    pick.replaceChildren();
    var none = el("option", "", "No project");
    none.value = "";
    pick.appendChild(none);
    var names = projects.filter(function (project) { return project.status !== "done" || project.name === wanted; })
      .map(function (project) { return project.name; });
    if (wanted && names.indexOf(wanted) < 0) names.push(wanted);
    names.forEach(function (name) {
      var option = el("option", "", name);
      option.value = name;
      pick.appendChild(option);
    });
    pick.value = wanted;
    pick.hidden = projects.length === 0 && !wanted;
  }
  function startProjectChat(name) {
    if (busy) { say("one moment: it is still answering. Press Stop to cut it short."); return; }
    setOps(false);
    $("project-sheet").hidden = true;
    if (transcript.length) newChat();
    currentChat.project = name;
    saveChats();
    paintProjectPick();
    $("input").focus();
    say("this conversation belongs to " + name);
  }
  $("project-pick").addEventListener("change", function () {
    var name = $("project-pick").value;
    if (busy) { paintProjectPick(); say("one moment: it is still answering."); return; }
    // A conversation already bound to a project cannot move to another, so choosing one starts a fresh conversation.
    if (transcript.length) newChat();
    currentChat.project = name || null;
    saveChats();
    paintProjectPick();
    say(name ? "new messages here belong to " + name : "no project");
  });
  function projectCard(project) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag " + (project.status === "active" ? "work" : project.status === "done" ? "ok" : ""), project.status));
    row.appendChild(el("strong", "", project.name));
    var open = el("button", "", "Open");
    open.addEventListener("click", function () { openProject(project.project_id); });
    var talk = el("button", "", "Chat");
    talk.title = "Start a conversation in this project";
    talk.addEventListener("click", function () { startProjectChat(project.name); });
    row.appendChild(open);
    row.appendChild(talk);
    item.appendChild(row);
    if (project.goal) item.appendChild(el("div", "obj", clip(project.goal, 160)));
    return item;
  }
  function paintProjects() {
    fill("projects", projects.map(projectCard), "no projects yet: press New project to give JARVIS a goal, guidance and a journal");
  }
  function projectNote(text, bad) {
    var line = $("project-note");
    line.textContent = text || "";
    line.className = bad ? "bad" : "ok";
  }
  function closeProject() { $("project-sheet").hidden = true; }
  function formRow(label, field, help) {
    var row = el("div", "set-row");
    row.appendChild(el("span", "name", label));
    row.appendChild(field);
    if (help) row.appendChild(el("span", "help", help));
    return row;
  }
  function textField(value, placeholder) {
    var field = el("input");
    field.type = "text";
    field.value = value || "";
    field.placeholder = placeholder || "";
    return field;
  }
  function areaField(value, rows, placeholder) {
    var field = el("textarea");
    field.rows = rows;
    field.value = value || "";
    field.placeholder = placeholder || "";
    return field;
  }
  function openProject(id) {
    $("project-sheet").hidden = false;
    projectNote("");
    if (!id) { paintProjectSheet(null, []); return; }
    api("/projects/" + encodeURIComponent(id)).then(function (detail) { paintProjectSheet(detail.project, detail.notes || []); },
      function () { projectNote("Could not open that project.", true); });
  }
  function paintProjectSheet(project, notes) {
    var body = $("project-body");
    body.replaceChildren();
    $("project-title").textContent = project ? project.name : "New project";
    var name = textField(project && project.name, "e.g. Prospecting");
    var goal = areaField(project && project.goal, 3, "What done looks like, in a sentence or two.");
    var folder = textField(project && project.folder, "e.g. sales (inside a folder JARVIS may use)");
    var guidance = areaField(project && project.guidance, 9, "How you want the work done: tone, language, limits, who it may contact, what to avoid. Every run of this project is told this.");
    body.appendChild(formRow("Name", name));
    body.appendChild(formRow("Goal", goal));
    body.appendChild(formRow("Folder", folder, "Where its files live, relative to a folder you granted in Settings, Files. Empty means that folder itself."));
    var cap = textField(project && project.daily_run_limit ? String(project.daily_run_limit) : "", "0 = no cap");
    var used = project && project.daily_run_limit ? " Used in the last 24 hours: " + project.runs_today + "." : "";
    body.appendChild(formRow("Daily runs", cap, "The most runs its scheduled tasks may start in 24 hours; past it they wait. Your own messages are never refused." + used));
    var tokenCap = textField(project && project.daily_token_limit ? String(project.daily_token_limit) : "", "0 = no cap");
    var tokensUsed = project ? " Used in the last 24 hours: " + Number(project.tokens_today || 0).toLocaleString() + " tokens." : "";
    body.appendChild(formRow("Daily tokens", tokenCap, "The most tokens (input plus output) its runs may use in 24 hours; past it, scheduled tasks wait. It counts what runs already used, so one long run can overshoot it; it never stops a run midway or refuses your own messages. Leave empty for no cap." + tokensUsed));
    body.appendChild(formRow("Guidance", guidance, "Your standing instructions. The project cannot widen what JARVIS may do: approvals and permissions stay as they are."));
    var status = null;
    if (project) {
      status = el("select");
      ["active", "paused", "done"].forEach(function (value) {
        var option = el("option", "", value);
        option.value = value;
        status.appendChild(option);
      });
      status.value = project.status;
      body.appendChild(formRow("Status", status, "Paused or done: its scheduled tasks stop firing. You can still chat in it."));
    }
    var acts = el("div", "acts");
    var save = el("button", "yes", project ? "Save" : "Create");
    save.addEventListener("click", function () {
      var limit = cap.value.trim() === "" ? 0 : Number(cap.value);
      if (!Number.isInteger(limit) || limit < 0) { projectNote("Daily runs must be a whole number, or empty for no cap.", true); return; }
      var tokens = tokenCap.value.trim() === "" ? 0 : Number(tokenCap.value.replace(/[\s,_]/g, ""));
      if (!Number.isInteger(tokens) || tokens < 0 || tokens > 2000000000) { projectNote("Daily tokens must be a whole number up to two billion, or empty for no cap.", true); return; }
      var fields = { name: name.value, goal: goal.value, folder: folder.value, guidance: guidance.value, daily_run_limit: limit, daily_token_limit: tokens };
      if (status) fields.status = status.value;
      save.disabled = true;
      putJson(project ? "/projects/" + encodeURIComponent(project.project_id) : "/projects", project ? "PATCH" : "POST", fields).then(function (saved) {
        projectNote(project ? "Saved." : "Created.");
        loadProjects();
        openProject(saved.project_id);
        projectNote(project ? "Saved." : "Created.");
      }, function (error) { projectNote(explain(error, "That was refused."), true); save.disabled = false; });
    });
    acts.appendChild(save);
    if (project) {
      var talk = el("button", "", "Chat in it");
      talk.addEventListener("click", function () { startProjectChat(project.name); });
      acts.appendChild(talk);
      var drop = el("button", "danger", "Delete");
      drop.addEventListener("click", function () {
        if (drop.textContent === "Delete") { drop.textContent = "Really delete?"; return; }
        api("/projects/" + encodeURIComponent(project.project_id), "DELETE").then(function () { closeProject(); loadProjects(); },
          function () { projectNote("Could not delete it.", true); });
      });
      acts.appendChild(drop);
    }
    body.appendChild(el("div", "set-row")).appendChild(acts);
    if (!project) return;
    var card = el("div", "card");
    card.appendChild(el("h4", "", "Journal"));
    card.appendChild(el("p", "dim", "What happened, written by JARVIS as it works and by you. The newest entries are shown to every run."));
    if (!notes.length) card.appendChild(el("div", "none", "no entries yet"));
    notes.forEach(function (entry) {
      var line = el("div", "jline");
      line.appendChild(el("span", "tag", entry.kind));
      line.appendChild(el("span", "dim", entry.created_at.slice(0, 10)));
      line.appendChild(el("span", "jtext", entry.text));
      card.appendChild(line);
    });
    var text = textField("", "Add an entry for JARVIS to read next time");
    var kind = el("select");
    ["owner", "decision", "blocker", "next"].forEach(function (value) {
      var option = el("option", "", value);
      option.value = value;
      kind.appendChild(option);
    });
    var add = el("button", "", "Add");
    add.addEventListener("click", function () {
      if (!text.value.trim()) { projectNote("Write the entry first.", true); return; }
      putJson("/projects/" + encodeURIComponent(project.project_id) + "/notes", "POST", { text: text.value, kind: kind.value }).then(function () {
        openProject(project.project_id);
      }, function (error) { projectNote(explain(error, "That entry was refused."), true); });
    });
    var write = el("div", "jwrite");
    write.appendChild(kind);
    write.appendChild(text);
    write.appendChild(add);
    card.appendChild(write);
    body.appendChild(card);
  }
  $("project-new").addEventListener("click", function () { openProject(null); });
  $("project-close").addEventListener("click", closeProject);
  $("project-sheet").addEventListener("click", function (event) { if (event.target === $("project-sheet")) closeProject(); });
  loadProjects();

  // ---- things JARVIS offers to remember --------------------------------------------------------------------------------
  // The model may only *propose* a memory (a page it read could say "remember this"), so a proposal waits here until you keep
  // it. Keeping files it again as your own statement, which is what lets it shape later answers; dismissing forgets it.
  var proposalsAt = 0;
  function proposalCard(memory) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag wait", "remember?"));
    item.appendChild(row);
    item.appendChild(el("div", "obj", clip(memory.content, 300)));
    var buttons = el("div", "row cmd");
    var keep = el("button", "yes", "Keep");
    var drop = el("button", "danger", "Dismiss");
    function settle(keepIt) {
      keep.disabled = true;
      drop.disabled = true;
      // The proposal goes first, and on Keep without the block that stops a forgotten claim returning: the same words filed
      // again as the person's own statement would otherwise be recognised as the claim already held and nothing would be saved.
      api("/memories/" + encodeURIComponent(memory.memory_id) + "/forget", "POST", { expected_version: memory.version, allow_relearn: keepIt })
        .then(function () {
          return keepIt
            ? api("/memories", "POST", { content: memory.content, memory_type: memory.memory_type, source_kind: "user_statement" })
            : null;
        })
        .then(function () {
          notice(keepIt ? "Remembered: " + clip(memory.content, 140) : "Dismissed. I will not offer that again.");
          if (keepIt) feel("pleased", 2.2, "nod");
          proposalsAt = 0;
          refreshProposals();
        }, function (error) {
          keep.disabled = false;
          drop.disabled = false;
          proposalsAt = 0;
          notice("Could not " + (keepIt ? "keep" : "dismiss") + " that (" + (error.status || "no reply") + ").");
        });
    }
    keep.addEventListener("click", function () { settle(true); });
    drop.addEventListener("click", function () { settle(false); });
    buttons.appendChild(keep);
    buttons.appendChild(drop);
    item.appendChild(buttons);
    return item;
  }
  // A procedure JARVIS wrote down for itself waits here the same way: it is used by no run until you approve it.
  function skillCard(skill) {
    var item = el("div", "item");
    var row = el("div", "row");
    row.appendChild(el("span", "tag wait", "learn this?"));
    item.appendChild(row);
    var lines = String(skill.description || "").split("\n");
    item.appendChild(el("div", "obj", clip(lines[0], 200)));
    var steps = el("div", "meta");
    skill.steps.forEach(function (step) {
      steps.appendChild(el("div", "dim", step.position + ". " + step.tool + " \u2014 " + clip(step.instruction, 120)));
    });
    item.appendChild(steps);
    var buttons = el("div", "row cmd");
    var yes = el("button", "yes", "Approve");
    var no = el("button", "danger", "Dismiss");
    function settle(approve) {
      yes.disabled = true;
      no.disabled = true;
      api("/skills/" + encodeURIComponent(skill.revision_id) + (approve ? "/approve" : "/dismiss"), "POST", {})
        .then(function () {
          notice(approve ? "Learned: " + clip(lines[0], 120) : "Dismissed. I will not use that.");
          if (approve) feel("pleased", 2.2, "nod");
          proposalsAt = 0;
          refreshProposals();
        }, function (error) {
          yes.disabled = false;
          no.disabled = false;
          proposalsAt = 0;
          notice("Could not " + (approve ? "approve" : "dismiss") + " that (" + (error.status || "no reply") + ").");
        });
    }
    yes.addEventListener("click", function () { settle(true); });
    no.addEventListener("click", function () { settle(false); });
    buttons.appendChild(yes);
    buttons.appendChild(no);
    item.appendChild(buttons);
    return item;
  }
  // Owner inspection uses the authenticated memory routes, never model-rendered markup.
  var memoryLoaded = false, memoryBusy = false;
  var memoryPageRequest = 0, memoryListRequest = 0, memoryDetailRequest = 0;
  var memoryCursors = [null], memoryPage = 0, memoryNext = null, memoryFilters = {};
  function memoryPaging() {
    $("memory-previous").disabled = memoryBusy || memoryPage === 0;
    $("memory-next").disabled = memoryBusy || !memoryNext;
  }
  function loadMemory(reset, page) {
    memoryLoaded = true;
    if (reset) {
      memoryPage = 0; memoryCursors = [null]; memoryNext = null; memoryListRequest = 0;
      memoryFilters = {};
      if ($("memory-type").value) memoryFilters.memory_type = $("memory-type").value;
      if ($("memory-entity").value.trim()) memoryFilters.entity_id = $("memory-entity").value.trim();
      $("memory-list").replaceChildren();
    }
    page = reset ? 0 : page === undefined ? memoryPage : page;
    var cursor = page > memoryPage ? memoryNext : memoryCursors[page];
    var body = Object.assign({ limit: 10 }, memoryFilters);
    if (cursor) body.cursor = cursor;
    var request = ++memoryPageRequest;
    ++memoryDetailRequest;
    $("memory-detail").replaceChildren();
    memoryBusy = true;
    memoryPaging();
    $("memory-status").textContent = "Loading memories…";
    return putJson("/memories/browse", "POST", body).then(function (reply) {
      if (request !== memoryPageRequest) return;
      memoryPage = page;
      memoryListRequest = request;
      memoryCursors = memoryCursors.slice(0, page + 1);
      memoryCursors[page] = cursor || null;
      memoryNext = reply.next_cursor;
      var rows = (reply.memories || []).map(function (memory) {
        var item = el("div", "item");
        item.appendChild(el("div", "dim", memory.memory_type + " · " + memory.confidence + " · " + memory.sensitivity));
        item.appendChild(el("div", "memory-text", memory.preview));
        var open = el("button", "", "Read details");
        open.type = "button";
        open.addEventListener("click", function () {
          if (!memoryBusy && request === memoryListRequest) readMemory(memory.memory_id);
        });
        item.appendChild(open);
        return item;
      });
      $("memory-list").replaceChildren.apply($("memory-list"), rows);
      $("memory-status").textContent = rows.length
        ? "Page " + (page + 1) + ": " + rows.length + " current memories. Previews only; read details for the full claim."
        : "No current memories match these filters.";
      memoryBusy = false;
      memoryPaging();
    }, function (error) {
      if (request !== memoryPageRequest) return;
      memoryBusy = false;
      memoryPaging();
      $("memory-status").textContent = explain(error, "Could not load memories.") + " Use Browse / refresh to try again.";
    });
  }
  function readMemory(id) {
    var request = ++memoryDetailRequest;
    var host = $("memory-detail");
    host.replaceChildren(el("p", "dim", "Loading memory details…"));
    return api("/memories/" + encodeURIComponent(id)).then(function (memory) {
      if (request !== memoryDetailRequest) return;
      host.replaceChildren(el("h3", "", "Memory details"), el("div", "memory-text", memory.content));
      var metadata = el("details", "memory-metadata");
      metadata.appendChild(el("summary", "", "Source and record details"));
      ["memory_id", "memory_type", "source_kind", "source_locator", "confidence", "sensitivity", "status", "effective_status",
        "created_at", "updated_at", "last_accessed_at", "importance", "retrieval_count", "admitted_by_actor_id", "admitted_at",
        "version", "superseded_by", "is_stated_as_fact", "carries_untrusted_trust"].forEach(function (key) {
        if (memory[key] !== undefined && memory[key] !== null) metadata.appendChild(el("div", "dim", key + ": " + memory[key]));
      });
      metadata.appendChild(el("div", "dim", "Entity IDs: " + (memory.entity_ids || []).join(", ")));
      if (memory.claim) metadata.appendChild(el("div", "memory-text", "Structured claim: " + JSON.stringify(memory.claim)));
      host.appendChild(metadata);
      var label = el("label", "", "Correction");
      label.setAttribute("for", "memory-correction");
      var correction = el("textarea");
      correction.id = "memory-correction";
      correction.rows = 5;
      correction.value = memory.content;
      host.appendChild(label);
      host.appendChild(correction);
      var actions = el("div", "row cmd");
      var save = el("button", "yes", "Save correction");
      var forget = el("button", "danger", "Forget this memory");
      var reload = el("button", "", "Reload details");
      [save, forget, reload].forEach(function (button) { button.type = "button"; actions.appendChild(button); });
      host.appendChild(actions);
      var confirmation = el("div", "memory-confirm");
      confirmation.hidden = true;
      confirmation.appendChild(el("p", "", "Forget this memory and prevent it from being learned again?"));
      var yes = el("button", "danger", "Yes, forget"), cancel = el("button", "", "Cancel");
      yes.type = cancel.type = "button";
      confirmation.appendChild(yes); confirmation.appendChild(cancel);
      host.appendChild(confirmation);
      var status = el("p", "dim");
      status.setAttribute("role", "status");
      host.appendChild(status);
      var writing = false, confirmed = false;
      var writable = memory.memory_id === id && Number.isSafeInteger(memory.version) && memory.version > 0;
      function controls() {
        save.disabled = forget.disabled = writing || !writable;
        correction.disabled = reload.disabled = yes.disabled = cancel.disabled = writing;
      }
      function write(action, body, message) {
        if (writing || !writable || request !== memoryDetailRequest) return;
        writing = true;
        controls();
        status.textContent = action === "correct" ? "Saving correction…" : "Forgetting memory…";
        putJson("/memories/" + encodeURIComponent(id) + "/" + action, "POST", body).then(function () {
          proposalsAt = 0;
          notice(message + (request === memoryDetailRequest ? "" : " Browse / refresh to update the memory list."));
          // A late write may succeed, but it must not erase a newer selection or its draft.
          if (request === memoryDetailRequest) loadMemory(true);
        }, function (error) {
          if (request !== memoryDetailRequest) return;
          writing = false; confirmed = false; confirmation.hidden = true;
          controls();
          status.textContent = error.status === 409
            ? "Changed since you opened it. Reload details before trying again."
            : explain(error, "Could not save this change. Reload details before trying again.");
        });
      }
      save.addEventListener("click", function () {
        var content = correction.value.trim();
        if (!content) { status.textContent = "Enter a correction before saving."; return; }
        write("correct", { content: content, expected_version: memory.version }, "Memory corrected.");
      });
      forget.addEventListener("click", function () {
        if (writing || !writable || request !== memoryDetailRequest) return;
        confirmed = true; confirmation.hidden = false;
        cancel.focus();
      });
      cancel.addEventListener("click", function () {
        confirmed = false; confirmation.hidden = true; forget.focus();
      });
      yes.addEventListener("click", function () {
        if (confirmed) write("forget", { expected_version: memory.version, allow_relearn: false }, "Memory forgotten.");
      });
      reload.addEventListener("click", function () { if (!writing && request === memoryDetailRequest) readMemory(id); });
      controls();
      if (!writable) status.textContent = "The memory identity or version is unavailable. Reload details before changing it.";
      host.focus();
    }, function (error) {
      if (request !== memoryDetailRequest) return;
      host.replaceChildren(el("p", "dim", explain(error, "Could not read this memory. Browse / refresh and reopen its details.")));
    });
  }
  $("memory-filters").addEventListener("submit", function (event) { event.preventDefault(); loadMemory(true); });
  $("memory-next").addEventListener("click", function () { if (!memoryBusy && memoryNext) loadMemory(false, memoryPage + 1); });
  $("memory-previous").addEventListener("click", function () { if (!memoryBusy && memoryPage > 0) loadMemory(false, memoryPage - 1); });

  function offerCard(card) { return card.skill ? skillCard(card.skill) : proposalCard(card); }
  function refreshProposals() {
    if (Date.now() - proposalsAt < 8000) return;
    proposalsAt = Date.now();
    var memories = api("/memories?limit=50").then(function (list) {
      var proposed = (list.memories || []).filter(function (memory) { return memory.status === "proposed"; }).slice(0, 5);
      return Promise.all(proposed.map(function (memory) {
        return api("/memories/" + encodeURIComponent(memory.memory_id)).then(function (detail) {
          return { memory_id: memory.memory_id, memory_type: memory.memory_type, version: detail.version, content: detail.content };
        });
      }));
    });
    var skills = api("/skills?limit=50").then(function (list) {
      var proposed = (list.skills || []).filter(function (skill) { return skill.state === "proposed"; }).slice(0, 5);
      return Promise.all(proposed.map(function (skill) {
        return api("/skills/" + encodeURIComponent(skill.revision_id)).then(function (detail) {
          return { skill: { revision_id: skill.revision_id, description: detail.description, steps: detail.steps || [] } };
        });
      }));
    }).catch(function () { return []; });
    Promise.all([memories, skills]).then(function (both) {
      var cards = both[0].concat(both[1]);
      $("remember").replaceChildren.apply($("remember"), cards.map(offerCard));
      $("tray-remember").replaceChildren.apply($("tray-remember"), cards.map(offerCard));
      S.proposals = cards.length;
    }, function () { proposalsAt = 0; });
  }

  // ---- what it can do: the tools, and which of them it asks about ------------------------------------------------------
  var abilitiesAt = 0;
  // Tools in families, so sixteen of them read as eight things: the same order and colours on the Ops page and in Settings.
  var FAMILIES = [
    ["Web", /^jarvis\.web\./, "94,227,255"], ["Files", /^jarvis\.files\./, "107,226,160"], ["Memory", /^jarvis\.memory\./, "176,150,255"],
    ["Sub-agents", /^jarvis\.agent\./, "255,120,214"], ["Commands & code", /^jarvis\.(command|code)\./, "255,138,92"],
    ["Mail & calendar", /^jarvis\.(gmail|calendar)\./, "255,180,84"], ["Schedule", /^jarvis\.schedule\./, "120,240,214"],
    ["Projects", /^jarvis\.project\./, "150,200,255"], ["Contacts", /^jarvis\.contacts\./, "255,214,120"], ["Skills", /^jarvis\.skill\./, "200,255,150"],
    ["Phone", /^jarvis\.voice\./, "255,130,130"], ["Browser", /^mcp\.browser\./, "130,190,255"], ["Documentation", /^mcp\.docs\./, "190,235,150"],
    ["Other programs", /^mcp\./, "200,180,220"]
  ];
  function families(tools) {
    var groups = FAMILIES.map(function (family) { return { name: family[0], rgb: family[2], tools: [] }; });
    var other = { name: "Other", rgb: "170,200,220", tools: [] };
    tools.forEach(function (tool) {
      var at = -1;
      FAMILIES.forEach(function (family, index) { if (at < 0 && family[1].test(String(tool.id))) at = index; });
      (at < 0 ? other : groups[at]).tools.push(tool);
    });
    groups.push(other);
    groups.forEach(function (group) { group.tools.sort(function (a, b) { return String(a.title).localeCompare(String(b.title)); }); });
    return groups.filter(function (group) { return group.tools.length; });
  }
  function familyLabel(group) {
    var label = el("div", "glabel");
    var dot = el("i", "gdot");
    dot.style.background = "rgb(" + group.rgb + ")";
    label.appendChild(dot);
    label.appendChild(el("span", "", group.name));
    return label;
  }
  function renderAbilities(tools) {
    var host = $("abilities");
    var asks = 0, off = 0;
    tools.forEach(function (tool) {
      if (!tool.callable || tool.denied) off += 1; else if (tool.asks_first) asks += 1;
    });
    host.replaceChildren();
    host.appendChild(el("div", "sum", tools.length + " tools  \u2022  " + asks + " ask first  \u2022  " + off + " off"));
    families(tools).forEach(function (group) {
      host.appendChild(familyLabel(group));
      group.tools.forEach(function (tool) {
        var row = el("div", "ability");
        var blocked = !tool.callable || tool.denied;
        var asking = !blocked && tool.asks_first;
        row.appendChild(el("span", "name", tool.title || tool.id));
        row.appendChild(el("span", "tag " + (blocked ? "bad" : asking ? "wait" : "ok"), blocked ? "off" : asking ? "asks" : "runs"));
        row.title = tool.id;
        host.appendChild(row);
      });
    });
  }
  function refreshAbilities() {
    if (Date.now() - abilitiesAt < 20000) return;
    abilitiesAt = Date.now();
    api("/tools").then(function (result) { renderAbilities(result.tools || []); }, function () { abilitiesAt = 0; });
  }

  // Dialogs share a focus boundary, including a project opened above Operations.
  var overlays = [$("ops"), $("settings"), $("project-sheet")], activeOverlay = null;
  var returnFocus = new Map();
  function dialogControls(overlay) {
    return Array.from(overlay.querySelectorAll("button, input, select, textarea, a[href], [tabindex]"))
      .filter(function (node) { return !node.disabled && node.tabIndex >= 0 && node.getClientRects().length > 0; });
  }
  function syncDialogs() {
    var next = overlays.filter(function (overlay) { return !overlay.hidden; }).pop() || null;
    var focused = document.activeElement;
    $("app").inert = !!next;
    document.body.classList.toggle("modal-open", !!next);
    overlays.forEach(function (overlay) { overlay.inert = overlay !== next; });
    $("open-ops").setAttribute("aria-expanded", String(!$("ops").hidden));
    $("open-settings").setAttribute("aria-expanded", String(!$("settings").hidden));
    if (next === activeOverlay) return;
    var previous = activeOverlay;
    activeOverlay = next;
    if (previous && previous.hidden) {
      var restore = returnFocus.get(previous);
      returnFocus.delete(previous);
      if (restore && restore.isConnected && restore.getClientRects().length && (!next || next.contains(restore))) { restore.focus(); return; }
    }
    if (next) {
      if (!returnFocus.has(next)) returnFocus.set(next, focused);
      var controls = dialogControls(next);
      (controls[0] || next.querySelector('[role="dialog"]')).focus();
    }
  }
  new MutationObserver(syncDialogs).observe(document.body, { attributes: true, attributeFilter: ["hidden"], subtree: true });
  window.addEventListener("keydown", function (event) {
    if (!activeOverlay) return;
    if (event.key === "Escape") {
      if (activeOverlay.id === "settings" && activeOverlay.querySelector(".combo-list:not([hidden])")) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      var close = activeOverlay.id === "ops" ? "ops-close" : activeOverlay.id === "settings" ? "settings-close" : "project-close";
      $(close).click();
    } else if (event.key === "Tab") {
      var controls = dialogControls(activeOverlay);
      var at = controls.indexOf(document.activeElement);
      if (!controls.length) { event.preventDefault(); activeOverlay.querySelector('[role="dialog"]').focus(); }
      else if (event.shiftKey && at <= 0) { event.preventDefault(); controls[controls.length - 1].focus(); }
      else if (!event.shiftKey && (at < 0 || at === controls.length - 1)) { event.preventDefault(); controls[0].focus(); }
    }
  }, true);
  syncDialogs();
  var settingsControls = JarvisSettings({ $: $, el: el, api: api, putJson: putJson, explain: explain, families: families, familyLabel: familyLabel, openActivity: function () { setOps(true); selectOpsSection("activity"); } });
  // ---- header and caption ---------------------------------------------------------------------------------------
  function uiState() {
    if (S.offline) return "offline";
    if (S.needYou > 0) return "waiting";
    if (S.listening) return "listening";
    if (S.speaking) return "speaking";
    // Waiting for the voice is still working: the face only "speaks" while there is sound.
    if (S.thinking || S.working > 0 || S.voiceBusy) return "working";
    return "idle";
  }
  var captions = {
    offline: "offline", idle: "", working: "working", listening: "listening",
    speaking: "speaking", waiting: "needs you"
  };
  function paintHeader() {
    var mode = uiState();
    var caption = $("caption");
    var preparing = S.voiceBusy && !S.speaking && !S.thinking && S.working === 0;
    var doing = window.JarvisMission ? JarvisMission.headline() : "";
    caption.textContent = preparing ? "preparing voice" : mode === "working" && (doing || S.activity) ? "working \u00B7 " + (doing || S.activity) : captions[mode];
    caption.className = mode === "waiting" ? "wait" : mode === "idle" || mode === "offline" ? "" : "live";
    var link = $("pill-link");
    link.className = "pill " + (S.offline ? "bad" : "on");
    link.lastChild.textContent = S.offline ? "offline" : "linked";
    var active = S.working + (S.thinking && !S.working ? 1 : 0);
    var work = $("pill-work");
    work.className = "pill" + (active > 0 ? " work" : "");
    work.lastChild.textContent = S.offline ? "Work status unavailable" : active + " working";
    work.hidden = active === 0;
    var wait = $("pill-wait");
    wait.className = "pill" + (S.needYou > 0 ? " wait" : "");
    wait.lastChild.textContent = S.offline ? "Decisions status unavailable" : S.needYou + " waiting";
    wait.hidden = S.needYou === 0;
    $("stop").hidden = active + S.needYou === 0;
    $("stage").dataset.state = mode;
    $("stage-talk").disabled = !Recognition;
    var status = $("composer-status");
    var statusText = S.offline ? (token ? "Offline · your draft stays here until JARVIS reconnects." : "Open with jarvis hud to connect. You can draft a message here.")
      : busy ? "Answering · you can draft your next message." : "";
    if (status.textContent !== statusText) status.textContent = statusText;
    status.className = "composer-status" + (S.offline ? " bad" : "");
    $("send").disabled = S.offline || busy;
    $("quiet").hidden = !(S.speaking || S.voiceBusy);
    document.title = (S.needYou > 0 ? "(" + S.needYou + ") " : "") + "JARVIS";
  }
  $("quiet").addEventListener("click", interruptSpeech);
  $("stage-talk").addEventListener("click", function () { $("mic").click(); });
  ["pill-work", "pill-wait"].forEach(function (id) { $(id).addEventListener("click", function () { setOps(true); selectOpsSection("activity"); }); });
  function paintClock() { $("clock").textContent = new Date().toLocaleTimeString([], { hour12: false }); }
  paintClock();
  setInterval(paintClock, 1000);

  // ---- asking out loud -------------------------------------------------------------------------------------------
  var WORDS = {
    "jarvis.code.run": "run some code", "jarvis.web.fetch": "fetch a web page",
    "jarvis.agent.delegate": "hand a task to a sub-agent", "jarvis.files.read": "read a file"
  };
  // When something new needs an answer and spoken answers are on, say so and listen for the yes or no, so the whole
  // exchange needs no hands. Approvals already waiting when the page opens are not announced again.
  function announce(approvals) {
    var seen = S.announced;
    S.announced = {};
    approvals.forEach(function (approval) { S.announced[approval.approval_id] = true; });
    if (seen === null || !S.speak) return;
    var fresh = approvals.filter(function (approval) { return !seen[approval.approval_id]; });
    if (fresh.length === 0) return;
    var what = fresh.length === 1 ? (WORDS[fresh[0].tool] || "use " + fresh[0].tool) : fresh.length + " things";
    speak("I need your permission to " + what + ". Say yes to allow it, or no.", function () {
      if (!S.wake && Recognition && S.pending.length) startListening(false);
    });
  }

  // ---- scheduled work reports back ---------------------------------------------------------------------------------
  // A task scheduled to run while you are away is only useful if you hear what it found. When a schedule fires (its last run
  // changes) and that run finishes, the answer is put in the conversation and, with spoken answers on, said out loud. What was
  // already fired when the page opened is not announced again.
  var firedSeen = null;
  var awaitingResults = {};
  function watchSchedules(schedules) {
    var now = {};
    schedules.forEach(function (schedule) { now[schedule.schedule_id] = schedule.last_run_id || ""; });
    if (firedSeen !== null) {
      schedules.forEach(function (schedule) {
        if (schedule.last_run_id && firedSeen[schedule.schedule_id] !== schedule.last_run_id) awaitingResults[schedule.last_run_id] = schedule.objective;
      });
    }
    firedSeen = now;
    if (Object.keys(awaitingResults).length === 0) return;
    api("/runs?limit=30").then(function (reply) {
      (reply.runs || []).forEach(function (run) {
        if (!awaitingResults[run.run_id] || !run.outcome) return;
        var objective = awaitingResults[run.run_id];
        delete awaitingResults[run.run_id];
        var text = run.outcome === "succeeded" && run.answer ? run.answer : "A scheduled task did not finish: " + objective;
        var inbox = scheduledInbox();
        inbox.transcript = (inbox.transcript || []).concat([{ role: "system", text: "Scheduled: " + objective }, { role: "jarvis", text: text }]).slice(-60);
        inbox.updated = Date.now();
        if (inbox === currentChat) {
          transcript = inbox.transcript;
          show("system", "Scheduled: " + objective);
          show("jarvis", text);
          scroll();
        } else {
          inbox.unread = (inbox.unread || 0) + 1;
          say("a scheduled task finished: its result is in History");
        }
        paintUnread();
        saveChats();
      });
    }, function () {});
  }

  // ---- watching the work you did not start from here ---------------------------------------------------------------------
  // A scheduled task or a sub-agent has no conversation on this page, but its run still has a stream. Following it feeds the live view
  // (mission.js) so you see what it is doing, with the same links. At most three at once, none for a run waiting on an answer (its
  // stream would sit open), and each stops when the run leaves the working list.
  var observers = {};
  function tagOf(objective) {
    var text = String(objective || "");
    if (text.indexOf("[scheduled]") === 0) return "scheduled";
    if (text.indexOf("[sub-agent]") === 0) return "sub-agent";
    var trigger = /^\[(mail|file|webhook) trigger\]/.exec(text);
    if (trigger) return trigger[1] + " trigger";
    return "";
  }
  async function observe(run) {
    var controller = new AbortController();
    observers[run.run_id] = controller;
    var tag = tagOf(run.objective);
    try {
      var response = await fetch("/api/v1/runs/" + encodeURIComponent(run.run_id) + "/stream", { headers: { Authorization: "Bearer " + token }, cache: "no-store", signal: controller.signal });
      if (!response.ok) return;
      var reader = response.body.getReader();
      var decoder = new TextDecoder();
      var buffer = "";
      for (;;) {
        var chunk = await reader.read();
        if (chunk.done) break;
        buffer += decoder.decode(chunk.value, { stream: true }).replace(/\r\n/g, "\n");
        var cut;
        while ((cut = buffer.indexOf("\n\n")) >= 0) {
          var frame = parseBlock(buffer.slice(0, cut));
          buffer = buffer.slice(cut + 2);
          if (!frame) continue;
          JarvisMission.event(run.run_id, tag, frame);
          if (TERMINAL[frame.event]) return;
        }
      }
    } catch (ignored) { /* the next poll starts it again if the run is still working */ }
    finally { if (observers[run.run_id] === controller) delete observers[run.run_id]; }
  }
  function observeRuns(runs) {
    if (!window.JarvisMission) return;
    var wanted = {};
    // Working runs first, then parked ones (they show as a held step), three in all: a browser allows six connections per host.
    var live = runs.filter(function (run) { return !run.outcome && run.run_id !== S.runId; });
    live.filter(function (run) { return run.state !== "awaiting_approval"; })
      .concat(live.filter(function (run) { return run.state === "awaiting_approval"; }))
      .slice(0, 3).forEach(function (run) { wanted[run.run_id] = run; });
    Object.keys(observers).forEach(function (id) { if (!wanted[id]) { observers[id].abort(); delete observers[id]; } });
    Object.keys(wanted).forEach(function (id) { if (!observers[id]) observe(wanted[id]); });
  }
  if (window.JarvisMission) {
    // The face takes an interest in what is being done.
    JarvisMission.onStart = function (item) {
      feel(item.kind === "web" || item.kind === "memory" ? "curious" : item.kind === "agent" ? "attentive" : "focused", 1.8);
    };
  }

  // ---- polling --------------------------------------------------------------------------------------------------
  var lastSignature = "", refreshPending = null, pollFailures = 0;
  var optionalPanels = { schedules: [], triggers: [] };
  function refresh() {
    if (refreshPending) return refreshPending;
    if (!token) { $("message").textContent = "No credential. Open this page with `jarvis hud`."; return Promise.resolve(); }
    var optionalProblems = [];
    function optional(name) {
      return api("/" + name).then(function (reply) { optionalPanels[name] = reply[name] || []; }, function (error) {
        if (error.status === 401) throw error;
        optionalProblems.push(name);
      });
    }
    refreshPending = Promise.allSettled([api("/runs?limit=30"), api("/approvals"), optional("schedules"), optional("triggers")]).then(function (settled) {
      var failed = settled.find(function (item) { return item.status === "rejected"; });
      if (failed) throw failed.reason;
      var results = settled.map(function (item) { return item.value; });
      S.offline = false; pollFailures = 0;
      $("message").textContent = optionalProblems.length ? "Could not refresh " + optionalProblems.join(" and ") + ". Their last available data is shown; conversation remains available." : "";
      var runs = results[0].runs, approvals = results[1].approvals;
      var signature = JSON.stringify([runs, approvals, optionalPanels]);
      if (signature !== lastSignature) {
        lastSignature = signature;
        renderPanels(runs, approvals, optionalPanels.schedules, optionalPanels.triggers);
      }
      announce(approvals); observeRuns(runs); watchSchedules(optionalPanels.schedules);
      refreshAbilities(); refreshProposals();
      if (Date.now() - digestAt > 60000) loadDigest();
      if (Date.now() - contactsAt > 30000) loadContacts();
      if (Date.now() - projectsAt > 15000) loadProjects();
      tickAges(); paintHeader();
    }).catch(function (error) {
      lastSignature = ""; S.offline = true; pollFailures += 1;
      $("message").textContent = error.status === 401 ? "The daemon rejected the credential. Open this page again with `jarvis hud`." : "Cannot reach the daemon.";
      paintHeader();
    }).finally(function () { refreshPending = null; });
    return refreshPending;
  }
  function poll() {
    refresh().finally(function () { setTimeout(poll, document.hidden ? 15000 : Math.min(30000, 1500 * Math.pow(2, pollFailures))); });
  }

  // ---- the face -------------------------------------------------------------------------------------------------
  // Just the face (see draw below), and what it is doing.
  var canvas = $("orb");
  var ctx = canvas.getContext("2d");
  var W = 0, H = 0;
  var palette = {
    idle: [104, 231, 224], working: [104, 231, 224], listening: [165, 214, 160],
    speaking: [181, 183, 236], waiting: [239, 192, 120], offline: [120, 136, 130]
  };
  var accentPalette = { mint: [104, 231, 224], ice: [125, 211, 252], amethyst: [196, 181, 253] };
  function colorForState(mode) {
    return mode === "idle" || mode === "working" ? accentPalette[document.documentElement.dataset.accent] || accentPalette.mint : palette[mode];
  }
  var current = colorForState("idle").slice();
  var energy = 0.12;
  var motionQuery = window.matchMedia ? matchMedia("(prefers-reduced-motion: reduce)") : null;
  var calm = false;
  // The face is not steered by the pointer. It has a mind of its own (head.js): it looks about, changes expression, blinks and
  // reacts to what is happening. The console only tells it what happened (JarvisHead.emote, nod, shake, mood).

  var TAU = Math.PI * 2;

  function resize() {
    var box = canvas.getBoundingClientRect();
    var ratio = Math.min(2, window.devicePixelRatio || 1);
    W = box.width; H = box.height;
    canvas.width = Math.max(1, Math.round(W * ratio));
    canvas.height = Math.max(1, Math.round(H * ratio));
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  }
  if (window.ResizeObserver) new ResizeObserver(resize).observe(canvas); else window.addEventListener("resize", resize);
  resize();

  function targetEnergy(mode, t) {
    switch (mode) {
      case "listening": return 0.28 + S.level * 1.1;
      case "speaking": return neural.enabled && S.voiceLevel > 0.01 ? 0.3 + S.voiceLevel * 0.9 : 0.4 + 0.35 * Math.abs(Math.sin(t * 7.3) * Math.sin(t * 3.1));
      case "working": return 0.45 + 0.12 * Math.sin(t * 2.2);
      case "waiting": return 0.42 + 0.16 * Math.sin(t * 5);
      case "offline": return 0.02;
      default: return 0.13 + 0.03 * Math.sin(t * 1.2);
    }
  }
  function rgba(alpha) {
    return "rgba(" + Math.round(current[0]) + "," + Math.round(current[1]) + "," + Math.round(current[2]) + "," + alpha + ")";
  }

  // The orbit is atmosphere, not a progress meter; only daemon state drives the labels and task counts.
  var FACE = 0.42;
  var motes = [];
  for (var moteIndex = 0; moteIndex < 42; moteIndex += 1) {
    motes.push({ a: moteIndex * 2.399, r: 0.55 + ((moteIndex * 0.618) % 1) * 1.1, sp: (moteIndex % 2 ? 1 : -1) * (0.01 + (moteIndex % 5) * 0.004), size: 0.8 + (moteIndex % 3) * 0.5 });
  }
  var captionAt = "";

  function drawPresenceOrbit(cx, cy, size, t, speed, quiet) {
    ctx.save();
    ctx.lineWidth = 1;
    ctx.strokeStyle = rgba(quiet ? 0.06 : 0.12);
    ctx.beginPath();
    ctx.arc(cx, cy, size * 0.7, 0, TAU);
    ctx.stroke();
    if (quiet) { ctx.restore(); return; }
    var turn = calm ? 0 : t * 0.035 * speed;
    // Perspective rings sit behind the face. They are ambient geometry, never invented telemetry.
    [-0.38, 0.38].forEach(function (tilt, index) {
      ctx.strokeStyle = rgba(index ? 0.055 : 0.1);
      ctx.beginPath();
      ctx.ellipse(cx, cy, size * 0.98, size * 0.67, tilt, 0, TAU);
      ctx.stroke();
      var start = (index ? -turn : turn) + index * Math.PI;
      ctx.strokeStyle = rgba(index ? 0.14 : 0.27);
      ctx.beginPath();
      ctx.ellipse(cx, cy, size * 0.98, size * 0.67, tilt, start, start + 0.5);
      ctx.stroke();
    });
    ctx.shadowColor = rgba(0.35);
    ctx.shadowBlur = 8;
    for (var segment = 0; segment < 12; segment += 1) {
      var angle = turn + segment * TAU / 12;
      ctx.strokeStyle = rgba(segment % 3 === 0 ? 0.42 : 0.16);
      ctx.lineWidth = segment % 3 === 0 ? 2 : 1;
      ctx.beginPath();
      ctx.arc(cx, cy, size * 0.75, angle, angle + TAU / 12 * 0.58);
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
    for (var tick = 0; tick < 60; tick += 1) {
      var a = tick * TAU / 60;
      var inner = size * (tick % 5 === 0 ? 0.79 : 0.81);
      ctx.strokeStyle = rgba(tick % 5 === 0 ? 0.32 : 0.12);
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(cx + Math.cos(a) * inner, cy + Math.sin(a) * inner);
      ctx.lineTo(cx + Math.cos(a) * size * 0.83, cy + Math.sin(a) * size * 0.83);
      ctx.stroke();
    }
    for (var node = 0; node < 4; node += 1) {
      var at = -turn * 0.5 + node * TAU / 4;
      ctx.fillStyle = rgba(0.6);
      ctx.beginPath();
      ctx.arc(cx + Math.cos(at) * size * 0.7, cy + Math.sin(at) * size * 0.7, 2, 0, TAU);
      ctx.fill();
    }
    // The reactor: three heavy arcs counter-rotating just outside the tick ring, lit like the face.
    ctx.lineCap = "round";
    ctx.shadowColor = rgba(0.6);
    ctx.shadowBlur = 14;
    for (var blade = 0; blade < 3; blade += 1) {
      var from = -turn * 1.6 + blade * TAU / 3;
      ctx.strokeStyle = rgba(0.42);
      ctx.lineWidth = Math.max(1.5, size * 0.008);
      ctx.beginPath();
      ctx.arc(cx, cy, size * 0.88, from, from + TAU / 3 * 0.42);
      ctx.stroke();
    }
    ctx.shadowBlur = 0;
    ctx.lineCap = "butt";
    // A radar sweep: a fading wedge of light that trails a bright edge round the ring.
    var sweep = turn * 3.2 - Math.PI / 2, tail = 0.9;
    var wedge = ctx.createConicGradient ? ctx.createConicGradient(sweep - tail, cx, cy) : null;
    if (wedge) {
      wedge.addColorStop(0, rgba(0));
      wedge.addColorStop(tail / TAU, rgba(0.16));
      wedge.addColorStop(tail / TAU + 0.001, rgba(0));
      wedge.addColorStop(1, rgba(0));
      ctx.fillStyle = wedge;
    } else ctx.fillStyle = rgba(0.05);
    ctx.beginPath();
    ctx.arc(cx, cy, size * 0.86, sweep - tail, sweep);
    ctx.arc(cx, cy, size * 0.7, sweep, sweep - tail, true);
    ctx.fill();
    ctx.strokeStyle = rgba(0.45);
    ctx.beginPath();
    ctx.moveTo(cx + Math.cos(sweep) * size * 0.7, cy + Math.sin(sweep) * size * 0.7);
    ctx.lineTo(cx + Math.cos(sweep) * size * 0.86, cy + Math.sin(sweep) * size * 0.86);
    ctx.stroke();
    // An outer dashed ring, drifting slowly, and a hairline inside the face's own circle.
    if (ctx.setLineDash) ctx.setLineDash([2, 9]);
    ctx.lineDashOffset = -turn * size * 0.6;
    ctx.strokeStyle = rgba(0.2);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(cx, cy, size * 1.12, 0, TAU);
    ctx.stroke();
    if (ctx.setLineDash) ctx.setLineDash([]);
    ctx.strokeStyle = rgba(0.07);
    ctx.beginPath();
    ctx.arc(cx, cy, size * 0.64, 0, TAU);
    ctx.stroke();
    // Reticle corners locked on the face; they breathe in and out a little unless motion is reduced.
    var breathe = calm ? 0 : Math.sin(t * 1.1) * 0.015;
    var boxX = size * (0.62 + breathe), boxY = size * (0.7 + breathe), arm = size * 0.07;
    ctx.strokeStyle = rgba(0.3);
    ctx.lineWidth = 1.2;
    [[-1, -1], [1, -1], [1, 1], [-1, 1]].forEach(function (corner) {
      var x = cx + corner[0] * boxX, y = cy + corner[1] * boxY;
      ctx.beginPath();
      ctx.moveTo(x - corner[0] * arm, y);
      ctx.lineTo(x, y);
      ctx.lineTo(x, y - corner[1] * arm);
      ctx.stroke();
    });
    ctx.restore();
  }

  // What the presence is doing, drawn around the face. Each effect follows a real state (speaking, listening, working, waiting);
  // none is a meter or a number. A change of state sends one shockwave out, so the change is seen as well as the colour.
  var stateFx = { mode: "", waves: [], rippleAt: 0, beatAt: 0 };
  function drawStateEffects(cx, cy, size, t, mode) {
    stateFx.t = t;
    if (mode !== stateFx.mode) {
      if (stateFx.mode && !calm) stateFx.waves.push({ born: t, from: 0.55, to: 1.45, life: 1.1, width: 2.2 });
      stateFx.mode = mode;
    }
    if (!calm && mode === "listening" && t >= stateFx.rippleAt) {
      stateFx.waves.push({ born: t, from: 0.62, to: 1.25, life: 1.4, width: 1 + energy * 2 });
      stateFx.rippleAt = t + 0.55;
    }
    if (!calm && mode === "waiting" && t >= stateFx.beatAt) {
      stateFx.waves.push({ born: t, from: 0.86, to: 1.08, life: 0.7, width: 2 });
      stateFx.waves.push({ born: t + 0.22, from: 0.86, to: 1.12, life: 0.8, width: 1.4 });
      stateFx.beatAt = t + 1.7;
    }
    ctx.save();
    stateFx.waves = stateFx.waves.filter(function (wave) { return t - wave.born < wave.life; });
    stateFx.waves.forEach(function (wave) {
      var u = (t - wave.born) / wave.life;
      if (u < 0) return;
      var eased = 1 - Math.pow(1 - u, 3);
      ctx.lineWidth = wave.width;
      ctx.strokeStyle = wave.rgb ? "rgba(" + wave.rgb + "," + 0.75 * (1 - u) + ")" : rgba(0.55 * (1 - u));
      ctx.beginPath();
      ctx.arc(cx, cy, size * (wave.from + (wave.to - wave.from) * eased), 0, TAU);
      ctx.stroke();
    });

    if (mode === "speaking") {
      // the voice as a ring of bars round the face, mirrored so it reads as one sound
      var bars = 96, inner = size * 0.93;
      ctx.lineCap = "round";
      ctx.lineWidth = Math.max(1.5, size * 0.011);
      for (var b = 0; b < bars; b += 1) {
        var angle = b / bars * TAU - Math.PI / 2;
        if (Math.abs(angle - Math.PI / 2) < 0.32) continue;   // a gap at the bottom, where the caption sits
        var mirror = Math.min(b, bars - b) / (bars / 2);
        var wave2 = calm ? 0.4 : Math.abs(Math.sin(mirror * 9.1 + t * 7.3) * Math.sin(mirror * 3.7 - t * 4.1));
        var len = size * (0.015 + wave2 * energy * 0.16);
        ctx.strokeStyle = rgba(0.25 + 0.6 * wave2);
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(angle) * inner, cy + Math.sin(angle) * inner);
        ctx.lineTo(cx + Math.cos(angle) * (inner + len), cy + Math.sin(angle) * (inner + len));
        ctx.stroke();
      }
    } else if (mode === "working" && !calm) {
      // data packets running round the segment ring with fading tails, and streams of light falling in towards the face
      ctx.lineCap = "round";
      for (var packet = 0; packet < 5; packet += 1) {
        var dir = packet % 2 ? -1 : 1;
        var head = t * (packet % 2 ? -1.25 : 0.95) + packet * TAU / 5;
        for (var tailStep = 0; tailStep < 6; tailStep += 1) {
          ctx.strokeStyle = rgba(0.8 * (1 - tailStep / 6));
          ctx.lineWidth = 2.6 - tailStep * 0.3;
          ctx.beginPath();
          var a0 = head - dir * tailStep * 0.05, a1 = head - dir * (tailStep + 1) * 0.05;
          ctx.arc(cx, cy, size * 0.75, Math.min(a0, a1), Math.max(a0, a1));
          ctx.stroke();
        }
      }
      for (var stream = 0; stream < 18; stream += 1) {
        var lane = stream * 2.399;
        var progress = (t * (0.35 + (stream % 4) * 0.08) + stream * 0.37) % 1;
        var r1 = size * (1.35 - progress * 0.7), r2 = r1 + size * 0.07;
        ctx.strokeStyle = rgba(0.5 * Math.sin(Math.PI * progress));
        ctx.lineWidth = 1.2;
        ctx.beginPath();
        ctx.moveTo(cx + Math.cos(lane) * r1, cy + Math.sin(lane) * r1);
        ctx.lineTo(cx + Math.cos(lane) * r2, cy + Math.sin(lane) * r2);
        ctx.stroke();
      }
    } else if (mode === "listening") {
      ctx.lineWidth = 2 + energy * 3;
      ctx.strokeStyle = rgba(0.15 + energy * 0.5);
      ctx.beginPath();
      ctx.arc(cx, cy, size * (0.92 + energy * 0.05), 0, TAU);
      ctx.stroke();
    }
    ctx.restore();
  }

  function draw(t, mode) {
    ctx.clearRect(0, 0, W, H);
    // While the live view is up, the face makes room for it and moves to the right.
    var compact = window.innerWidth <= 960;
    var room = !compact && window.JarvisMission ? Math.min(350, W * 0.38) * JarvisMission.dock(W) : 0;
    var avail = W - room;
    // The mobile stage is compact; the desktop leaves room for the controls below.
    var top = compact ? 12 : 20, bottom = compact ? 8 : 100;
    var usable = Math.max(80, H - top - bottom);
    // The desktop face leaves room round it for the rings and the voice; the small mobile stage gives the face most of its space.
    var size = compact ? Math.min(avail * 0.56, usable / 1.6) : Math.min(avail * FACE, usable / 2.05);
    var cx = room + avail / 2, cy = top + usable / 2;
    var quiet = document.documentElement.dataset.atmosphere === "quiet";
    var reach = size * 0.5;
    lastFace = { cx: cx, cy: cy, size: size };
    if (reach < 30) return;
    var speed = calm ? 0.2 : (mode === "working" ? 1.8 : mode === "waiting" ? 1.2 : mode === "offline" ? 0.1 : mode === "idle" ? 0.5 : 1);

    // light, wide and faint, so the stage has a presence without an edge
    var halo = ctx.createRadialGradient(cx, cy, size * 0.1, cx, cy, size * 1.5);
    halo.addColorStop(0, rgba((0.12 + energy * 0.18) * (quiet ? 0.3 : 1)));
    halo.addColorStop(0.5, rgba((0.04 + energy * 0.05) * (quiet ? 0.3 : 1)));
    halo.addColorStop(1, rgba(0));
    ctx.fillStyle = halo;
    ctx.fillRect(0, 0, W, H);

    drawPresenceOrbit(cx, cy, size, t, speed, quiet);

    // dust drifting round the face
    if (!quiet) motes.forEach(function (mote, index) {
      var angle = mote.a + (calm ? 0 : t * mote.sp * speed);
      var x = cx + Math.cos(angle) * mote.r * size * 0.95, y = cy + Math.sin(angle) * mote.r * size * 0.72;
      var twinkle = calm ? 0.5 : 0.5 + 0.5 * Math.sin(t * 1.3 + index);
      ctx.fillStyle = rgba((0.05 + 0.16 * twinkle) * (0.5 + energy));
      ctx.beginPath();
      ctx.arc(x, y, mote.size, 0, TAU);
      ctx.fill();
    });

    // the face: a real mesh, whose mouth follows the voice and whose eyes, brows and head move on their own
    if (window.JarvisHead) {
      var mouth = 0;
      if (mode === "speaking") {
        // the real audio level when the neural voice is playing; a synthetic syllable rhythm for the browser voice
        mouth = S.voiceKind === "neural"
          ? Math.min(1, S.voiceLevel * 1.6)
          : 0.12 + 0.88 * Math.pow(Math.abs(Math.sin(t * 9.3) * Math.sin(t * 3.7 + 0.8)), 0.7);
      }
      JarvisHead.draw(ctx, cx, cy, size * 1.55, { rgb: current, energy: energy, mode: mode, mouth: mouth, t: t, calm: calm });
    }

    // the state around the face: shockwaves on a change, the voice as a ring, ripples while listening, packets while working
    if (!quiet) drawStateEffects(cx, cy, size, t, mode);

    // what JARVIS is doing right now: nodes, beams and the effect for each kind of work (mission.js)
    if (window.JarvisMission) JarvisMission.draw(ctx, cx, cy, reach, t, current, calm, W, H, room);

    // the caption sits under the face, in the gap of the rings, wherever the face is
    var captionTop = Math.round(Math.min(cy + size * 1.02, H - 96));
    var place = captionTop + "|" + Math.round(room);
    if (place !== captionAt) {
      captionAt = place;
      var label = $("caption");
      label.style.top = captionTop + "px";
      label.style.bottom = "auto";
      label.style.left = Math.round(room) + "px";
    }
  }

  // ---- moments: short, one-off effects for things that just happened -----------------------------------------------
  // Each follows a real event (an answer you gave, the wake word heard, a step taken, a long quiet). None runs with reduced motion
  // or the quiet atmosphere, and none carries anything that is not also on the page as text.
  var lastFace = null;
  function showy() { return !calm && document.documentElement.dataset.atmosphere !== "quiet" && !document.hidden; }
  function faceCentre() {
    if (!lastFace) return null;
    var box = canvas.getBoundingClientRect();
    if (!box.width || !box.height) return null;
    return { x: box.left + lastFace.cx, y: box.top + lastFace.cy, size: lastFace.size, box: box };
  }
  // Shockwaves from the face, in a colour of their own (green for yes, red for no) or the presence colour.
  function burst(rgb, rings, from, to) {
    if (!showy()) return;
    for (var k = 0; k < rings; k += 1) {
      stateFx.waves.push({ born: (stateFx.t || 0) + k * 0.16, from: from || 0.4, to: (to || 1.6) + k * 0.15, life: 1.2, width: Math.max(1, 3 - k), rgb: rgb });
    }
  }
  // A wash of light over the stage, centred on the face.
  function flash(kind) {
    var stage = $("stage"), at = faceCentre();
    if (!showy()) return;
    if (at) {
      stage.style.setProperty("--fx", Math.round(at.x - at.box.left) + "px");
      stage.style.setProperty("--fy", Math.round(at.y - at.box.top) + "px");
    }
    stage.classList.remove("flash-yes", "flash-no", "flash-wake");
    void stage.offsetWidth;
    stage.classList.add("flash-" + kind);
    setTimeout(function () { stage.classList.remove("flash-" + kind); }, 1500);
  }
  // An answered question folds into the face, which answers with a flash: green for yes, red for no.
  function celebrate(approvalId, approve) {
    var colour = approve ? "165,214,160" : "250,129,123";
    var card = null, to = faceCentre();
    document.querySelectorAll("#tray-waiting .item").forEach(function (item) { if (item.dataset.approval === approvalId) card = item; });
    function land() { burst(colour, 2); flash(approve ? "yes" : "no"); }
    if (!card || !to || !showy() || !card.animate || !card.getClientRects().length) { land(); return; }
    var from = card.getBoundingClientRect();
    var ghost = card.cloneNode(true);
    ghost.querySelectorAll("[id]").forEach(function (node) { node.removeAttribute("id"); });
    ghost.setAttribute("aria-hidden", "true");
    ghost.inert = true;
    ghost.classList.add("ghost", approve ? "yes" : "no");
    ghost.style.left = from.left + "px";
    ghost.style.top = from.top + "px";
    ghost.style.width = from.width + "px";
    document.body.appendChild(ghost);
    var dx = to.x - (from.left + from.width / 2), dy = to.y - (from.top + from.height / 2);
    ghost.animate([
      { transform: "none", opacity: 1, filter: "none" },
      { transform: "translate(" + dx * 0.12 + "px, " + dy * 0.12 + "px) scale(1.04)", opacity: 1, filter: "brightness(1.6)", offset: 0.22 },
      { transform: "translate(" + dx + "px, " + dy + "px) scale(0.04)", opacity: 0, filter: "brightness(3)" }
    ], { duration: 720, easing: "cubic-bezier(0.6, 0, 0.8, 0.4)" }).onfinish = function () { ghost.remove(); land(); };
  }
  // The wake word: the eyes flare, the face looks up, the stage's corners snap in.
  function wakeFlare() {
    if (window.JarvisHead && JarvisHead.flare) JarvisHead.flare();
    feel("alert", 1.6);
    burst(null, 1, 0.5, 1.3);
    flash("wake");
  }
  // A step the run took leaves the face as a spark and lands on its chip in the conversation.
  function launchPacket(target, waiting) {
    var from = faceCentre();
    if (!from || !showy() || !target.animate || !target.getClientRects().length) return;
    var to = target.getBoundingClientRect();
    if (to.bottom < 0 || to.top > window.innerHeight) return;
    var dot = el("span", "packet" + (waiting ? " wait" : ""));
    dot.setAttribute("aria-hidden", "true");
    dot.style.left = from.x + "px";
    dot.style.top = from.y + "px";
    document.body.appendChild(dot);
    var dx = to.left + Math.min(to.width / 2, 28) - from.x, dy = to.top + to.height / 2 - from.y;
    var lift = -Math.min(180, Math.abs(dx) * 0.4);
    dot.animate([
      { transform: "translate(-50%, -50%) scale(0.3)", opacity: 0 },
      { transform: "translate(calc(-50% + " + dx * 0.45 + "px), calc(-50% + " + (dy * 0.45 + lift) + "px)) scale(1.3)", opacity: 1, offset: 0.45 },
      { transform: "translate(calc(-50% + " + dx + "px), calc(-50% + " + dy + "px)) scale(0.5)", opacity: 0.9 }
    ], { duration: 820, easing: "cubic-bezier(0.3, 0, 0.3, 1)" }).onfinish = function () {
      dot.remove();
      target.classList.add("landed");
      setTimeout(function () { target.classList.remove("landed"); }, 900);
    };
    stateFx.waves.push({ born: stateFx.t || 0, from: 0.5, to: 0.95, life: 0.6, width: 1.5, rgb: waiting ? "239,192,120" : null });
  }
  // A long quiet: the face dims and a large clock fades in behind it, until someone moves, types or JARVIS has something to do.
  var lastStir = Date.now();
  ["pointermove", "pointerdown", "keydown", "wheel", "touchstart"].forEach(function (name) {
    window.addEventListener(name, function () { lastStir = Date.now(); }, { passive: true });
  });
  function paintAmbient() {
    var mode = uiState();
    if (mode !== "idle") lastStir = Date.now();
    var dozing = Date.now() - lastStir > 90000 && $("settings").hidden && $("ops").hidden;
    if (dozing) {
      var now = new Date();
      $("ambient-time").textContent = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
      $("ambient-date").textContent = now.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long" });
    }
    $("stage").classList.toggle("ambient", dozing);
  }
  setInterval(paintAmbient, 1000);

  function frame(stamp) {
    if (document.hidden || !$("settings").hidden || !$("ops").hidden || window.innerHeight <= 600 && window.innerWidth <= 960) {
      setTimeout(function () { requestAnimationFrame(frame); }, 250); return;
    }
    var t = stamp / 1000;
    calm = document.documentElement.dataset.motion === "reduced" || !!(motionQuery && motionQuery.matches);
    var mode = uiState();
    var target = colorForState(mode);
    for (var i = 0; i < 3; i += 1) current[i] += (target[i] - current[i]) * 0.08;
    readLevel();
    readVoiceLevel();
    energy += (targetEnergy(mode, calm ? 0 : t) - energy) * 0.12;
    draw(t, mode);
    if (calm) setTimeout(function () { requestAnimationFrame(frame); }, 250); else requestAnimationFrame(frame);
  }
  // The face is a button: clicking it silences JARVIS. A node of the live view under the pointer is named, and pressing it opens its step.
  function canvasPoint(event) {
    var box = canvas.getBoundingClientRect();
    return [event.clientX - box.left, event.clientY - box.top];
  }
  canvas.addEventListener("mousemove", function (event) {
    if (!window.JarvisMission) return;
    var at = canvasPoint(event);
    canvas.style.cursor = JarvisMission.hover(at[0], at[1]) ? "pointer" : "";
  });
  canvas.addEventListener("mouseleave", function () { if (window.JarvisMission) JarvisMission.hover(-99, -99); canvas.style.cursor = ""; });
  canvas.addEventListener("click", function (event) {
    var at = canvasPoint(event);
    if (window.JarvisMission && JarvisMission.click(at[0], at[1])) return;
    interruptSpeech();
  });
  requestAnimationFrame(frame);

  loadProfileHistory();
  poll();
  paintHeader();
  // a greeting when the console opens: a smile and a nod, then it carries on with its own business
  setTimeout(function () { feel("pleased", 2.4, "nod"); }, 900);
})();
