import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';

const source = readFileSync(new URL('../apps/jarvisd/src/hud/hud.js', import.meta.url), 'utf8');
function harness() {
  const calls = [], message = {}, renders = [];
  const context = { token: 'synthetic', S: { offline: true }, Date, Promise, document: { hidden: false },
    $(id) { assert.equal(id, 'message'); return message; },
    api(path) { return new Promise((resolve, reject) => { calls.push({ path, resolve, reject }); }); },
    renderPanels(...args) { renders.push(args); }, announce() {}, observeRuns() {}, watchSchedules() {},
    refreshAbilities() {}, refreshProposals() {}, loadDigest() {}, loadContacts() {}, loadProjects() {}, tickAges() {}, paintHeader() {},
    digestAt: Date.now(), contactsAt: Date.now(), projectsAt: Date.now(), setTimeout() {} };
  vm.runInNewContext(source.slice(source.indexOf('  var lastSignature ='), source.indexOf('  // ---- the face')), context);
  function respond(name, value, error) {
    const request = calls.filter(call => call.path.startsWith('/' + name)).at(-1);
    if(error) request.reject(error); else request.resolve(value);
  }
  return { context, calls, message, renders, respond };
}

test('optional panel failure preserves conversation availability and shows stale-data disclosure', async () => {
  const h = harness(), pending = h.context.refresh();
  assert.equal(h.context.refresh(), pending);
  assert.equal(h.calls.length, 4);
  h.respond('runs', { runs: [] }); h.respond('approvals', { approvals: [] });
  h.respond('schedules', null, { status: 503 }); h.respond('triggers', { triggers: [] });
  await pending;
  assert.equal(h.context.S.offline, false);
  assert.match(h.message.textContent, /schedules.*last available data/);
  assert.equal(h.renders.length, 1);
});

test('authentication failure on an optional endpoint still fails closed', async () => {
  const h = harness(), pending = h.context.refresh();
  h.respond('runs', { runs: [] }); h.respond('approvals', { approvals: [] });
  h.respond('schedules', null, { status: 401 }); h.respond('triggers', { triggers: [] });
  await pending;
  assert.equal(h.context.S.offline, true);
  assert.equal(h.renders.length, 0);
  assert.match(h.message.textContent, /rejected the credential/);
});

test('failed core request does not release the polling guard while other requests are outstanding', async () => {
  const h = harness(), pending = h.context.refresh();
  h.respond('runs', null, { status: 503 });
  await Promise.resolve(); await Promise.resolve();
  assert.equal(h.context.refresh(), pending);
  assert.equal(h.calls.length, 4);
  h.respond('approvals', { approvals: [] }); h.respond('schedules', { schedules: [] }); h.respond('triggers', { triggers: [] });
  await pending;
  assert.equal(h.context.S.offline, true);
  assert.equal(h.context.pollFailures, 1);
});
