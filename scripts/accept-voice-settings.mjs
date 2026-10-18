// Opt-in real local synthesis, browser playback and process-restart persistence.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { cp, mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live to synthesize and play fixed sample text.');
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const cache = option('--cache'), modulePath = option('--playwright');
assert(cache && isAbsolute(cache), 'Pass --cache with the prepared speech-kokoro-v1 directory beneath it.');
assert(modulePath && isAbsolute(modulePath), 'Pass --playwright with the absolute index.mjs path.');
assert.equal(JSON.parse(await readFile(join(dirname(modulePath), 'package.json'), 'utf8')).version, '1.62.1');
const { chromium } = await import(pathToFileURL(modulePath).href);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const daemon = option('--daemon') || join(repo, 'target/debug', process.platform === 'win32' ? 'jarvisd.exe' : 'jarvisd');
const evidence = join(repo, 'target/voice-settings-acceptance'); await mkdir(evidence, { recursive: true });
const root = await mkdtemp(join(evidence, 'profile-'));
await mkdir(join(root, 'config')); await cp(cache, join(root, 'cache'), { recursive: true });
const report = { checks: [], passed: false }, pass = text => { report.checks.push(text); console.log('PASS ' + text); };
let child, browser, log, credential, origin, previewCount = 0;
const primary = createServer((request, response) => { previewCount++; request.resume(); response.writeHead(503); response.end(); });
const listen = server => new Promise(done => server.listen(0, '127.0.0.1', () => done(server.address().port)));
async function poll(label, check, timeout = 60000) {
  const start = Date.now(); while (Date.now() - start < timeout) { if (await check()) return; await delay(250); } throw Error('Timed out: ' + label);
}
const headers = () => ({ authorization: 'Bearer ' + credential, 'content-type': 'application/json' });
async function start() {
  log = await open(join(root, 'daemon.log'), 'a');
  child = spawn(daemon, ['--root', root], { windowsHide: true, stdio: ['ignore', log.fd, log.fd] });
  await poll('daemon readiness', async () => {
    if (child.exitCode !== null) throw Error('Disposable daemon exited; inspect its log.');
    try { credential = (await readFile(join(root, 'config/client.credential'), 'utf8')).trim(); return (await fetch(origin + '/api/v1/settings/voices', { headers: headers() })).ok; } catch { return false; }
  });
}
async function stop() {
  assert((await fetch(origin + '/api/v1/shutdown', { method: 'POST', headers: headers() })).ok);
  await poll('shutdown', async () => child.exitCode !== null, 40000); await log.close(); log = null;
}
async function getLibrary() { const response = await fetch(origin + '/api/v1/settings/voices', { headers: headers() }); assert(response.ok); return response.json(); }
async function preview(voice) {
  const response = await fetch(origin + '/api/v1/settings/voices/preview', { method: 'POST', headers: headers(), body: JSON.stringify({ voice, text: 'Hello. This is how I will sound with these voice settings.' }) });
  assert.equal(response.status, 200); assert.equal(response.headers.get('content-type'), 'audio/wav');
  const bytes = Buffer.from(await response.arrayBuffer()); assert.equal(bytes.subarray(0, 4).toString(), 'RIFF');
  assert.equal(bytes.readUInt32LE(24), 24000); assert(bytes.subarray(44).some(byte => byte !== 0));
  return { bytes, duration: (bytes.length - 44) / 48000 };
}
try {
  const primaryPort = await listen(primary), reserve = createServer(), port = await listen(reserve);
  await new Promise(done => reserve.close(done)); origin = `http://127.0.0.1:${port}`;
  await writeFile(join(root, 'config/voice.key'), 'fixture-not-a-live-key');
  await writeFile(join(root, 'config/config.toml'), `schema_version = 1
[profile]
name = "voice-settings-acceptance"
[logging]
level = "warn"
[daemon]
shutdown_timeout_seconds = 30
http_enabled = true
http_port = ${port}
executor_model = "scripted"
[[daemon.speech.providers]]
id = "cloud"
kind = "elevenlabs"
base_url = "http://127.0.0.1:${primaryPort}"
key_ref = '${join(root, 'config/voice.key')}'
voice = "fixture"
model = "eleven_v4_turbo"
[[daemon.speech.providers]]
id = "local"
kind = "embedded-kokoro"
voice = "af_heart"
allow_download = false
`);
  await start();
  assert.equal((await fetch(origin + '/api/v1/settings/voices')).status, 401);
  const initial = await getLibrary(); assert.equal(initial.library.revision, 0);
  const fallback = await fetch(origin + '/api/v1/speech', { method: 'POST', headers: headers(), body: JSON.stringify({ text: 'The backup voice still works.' }) });
  assert.equal(fallback.status, 200); assert.equal(fallback.headers.get('x-jarvis-speech-backup'), 'true'); await fallback.arrayBuffer();
  pass('configured fallback remains active before library editing');

  browser = await chromium.launch({ headless: true, ...(process.platform === 'win32' ? { channel: 'msedge' } : {}), args: ['--autoplay-policy=no-user-gesture-required'] });
  report.browser_version = browser.version();
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  const errors = []; page.on('pageerror', error => errors.push(error.message));
  await page.addInitScript(() => {
    window.__voiceEvents = [];
    for (const type of ['playing', 'ended', 'error']) document.addEventListener(type, event => {
      if (event.target.tagName === 'AUDIO') window.__voiceEvents.push({ type, time: event.target.currentTime, duration: event.target.duration });
    }, true);
  });
  await page.goto(origin + '/#token=' + encodeURIComponent(credential));
  await page.locator('#pill-link').getByText('linked', { exact: true }).waitFor();
  await page.locator('#open-settings').click(); await page.locator('#settings-tab-voice').click();
  await page.getByText('Provider and model settings', { exact: true }).click();
  await page.locator('#voice-control-stability').waitFor();
  assert.equal(await page.locator('#voice-control-speed').count(), 0);
  await page.locator('#voice-library-picker').selectOption('local');
  await page.getByText('More ways to add a voice', { exact: true }).click();
  await page.getByRole('button', { name: 'Copy selected voice', exact: true }).click();
  await page.getByLabel('Name', { exact: true }).fill('Reading locally');
  await page.locator('#voice-library-voice').selectOption('bm_george');
  await page.getByText('Provider and model settings', { exact: true }).click();
  await page.locator('#voice-control-speed').fill('0.8');
  const selected = await page.locator('#voice-library-picker').inputValue();
  await page.locator('#voice-library-preview').click();
  await page.waitForFunction(() => window.__voiceEvents.some(e => e.type === 'ended' && e.time > 0), null, { timeout: 60000 });
  assert.equal((await getLibrary()).library.revision, 0);
  assert.equal((await getLibrary()).library.voices.length, 2);
  pass('browser plays the selected unsaved George voice without saving or cloud fallback');
  report.media = await page.evaluate(() => window.__voiceEvents);
  const voice = { id: selected, name: 'Reading locally', provider: 'local', model: 'kokoro-v1.0', voice: 'bm_george', settings: { speed: 0.8 } };
  const slow = await preview(voice), fast = await preview({ ...voice, settings: { speed: 1.4 } });
  assert(fast.duration < slow.duration * 0.75); report.speed = { slow_seconds: slow.duration, fast_seconds: fast.duration };
  await writeFile(join(root, 'slow.wav'), slow.bytes); await writeFile(join(root, 'fast.wav'), fast.bytes);
  pass('native speed changes actual decoded WAV duration');

  await page.getByText('Default voice and backups', { exact: true }).click();
  await page.locator('#voice-library-console').selectOption(selected);
  await page.locator('#voice-library-save').click();
  await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.', { exact: true }).waitFor();
  const saved = await getLibrary(); assert.equal(saved.library.revision, 1); assert.equal(saved.library.channels.console, selected);
  assert.equal(saved.library.voices.find(v => v.id === selected).voice, 'bm_george');
  assert(Math.abs(saved.library.voices.find(v => v.id === selected).settings.speed - 0.8) < 0.0001);
  const spoken = await fetch(origin + '/api/v1/speech', { method: 'POST', headers: headers(), body: JSON.stringify({ text: 'These are the saved console settings.' }) });
  assert.equal(spoken.status, 200); assert.equal(spoken.headers.get('x-jarvis-speech-provider'), 'local');
  assert.equal(spoken.headers.get('x-jarvis-speech-backup'), 'false'); await spoken.arrayBuffer();
  await page.locator('#settings-body').evaluate(el => { el.scrollTop = 0; });
  await page.screenshot({ path: join(root, 'desktop.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: join(root, 'mobile.png'), fullPage: true });
  assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
  pass('saved console override applies immediately and editor fits mobile');

  await stop(); await start(); const restarted = await getLibrary(); assert.deepEqual(restarted.library, saved.library);
  const status = await (await fetch(origin + '/api/v1/speech', { headers: headers() })).json(); assert.equal(status.voice, 'bm_george');
  const stale = await fetch(origin + '/api/v1/settings/voices', { method: 'PUT', headers: headers(), body: JSON.stringify(initial.library) }); assert.equal(stale.status, 409);
  pass('real daemon restart preserves edits and refuses stale replacement');
  const empty = { schema_version: 1, revision: restarted.library.revision, voices: [], default_voice: null, backups: [], channels: { console: null } };
  assert((await fetch(origin + '/api/v1/settings/voices', { method: 'PUT', headers: headers(), body: JSON.stringify(empty) })).ok);
  await stop(); await start();
  assert.equal((await (await fetch(origin + '/api/v1/speech', { headers: headers() })).json()).enabled, false);
  const restore = { ...saved.library, revision: (await getLibrary()).library.revision };
  assert((await fetch(origin + '/api/v1/settings/voices', { method: 'PUT', headers: headers(), body: JSON.stringify(restore) })).ok);
  assert.equal((await (await fetch(origin + '/api/v1/speech', { headers: headers() })).json()).voice, 'bm_george');
  pass('an empty library after restart can be re-enabled without another restart');
  assert.equal(errors.length, 0); assert.equal(previewCount, 1); report.passed = true;
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    try { await stop(); report.stopped = true; } catch { child.kill(); report.passed = false; report.stopped = false; }
  }
  primary.closeAllConnections(); await new Promise(done => primary.close(done)); await log?.close();
  await writeFile(join(root, 'verification.json'), JSON.stringify(report, null, 2)); console.log('Evidence: ' + root);
}
assert(report.passed, 'Voice settings acceptance failed');
