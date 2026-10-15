import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source = fs.readFileSync(new URL('../apps/jarvisd/src/hud/hud.js', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
function section(start, end) {
  const from = source.indexOf(start), to = source.indexOf(end, from);
  assert.ok(from >= 0 && to > from, `HUD test boundary missing: ${start}`);
  return source.slice(from, to);
}
function node() {
  return { textContent: '', lastChild: { textContent: '' }, dataset: {}, hidden: false, disabled: false,
    attributes: {}, setAttribute(name, value) { this.attributes[name] = value; } };
}
function headerContext(overrides = {}, token = 'fixture') {
  const nodes = new Map();
  const state = { offline: false, needYou: 0, working: 0, thinking: 0, listening: false, speaking: false, voiceBusy: false, ...overrides };
  const context = { S: state, token, busy: false, Recognition: true, document: {}, window: {},
    $(id) { if (!nodes.has(id)) nodes.set(id, node()); return nodes.get(id); } };
  vm.runInNewContext(section('  function uiState()', '  $("quiet").addEventListener'), context);
  return { context, nodes, state };
}

for (const [state, expected] of [
  [{}, 'idle'], [{ working: 2 }, 'working'], [{ thinking: 1 }, 'working'],
  [{ listening: true }, 'listening'], [{ speaking: true }, 'speaking'],
  [{ needYou: 1, listening: true }, 'waiting'], [{ offline: true, needYou: 1 }, 'offline']
]) {
  test(`header derives ${expected} from real state`, () => {
    const { context, nodes } = headerContext(state);
    context.paintHeader();
    assert.equal(nodes.get('stage').dataset.state, expected);
    assert.equal(nodes.get('send').disabled, expected === 'offline');
    assert.equal(nodes.get('pill-wait').lastChild.textContent, `${state.needYou || 0} waiting`);
  });
}

test('offline and busy messaging remains truthful, with decisions still visible', () => {
  const { context, nodes, state } = headerContext({ needYou: 2, offline: true }, null);
  context.paintHeader();
  assert.match(nodes.get('composer-status').textContent, /jarvis hud/);
  assert.equal(nodes.get('stop').hidden, false);
  state.offline = false;
  context.busy = true;
  context.paintHeader();
  assert.equal(nodes.get('send').disabled, true);
  assert.match(nodes.get('composer-status').textContent, /draft/);
  assert.equal(context.document.title, '(2) JARVIS');
});

test('presence rail never presents stale task counts as live while offline', () => {
  const { context, nodes, state } = headerContext({ offline: true, working: 4, needYou: 2 });
  context.paintHeader();
  assert.equal(nodes.get('presence-work-value').textContent, '—');
  assert.equal(nodes.get('presence-wait-value').textContent, '—');
  assert.equal(nodes.get('presence-work-detail').textContent, 'Link unavailable');
  state.offline = false;
  context.paintHeader();
  assert.equal(nodes.get('presence-work-value').textContent, '04');
  assert.equal(nodes.get('presence-wait-value').textContent, '02');
  context.Recognition = null;
  context.paintHeader();
  assert.equal(nodes.get('stage-talk').disabled, true);
});

test('stage shortcuts focus or open existing controls without starting a run', () => {
  const handlers = {}, calls = [];
  const context = { $(id) { return { addEventListener(name, callback) { handlers[id] = callback; }, focus() { calls.push(`focus:${id}`); }, click() { calls.push(`click:${id}`); } }; }, setOps(value) { calls.push(`ops:${value}`); } };
  vm.runInNewContext(section('  $("stage-write").addEventListener', '  function paintClock()'), context);
  handlers['stage-write']();
  handlers['stage-talk']();
  handlers['presence-wait']();
  assert.deepEqual(calls, ['focus:input', 'click:mic', 'ops:true']);
});

test('voice preparation exposes an accessible silence control', () => {
  const { context, nodes } = headerContext({ voiceBusy: true });
  context.paintHeader();
  assert.equal(nodes.get('quiet').hidden, false);
  assert.equal(nodes.get('stage-state').textContent, 'Preparing voice');
});

function composerContext({ offline = false, busy = false, value = 'A draft' } = {}) {
  const input = { value, addEventListener(name, callback) { handlers[name] = callback; } }, sent = [], handlers = {};
  const context = { S: { offline }, busy, send(text) { sent.push(text); },
    $(id) { return id === 'input' ? input : { addEventListener(name, callback) { handlers[name] = callback; }, requestSubmit() { handlers.submit({ preventDefault() {} }); } }; } };
  vm.runInNewContext(section('  $("composer").addEventListener', '  // ---- voice: speaking'), context);
  return { input, sent, submit: () => handlers.submit({ preventDefault() {} }), keydown: event => handlers.keydown(event) };
}
for (const options of [{ offline: true }, { busy: true }, { value: '   ' }]) {
  test(`submission keeps unsent drafts: ${JSON.stringify(options)}`, () => {
    const fixture = composerContext(options);
    const original = fixture.input.value;
    fixture.submit();
    assert.equal(fixture.input.value, original);
    assert.equal(fixture.sent.length, 0);
  });
}
test('connected submission sends once and clears the submitted draft', () => {
  const fixture = composerContext();
  fixture.submit();
  assert.deepEqual(fixture.sent, ['A draft']);
  assert.equal(fixture.input.value, '');
});

test('large multiline drafts are submitted intact and Shift Enter does not send', () => {
  const value = 'paragraph\n'.repeat(10000) + 'TAIL';
  const fixture = composerContext({ value });
  fixture.keydown({ key: 'Enter', shiftKey: true, preventDefault() { assert.fail('Shift Enter must insert a newline'); } });
  assert.equal(fixture.sent.length, 0);
  fixture.keydown({ key: 'Enter', isComposing: true, preventDefault() { assert.fail('IME composition must not submit'); } });
  fixture.keydown({ key: 'Enter', preventDefault() {} });
  assert.deepEqual(fixture.sent, [value]);
  const html = fs.readFileSync(new URL('../apps/jarvisd/src/hud/index.html', import.meta.url), 'utf8');
  assert.match(html, /<textarea id="input"/);
  assert.doesNotMatch(html, /id="input"[^>]*maxlength/);
});

test('suggestions only prepare an editable draft, never start a run', () => {
  const input = { value: '', focus() { this.focused = true; } }, chat = { appendChild(child) { this.child = child; } };
  function el(tag, className, text) {
    return { tag, className, textContent: text, children: [], listeners: {}, appendChild(child) { this.children.push(child); },
      setAttribute() {}, addEventListener(name, callback) { this.listeners[name] = callback; } };
  }
  const context = { currentChat: {}, chat, el, $(id) { assert.equal(id, 'input'); return input; }, send() { assert.fail('A suggestion must not send'); } };
  vm.runInNewContext(section('  function renderEmpty()', '  function renderChat()'), context);
  context.renderEmpty();
  const suggestions = chat.child.children.find(child => child.className === 'suggest');
  assert.equal(suggestions.children.length, 3);
  suggestions.children[0].listeners.click();
  assert.equal(input.value, 'What can you do for me?');
  assert.equal(input.focused, true);
});

function shortcutContext(focused, overlay = false) {
  let handler;
  const calls = [];
  const context = { document: { activeElement: focused, addEventListener(name, callback) { handler = callback; }, querySelector() { return overlay; } },
    $(id) { return { hidden: true, focus() { calls.push(`focus:${id}`); }, click() { calls.push(`click:${id}`); } }; },
    setOps(open) { calls.push(`ops:${open}`); }, newChat() { calls.push('new-chat'); } };
  vm.runInNewContext(section('  document.addEventListener("keydown", function (event) {\n    var focused', '  // ---- the Ops page'), context);
  return { calls, press(key, extra = {}) { handler({ key, preventDefault() {}, ...extra }); } };
}
for (const focused of [{ tagName: 'INPUT', type: 'search' }, { tagName: 'INPUT', type: 'password' }, { tagName: 'TEXTAREA' }, { tagName: 'SELECT' }, { isContentEditable: true }]) {
  test(`shortcuts never hijack editing: ${focused.tagName || 'contenteditable'} ${focused.type || ''}`, () => {
    const fixture = shortcutContext(focused);
    for (const key of ['o', 'n', '/', 'm']) fixture.press(key);
    assert.equal(fixture.calls.length, 0);
  });
}
test('shortcuts respect modifiers and open modal boundaries', () => {
  const fixture = shortcutContext({ tagName: 'BODY' });
  fixture.press('o', { ctrlKey: true });
  fixture.press('n', { metaKey: true });
  fixture.press('m', { altKey: true });
  assert.equal(fixture.calls.length, 0);
  fixture.press('/');
  assert.deepEqual(fixture.calls, ['focus:input']);
  const modal = shortcutContext({ tagName: 'BUTTON' }, true);
  modal.press('o');
  assert.equal(modal.calls.length, 0);
});

test('dialog nesting restores focus and isolates only the top dialog', () => {
  let context;
  function focusable(id) { return { id, tabIndex: 0, isConnected: true, disabled: false, getClientRects() { return [1]; }, focus() { context.document.activeElement = this; } }; }
  const opener = focusable('open-ops'), opsControl = focusable('project-new'), projectControl = focusable('project-close');
  function overlay(id, control) { return { id, hidden: true, querySelectorAll() { return [control]; }, querySelector() { return control; }, contains(candidate) { return candidate === control; } }; }
  const ops = overlay('ops', opsControl), settings = overlay('settings', focusable('settings-search')), project = overlay('project-sheet', projectControl);
  const nodes = new Map([['app', {}], ['ops', ops], ['settings', settings], ['open-ops', node()], ['open-settings', node()]]);
  context = { overlays: [ops, settings, project], activeOverlay: null, returnFocus: new Map(), document: { activeElement: opener, body: { classList: { toggle() {} } } }, $(id) { return nodes.get(id); } };
  vm.runInNewContext(section('  function dialogControls(', '  new MutationObserver(syncDialogs)'), context);
  ops.hidden = false;
  context.syncDialogs();
  assert.equal(context.document.activeElement, opsControl);
  assert.equal(nodes.get('app').inert, true);
  project.hidden = false;
  context.syncDialogs();
  assert.equal(context.document.activeElement, projectControl);
  assert.equal(ops.inert, true);
  project.hidden = true;
  context.syncDialogs();
  assert.equal(context.document.activeElement, opsControl);
  ops.hidden = true;
  context.syncDialogs();
  assert.equal(context.document.activeElement, opener);
  assert.equal(nodes.get('app').inert, false);
});

test('Escape closes only the top dialog before document handlers run', () => {
  let handler;
  const closed = [];
  const context = { activeOverlay: { id: 'project-sheet' },
    window: { addEventListener(name, callback, capture) { assert.equal(name, 'keydown'); assert.equal(capture, true); handler = callback; } },
    $(id) { return { click() { closed.push(id); } }; } };
  vm.runInNewContext(section('  window.addEventListener("keydown", function (event) {\n    if (!activeOverlay)', '  syncDialogs();'), context);
  let stopped = false;
  handler({ key: 'Escape', preventDefault() {}, stopImmediatePropagation() { stopped = true; } });
  assert.deepEqual(closed, ['project-close']);
  assert.equal(stopped, true);
  context.activeOverlay = { id: 'settings', querySelector() { return {}; } };
  handler({ key: 'Escape', preventDefault() { assert.fail('Picker Escape belongs to the picker'); } });
  assert.deepEqual(closed, ['project-close']);
});

test('accent choices do not change the approval, listening or offline state colors', () => {
  const context = { document: { documentElement: { dataset: {} } } };
  vm.runInNewContext(section('  var palette = {', '  var current = colorForState'), context);
  for (const [accent, expected] of [['mint', [104, 231, 224]], ['ice', [125, 211, 252]], ['amethyst', [196, 181, 253]], ['invalid', [104, 231, 224]]]) {
    context.document.documentElement.dataset.accent = accent;
    assert.deepEqual(Array.from(context.colorForState('idle')), expected);
    assert.deepEqual(Array.from(context.colorForState('working')), expected);
    assert.deepEqual(Array.from(context.colorForState('waiting')), [239, 192, 120]);
    assert.deepEqual(Array.from(context.colorForState('listening')), [165, 214, 160]);
    assert.deepEqual(Array.from(context.colorForState('offline')), [120, 136, 130]);
  }
});

function orbitContext(calm = false) {
  const arcs = [];
  const context = { calm, TAU: Math.PI * 2, rgba(alpha) { return String(alpha); },
    ctx: { save() {}, restore() {}, beginPath() {}, stroke() {}, fill() {}, moveTo() {}, lineTo() {}, arc(...args) { arcs.push(args); } } };
  vm.runInNewContext(section('  function drawPresenceOrbit(', '  function draw(t, mode)'), context);
  return { context, arcs };
}
test('quiet atmosphere removes the animated segments and orbiting markers', () => {
  const { context, arcs } = orbitContext();
  context.drawPresenceOrbit(100, 100, 80, 10, 1, true);
  assert.equal(arcs.length, 1);
  arcs.length = 0;
  context.drawPresenceOrbit(100, 100, 80, 10, 1, false);
  assert.equal(arcs.length, 17);
});
test('cinematic orbit positions stay fixed with reduced motion', () => {
  const { context, arcs } = orbitContext(true);
  context.drawPresenceOrbit(100, 100, 80, 10, 1, false);
  const before = structuredClone(arcs);
  arcs.length = 0;
  context.drawPresenceOrbit(100, 100, 80, 90, 2, false);
  assert.deepEqual(arcs, before);
});

test('reduced motion updates at runtime and stops ambient energy pulsing', () => {
  const context = { document: { documentElement: { dataset: { motion: 'reduced' } } }, motionQuery: { matches: false },
    colorForState() { return [1, 2, 3]; }, current: [1, 2, 3], energy: 0, calm: false, uiState() { return 'idle'; },
    readLevel() {}, readVoiceLevel() {}, targetEnergy(mode, t) { context.energyTime = t; return 0; },
    draw() { context.drawCalm = context.calm; }, requestAnimationFrame() {} };
  vm.runInNewContext(section('  function frame(stamp)', '  // The face is a button'), context);
  context.frame(10000);
  assert.equal(context.drawCalm, true);
  assert.equal(context.energyTime, 0);
  context.document.documentElement.dataset.motion = 'system';
  context.frame(11000);
  assert.equal(context.drawCalm, false);
  assert.equal(context.energyTime, 11);
  context.motionQuery.matches = true;
  context.frame(12000);
  assert.equal(context.drawCalm, true);
});
