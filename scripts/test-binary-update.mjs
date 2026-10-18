// Real public signature staging and production CLI fail-closed checks.
// Requires a freshly built jarvis, no owner profile/credentials/service changes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const binary = path.resolve(process.argv[2] ?? `target/debug/jarvis${process.platform === 'win32' ? '.exe' : ''}`);
const tag = 'ci-70cfc6eecb4eb2c69e227fc4599e73234b4a630d';
const temp = path.resolve(os.tmpdir());
const root = fs.mkdtempSync(path.join(temp, 'jarvis-update-live-'));
const install = path.join(root, 'install'), profile = path.join(root, 'profile'), home = path.join(root, 'home');
for (const dir of [install, profile, home]) fs.mkdirSync(dir);
const target = path.join(install, path.basename(binary));
fs.copyFileSync(binary, target);
const env = { ...process.env, HOME: home, USERPROFILE: home, LOCALAPPDATA: path.join(home, 'local'), APPDATA: path.join(home, 'roaming'), XDG_CONFIG_HOME: path.join(home, 'config'), XDG_DATA_HOME: path.join(home, 'data'), XDG_STATE_HOME: path.join(home, 'state'), XDG_CACHE_HOME: path.join(home, 'cache') };
function run(program, args, timeout = 180000) {
  const result = spawnSync(program, args, { env, windowsHide: true, encoding: 'utf8', timeout, maxBuffer: 1024 * 1024, input: '' });
  assert.ifError(result.error);
  return result;
}
function digest(file) { return createHash('sha256').update(fs.readFileSync(file)).digest('hex'); }
try {
  const before = digest(target);
  const result = run(binary, ['update', 'stage', tag, '0.1.0', '--dir', install], 600000);
  assert.equal(result.status, 0, result.stderr);
  const { stage, updater } = JSON.parse(result.stdout);
  const stageDir = path.join(install, '.jarvis-update', stage.id);
  assert.equal(stage.tag, tag);
  assert.equal(digest(target), before, 'staging must preserve the installed binary');
  const activate = ['update', 'activate', stage.id, '--dir', install, '--root', profile];
  const manifest = path.join(stageDir, 'release-manifest.txt');
  const original = fs.readFileSync(manifest);
  fs.appendFileSync(manifest, 'tampered\n');
  const tampered = run(updater, activate);
  assert.notEqual(tampered.status, 0);
  assert.match(tampered.stderr, /signature verification/);
  assert.equal(digest(target), before);
  fs.writeFileSync(manifest, original);
  // This historical signed release predates the installation-lock protocol.
  // It may be staged, but cannot be activated or silently treated as compatible.
  const incompatible = run(updater, activate);
  assert.notEqual(incompatible.status, 0);
  assert.match(incompatible.stderr, /maintenance protocol probe failed/);
  assert.equal(digest(target), before);
  const journal = path.join(install, '.jarvis-update', 'journal.json');
  assert.equal(fs.existsSync(journal), false);
  assert.equal(fs.existsSync(path.join(profile, 'data', 'jarvis.sqlite3')), false);
  fs.writeFileSync(journal, '{}');
  const daemon = run(target, ['daemon', '--root', profile], 30000);
  assert.notEqual(daemon.status, 0, 'malformed update evidence must block daemon startup');
  const doctor = run(target, ['doctor', '--root', profile, '--json'], 30000);
  assert.match(doctor.stdout, /installation.update_unsettled/);
  assert.equal(digest(target), before);
  console.log(JSON.stringify({ platform: process.platform, tag, stageVerified: true, tamperedSignatureRefused: true, legacyProtocolRefused: true, startupRefused: true, doctorDiagnosed: true, installedBinaryPreserved: true }));
} finally {
  const resolved = path.resolve(root);
  assert.equal(path.dirname(resolved), temp);
  assert.match(path.basename(resolved), /^jarvis-update-live-/);
  fs.rmSync(resolved, { recursive: true, force: true });
}
