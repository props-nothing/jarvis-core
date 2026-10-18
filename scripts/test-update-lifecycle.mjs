// Real signed update + authenticated daemon readiness. Native service mutation is
// restricted to disposable hosted runners; the default is an isolated profile.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const [sourceArg, tag, version, mode] = process.argv.slice(2);
assert.match(tag ?? '', /^(ci-[0-9a-f]{40}|v\d+\.\d+\.\d+)$/);
assert.match(version ?? '', /^\d+\.\d+\.\d+$/);
assert.ok(mode === undefined || mode === '--native-service');
const native = mode === '--native-service';
if (native) {
  assert.equal(process.env.GITHUB_ACTIONS, 'true', 'native service tests require a disposable hosted runner');
  assert.equal(process.env.RUNNER_ENVIRONMENT, 'github-hosted');
}
const windows = process.platform === 'win32', mac = process.platform === 'darwin';
const source = path.resolve(sourceArg);
const temp = fs.realpathSync(mac ? '/tmp' : os.tmpdir());
const root = fs.mkdtempSync(path.join(temp, 'ju-'));
const install = path.join(root, 'installed bin'), profile = path.join(root, 'profile');
const home = native ? os.homedir() : path.join(root, 'home');
for (const directory of [install, profile, home]) fs.mkdirSync(directory, { recursive: true });
const target = path.join(install, windows ? 'jarvis.exe' : 'jarvis');
fs.copyFileSync(source, target);
if (!windows) fs.chmodSync(target, 0o700);
const env = { ...process.env };
// No real provider or ambient JARVIS override can enter this disposable profile.
for (const key of Object.keys(env)) if (key.startsWith('JARVIS_')) delete env[key];
if (!native) Object.assign(env, { HOME: home, USERPROFILE: home,
  APPDATA: path.join(home, 'roaming'), LOCALAPPDATA: path.join(home, 'local'),
  XDG_CONFIG_HOME: path.join(home, 'config'), XDG_DATA_HOME: path.join(home, 'data'),
  XDG_STATE_HOME: path.join(home, 'state'), XDG_CACHE_HOME: path.join(home, 'cache') });
const scoped = native ? [] : ['--root', profile];
const configDir = !native ? path.join(profile, 'config') : windows ? path.join(env.APPDATA, 'JARVIS')
  : mac ? path.join(home, 'Library', 'Application Support', 'JARVIS') : path.join(env.XDG_CONFIG_HOME ?? path.join(home, '.config'), 'jarvis');
const dataDir = !native ? path.join(profile, 'data') : windows ? path.join(env.LOCALAPPDATA, 'JARVIS')
  : mac ? configDir : path.join(env.XDG_DATA_HOME ?? path.join(home, '.local', 'share'), 'jarvis');
const database = path.join(dataDir, 'jarvis.sqlite3');
let child, service, serviceOwned = false, configured = false;
const lifecycleIds = new Set();
function run(program, args, timeout = 30000, allowTimeout = false) {
  const result = spawnSync(program, args, { env, cwd: root, windowsHide: true,
    encoding: 'utf8', timeout, maxBuffer: 1024 * 1024, input: '' });
  if (!(allowTimeout && result.error?.code === 'ETIMEDOUT')) assert.ifError(result.error);
  return result;
}
function ok(program, args, timeout) {
  const result = run(program, args, timeout);
  assert.equal(result.status, 0, `${path.basename(program)} ${args[0]}: ${result.stderr}\n${result.stdout}`);
  return result.stdout;
}
function cli(args) { return ok(target, [...args, ...scoped]); }
function hash(file) { return createHash('sha256').update(fs.readFileSync(file)).digest('hex'); }
function samePath(a, b) { return fs.realpathSync.native(a).replace(/^\\\\\?\\/, '').toLowerCase() === fs.realpathSync.native(b).replace(/^\\\\\?\\/, '').toLowerCase(); }
const registry = String.raw`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`;
function ps(script) { return ok('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script]); }
async function readiness() {
  const deadline = Date.now() + 45000;
  let last;
  while (Date.now() < deadline) {
    last = run(target, ['status', '--json', ...scoped], 5000, true);
    if (last.status === 0) {
      const status = JSON.parse(last.stdout);
      if (status.ready && status.live) {
        const contract = JSON.parse(ok(target, ['update', 'probe']));
        assert.equal(status.daemon_version, contract.version);
        assert.equal(status.protocol_version, contract.compatibility.protocol);
        assert.equal(status.config_schema, contract.compatibility.configuration);
        assert.equal(status.database_schema, contract.compatibility.database);
        const health = JSON.parse(cli(['health', '--json']));
        assert.equal(health.live, true); assert.equal(health.ready, true);
        assert.ok(!lifecycleIds.has(status.daemon_id), 'a new startup must have a new daemon identity');
        lifecycleIds.add(status.daemon_id);
        return status;
      }
    }
    await delay(200);
  }
  throw new Error(`daemon did not become ready: ${last?.stderr}`);
}
async function start() {
  if (!native) {
    const log = fs.openSync(path.join(root, 'daemon.txt'), 'a');
    child = spawn(target, ['daemon', ...scoped], { env, cwd: root, windowsHide: true, stdio: ['ignore', log, log] });
    fs.closeSync(log);
    child.on('error', error => { console.error(error.message); });
  } else if (windows) {
    // Execute precisely the persisted command, as Run would, without logging off.
    ps("$entry = (Get-ItemProperty -LiteralPath 'HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' -Name JARVIS).JARVIS; & $env:ComSpec /d /c $entry; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }");
  } else if (mac) ok('launchctl', ['start', 'dev.jarvis.jarvisd']);
  else ok('systemctl', ['--user', 'start', 'jarvisd.service']);
  await readiness();
  if (native) verifyManager();
}
function verifyManager() {
  assert.equal(JSON.parse(ok(target, ['service', '--json'])).drift, 'current');
  if (windows) {
    // CIM expands short (8.3) paths; compare resolved paths, not their spelling.
    // -InputObject also preserves [] when no process matches, for a useful failure.
    const processes = JSON.parse(ps("ConvertTo-Json -Compress -InputObject @(Get-CimInstance Win32_Process -Filter \"Name = 'jarvis.exe'\" | Where-Object { $_.ExecutablePath -and $_.CommandLine -match 'daemon' } | Select-Object ProcessId,ExecutablePath)"));
    const list = processes.filter(process => samePath(process.ExecutablePath, target));
    assert.equal(list.length, 1); assert.ok(samePath(list[0].ExecutablePath, target));
    assert.ok(ok('reg', ['query', registry, '/v', 'JARVIS']).includes('Start-Process'));
  } else {
    let pid;
    if (mac) {
      const line = ok('launchctl', ['list']).split('\n').find(line => line.trim().endsWith('\tdev.jarvis.jarvisd'));
      assert.ok(line, 'launchd job must be loaded'); pid = line.trim().split(/\s+/)[0];
    } else {
      ok('systemctl', ['--user', 'is-active', '--quiet', 'jarvisd.service']);
      pid = ok('systemctl', ['--user', 'show', 'jarvisd.service', '--property=MainPID', '--value']).trim();
    }
    assert.match(pid, /^[1-9][0-9]*$/);
    const command = ok('ps', ['-p', pid, '-o', 'command=']);
    assert.ok(command.includes(target) && command.includes('daemon'), command);
  }
}
async function stop() {
  if (!configured) return;
  cli(['stop']);
  if (native && !windows && !mac) ok('systemctl', ['--user', 'stop', 'jarvisd.service']);
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    const missingSidecars = ['-wal', '-shm', '-journal'].every(suffix => !fs.existsSync(database + suffix));
    if ((!child || child.exitCode !== null) && missingSidecars) { child = undefined; return; }
    await delay(100);
  }
  throw new Error('daemon failed to close database cleanly');
}
function writeSentinel(value) {
  const db = new DatabaseSync(database);
  try {
    db.exec('CREATE TABLE IF NOT EXISTS update_acceptance (id INTEGER PRIMARY KEY, value TEXT NOT NULL)');
    db.prepare('INSERT OR REPLACE INTO update_acceptance VALUES (1, ?)').run(value);
    db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  } finally { db.close(); }
}
function assertSentinel(value) {
  const db = new DatabaseSync(database, { readOnly: true });
  try { assert.equal(db.prepare('SELECT value FROM update_acceptance WHERE id=1').get().value, value); }
  finally { db.close(); }
}
try {
  assert.equal(fs.existsSync(configDir), false, 'refuse an existing profile');
  assert.equal(fs.existsSync(dataDir), false, 'refuse existing canonical data');
  service = JSON.parse(ok(target, ['service', '--json']));
  assert.equal(service.drift, 'absent', 'refuse an existing service definition');
  if (native && windows) assert.notEqual(run('reg', ['query', registry, '/v', 'JARVIS']).status, 0, 'refuse an existing login launcher');
  const listener = net.createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  fs.mkdirSync(configDir, { recursive: true });
  // These abilities were explicitly disabled for this provider-free test profile.
  // A file also applies to processes launched by a native manager's environment.
  fs.writeFileSync(path.join(configDir, 'abilities-seeded.txt'), 'browser\ndocs\n');
  fs.writeFileSync(path.join(configDir, 'config.toml'), `schema_version = 1\n[profile]\nname = "update-gate"\n[logging]\nlevel = "info"\n[daemon]\nhttp_enabled = true\nhttp_port = ${port}\nshutdown_timeout_seconds = 15\n`);
  configured = true;
  if (native) {
    // Prove refusal before allowing the production installer to create anything.
    fs.mkdirSync(path.dirname(service.definition), { recursive: true });
    fs.writeFileSync(service.definition, 'externally managed acceptance fixture\n');
    assert.notEqual(run(target, ['service', 'install']).status, 0);
    assert.equal(fs.readFileSync(service.definition, 'utf8'), 'externally managed acceptance fixture\n');
    fs.unlinkSync(service.definition);
    serviceOwned = true;
    ok(target, ['service', 'install']);
    if (!windows) { await readiness(); verifyManager(); await stop(); }
    // Reconciliation must also work when the owned service already exists.
    ok(target, ['service', 'install']);
    if (!windows) { await readiness(); verifyManager(); await stop(); }
  }
  await start(); await stop();
  writeSentinel('before-update');
  const before = hash(target), definition = native ? fs.readFileSync(service.definition) : null;
  const staged = JSON.parse(ok(target, ['update', 'stage', tag, version, '--dir', install], 600000));
  const { updater, stage } = staged;
  assert.equal(hash(target), before);
  assert.notEqual(stage.sha256, before, 'acceptance requires distinct previous and candidate binaries');
  const activate = ['update', 'activate', stage.id, '--dir', install, ...scoped];
  await start();
  const busy = run(updater, activate);
  assert.notEqual(busy.status, 0, 'running installation must refuse activation');
  assert.equal(hash(target), before); await stop();
  const activated = JSON.parse(ok(updater, activate, 180000)).update;
  assert.equal(activated.state, 'applied'); assert.equal(hash(target), stage.sha256);
  assertSentinel('before-update');
  cli(['backup', 'verify', activated.backup]);
  if (native) assert.deepEqual(fs.readFileSync(service.definition), definition);
  await start(); await stop();
  // Data written after activation must survive binary rollback.
  writeSentinel('after-update');
  const afterDatabase = hash(database);
  const rolled = JSON.parse(ok(updater, ['update', 'rollback', '--dir', install, ...scoped], 180000)).update;
  assert.equal(rolled.state, 'rolled_back'); assert.equal(hash(target), before);
  assert.equal(hash(database), afterDatabase); assertSentinel('after-update');
  await start(); await stop();
  // Pending journal refusal + evidence-driven recovery against the real binary.
  const journal = path.join(install, '.jarvis-update', 'journal.json');
  const interrupted = JSON.parse(fs.readFileSync(journal));
  interrupted.state = 'rolling_back'; interrupted.database_sha256 = hash(database);
  interrupted.database_bytes = fs.statSync(database).size;
  fs.writeFileSync(journal, JSON.stringify(interrupted));
  assert.notEqual(run(target, ['daemon', ...scoped], 15000).status, 0);
  const recovered = JSON.parse(ok(updater, ['update', 'recover', '--dir', install, ...scoped], 180000)).update;
  assert.equal(recovered.state, 'rolled_back'); assertSentinel('after-update');
  await start(); await stop();
  if (native) {
    assert.deepEqual(fs.readFileSync(service.definition), definition);
    ok(target, ['service', 'uninstall']); serviceOwned = false;
    assert.equal(fs.existsSync(service.definition), false);
    if (windows) assert.notEqual(run('reg', ['query', registry, '/v', 'JARVIS']).status, 0);
    assertSentinel('after-update');
  }
  console.log(JSON.stringify({ platform: process.platform, arch: process.arch, tag, nativeService: native,
    distinctSignedActivation: true, activeOwnerRefused: true, authenticatedReadiness: true,
    snapshotVerified: true, rollbackPreservesNewData: true, pendingStartupRefused: true,
    recoveryReady: true, lifecycleCount: lifecycleIds.size, servicePreserved: native, serviceUninstalled: native }));
} catch (error) {
  const log = path.join(root, 'daemon.txt');
  if (fs.existsSync(log)) console.error(fs.readFileSync(log, 'utf8').slice(-6000));
  throw error;
} finally {
  try { await stop(); } catch (error) { console.error(`cleanup stop: ${error.message}`); }
  if (child && child.exitCode === null) { child.kill(); await delay(500); }
  if (serviceOwned) run(target, ['service', 'uninstall']);
  assert.equal(path.dirname(path.resolve(root)), temp);
  assert.match(path.basename(root), /^ju-/);
  fs.rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
