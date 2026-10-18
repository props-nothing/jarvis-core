// Opt-in live acceptance: actual local synthesis, daemon routing and browser playback.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live: this requires an installed local Kokoro server.');
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const base = new URL(option('--server') || 'http://127.0.0.1:8880/v1');
assert(base.protocol === 'http:' && base.hostname === '127.0.0.1' && !base.username && !base.password && !base.search && !base.hash,
  'This acceptance harness only sends fixed test text to literal loopback HTTP.');
const modulePath = option('--playwright');
assert(modulePath, 'Pass --playwright /absolute/path/to/playwright/index.mjs.');
const packageInfo = JSON.parse(await readFile(join(dirname(resolve(modulePath)), 'package.json'), 'utf8'));
assert.equal(packageInfo.version, '1.62.1', 'Use the researched Playwright verifier version.');
const { chromium } = await import(pathToFileURL(resolve(modulePath)).href);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const suffix = process.platform === 'win32' ? '.exe' : '';
const daemon = resolve(option('--daemon') || join(repo, 'target/release/jarvisd' + suffix));
const cli = resolve(option('--cli') || join(repo, 'target/release/jarvis' + suffix));
const evidence = join(repo, 'target/speech-local-acceptance');
await mkdir(evidence, { recursive: true });
const root = await mkdtemp(join(evidence, 'run-'));
await mkdir(join(root, 'config'));
const report = { passed: false, platform: process.platform, playwright: packageInfo.version, checks: [], requests: [] };
const pass = name => { report.checks.push(name); console.log('PASS ' + name); };
const poll = async (name, fn, timeout = 30000) => {
  const end = Date.now() + timeout;
  do { if (await fn()) return; await delay(200); } while (Date.now() < end);
  throw Error(name + ' timed out');
};
const listen = async server => { server.listen(0, '127.0.0.1'); await once(server, 'listening'); return server.address().port; };
let failPrimary = false, primaryAttempts = 0;
const primary = createServer(async (req, res) => {
  if (req.method !== 'POST' || req.url !== '/v1/audio/speech') { res.writeHead(404).end(); return; }
  primaryAttempts++;
  if (failPrimary) { req.resume(); res.writeHead(503, { 'content-type': 'application/json' }).end('{"error":"controlled outage"}'); return; }
  try {
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const upstream = await fetch(base.href.replace(/\/$/, '') + '/audio/speech', {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: Buffer.concat(chunks), signal: AbortSignal.timeout(25000)
    });
    res.writeHead(upstream.status, { 'content-type': upstream.headers.get('content-type') || 'application/octet-stream' });
    await pipeline(Readable.fromWeb(upstream.body), res);
  } catch { res.destroy(); }
});
let child, browser, daemonLog;
try {
  const primaryPort = await listen(primary);
  const reserve = createServer(); const port = await listen(reserve);
  await new Promise(done => reserve.close(done));
  await writeFile(join(root, 'config/config.toml'), `schema_version = 1
[profile]
name = "speech-acceptance"
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
model = "kokoro"
voice = "bm_george"
[[daemon.speech.providers]]
id = "local"
kind = "openai-compatible"
base_url = ${JSON.stringify(base.href.replace(/\/$/, ''))}
model = "kokoro"
voice = "af_heart"
`);
  daemonLog = await open(join(root, 'daemon.log'), 'w');
  child = spawn(daemon, ['--root', root], { windowsHide: true, stdio: ['ignore', daemonLog.fd, daemonLog.fd] });
  child.on('error', () => {});
  const origin = `http://127.0.0.1:${port}`;
  let credential;
  await poll('daemon readiness', async () => {
    if (child.exitCode !== null) throw Error('acceptance daemon exited; inspect its local log');
    try {
      credential = (await readFile(join(root, 'config/client.credential'), 'utf8')).trim();
      return (await fetch(origin + '/api/v1/speech', { headers: { authorization: 'Bearer ' + credential }, signal: AbortSignal.timeout(1000) })).ok;
    } catch { return false; }
  });
  const headers = { authorization: 'Bearer ' + credential, 'content-type': 'application/json' };
  const status = async () => (await fetch(origin + '/api/v1/speech', { headers })).json();
  assert.equal((await fetch(origin + '/api/v1/speech')).status, 401);
  pass('speech refuses unauthenticated access');
  const speak = async (name, preferred) => {
    const start = performance.now();
    const response = await fetch(origin + '/api/v1/speech', { method: 'POST', headers,
      body: JSON.stringify({ text: 'JARVIS is speaking through the local voice server.', ...(preferred ? { provider: preferred } : {}) }),
      signal: AbortSignal.timeout(55000) });
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('content-type'), 'audio/mpeg');
    const reader = response.body.getReader(); const chunks = []; let first;
    while (true) { const { done, value } = await reader.read(); if (done) break; first ??= performance.now() - start; chunks.push(Buffer.from(value)); }
    const audio = Buffer.concat(chunks); assert(audio.length > 1000);
    assert(audio.subarray(0, 3).toString() === 'ID3' || (audio[0] === 255 && (audio[1] & 224) === 224));
    await writeFile(join(root, name + '.mp3'), audio);
    const result = { name, provider: response.headers.get('x-jarvis-speech-provider'), backup: response.headers.get('x-jarvis-speech-backup'),
      first_audio_ms: Math.round(first), total_ms: Math.round(performance.now() - start), bytes: audio.length };
    report.requests.push(result); return result;
  };
  assert.equal((await speak('primary')).provider, 'primary'); pass('primary produces real model audio');
  failPrimary = true;
  const fallback = await speak('fallback'); assert.equal(fallback.provider, 'local'); assert.equal(fallback.backup, 'true');
  assert.equal((await status()).providers[0].health, 'cooling_down'); pass('primary HTTP 503 selects real local backup and opens breaker');
  const attemptsAfterFailure = primaryAttempts;
  failPrimary = false;
  assert.equal((await speak('same-answer', 'local')).provider, 'local');
  assert.equal((await speak('cooldown')).provider, 'local');
  assert.equal(primaryAttempts, attemptsAfterFailure); pass('answer affinity and cooldown avoid primary retries');
  console.log('Waiting for the real 30-second recovery interval');
  await delay(31000);
  assert.equal((await speak('recovered')).provider, 'primary');
  assert.equal((await status()).providers[0].health, 'healthy'); pass('new answer recovers the primary after one successful probe');

  failPrimary = true;
  browser = await chromium.launch({ headless: true, ...(process.platform === 'win32' ? { channel: 'msedge' } : {}), args: ['--autoplay-policy=no-user-gesture-required'] });
  report.browser_version = browser.version();
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const selections = [], browserErrors = [];
  page.on('pageerror', error => browserErrors.push(error.message));
  page.on('response', response => { if (response.request().method() === 'POST' && response.url().endsWith('/api/v1/speech')) {
    selections.push({ status: response.status(), provider: response.headers()['x-jarvis-speech-provider'] });
  } });
  await page.addInitScript(() => {
    window.__speechMedia = [];
    window.Audio = new Proxy(window.Audio, { construct(target, args) {
      const audio = Reflect.construct(target, args);
      const record = { playing: false, ended: false, error: null, time: 0 };
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
  await page.locator('#voice-options-toggle').click();
  await page.locator('#speak').check();
  await page.getByRole('textbox', { name: 'Message', exact: true }).fill('Check the local backup voice.');
  await page.getByRole('button', { name: 'Send message', exact: true }).click();
  await page.locator('#hint').filter({ hasText: 'Using a backup voice for this answer.' }).waitFor({ timeout: 30000 });
  await page.screenshot({ path: join(root, 'backup-voice.png'), fullPage: true });
  await page.waitForFunction(() => window.__speechMedia.length > 0 &&
    window.__speechMedia.every(m => m.ended && m.playing && m.time > 0), null, { timeout: 60000 });
  report.browser_media = await page.evaluate(() => window.__speechMedia);
  assert(report.browser_media.every(m => m.error === null));
  assert(selections.length > 0 && selections.every(s => s.status === 200 && s.provider === 'local'));
  assert.equal(browserErrors.length, 0);
  report.browser_selections = selections;
  pass('real browser displays backup note and decodes/plays local MP3 through the console');
  report.passed = true;
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    const result = spawnSync(cli, ['stop', '--root', root], { windowsHide: true, timeout: 15000, encoding: 'utf8' });
    report.cleanup = { stopped: result.status === 0 };
    if (result.status !== 0) report.passed = false;
    await poll('acceptance daemon shutdown', async () => child.exitCode !== null, 15000);
  }
  primary.closeAllConnections(); await new Promise(done => primary.close(done));
  await daemonLog?.close();
  await writeFile(join(root, 'verification.json'), JSON.stringify(report, null, 2));
  console.log('Evidence: ' + root);
}
assert(report.passed, 'Live speech acceptance or cleanup failed');
