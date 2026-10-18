import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('./accept-voice-settings.mjs', import.meta.url));
const invoke = args => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', timeout: 5000, windowsHide: true });
const designScript = fileURLToPath(new URL('./accept-voice-design.mjs', import.meta.url));
test('voice editor acceptance requires opt-in before filesystem or synthesis effects', () => {
  const result = invoke([]); assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --run-live/);
});

test('voice design acceptance requires opt-in, an absolute cache and a browser verifier', () => {
  for (const [args, message] of [[[], /Pass --run-live/], [['--run-live', '--cache', 'relative'], /Pass --cache/], [['--run-live', '--cache', resolve('unused')], /Pass --playwright/]]) {
    const result = spawnSync(process.execPath, [designScript, ...args], { encoding: 'utf8', timeout: 5000, windowsHide: true });
    assert.notEqual(result.status, 0); assert.match(result.stderr, message);
  }
});
test('voice editor acceptance requires an explicit cache and browser verifier', () => {
  for (const cache of ['', 'relative']) {
    const result = invoke(['--run-live', '--cache', cache]); assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --cache/);
  }
  const result = invoke(['--run-live', '--cache', resolve('unused')]); assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --playwright/);
});
