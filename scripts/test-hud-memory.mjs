import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source = fs.readFileSync(new URL('../apps/jarvisd/src/hud/hud.js', import.meta.url), 'utf8');
const start = source.indexOf('  var memoryLoaded = false');
const end = source.indexOf('  function offerCard', start);
assert.ok(start >= 0 && end > start);

class Element {
  constructor(tag = 'div') {
    this.tagName = tag;
    this.children = [];
    this.listeners = new Map();
    this.disabled = false;
    this.value = '';
    this.text = '';
  }
  set innerHTML(value) { throw new Error('Memory must never insert markup'); }
  set textContent(value) { this.children = []; this.text = String(value); }
  get textContent() { return this.text + this.children.map(child => child.textContent).join(''); }
  appendChild(child) { this.children.push(child); return child; }
  replaceChildren(...children) { this.text = ''; this.children = children; }
  setAttribute(name, value) { this[name] = value; }
  addEventListener(name, handler) { this.listeners.set(name, handler); }
  focus() { this.focused = true; }
  dispatch(name) { if (!this.disabled) return this.listeners.get(name)?.({ preventDefault() {} }); }
  descendants() { return this.children.flatMap(child => [child, ...child.descendants()]); }
}
const plain = value => JSON.parse(JSON.stringify(value));
const flush = () => new Promise(resolve => setImmediate(resolve));
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function harness({ browse = () => ({ memories: [], next_cursor: null }), read = () => detail(), write = () => ({}) } = {}) {
  const nodes = new Map();
  const calls = [], notices = [];
  const context = {
    $(id) { if (!nodes.has(id)) nodes.set(id, new Element()); return nodes.get(id); },
    el(tag, className = '', text = '') { const node = new Element(tag); node.className = className; node.textContent = text; return node; },
    api(path) { calls.push({ path, method: 'GET' }); return Promise.resolve().then(() => read(path)); },
    putJson(path, method, body) {
      calls.push({ path, method, body: plain(body) });
      return Promise.resolve().then(() => path === '/memories/browse' ? browse(body) : write(path, body));
    },
    encodeURIComponent,
    explain(error, fallback) { return error.message || fallback; },
    notice(text) { notices.push(text); },
    proposalsAt: 1
  };
  vm.runInNewContext(source.slice(start, end), context);
  return { context, calls, notices, node: context.$, button(text) {
    const button = context.$('memory-detail').descendants().find(node => node.tagName === 'button' && node.textContent === text);
    assert.ok(button, text);
    return button;
  } };
}
function detail() {
  return { memory_id: 'claim-1', memory_type: 'preference', confidence: 'confirmed', sensitivity: 'internal',
    status: 'active', effective_status: 'active', source_kind: 'user_statement', source_locator: 'session:fixture',
    content: '<img src=x onerror=steal()> a full remembered claim', entity_ids: ['entity-1'], version: 7 };
}
function preview() { return { ...detail(), preview: '<script>preview is text</script>' }; }

test('browse needs no keyword and exact details remain plain text with provenance', async () => {
  const fixture = harness({ browse: () => ({ memories: [preview()], next_cursor: 'next-1' }) });
  await fixture.context.loadMemory(true);
  assert.deepEqual(fixture.calls[0], { path: '/memories/browse', method: 'POST', body: { limit: 10 } });
  assert.ok(fixture.node('memory-list').textContent.includes('<script>preview is text</script>'));
  const open = fixture.node('memory-list').descendants().find(node => node.tagName === 'button');
  open.dispatch('click');
  await flush();
  assert.equal(fixture.calls[1].path, '/memories/claim-1');
  assert.ok(fixture.node('memory-detail').textContent.includes('<img src=x onerror=steal()>'));
  assert.match(fixture.node('memory-detail').textContent, /source_locator: session:fixture/);
  assert.match(fixture.node('memory-detail').textContent, /Entity IDs: entity-1/);
  assert.equal(fixture.node('memory-detail').focused, true);
});

test('older and newer pages use cursors and filters restart at the first page', async () => {
  const fixture = harness({ browse: body => ({ memories: [preview()], next_cursor: body.cursor ? null : 'next-1' }) });
  await fixture.context.loadMemory(true);
  fixture.node('memory-next').dispatch('click');
  await flush();
  assert.equal(fixture.calls.at(-1).body.cursor, 'next-1');
  assert.equal(fixture.node('memory-next').disabled, true);
  fixture.node('memory-previous').dispatch('click');
  await flush();
  assert.equal(fixture.calls.at(-1).body.cursor, undefined);
  fixture.node('memory-type').value = 'preference';
  fixture.node('memory-entity').value = ' entity-1 ';
  fixture.node('memory-filters').dispatch('submit');
  await flush();
  assert.deepEqual(fixture.calls.at(-1).body, { limit: 10, memory_type: 'preference', entity_id: 'entity-1' });
});

test('correction sends the inspected version and refreshes discovery after success', async () => {
  const fixture = harness();
  await fixture.context.readMemory('claim-1');
  const correction = fixture.node('memory-detail').descendants().find(node => node.tagName === 'textarea');
  correction.value = '  A corrected claim  ';
  fixture.button('Save correction').dispatch('click');
  await flush();
  assert.deepEqual(fixture.calls.find(call => call.path.endsWith('/correct')), {
    path: '/memories/claim-1/correct', method: 'POST', body: { content: 'A corrected claim', expected_version: 7 }
  });
  assert.equal(fixture.calls.at(-1).path, '/memories/browse');
  assert.deepEqual(fixture.notices, ['Memory corrected.']);
});

test('forget requires explicit inline confirmation and prevents relearning', async () => {
  const fixture = harness();
  await fixture.context.readMemory('claim-1');
  fixture.button('Forget this memory').dispatch('click');
  assert.equal(fixture.calls.length, 1);
  fixture.button('Cancel').dispatch('click');
  assert.equal(fixture.calls.length, 1);
  fixture.button('Forget this memory').dispatch('click');
  const yes = fixture.button('Yes, forget');
  yes.dispatch('click');
  yes.dispatch('click');
  await flush();
  const writes = fixture.calls.filter(call => call.path.endsWith('/forget'));
  assert.equal(writes.length, 1);
  assert.deepEqual(writes[0].body, { expected_version: 7, allow_relearn: false });
});

test('stale browse and detail responses cannot replace newer selections', async () => {
  const oldPage = deferred(), oldDetail = deferred();
  let pages = 0;
  const fixture = harness({ browse: () => ++pages === 1 ? oldPage.promise : { memories: [], next_cursor: null },
    read: path => path.endsWith('/old') ? oldDetail.promise : { ...detail(), memory_id: 'new' } });
  const first = fixture.context.loadMemory(true);
  await fixture.context.loadMemory(true);
  oldPage.resolve({ memories: [preview()], next_cursor: 'stale' });
  await first;
  assert.equal(fixture.node('memory-list').children.length, 0);
  assert.equal(fixture.node('memory-next').disabled, true);
  const read = fixture.context.readMemory('old');
  await fixture.context.readMemory('new');
  oldDetail.resolve({ ...detail(), content: 'stale old claim' });
  await read;
  assert.doesNotMatch(fixture.node('memory-detail').textContent, /stale old claim/);
  assert.equal(fixture.button('Save correction').disabled, false);
});

test('failed requests show recovery text without claiming a write succeeded', async () => {
  const fixture = harness({ browse: () => { throw new Error('Browse unavailable'); },
    write: () => { throw new Error('Changed by another writer; reopen details'); } });
  await fixture.context.loadMemory(true);
  assert.match(fixture.node('memory-status').textContent, /Browse unavailable/);
  await fixture.context.readMemory('claim-1');
  fixture.button('Forget this memory').dispatch('click');
  fixture.button('Yes, forget').dispatch('click');
  await flush();
  assert.match(fixture.node('memory-detail').textContent, /reopen details/);
  assert.equal(fixture.button('Forget this memory').disabled, false);
  assert.equal(fixture.notices.length, 0);
});

test('memory inspector lazy loading targets the real Operations overlay', () => {
  assert.ok(source.includes('if (!memoryLoaded && !$("ops").hidden) loadMemory(true)'));
  const html = fs.readFileSync(new URL('../apps/jarvisd/src/hud/index.html', import.meta.url), 'utf8');
  for (const id of ['ops', 'memory-filters', 'memory-type', 'memory-entity', 'memory-status', 'memory-list', 'memory-detail', 'memory-previous', 'memory-next']) {
    assert.ok(html.includes('id="' + id + '"'), id);
  }
});

test('opening Operations loads once and closing or reopening preserves the inspected draft', async () => {
  const fixture = harness();
  Object.assign(fixture.context, { abilitiesAt: 1, loadProjects() {} });
  vm.runInNewContext(source.slice(source.indexOf('  function setOps(open)'), source.indexOf('  $("open-ops").addEventListener')), fixture.context);
  fixture.context.setOps(false);
  assert.equal(fixture.calls.length, 0);
  fixture.context.setOps(true);
  await flush();
  assert.equal(fixture.calls.filter(call => call.path === '/memories/browse').length, 1);
  await fixture.context.readMemory('claim-1');
  const correction = fixture.node('memory-detail').descendants().find(node => node.tagName === 'textarea');
  correction.value = 'An unsaved correction';
  fixture.context.setOps(false);
  fixture.context.setOps(true);
  await flush();
  assert.equal(fixture.calls.filter(call => call.path === '/memories/browse').length, 1);
  assert.equal(correction.value, 'An unsaved correction');
});

test('a failed older page retains working details and permits retry without skipping a cursor', async () => {
  let fail = true;
  const fixture = harness({ browse: body => {
    if (body.cursor && fail) throw new Error('Temporary page error');
    return { memories: [preview()], next_cursor: body.cursor ? null : 'next-1' };
  } });
  await fixture.context.loadMemory(true);
  fixture.node('memory-next').dispatch('click');
  await flush();
  assert.match(fixture.node('memory-status').textContent, /Temporary page error/);
  fixture.node('memory-list').descendants().find(node => node.tagName === 'button').dispatch('click');
  await flush();
  assert.equal(fixture.calls.at(-1).path, '/memories/claim-1');
  fail = false;
  fixture.node('memory-next').dispatch('click');
  await flush();
  assert.equal(fixture.calls.at(-1).body.cursor, 'next-1');
  assert.match(fixture.node('memory-status').textContent, /Page 2/);
});

test('paging uses submitted filters until a new filter submission resets the cursor', async () => {
  const fixture = harness({ browse: () => ({ memories: [preview()], next_cursor: 'next-1' }) });
  fixture.node('memory-type').value = 'semantic';
  await fixture.context.loadMemory(true);
  fixture.node('memory-type').value = 'preference';
  fixture.node('memory-next').dispatch('click');
  await flush();
  assert.deepEqual(fixture.calls.at(-1).body, { limit: 10, memory_type: 'semantic', cursor: 'next-1' });
  fixture.node('memory-filters').dispatch('submit');
  await flush();
  assert.deepEqual(fixture.calls.at(-1).body, { limit: 10, memory_type: 'preference' });
});

test('refresh invalidates pending details and old correction controls', async () => {
  const pending = deferred();
  const fixture = harness({ read: path => path.endsWith('/pending') ? pending.promise : detail() });
  await fixture.context.readMemory('claim-1');
  const oldSave = fixture.button('Save correction');
  const read = fixture.context.readMemory('pending');
  await fixture.context.loadMemory(true);
  pending.resolve({ ...detail(), memory_id: 'pending', content: 'An old selection' });
  await read;
  oldSave.dispatch('click');
  await flush();
  assert.equal(fixture.node('memory-detail').textContent, '');
  assert.equal(fixture.calls.some(call => call.path.endsWith('/correct')), false);
});

for (const outcome of ['success', 'failure']) {
  test(`late write ${outcome} cannot overwrite a newer detail selection or draft`, async () => {
    const pending = deferred();
    const fixture = harness({ read: path => ({ ...detail(), memory_id: path.split('/').at(-1) }), write: () => pending.promise });
    await fixture.context.readMemory('claim-1');
    fixture.button('Save correction').dispatch('click');
    await flush();
    await fixture.context.readMemory('claim-2');
    const correction = fixture.node('memory-detail').descendants().find(node => node.tagName === 'textarea');
    correction.value = 'Keep this newer draft';
    const status = fixture.node('memory-status').textContent;
    if (outcome === 'success') pending.resolve({});
    else pending.reject(new Error('Old write failed'));
    await flush();
    assert.match(fixture.node('memory-detail').textContent, /memory_id: claim-2/);
    assert.equal(correction.value, 'Keep this newer draft');
    assert.equal(fixture.node('memory-status').textContent, status);
    assert.equal(fixture.calls.some(call => call.path === '/memories/browse'), false);
  });
}

test('conflicts retain the correction draft and reload obtains the new write version', async () => {
  let version = 7;
  const fixture = harness({ read: () => ({ ...detail(), version }), write: () => {
    const error = new Error('Version conflict'); error.status = 409; throw error;
  } });
  await fixture.context.readMemory('claim-1');
  const correction = fixture.node('memory-detail').descendants().find(node => node.tagName === 'textarea');
  correction.value = 'My correction';
  fixture.button('Save correction').dispatch('click');
  await flush();
  assert.equal(correction.value, 'My correction');
  assert.match(fixture.node('memory-detail').textContent, /Changed since you opened it.*Reload details/);
  assert.deepEqual(fixture.notices, []);
  version = 8;
  fixture.button('Reload details').dispatch('click');
  await flush();
  fixture.button('Save correction').dispatch('click');
  await flush();
  assert.equal(fixture.calls.at(-1).body.expected_version, 8);
});

test('missing identity or version fails closed before correction or forget can be sent', async () => {
  for (const change of [{ version: undefined }, { version: 0 }, { memory_id: 'different-claim' }]) {
    const fixture = harness({ read: () => ({ ...detail(), ...change }) });
    await fixture.context.readMemory('claim-1');
    assert.equal(fixture.button('Save correction').disabled, true);
    assert.equal(fixture.button('Forget this memory').disabled, true);
    // Even a direct handler invocation cannot bypass the UI's read/version guard.
    fixture.button('Save correction').listeners.get('click')();
    fixture.button('Forget this memory').listeners.get('click')();
    fixture.button('Yes, forget').listeners.get('click')();
    await flush();
    assert.equal(fixture.calls.length, 1);
    assert.deepEqual(fixture.notices, []);
  }
});

test('blank corrections, duplicate saves and unconfirmed forgets cannot produce extra writes', async () => {
  const pending = deferred();
  const fixture = harness({ write: () => pending.promise });
  await fixture.context.readMemory('claim-1');
  fixture.button('Yes, forget').listeners.get('click')();
  const correction = fixture.node('memory-detail').descendants().find(node => node.tagName === 'textarea');
  correction.value = '  ';
  fixture.button('Save correction').dispatch('click');
  assert.equal(fixture.calls.length, 1);
  correction.value = 'A valid correction';
  const save = fixture.button('Save correction');
  save.listeners.get('click')();
  save.listeners.get('click')();
  await flush();
  assert.equal(fixture.calls.filter(call => call.path.endsWith('/correct')).length, 1);
  pending.resolve({});
  await flush();
});

test('failed detail reads clear the previous record and expose no stale write controls', async () => {
  const fixture = harness({ read: path => {
    if (path.endsWith('/missing')) throw new Error('Memory unavailable');
    return detail();
  } });
  await fixture.context.readMemory('claim-1');
  await fixture.context.readMemory('missing');
  assert.match(fixture.node('memory-detail').textContent, /Memory unavailable/);
  assert.equal(fixture.node('memory-detail').descendants().some(node => node.tagName === 'button'), false);
});
