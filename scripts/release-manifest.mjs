// The signed byte format is intentionally readable without a JSON/runtime dependency.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const platforms = ['linux-x86_64', 'linux-aarch64', 'macos-aarch64', 'macos-x86_64', 'windows-x86_64'];
export const repository = 'props-nothing/jarvis-core';

export function manifest(tag, commit, files) {
  assert.match(commit, /^[0-9a-f]{40}$/);
  assert.ok(tag === `ci-${commit}` || /^v\d+\.\d+\.\d+$/.test(tag), 'invalid release tag');
  assert.equal(files.length, platforms.length, 'exactly five archives required');
  let version;
  const rows = platforms.map(platform => {
    const extension = platform.startsWith('windows-') ? 'zip' : 'tar.gz';
    const candidates = files.filter(file => path.basename(file).endsWith(`-${platform}.${extension}`));
    assert.equal(candidates.length, 1, `missing or duplicate ${platform}`);
    const file = candidates[0], name = path.basename(file);
    const match = name.match(/^jarvis-(\d+\.\d+\.\d+)-/);
    assert.ok(match, 'invalid package version');
    version ??= match[1];
    assert.equal(match[1], version, 'mixed package versions');
    assert.equal(name, `jarvis-${version}-${platform}.${extension}`);
    const bytes = fs.readFileSync(file);
    assert.ok(bytes.length > 0 && bytes.length <= 268435456, 'archive size outside installer limit');
    return [platform, name, bytes.length, createHash('sha256').update(bytes).digest('hex')].join('\t');
  });
  if (tag.startsWith('v')) assert.equal(tag, `v${version}`, 'tag differs from package version');
  return ['JARVIS-RELEASE-1', `repository=${repository}`, `tag=${tag}`, `commit=${commit}`, `version=${version}`, ...rows].join('\n') + '\n';
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [output, tag, commit, ...files] = process.argv.slice(2);
  assert.ok(output, 'usage: release-manifest.mjs OUTPUT TAG COMMIT ARCHIVES...');
  fs.writeFileSync(output, manifest(tag, commit, files), { flag: 'wx' });
}
