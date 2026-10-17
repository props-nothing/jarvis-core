// End-to-end installation from public assets with a fresh home and restricted PATH.
// The runner has development tools; the installer/program cannot discover them.
// This is fresh-profile acceptance, not a clean VM or live-provider onboarding claim.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [tag, version, explicitHistoricalDigest] = process.argv.slice(2);
assert.match(tag ?? '', /^(ci-[0-9a-f]{40}|v\d+\.\d+\.\d+)$/);
assert.match(version ?? '', /^\d+\.\d+\.\d+$/);
if (explicitHistoricalDigest) assert.match(explicitHistoricalDigest, /^[0-9a-f]{64}$/);
const root = fileURLToPath(new URL('../', import.meta.url));
const tempRoot = path.resolve(os.tmpdir());
const dir = fs.mkdtempSync(path.join(tempRoot, 'jarvis-published-'));
const home = path.join(dir, 'home'), bin = path.join(dir, 'system-tools'), destination = path.join(dir, 'installed');
for (const directory of [home, bin]) fs.mkdirSync(directory);
const windows = process.platform === 'win32';
const env = { HOME: home, USERPROFILE: home, TMPDIR: dir, TEMP: dir, TMP: dir,
  XDG_CONFIG_HOME: path.join(home, 'config'), XDG_DATA_HOME: path.join(home, 'data'),
  XDG_STATE_HOME: path.join(home, 'state'), XDG_CACHE_HOME: path.join(home, 'cache') };
let shell, installArgs;
function run(program, args, timeout = 30000) {
  const result = spawnSync(program, args, { cwd: dir, env, encoding: 'utf8', windowsHide: true, timeout, maxBuffer: 1024 * 1024, input: '' });
  assert.ifError(result.error);
  return result;
}
try {
  if (windows) {
    env.SystemRoot = process.env.SystemRoot;
    env.WINDIR = process.env.SystemRoot;
    env.LOCALAPPDATA = path.join(home, 'local');
    env.APPDATA = path.join(home, 'roaming');
    env.PROCESSOR_ARCHITECTURE = process.env.PROCESSOR_ARCHITECTURE;
    if (process.env.PROCESSOR_ARCHITEW6432) env.PROCESSOR_ARCHITEW6432 = process.env.PROCESSOR_ARCHITEW6432;
    const system = path.join(env.SystemRoot, 'System32');
    env.PATHEXT = '.COM;.EXE;.BAT;.CMD';
    env.ComSpec = path.join(system, 'cmd.exe');
    env.PATH = [system, path.join(system, 'WindowsPowerShell', 'v1.0')].join(path.delimiter);
    shell = path.join(system, 'WindowsPowerShell', 'v1.0', 'powershell.exe');
    installArgs = ['-NoProfile', '-NonInteractive', '-File', path.join(root, 'scripts', 'install-prebuilt.ps1'),
      '-Tag', tag, '-Version', version, '-Dir', destination, '-NoPath'];
    if (explicitHistoricalDigest) installArgs.push('-Sha256', explicitHistoricalDigest);
  } else {
    // Only OS bootstrap utilities are reachable; no compiler or scripting runtime.
    for (const tool of ['bash', 'env', 'uname', 'curl', 'tar', 'gzip', 'head', 'wc', 'chmod', 'mktemp', 'tr', 'mkdir', 'rm', 'sha256sum', 'shasum']) {
      const resolved = spawnSync('/bin/sh', ['-c', 'command -v "$1"', 'sh', tool], { encoding: 'utf8', windowsHide: true, env: { PATH: '/usr/bin:/bin' } });
      if (resolved.status === 0) fs.symlinkSync(resolved.stdout.trim(), path.join(bin, tool));
      else assert.ok(['sha256sum', 'shasum'].includes(tool), `missing OS utility: ${tool}`);
    }
    env.PATH = bin;
    shell = path.join(bin, 'bash');
    installArgs = [path.join(root, 'scripts', 'install-prebuilt.sh'), tag, version];
    if (explicitHistoricalDigest) installArgs.push(explicitHistoricalDigest);
    installArgs.push('--dir', destination, '--no-path');
  }
  const probe = windows
    ? run(shell, ['-NoProfile', '-NonInteractive', '-Command', "foreach ($tool in @('git','rustc','cargo','node','python','python3','gh')) { if (Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue) { throw ('Development tool visible: ' + $tool) } }"])
    : run(shell, ['-c', 'for tool in git rustc cargo node python python3 gh; do if command -v "$tool"; then exit 1; fi; done; exit 0']);
  assert.equal(probe.status, 0, probe.stdout + probe.stderr);
  const installed = run(shell, installArgs, 600000);
  assert.equal(installed.status, 0, installed.stdout + installed.stderr);
  assert.match(installed.stdout, /Setup has not run/);
  const program = path.join(destination, windows ? 'jarvis.exe' : 'jarvis');
  const before = createHash('sha256').update(fs.readFileSync(program)).digest('hex');
  for (const args of [['version'], ['daemon', '--version']]) {
    const result = run(program, args);
    assert.equal(result.status, 0, result.stderr);
    assert.ok(result.stdout.includes(version), result.stdout);
  }
  const profile = path.join(dir, 'fresh-profile');
  const doctor = run(program, ['doctor', '--json', '--root', profile]);
  assert.notEqual(doctor.status, 0, 'fresh unconfigured profile must not be reported healthy');
  const report = JSON.parse(doctor.stdout);
  assert.ok(report.checks.length > 0, 'doctor returns actionable findings without a toolchain');
  assert.match(doctor.stdout, /config|configuration/);
  assert.equal(fs.existsSync(path.join(profile, 'config', 'jarvis.toml')), false, 'diagnosis does not perform onboarding');
  assert.equal(fs.existsSync(path.join(profile, 'data', 'jarvis.sqlite3')), false, 'diagnosis does not create canonical data');
  const repeated = run(shell, installArgs);
  assert.notEqual(repeated.status, 0, 'first-install bootstrap must not replace an installation');
  assert.match(repeated.stderr, /already exists/);
  assert.equal(createHash('sha256').update(fs.readFileSync(program)).digest('hex'), before);
  console.log(`Public ${explicitHistoricalDigest ? 'historical checksum' : 'signed'} installation passed on ${process.platform}/${process.arch}: versions, fresh-profile diagnosis and existing-install preservation; no Git/Rust/Node/Python/gh visible.`);
} catch (error) {
  console.error(error);
  throw error;
} finally {
  assert.equal(path.dirname(path.resolve(dir)), tempRoot);
  assert.ok(path.basename(dir).startsWith('jarvis-published-'));
  fs.rmSync(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
