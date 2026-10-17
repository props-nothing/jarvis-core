import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { manifest, platforms } from './release-manifest.mjs';

const commit = 'a'.repeat(40);
function fixture(check) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-manifest-'));
  try {
    const files = platforms.map(platform => {
      const file = path.join(dir, `jarvis-0.1.0-${platform}.${platform.startsWith('windows-') ? 'zip' : 'tar.gz'}`);
      fs.writeFileSync(file, `artifact ${platform}`); return file;
    });
    check(files);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}
test('manifest binds tag, commit, version and every exact archive byte', () => fixture(files => {
  const lines = manifest(`ci-${commit}`, commit, [...files].reverse()).trimEnd().split('\n');
  assert.deepEqual(lines.slice(0, 5), ['JARVIS-RELEASE-1', 'repository=props-nothing/jarvis-core', `tag=ci-${commit}`, `commit=${commit}`, 'version=0.1.0']);
  for (const [i, platform] of platforms.entries()) {
    const payload = `artifact ${platform}`;
    assert.equal(lines[i + 5], [platform, path.basename(files[i]), Buffer.byteLength(payload), createHash('sha256').update(payload).digest('hex')].join('\t'));
  }
}));
test('missing, duplicate, foreign, empty, mixed-version and relabelled releases fail closed', () => fixture(files => {
  assert.throws(() => manifest(`ci-${commit}`, commit, files.slice(1)));
  assert.throws(() => manifest(`ci-${commit}`, commit, [files[0], ...files.slice(0, 4)]));
  assert.throws(() => manifest(`ci-${'b'.repeat(40)}`, commit, files));
  assert.throws(() => manifest('latest', commit, files));
  assert.throws(() => manifest('v0.2.0', commit, files));
  const renamed = files[0].replace('0.1.0', '0.2.0');
  fs.copyFileSync(files[0], renamed);
  assert.throws(() => manifest('v0.1.0', commit, [renamed, ...files.slice(1)]));
  fs.writeFileSync(files[0], '');
  assert.throws(() => manifest('v0.1.0', commit, files));
}));
