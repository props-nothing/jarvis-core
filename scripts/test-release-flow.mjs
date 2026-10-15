import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { releaseSource } from './release-policy.mjs';

const workflow = fs.readFileSync(new URL('../.github/workflows/release.yml', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const ci = fs.readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8').replace(/\r\n/g, '\n');
const context = { repository: 'owner/repository', sha: 'a'.repeat(40), ref: 'refs/heads/main' };
const testedSha = 'b'.repeat(40);
function completedRun(overrides = {}) {
  return { workflow_run: { name: 'CI', status: 'completed', conclusion: 'success', event: 'push',
    head_branch: 'main', head_repository: { full_name: context.repository }, head_sha: testedSha, ...overrides } };
}

test('automatic archives build the tested SHA, not the newer default branch tip', () => {
  assert.equal(releaseSource('workflow_run', completedRun(), context), testedSha);
});
for (const [name, overrides] of [
  ['failed CI', { conclusion: 'failure' }],
  ['cancelled CI', { conclusion: 'cancelled' }],
  ['skipped CI', { conclusion: 'skipped' }],
  ['unfinished CI', { status: 'in_progress' }],
  ['another workflow', { name: 'Other' }],
  ['pull requests', { event: 'pull_request' }],
  ['manual CI runs', { event: 'workflow_dispatch' }],
  ['other branches', { head_branch: 'feature' }],
  ['forks', { head_repository: { full_name: 'foreign/repository' } }],
  ['missing repository', { head_repository: null }],
  ['missing SHA', { head_sha: null }],
  ['non-SHA refs', { head_sha: 'main' }],
  ['injected SHA', { head_sha: `${testedSha}\nready=true` }],
]) {
  test(`source selection fails closed for ${name}`, () => {
    assert.equal(releaseSource('workflow_run', completedRun(overrides), context), null);
  });
}

test('manual and version-tag runs select their exact commit; branch pushes and other events do not', () => {
  assert.equal(releaseSource('workflow_dispatch', {}, context), context.sha);
  assert.equal(releaseSource('push', {}, { ...context, ref: 'refs/tags/v0.1.0' }), context.sha);
  assert.equal(releaseSource('push', {}, context), null);
  assert.equal(releaseSource('pull_request', {}, context), null);
  assert.equal(releaseSource('workflow_run', {}, context), null);
  assert.equal(releaseSource('workflow_dispatch', {}, { ...context, sha: '' }), null);
});

test('the runner entry point writes only an eligible SHA to GitHub outputs', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-release-policy-'));
  try {
    const eventPath = path.join(dir, 'event.json'), output = path.join(dir, 'output.txt');
    for (const [event, expected] of [[completedRun(), `ready=true\nsha=${testedSha}\n`],
      [completedRun({ conclusion: 'failure' }), 'ready=false\nsha=\n']]) {
      fs.writeFileSync(eventPath, JSON.stringify(event));
      fs.writeFileSync(output, '');
      const result = spawnSync(process.execPath, [fileURLToPath(new URL('./release-policy.mjs', import.meta.url))], {
        encoding: 'utf8', env: { ...process.env, GITHUB_EVENT_NAME: 'workflow_run', GITHUB_EVENT_PATH: eventPath,
          GITHUB_OUTPUT: output, GITHUB_REPOSITORY: context.repository, GITHUB_SHA: context.sha, GITHUB_REF: context.ref },
      });
      assert.equal(result.status, 0, result.stderr);
      assert.equal(fs.readFileSync(output, 'utf8'), expected);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

function job(name) {
  const match = workflow.match(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z-]+:|$(?![\\s\\S]))`, 'm'));
  assert.ok(match, `missing job: ${name}`);
  return match[1];
}

test('workflow wiring gates all packaging paths and keeps builds read-only', () => {
  assert.match(workflow, /workflow_run:\n    workflows: \[CI\]\n    types: \[completed\]\n    branches: \[main\]/);
  assert.match(job('source'), /run: node scripts\/release-policy\.mjs/);
  assert.match(ci, /  workflow_call:/);
  assert.match(job('quality'), /needs: source/);
  assert.match(job('quality'), /needs\.source\.outputs\.ready == 'true' && github\.event_name != 'workflow_run'/);
  assert.match(job('quality'), /uses: \.\/\.github\/workflows\/ci.yml/);
  assert.match(job('package'), /needs: \[source, quality\]/);
  assert.match(job('package'), /needs\.source\.outputs\.ready == 'true'/);
  assert.match(job('package'), /needs\.quality\.result == 'success' \|\| \(github\.event_name == 'workflow_run' && needs\.quality\.result == 'skipped'\)/);
  assert.match(job('package'), /ref: \$\{\{ needs\.source\.outputs\.sha \}\}/);
  assert.match(job('package'), /contents: read/);
  assert.match(job('package'), /overwrite: true/);
  assert.doesNotMatch(job('package'), /contents: write|gh release/);
  assert.match(job('draft'), /needs: \[source, package\]/);
  assert.match(job('draft'), /gh run download "\$GITHUB_RUN_ID"/);
  for (const line of (workflow + ci).split('\n').filter(line => /uses:/.test(line) && !line.includes('./'))) {
    assert.match(line, /@[0-9a-f]{40}(?:\s|$)/);
  }
});

const bash = process.platform === 'win32' ? 'C:\\Program Files\\Git\\bin\\bash.exe' : '/bin/bash';

for (const [command, output] of [['clippy', 'clippy.txt'], ['test', 'test.txt']]) {
  for (const exitCode of [0, 23]) {
    test(`CI ${command} streams diagnostics and preserves Cargo exit ${exitCode}`, () => {
      const match = ci.match(new RegExp(`^        shell: bash\\n(?:        #[^\\n]*\\n)*        run: (cargo ${command}[^\\n]+)$`, 'm'));
      assert.ok(match, `missing Bash CI ${command} command`);
      const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-ci-output-'));
      try {
        fs.mkdirSync(path.join(dir, 'bin'));
        fs.writeFileSync(path.join(dir, 'bin', 'cargo'), `#!/usr/bin/env bash
printf 'Cargo stdout diagnostic\\n'
printf 'Cargo stderr diagnostic\\n' >&2
exit ${exitCode}
`, { mode: 0o755 });
        const result = spawnSync(bash, ['-e', '-o', 'pipefail', '-c', 'export PATH="$PWD/bin:$PATH"\n' + match[1]], {
          cwd: dir, encoding: 'utf8',
        });
        assert.ifError(result.error);
        assert.equal(result.status, exitCode, result.stderr);
        const diagnostics = 'Cargo stdout diagnostic\nCargo stderr diagnostic\n';
        assert.equal(result.stdout.replace(/\r\n/g, '\n'), diagnostics);
        assert.equal(fs.readFileSync(path.join(dir, output), 'utf8').replace(/\r\n/g, '\n'), diagnostics);
      } finally { fs.rmSync(dir, { recursive: true, force: true }); }
    });
  }
}

const annotationScript = ci.split('      - name: Annotate test failures\n')[1]
  .split('        run: |\n')[1].split('\n      - name: ')[0]
  .split('\n').map(line => line.replace(/^          /, '')).join('\n');

for (const [name, kind, lines] of [
  ['stack overflow before the log tail', 'crash', [
    "thread 'xml_depth' (123) has overflowed its stack",
    'fatal runtime error: stack overflow, aborting',
    "  process didn't exit successfully: `test-harness` (signal: 6, SIGABRT: process abort signal)",
  ]],
  ['ordinary assertion failure', 'panic', [
    "thread 'activity' (123) panicked at fixture.rs:1:1:",
    'assertion `left == right` failed',
    '  left: 1',
    ' right: 2',
  ]],
]) {
  test(`CI annotates ${name} without treating successful test names as panics`, () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-ci-annotations-'));
    try {
      fs.writeFileSync(path.join(dir, 'test.txt'), [
        'test readiness::only_derived_items_are_free_of_an_assertion ... ok',
        'test fixture::failure ... FAILED', ...lines,
        ...Array(60).fill('test result: ok. 0 passed; 0 failed;'),
      ].join('\n') + '\n');
      const result = spawnSync(bash, ['-e', '-o', 'pipefail', '-c', annotationScript], { cwd: dir, encoding: 'utf8' });
      assert.ifError(result.error);
      assert.equal(result.status, 0, result.stderr);
      for (const line of lines) assert.ok(result.stdout.includes(`::error::${kind}: ${line}`), result.stdout);
      assert.doesNotMatch(result.stdout, /::error::panic: test readiness/);
    } finally { fs.rmSync(dir, { recursive: true, force: true }); }
  });
}

const draftScript = job('draft').split('      - name: Verify checksums and attach to a draft only\n')[1]
  .split('        run: |\n')[1].split('\n').map(line => line.replace(/^          /, '')).join('\n');

function attachDraft(state, damage) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-release-draft-'));
  try {
    fs.mkdirSync(path.join(dir, 'bin'));
    fs.writeFileSync(path.join(dir, 'bin', 'gh'), `#!/usr/bin/env bash
printf '%s\\n' "$*" >> "$CALL_LOG"
case "$1 $2" in
  'release view')
    [ "$FIXTURE_STATE" != missing ] || exit 1
    [ "$FIXTURE_STATE" != error ] || exit 2
    [ "$FIXTURE_STATE" != published ] || { echo false; exit 0; }
    echo true ;;
  'release create') [ "$FIXTURE_STATE" != error ] ;;
  'release upload') exit 0 ;;
  *) exit 1 ;;
esac
`, { mode: 0o755 });
    for (let i = 0; i < 5; i++) {
      if (damage === 'missing platform' && i === 0) continue;
      const target = path.join(dir, 'dist', `jarvis-platform-${i}`);
      fs.mkdirSync(target, { recursive: true });
      const name = `jarvis-platform-${i}.${i === 4 ? 'zip' : 'tar.gz'}`;
      fs.writeFileSync(path.join(target, name), 'fixture archive');
      if (!(damage === 'missing' && i === 0)) {
        const digest = createHash('sha256').update(damage === 'corrupt' && i === 0 ? 'different' : 'fixture archive').digest('hex');
        fs.writeFileSync(path.join(target, `${name}.sha256`), `${digest}  ${name}\n`);
      }
    }
    const result = spawnSync(bash, ['-e', '-o', 'pipefail', '-c', 'export PATH="$PWD/bin:$PATH"\n' + draftScript], {
      cwd: dir, encoding: 'utf8', env: { ...process.env, FIXTURE_STATE: state, CALL_LOG: 'calls.log',
        GITHUB_REF_NAME: 'v0.1.0', GITHUB_REPOSITORY: context.repository },
    });
    assert.ifError(result.error);
    return { ...result, calls: fs.existsSync(path.join(dir, 'calls.log')) ? fs.readFileSync(path.join(dir, 'calls.log'), 'utf8') : '' };
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

for (const state of ['missing', 'draft']) {
  test(`verified packages attach to a ${state} draft`, () => {
    const result = attachDraft(state);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.calls, /release upload v0.1.0/);
    if (state === 'missing') assert.match(result.calls, /release create .*--draft --prerelease --verify-tag/);
    else assert.doesNotMatch(result.calls, /release create/);
  });
}
test('published releases are never modified', () => {
  const result = attachDraft('published');
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.calls, /release create|release upload/);
});
for (const damage of ['missing', 'corrupt', 'missing platform']) {
  test(`${damage} package/checksum data prevent any release operation`, () => {
    const result = attachDraft('missing', damage);
    assert.notEqual(result.status, 0);
    assert.equal(result.calls, '');
  });
}
test('release lookup or creation failure cannot reach upload', () => {
  const result = attachDraft('error');
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.calls, /release upload/);
});
