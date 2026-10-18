// Negative controls for the OS-shell drivers, run on developer CI only.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

test('Unix clean driver refuses a developer host before installation or provider calls', () => {
  const report = path.join(os.tmpdir(), `clean-refusal-${Date.now()}`);
  const bash = process.platform === 'win32' ? path.join(process.env.ProgramFiles, 'Git', 'bin', 'bash.exe') : 'bash';
  const result = spawnSync(bash, ['scripts/test-clean-lifecycle.sh', '--disposable',
    `ci-${'a'.repeat(40)}`, `ci-${'b'.repeat(40)}`, '0.1.0', '0.1.0', report,
    'synthetic-model', 'https://example.invalid/v1', '/missing-model-key', '/missing-voice-key'],
  { encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Clean-machine prerequisite failed:/);
  assert.equal(fs.existsSync(report), false);
});

test('Windows clean driver refuses a developer host before installation or provider calls', { skip: process.platform !== 'win32' }, () => {
  const report = path.join(os.tmpdir(), `clean-refusal-${Date.now()}`);
  const result = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-File',
    'scripts/test-clean-lifecycle.ps1', '-Disposable', '-ReportDir', report],
  { encoding: 'utf8', timeout: 30000, windowsHide: true });
  assert.ifError(result.error);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Clean-machine prerequisite failed:/);
  assert.equal(fs.existsSync(report), false);
});
