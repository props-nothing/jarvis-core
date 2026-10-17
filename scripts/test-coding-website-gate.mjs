import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdir, mkdtemp, readFile, realpath, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { disposableProfile } from './coding-website-profile.mjs';

const gate = fileURLToPath(new URL('./accept-coding-website.mjs', import.meta.url));
const run = args => spawnSync(process.execPath, [gate, ...args], {encoding:'utf8', timeout:10_000, windowsHide:true});

test('ordinary invocation refuses before any live account work', () => {
  const result = run([]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Pass --run-live explicitly/);
});

test('live opt-in still requires an explicit verifier', () => {
  const result = run(['--run-live']);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Pass --playwright/);
});

test('wrong browser version is rejected before importing verifier code', async t => {
  const folder = await mkdtemp(join(tmpdir(), 'jarvis-verifier-control-'));
  t.after(async () => {
    assert.equal(dirname(folder), resolve(tmpdir()));
    assert(basename(folder).startsWith('jarvis-verifier-control-'));
    await rm(folder, {recursive:true, force:true});
  });
  const marker = join(folder, 'imported.txt');
  await writeFile(join(folder, 'package.json'), '{"version":"0.0.0"}');
  const entry = join(folder, 'index.mjs');
  await writeFile(entry, `import {writeFileSync} from 'node:fs'; writeFileSync(${JSON.stringify(marker)},'executed');`);
  const result = run(['--run-live','--playwright',entry]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /pinned browser verifier/);
  await assert.rejects(readFile(marker), {code:'ENOENT'});
});

async function profileFixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'jarvis-profile-control-'));
  t.after(async () => {
    assert.equal(dirname(root), resolve(tmpdir()));
    assert(basename(root).startsWith('jarvis-profile-control-'));
    await rm(root, {recursive:true, force:true});
  });
  const evidence = join(root, 'evidence');
  await mkdir(evidence);
  const valid = join(evidence, 'run-valid');
  const outside = join(root, 'run-owner');
  await mkdir(valid); await mkdir(outside);
  return {root, evidence, valid, outside};
}

test('continuation accepts only direct disposable profiles, never owner or nested folders', async t => {
  const {evidence, valid, outside} = await profileFixture(t);
  assert.equal(await disposableProfile(evidence, valid), await realpath(valid));
  await assert.rejects(disposableProfile(evidence, outside), /disposable profiles/);
  const nested = join(valid, 'run-nested');
  await mkdir(nested);
  await assert.rejects(disposableProfile(evidence, nested), /disposable profiles/);
  const ordinary = join(evidence, 'owner');
  await mkdir(ordinary);
  await assert.rejects(disposableProfile(evidence, ordinary), /not a disposable/);
});

test('an evidence-directory link cannot redirect continuation to an owner profile', async t => {
  const {evidence, outside} = await profileFixture(t);
  const link = join(evidence, 'run-linked');
  await symlink(outside, link, process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(disposableProfile(evidence, link), /disposable profiles/);
});
