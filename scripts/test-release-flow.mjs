import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { releaseSource } from './release-policy.mjs';
import { manifest } from './release-manifest.mjs';

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
        encoding: 'utf8', windowsHide: true, env: { ...process.env, GITHUB_EVENT_NAME: 'workflow_run', GITHUB_EVENT_PATH: eventPath,
          GITHUB_OUTPUT: output, GITHUB_REPOSITORY: context.repository, GITHUB_SHA: context.sha, GITHUB_REF: context.ref },
      });
      assert.equal(result.status, 0, result.stderr);
      assert.equal(fs.readFileSync(output, 'utf8'), expected);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

function job(name, source = workflow) {
  const match = source.match(new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z-]+:|$(?![\\s\\S]))`, 'm'));
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
  assert.match(job('draft'), /group: release-publish-\$\{\{ needs\.source\.outputs\.sha \}\}\n      cancel-in-progress: false/);
  assert.match(job('draft'), /RELEASE_SHA: \$\{\{ needs\.source\.outputs\.sha \}\}/);
  assert.match(job('draft'), /gh run download "\$GITHUB_RUN_ID"/);
  assert.match(job('draft'), /id-token: write/);
  assert.doesNotMatch(job('package'), /id-token: write/);
  const install = job('verify-install');
  assert.match(install, /needs: \[source, draft\]/);
  assert.match(install, /needs\.draft\.result == 'success'/);
  assert.match(install, /ref: \$\{\{ needs\.source\.outputs\.sha \}\}/);
  assert.match(install, /os: \[ubuntu-latest, ubuntu-24.04-arm, macos-latest, windows-latest\]/);
  assert.match(install, /node scripts\/test-published-install.mjs "\$RELEASE_TAG" "\$PACKAGE_VERSION"/);
  assert.doesNotMatch(install, /id-token: write|contents: write/);
  for (const line of (workflow + ci).split('\n').filter(line => /uses:/.test(line) && !line.includes('./'))) {
    assert.match(line, /@[0-9a-f]{40}(?:\s|$)/);
  }
});

test('native test partitions cover every workspace package exactly once on every OS', () => {
  const root = new URL('../', import.meta.url);
  const manifest = fs.readFileSync(new URL('Cargo.toml', root), 'utf8');
  const paths = [...manifest.match(/members = \[([\s\S]*?)\]/)[1].matchAll(/"([^"]+)"/g)].map(match => match[1]);
  const packages = paths.map(member => fs.readFileSync(new URL(`${member}/Cargo.toml`, root), 'utf8').match(/^name = "([^"]+)"/m)[1]);
  const rows = [...job('rust-tests', ci).matchAll(/\{ os: ([\w-]+), suite: ([\w-]+), packages: '([^']+)' \}/g)];
  for (const os of ['ubuntu-latest', 'macos-latest', 'windows-latest']) {
    const selected = [];
    const nativeRows = rows.filter(row => row[1] === os);
    assert.ok(nativeRows.length, `no native tests for ${os}`);
    for (const row of nativeRows) {
      const args = row[3].split(' ');
      const excluded = args.flatMap((arg, i) => arg === '--exclude' ? [args[i + 1]] : []);
      const explicit = args.flatMap((arg, i) => arg === '-p' ? [args[i + 1]] : []);
      selected.push(...(args.includes('--workspace') ? packages.filter(name => !excluded.includes(name)) : explicit));
      for (const name of [...excluded, ...explicit]) assert.ok(packages.includes(name), `unknown package ${name}`);
    }
    assert.deepEqual(selected.sort(), [...packages].sort(), `${os}: missing or duplicate package coverage`);
    assert.equal(nativeRows.filter(row => ['all', 'remaining'].includes(row[2])).length, 1, `${os}: exactly one native acceptance lane`);
  }
});

test('job separation retains native gates, cold-build validation and timeout headroom', () => {
  assert.match(ci, /push:\n    branches: \[main\]/);
  assert.match(job('source-checks', ci), /cargo fmt --check/);
  assert.match(job('source-checks', ci), /git diff --exit-code -- \.\.\/src\/hud\/generated/);
  for (const name of ['rust-lint', 'rust-tests']) {
    const body = job(name, ci);
    assert.match(body, /windows-latest' && 60 \|\| 30/);
    assert.match(body, /Swatinem\/rust-cache@[0-9a-f]{40}/);
    assert.match(body, /save-if: \$\{\{ github.event_name != 'pull_request' \}\}/);
    assert.doesNotMatch(body, /if:.*cache-hit/);
    assert.match(body, /name: Retain diagnostics\n        if: always\(\)/);
  }
  const tests = job('rust-tests', ci);
  const peer = tests.split('      - name: Build daemon test peer\n')[1].split('      - name: ')[0];
  assert.match(peer, /if: matrix.suite == 'daemon'/);
  assert.match(peer, /cargo build -p jarvis-mcp-transport --features fixture-peer --bin fixture-peer --locked/);
  assert.ok(tests.indexOf('name: Build daemon test peer') < tests.indexOf('name: Test workspace'));
  assert.match(tests.split('      - name: Test workspace\n')[1].split('      - name: ')[0], /ACCEPTANCE_REQUIRE_FIXTURE_PEER: "1"/);
  assert.match(tests, /cargo test .*--all-features --locked --no-fail-fast/);
  assert.equal([...tests.matchAll(/cargo build --workspace --all-features --locked/g)].length, 1);
  for (const phase of [1, 2, 3]) {
    const gate = tests.split(`      - name: Phase ${phase} acceptance gate\n`)[1].split('      - name: ')[0];
    assert.match(gate, /if: matrix.suite == 'all' \|\| matrix.suite == 'remaining'/);
    assert.match(gate, /ACCEPTANCE_REQUIRE_BINARIES: "1"/);
    assert.match(gate, /ACCEPTANCE_REQUIRE_FIXTURE_PEER: "1"/);
    assert.match(gate, new RegExp(`bash scripts/ci-run-gate.sh phase${phase}_gate`));
  }
  const codingFixture = tests.split('      - name: Build coding lifecycle agent fixture\n')[1].split('      - name: ')[0];
  assert.match(codingFixture, /if: matrix.suite == 'all' \|\| matrix.suite == 'remaining'/);
  assert.match(codingFixture, /cargo build -p jarvis-runtimes --example fake_acp_agent --locked/);
  const codingGate = tests.split('      - name: Coding lifecycle acceptance gate\n')[1].split('      - name: ')[0];
  assert.match(codingGate, /if: matrix.suite == 'all' \|\| matrix.suite == 'remaining'/);
  assert.match(codingGate, /ACCEPTANCE_REQUIRE_BINARIES: "1"/);
  assert.match(codingGate, /ACCEPTANCE_REQUIRE_FIXTURE_PEER: "1"/);
  assert.match(codingGate, /bash scripts\/ci-run-gate.sh coding_lifecycle/);
  assert.ok(tests.indexOf('name: Build current acceptance binaries once') < tests.indexOf('name: Coding lifecycle acceptance gate'));
  assert.ok(tests.indexOf('name: Build coding lifecycle agent fixture') < tests.indexOf('name: Coding lifecycle acceptance gate'));
  assert.match(job('package'), /key: release-\$\{\{ matrix.label \}\}/);
  assert.doesNotMatch(job('package'), /if:.*cache-hit/);
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
        const commandLine = match[1].replace('${{ matrix.packages }}', '--workspace');
        const result = spawnSync(bash, ['-e', '-o', 'pipefail', '-c', 'export PATH="$PWD/bin:$PATH"\n' + commandLine], {
          cwd: dir, encoding: 'utf8', windowsHide: true,
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

test('draft attachment requires a ready source, a successful complete matrix and no cancellation', () => {
  const condition = job('draft').match(/^    if: \$\{\{ (.*) \}\}$/m)?.[1];
  assert.ok(condition);
  // Exercise the production expression with failed/skipped/cancelled dependencies.
  const eligible = new Function('cancelled', 'needs', 'return ' + condition + ';');
  for (const ready of ['true', 'false', '']) {
    for (const result of ['success', 'failure', 'cancelled', 'skipped']) {
      for (const cancelled of [true, false]) {
        assert.equal(eligible(() => cancelled, { source: { outputs: { ready } }, package: { result } }),
          !cancelled && ready === 'true' && result === 'success', [ready, result, cancelled].join('/'));
      }
    }
  }
});

const draftScript = job('draft').split('      - name: Verify packages, upload and publish\n')[1]
  .split('        run: |\n')[1].split('\n').map(line => line.replace(/^          /, '')).join('\n');

function attachDraft(state, damage, overrides = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jarvis-release-draft-'));
  try {
    fs.mkdirSync(path.join(dir, 'bin'));
    fs.mkdirSync(path.join(dir, 'scripts'));
    fs.copyFileSync(new URL('./release-manifest.mjs', import.meta.url), path.join(dir, 'scripts', 'release-manifest.mjs'));
    fs.writeFileSync(path.join(dir, 'bin', 'cosign'), `#!/usr/bin/env bash
set -eu
printf '%s\\n' "$*" >> signing.log
case "$1" in
  sign-blob)
    [ "$FIXTURE_STATE" != 'sign error' ] || exit 19
    while [ "$1" != --bundle ]; do shift; done
    printf 'fixture bundle' > "$2" ;;
  verify-blob) [ "$FIXTURE_STATE" != 'verify error' ] ;;
  *) exit 20 ;;
esac
`, { mode: 0o755 });
    fs.writeFileSync(path.join(dir, 'bin', 'gh'), [
      '#!/usr/bin/env bash',
      'printf \'%s\\n\' "$*" >> "$CALL_LOG"',
      'case "$1 $2" in',
      '  \'api \'*)',
      '    [ "$FIXTURE_STATE" != "ref error" ] || exit 2',
      '    if [[ "$2" == */releases/* ]]; then',
      '      [ "$FIXTURE_STATE" != "metadata error" ] || exit 2',
      '      if [[ "$*" == *--jq* ]]; then',
      '        draft=false; [ "$FIXTURE_STATE" != "still draft" ] || draft=true',
      '        printf \'%s\\ttrue\\t%s\\t%s\\n\' "$draft" "$RELEASE_SHA" "$FIXTURE_TAG"',
      '      else cat metadata.json; touch metadata-read; fi',
      '    elif [[ "$2" == */commits/* ]]; then',
      '      if [ "$FIXTURE_STATE" = "wrong tag commit" ]; then echo "wrong"; else echo "$RELEASE_SHA"; fi',
      '    elif [ "$FIXTURE_STATE" = "existing tag" ] || { [ "$FIXTURE_STATE" = "tag race" ] && [ -f metadata-read ]; }; then',
      '      echo "refs/tags/ci-$RELEASE_SHA"',
      '    fi',
      '    exit 0 ;;',
      '  \'release view\')',
      '    if [[ "$*" == *databaseId* ]]; then echo 42; exit 0; fi',
      '    [ "$FIXTURE_STATE" != missing ] || exit 1',
      '    [ "$FIXTURE_STATE" != error ] || exit 2',
      '    [ "$FIXTURE_STATE" != published ] || { printf \'false\\t%s\\n\' "$RELEASE_SHA"; exit 0; }',
      '    [ "$FIXTURE_STATE" != "wrong target" ] || { printf \'true\\tmain\\n\'; exit 0; }',
      '    printf \'true\\t%s\\n\' "$RELEASE_SHA" ;;',
      '  \'release create\') [ "$FIXTURE_STATE" != error ] ;;',
      '  \'release upload\') [ "$FIXTURE_STATE" != "upload error" ] ;;',
      '  \'release edit\') [ "$FIXTURE_STATE" != "publish error" ] ;;',
      '  *) exit 1 ;;',
      'esac', '',
    ].join('\n'), { mode: 0o755 });
    const labels = ['linux-x86_64', 'linux-aarch64', 'macos-aarch64', 'macos-x86_64', 'windows-x86_64'];
    const assets = [];
    for (let i = 0; i < labels.length; i++) {
      if (damage === 'missing platform' && i === 0) continue;
      const label = damage === 'substituted platform' && i === 0 ? 'linux-unknown' : labels[i];
      const target = path.join(dir, 'dist', 'jarvis-' + label);
      fs.mkdirSync(target, { recursive: true });
      const name = 'jarvis-0.1.0-' + label + (i === 4 ? '.zip' : '.tar.gz');
      fs.writeFileSync(path.join(target, name), 'fixture archive');
      if (!(damage === 'missing' && i === 0)) {
        const digest = createHash('sha256').update(damage === 'corrupt' && i === 0 ? 'different' : 'fixture archive').digest('hex');
        fs.writeFileSync(path.join(target, name + '.sha256'), digest + '  ' + name + '\n');
      }
      for (const file of fs.readdirSync(target)) {
        const bytes = fs.readFileSync(path.join(target, file));
        assets.push({ name: file, state: 'uploaded', size: bytes.length, digest: 'sha256:' + createHash('sha256').update(bytes).digest('hex') });
      }
    }
    const tag = overrides.GITHUB_REF?.startsWith('refs/tags/v') ? overrides.GITHUB_REF_NAME : 'ci-' + testedSha;
    if (!damage) {
      const files = labels.map((label, i) => path.join(dir, 'dist', 'jarvis-' + label, 'jarvis-0.1.0-' + label + (i === 4 ? '.zip' : '.tar.gz')));
      for (const [name, value] of [['release-manifest.txt', manifest(tag, testedSha, files)], ['release-manifest.sigstore.json', 'fixture bundle']]) {
        const bytes = Buffer.from(value);
        assets.push({ name, state: 'uploaded', size: bytes.length, digest: 'sha256:' + createHash('sha256').update(bytes).digest('hex') });
      }
    }
    const metadata = { draft: true, prerelease: true, target_commitish: testedSha, tag_name: tag, assets };
    if (state === 'remote missing asset') assets.pop();
    if (state === 'remote extra asset') assets.push({ ...assets[0], name: 'unexpected' });
    if (state === 'remote duplicate asset') assets[0] = { ...assets[1] };
    if (state === 'remote digest mismatch') assets[0].digest = 'sha256:' + '0'.repeat(64);
    if (state === 'remote signature mismatch') assets.at(-1).digest = 'sha256:' + '0'.repeat(64);
    if (state === 'remote absent digest') delete assets[0].digest;
    if (state === 'remote size mismatch') assets[0].size++;
    if (state === 'remote incomplete asset') assets[0].state = 'starter';
    if (state === 'remote published') metadata.draft = false;
    if (state === 'remote stable') metadata.prerelease = false;
    if (state === 'remote wrong target') metadata.target_commitish = context.sha;
    if (state === 'remote wrong tag') metadata.tag_name = 'ci-' + context.sha;
    fs.writeFileSync(path.join(dir, 'metadata.json'), state === 'malformed metadata' ? '{' : JSON.stringify(metadata));
    const result = spawnSync(bash, ['-e', '-o', 'pipefail', '-c', 'export PATH="$PWD/bin:$PATH"\n' + draftScript], {
      cwd: dir, encoding: 'utf8', windowsHide: true, env: { ...process.env, FIXTURE_STATE: state, FIXTURE_TAG: tag, CALL_LOG: 'calls.log',
        GITHUB_REF: 'refs/heads/main', GITHUB_REF_NAME: 'main', GITHUB_REPOSITORY: context.repository,
        GITHUB_SHA: context.sha, RELEASE_SHA: testedSha, GITHUB_RUN_ID: '123', GITHUB_OUTPUT: 'outputs.txt',
        GITHUB_SERVER_URL: 'https://github.com', ...overrides },
    });
    assert.ifError(result.error);
    return { ...result, calls: fs.existsSync(path.join(dir, 'calls.log')) ? fs.readFileSync(path.join(dir, 'calls.log'), 'utf8') : '',
      signing: fs.existsSync(path.join(dir, 'signing.log')) ? fs.readFileSync(path.join(dir, 'signing.log'), 'utf8') : '' };
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

for (const state of ['missing', 'draft']) {
  test('verified packages attach to a ' + state + ' draft before automatic publication', () => {
    const result = attachDraft(state);
    assert.equal(result.status, 0, result.stderr);
    const verb = state === 'missing' ? 'create' : 'upload';
    const mutation = result.calls.split('\n').find(line => line.startsWith('release ' + verb + ' '));
    assert.ok(mutation?.startsWith('release ' + verb + ' ci-' + testedSha + ' '), result.calls);
    assert.equal(mutation.match(/dist\/jarvis-/g).length, 10, 'all five archives and five checksums attach together');
    assert.match(mutation, /dist\/release-manifest.txt dist\/release-manifest.sigstore.json/);
    assert.match(result.signing, /sign-blob --yes --timeout 2m --new-bundle-format --bundle/);
    assert.match(result.signing, /--certificate-identity https:\/\/github.com\/owner\/repository\/\.github\/workflows\/release.yml@refs\/heads\/main/);
    assert.match(result.signing, /--certificate-oidc-issuer https:\/\/token.actions.githubusercontent.com/);
    assert.ok(!result.calls.includes(context.sha), 'the newer default-branch SHA must not select the draft');
    if (state === 'missing') {
      assert.ok(mutation.includes('--draft --prerelease --target ' + testedSha));
      assert.ok(mutation.includes('https://github.com/' + context.repository + '/actions/runs/123'));
      assert.doesNotMatch(result.calls, /release upload|--verify-tag/);
    } else {
      assert.match(mutation, /--clobber/);
      assert.doesNotMatch(result.calls, /release create/);
    }
    const calls = result.calls.trim().split('\n');
    const publishIndex = calls.findIndex(line => line.startsWith('release edit '));
    assert.ok(publishIndex > calls.indexOf(mutation));
    assert.ok(calls.slice(0, publishIndex).includes('api repos/owner/repository/releases/42'), 'read uploaded asset metadata before publication');
    assert.ok(calls[publishIndex].includes('--draft=false --prerelease --latest=false --target ' + testedSha));
    assert.ok(calls.at(-1).includes('/commits/ci-' + testedSha), 'verify published tag target');
  });
  test('version-tag packages publish from a ' + state + ' draft after verifying the existing tag', () => {
    const result = attachDraft(state, undefined, { GITHUB_REF: 'refs/tags/v0.1.0', GITHUB_REF_NAME: 'v0.1.0' });
    assert.equal(result.status, 0, result.stderr);
    if (state === 'missing') assert.match(result.calls, /release create v0\.1\.0 .*--draft --prerelease --target [0-9a-f]{40} --verify-tag/);
    else assert.match(result.calls, /release upload v0\.1\.0/);
    assert.doesNotMatch(result.calls, /ci-[0-9a-f]{40}|matching-refs/m);
    assert.match(result.calls, /release edit v0\.1\.0 .*--verify-tag/);
    assert.ok(result.calls.indexOf('/commits/v0.1.0') < result.calls.indexOf('release edit '));
  });
}

test('manual feature branches cannot publish under the trusted main identity', () => {
  const result = attachDraft('missing', undefined, { GITHUB_REF: 'refs/heads/release-candidate', GITHUB_REF_NAME: 'release-candidate' });
  assert.notEqual(result.status, 0);
  assert.match(result.stdout, /Signed publication requires main or a version tag/);
  assert.equal(result.signing, '');
  assert.doesNotMatch(result.calls, /release create|release upload|release edit/);
});
for (const state of ['sign error', 'verify error']) {
  test(state + ' prevents any draft mutation or publication', () => {
    const result = attachDraft(state);
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.calls, /release create|release upload|release edit/);
  });
}
for (const state of ['published', 'wrong target', 'existing tag', 'ref error']) {
  test(state + ' prevents draft mutation', () => {
    const result = attachDraft(state);
    assert.notEqual(result.status, 0);
    assert.doesNotMatch(result.calls, /release create|release upload/);
  });
}
test('published version-tag releases are never modified', () => {
  const result = attachDraft('published', undefined, { GITHUB_REF: 'refs/tags/v0.1.0', GITHUB_REF_NAME: 'v0.1.0' });
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.calls, /release create|release upload/);
});
for (const damage of ['missing', 'corrupt', 'missing platform', 'substituted platform']) {
  test(damage + ' package/checksum data prevent any release operation', () => {
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

test('a failed upload remains a failed draft job', () => {
  const result = attachDraft('upload error');
  assert.notEqual(result.status, 0);
  assert.match(result.calls, /release upload/);
  assert.doesNotMatch(result.calls, /release create|release edit/);
});

test('invalid tested SHA prevents all GitHub operations', () => {
  const result = attachDraft('missing', undefined, { RELEASE_SHA: 'main' });
  assert.notEqual(result.status, 0);
  assert.equal(result.calls, '');
});

for (const state of ['remote missing asset', 'remote extra asset', 'remote duplicate asset',
  'remote digest mismatch', 'remote signature mismatch', 'remote absent digest', 'remote size mismatch', 'remote incomplete asset',
  'remote published', 'remote stable', 'remote wrong target', 'remote wrong tag',
  'malformed metadata', 'metadata error', 'tag race']) {
  test(state + ' prevents publication after upload', () => {
    const result = attachDraft(state);
    assert.notEqual(result.status, 0, result.stdout);
    assert.match(result.calls, /release upload/);
    assert.doesNotMatch(result.calls, /release edit/);
  });
}

test('a moved version tag prevents publication', () => {
  const result = attachDraft('wrong tag commit', undefined, { GITHUB_REF: 'refs/tags/v0.1.0', GITHUB_REF_NAME: 'v0.1.0' });
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.calls, /release edit/);
});

for (const state of ['publish error', 'still draft', 'wrong tag commit']) {
  test(state + ' fails the publication job', () => {
    const result = attachDraft(state);
    assert.notEqual(result.status, 0);
    assert.equal(result.calls.match(/release edit/g)?.length, 1, 'never retry an uncertain publish mutation');
  });
}
