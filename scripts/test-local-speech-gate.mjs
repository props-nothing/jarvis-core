import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const harness = fileURLToPath(new URL('./accept-local-speech.mjs', import.meta.url));
const invoke = args => spawnSync(process.execPath, [harness, ...args], {
  encoding: 'utf8', timeout: 5000, windowsHide: true,
});

test('live speech acceptance requires explicit opt-in', () => {
  const result = invoke([]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Pass --run-live/);
});

test('live speech acceptance refuses external destinations and URL credentials before setup', () => {
  for (const server of ['https://api.openai.com/v1', 'http://192.0.2.1/v1',
    'http://user:secret@127.0.0.1/v1', 'http://127.0.0.1/v1?key=value']) {
    const result = invoke(['--run-live', '--server', server]);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /only sends fixed test text to literal loopback HTTP/);
  }
});

test('live speech acceptance requires an explicit browser verifier', () => {
  const result = invoke(['--run-live']);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Pass --playwright/);
});
