import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source = readFileSync(new URL('../apps/jarvisd/src/hud/hud.js', import.meta.url), 'utf8');
function harness({ profile = 'a'.repeat(64), touched = false, broken = false } = {}) {
  const values = new Map([
    ['jarvis-chats', JSON.stringify([{ id: 'unscoped', transcript: [] }])],
    ['jarvis-chats:' + 'b'.repeat(64), JSON.stringify([{ id: 'other-profile', transcript: [] }])],
    ['jarvis-chats:' + 'a'.repeat(64), JSON.stringify([{ id: 'saved', session: 'saved-session', transcript: [{ role: 'user', text: 'saved text' }, null] }])]
  ]);
  const nodes = {}, rendered = [], notes = [];
  let resolve;
  const response = new Promise(yes => { resolve = yes; });
  const current = { id: 'current', session: 'current-session', transcript: [{ role: 'user', text: 'typed while loading' }] };
  const context = { token: 'synthetic', api: () => response, Promise, Error,
    historyScope: null, historyTouched: touched, MAX_CHATS: 30, CHATS_KEY: null, CURRENT_KEY: null,
    chats: [current], currentChat: current, transcript: current.transcript, S: { session: current.session },
    localStorage: { getItem(key) { if(broken) throw new Error('Blocked'); return values.get(key) || null; } },
    storedPreference(key) { return values.get(key) || null; },
    $(id) { return nodes[id] ||= {}; }, historyNotice(note) { notes.push(note); },
    renderChat() { rendered.push(context.currentChat.id); }, paintProjectPick() {}, paintUnread() {}, saveChats() {} };
  vm.runInNewContext(source.slice(source.indexOf('  function loadProfileHistory()'), source.indexOf('  function newChatRecord()')), context);
  return { context, rendered, notes, resolve: () => resolve({ profile_id: profile }) };
}

test('history loads only the authenticated profile and filters malformed messages', async () => {
  const h = harness(), pending = h.context.loadProfileHistory(); h.resolve(); await pending;
  assert.equal(h.context.currentChat.id, 'saved');
  assert.equal(h.context.transcript.length, 1);
  assert.equal(h.context.S.session, 'saved-session');
  assert.equal(h.context.chats.some(item => ['unscoped', 'other-profile'].includes(item.id)), false);
});

test('a late history response cannot replace a conversation started while it was loading', async () => {
  const h = harness({ touched: true }), pending = h.context.loadProfileHistory();
  assert.equal(h.context.currentChat.id, 'current');
  h.resolve(); await pending;
  assert.equal(h.context.currentChat.id, 'current');
  assert.equal(h.context.S.session, 'current-session');
  assert.equal(h.rendered.length, 0);
  assert.equal(h.context.chats.length, 2);
});

test('legacy unscoped history does not trigger a migration banner or enter the current profile', async () => {
  const h = harness({ profile: 'c'.repeat(64) }), pending = h.context.loadProfileHistory();
  h.resolve(); await pending;
  assert.equal(h.context.chats.some(item => item.id === 'unscoped'), false);
  assert.deepEqual(h.notes, [undefined]);
});

test('unknown profile and blocked storage are reported without loading unrelated history', async () => {
  for (const options of [{ profile: 'invalid' }, { broken: true }]) {
    const h = harness(options), pending = h.context.loadProfileHistory(); h.resolve(); await pending;
    assert.equal(h.context.currentChat.id, 'current');
    assert.equal(h.rendered.length, 0);
    assert.match(h.notes.at(-1), /unavailable|could not be read/);
  }
});
