// Real embedded synthesis and console playback. Prepare the pinned cache with embedded_live first.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { cp, mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live to synthesize fixed text with the real embedded model.');
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const cache = option('--cache'), modulePath = option('--playwright');
assert(cache && isAbsolute(cache), 'Pass --cache /absolute/prepared/cache (containing speech-kokoro-v1).');
assert(modulePath && isAbsolute(modulePath), 'Pass --playwright /absolute/path/to/playwright/index.mjs.');
const packageInfo = JSON.parse(await readFile(join(dirname(modulePath), 'package.json'), 'utf8'));
assert.equal(packageInfo.version, '1.62.1');
const { chromium } = await import(pathToFileURL(modulePath).href);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const daemon = resolve(option('--daemon') || join(repo, 'target/release/jarvisd' + suffix));
const evidence = join(repo, 'target/speech-embedded-acceptance');
await mkdir(evidence, { recursive: true });
const root = await mkdtemp(join(evidence, 'console-'));
await mkdir(join(root, 'config'));
await mkdir(join(root, 'cache'));
await cp(join(cache, 'speech-kokoro-v1'), join(root, 'cache/speech-kokoro-v1'), { recursive: true, errorOnExist: true });
const report = { passed: false, platform: process.platform, downloads_allowed: false, checks: [], requests: [] };
const pass = name => { report.checks.push(name); console.log('PASS ' + name); };
const poll = async (name, fn, timeout = 30000) => {
  const end = Date.now() + timeout;
  do { if (await fn()) return; await delay(100); } while (Date.now() < end);
  throw Error(name + ' timed out');
};
const listen = async server => { server.listen(0, '127.0.0.1'); await once(server, 'listening'); return server.address().port; };
let primaryAttempts = 0;
const primary = createServer((req, res) => { primaryAttempts++; req.resume(); res.writeHead(503).end(); });
let child, browser, daemonLog, origin, credential;
const usage = () => {
  if (process.platform !== 'win32') return null;
  const p = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
    `$p = Get-Process -Id ${child.pid}; @{ cpu_ms = $p.TotalProcessorTime.TotalMilliseconds; peak_working_set_bytes = $p.PeakWorkingSet64 } | ConvertTo-Json -Compress`],
    { windowsHide: true, encoding: 'utf8' });
  assert.equal(p.status, 0); return JSON.parse(p.stdout);
};
try {
  const primaryPort = await listen(primary);
  const reserve = createServer(); const port = await listen(reserve); await new Promise(done => reserve.close(done));
  await writeFile(join(root, 'config/config.toml'), `schema_version = 1
[profile]
name = "embedded-speech-acceptance"
[logging]
level = "warn"
[daemon]
shutdown_timeout_seconds = 30
http_enabled = true
http_port = ${port}
executor_model = "scripted"
[[daemon.speech.providers]]
id = "primary"
kind = "openai-compatible"
base_url = "http://127.0.0.1:${primaryPort}/v1"
model = "unavailable"
voice = "unavailable"
[[daemon.speech.providers]]
id = "local"
kind = "embedded-kokoro"
voice = "af_heart"
allow_download = false
`);
  daemonLog = await open(join(root, 'daemon.log'), 'w');
  child = spawn(daemon, ['--root', root], { windowsHide: true, stdio: ['ignore', daemonLog.fd, daemonLog.fd] });
  origin = `http://127.0.0.1:${port}`;
  await poll('daemon readiness', async () => {
    if (child.exitCode !== null) throw Error('acceptance daemon exited; inspect its local log');
    try {
      credential = (await readFile(join(root, 'config/client.credential'), 'utf8')).trim();
      return (await fetch(origin + '/api/v1/speech', { headers: { authorization: 'Bearer ' + credential } })).ok;
    } catch { return false; }
  });
  const headers = { authorization: 'Bearer ' + credential, 'content-type': 'application/json' };
  assert.equal((await fetch(origin + '/api/v1/speech')).status, 401);
  pass('speech rejects unauthenticated access');
  for (const name of ['cold', 'warm']) {
    const before = usage(), start = performance.now();
    const response = await fetch(origin + '/api/v1/speech', { method: 'POST', headers,
      body: JSON.stringify({ text: 'JARVIS can speak without a separate speech server.' }), signal: AbortSignal.timeout(55000) });
    const first = performance.now() - start;
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('content-type'), 'audio/wav');
    assert.equal(response.headers.get('x-jarvis-speech-provider'), 'local');
    assert.equal(response.headers.get('x-jarvis-speech-backup'), 'true');
    const audio = Buffer.from(await response.arrayBuffer());
    assert.equal(audio.subarray(0, 4).toString(), 'RIFF');
    assert.equal(audio.readUInt32LE(24), 24000);
    assert.equal(audio.readUInt16LE(34), 16);
    await writeFile(join(root, name + '.wav'), audio);
    const after = usage();
    report.requests.push({ name, first_audio_ms: Math.round(first), bytes: audio.length,
      cpu_ms: after ? after.cpu_ms - before.cpu_ms : null, peak_working_set_bytes: after?.peak_working_set_bytes });
  }
  assert.equal(primaryAttempts, 1);
  pass('primary failure selects real embedded WAV and respects cooldown');
  browser = await chromium.launch({ headless: true, ...(process.platform === 'win32' ? { channel: 'msedge' } : {}), args: ['--autoplay-policy=no-user-gesture-required'] });
  report.browser_version = browser.version();
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const browserErrors = [], selections = [];
  page.on('pageerror', error => browserErrors.push(error.message));
  page.on('response', response => { if (response.request().method() === 'POST' && response.url().endsWith('/api/v1/speech'))
    selections.push({ status: response.status(), provider: response.headers()['x-jarvis-speech-provider'], type: response.headers()['content-type'] }); });
  await page.addInitScript(() => {
    window.__speechMedia = [];
    window.Audio = new Proxy(window.Audio, { construct(target, args) {
      const audio = Reflect.construct(target, args), record = { playing: false, ended: false, error: null, time: 0 };
      window.__speechMedia.push(record);
      audio.addEventListener('playing', () => { record.playing = true; });
      audio.addEventListener('timeupdate', () => { record.time = audio.currentTime; });
      audio.addEventListener('ended', () => { record.ended = true; });
      audio.addEventListener('error', () => { record.error = audio.error?.code; });
      return audio;
    } });
  });
  await page.goto(origin + '/#token=' + encodeURIComponent(credential));
  await page.locator('#pill-link').getByText('linked', { exact: true }).waitFor({ timeout: 15000 });
  await page.locator('#voice-options-toggle').click(); await page.locator('#speak').check();
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Check the embedded backup voice.');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await page.locator('#hint').filter({ hasText: 'Using a backup voice for this answer.' }).waitFor({ timeout: 30000 });
  await page.screenshot({ path: join(root, 'embedded-voice.png'), fullPage: true });
  await page.waitForFunction(() => window.__speechMedia.length > 0 && window.__speechMedia.every(m => m.ended && m.playing && m.time > 0), null, { timeout: 60000 });
  report.browser_media = await page.evaluate(() => window.__speechMedia);
  assert(report.browser_media.every(m => m.error === null));
  assert(selections.length > 0 && selections.every(s => s.status === 200 && s.provider === 'local' && s.type === 'audio/wav'));
  assert.equal(browserErrors.length, 0); report.browser_selections = selections;
  pass('console shows backup selection and plays real embedded WAV to completion');
  report.passed = true;
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    try {
      const response = await fetch(origin + '/api/v1/shutdown', { method: 'POST',
        headers: { authorization: 'Bearer ' + credential }, signal: AbortSignal.timeout(10000) });
      assert(response.ok);
      await poll('acceptance daemon shutdown', async () => child.exitCode !== null, 40000);
      report.cleanup = { stopped: true };
    } catch {
      report.cleanup = { stopped: false }; report.passed = false;
      child.kill(); // Only the disposable daemon started above, never the owner's daemon.
    }
  }
  primary.closeAllConnections(); await new Promise(done => primary.close(done)); await daemonLog?.close();
  await writeFile(join(root, 'verification.json'), JSON.stringify(report, null, 2)); console.log('Evidence: ' + root);
}
assert(report.passed, 'Embedded speech acceptance failed');
