import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const harness = fileURLToPath(new URL('./accept-embedded-speech.mjs', import.meta.url));
const invoke = args => spawnSync(process.execPath, [harness, ...args], { encoding: 'utf8', timeout: 5000, windowsHide: true });
test('embedded acceptance needs opt-in before any filesystem effects', () => {
  const result = invoke([]); assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --run-live/);
});
test('embedded acceptance needs an explicit absolute prepared cache', () => {
  for (const args of [[], ['--cache', 'relative']]) {
    const result = invoke(['--run-live', ...args]); assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --cache/);
  }
});
test('embedded acceptance needs an explicit browser verifier before creating a profile', () => {
  const result = invoke(['--run-live', '--cache', resolve('unused')]);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /Pass --playwright/);
});
