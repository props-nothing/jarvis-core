// Opt-in real native blending, exact preview, browser export and restart/delete acceptance.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { cp, mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live to synthesize and play fixed sample text.');
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const cache = option('--cache'), modulePath = option('--playwright');
assert(cache && isAbsolute(cache), 'Pass --cache with a prepared speech-kokoro-v1 directory beneath it.');
assert(modulePath && isAbsolute(modulePath), 'Pass --playwright with the absolute index.mjs path.');
assert.equal(JSON.parse(await readFile(join(dirname(modulePath), 'package.json'), 'utf8')).version, '1.62.1');
const { chromium } = await import(pathToFileURL(modulePath).href);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const daemon = option('--daemon') || join(repo, 'target/debug', process.platform === 'win32' ? 'jarvisd.exe' : 'jarvisd');
const evidence = join(repo, 'target/voice-design-acceptance'); await mkdir(evidence, { recursive: true });
const root = await mkdtemp(join(evidence, 'profile-'));
await mkdir(join(root, 'config')); await cp(cache, join(root, 'cache'), { recursive: true });
const report = { checks: [], passed: false }, pass = text => { report.checks.push(text); console.log('PASS ' + text); };
let child, browser, log, credential, origin;
async function poll(label, check, timeout = 90000) {
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
const sample = 'Hello. This is how I will sound with these voice settings.';
async function preview(voice) {
  const response = await fetch(origin + '/api/v1/settings/voices/preview', { method: 'POST', headers: headers(), body: JSON.stringify({ voice, text: sample }) });
  assert.equal(response.status, 200); assert.equal(response.headers.get('content-type'), 'audio/wav');
  const bytes = Buffer.from(await response.arrayBuffer()); assert.equal(bytes.subarray(0, 4).toString(), 'RIFF');
  assert.equal(bytes.readUInt32LE(24), 24000); assert(bytes.subarray(44).some(byte => byte !== 0));
  return bytes;
}
function withWeights(voice, heart, language = 'en-us') {
  const edited = structuredClone(voice); edited.blend.language = language;
  for (const part of edited.blend.sources) part.weight = part.voice === 'af_heart' ? heart : 100 - heart;
  return edited;
}
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
try {
  const reserve = createServer(); const port = await new Promise(done => reserve.listen(0, '127.0.0.1', () => done(reserve.address().port)));
  await new Promise(done => reserve.close(done)); origin = `http://127.0.0.1:${port}`;
  await writeFile(join(root, 'config/config.toml'), `schema_version = 1
[profile]
name = "voice-design-acceptance"
[logging]
level = "warn"
[daemon]
shutdown_timeout_seconds = 30
http_enabled = true
http_port = ${port}
executor_model = "scripted"
[[daemon.speech.providers]]
id = "local"
kind = "embedded-kokoro"
voice = "af_heart"
allow_download = false
`);
  await start();
  const initial = await getLibrary(); assert.equal(initial.library.voices.length, 1);
  assert.equal((await fetch(origin + '/api/v1/settings/voices/design/export')).status, 401);
  browser = await chromium.launch({ headless: true, ...(process.platform === 'win32' ? { channel: 'msedge' } : {}), args: ['--autoplay-policy=no-user-gesture-required'] });
  report.browser_version = browser.version();
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 }, acceptDownloads: true });
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
  await page.getByText('More ways to add a voice', { exact: true }).click();
  await page.locator('#voice-library-design').click();
  await page.getByLabel('Name', { exact: true }).fill('Evening blend');
  await page.locator('#voice-blend-balance').focus(); await page.keyboard.press('ArrowRight');
  assert.equal(await page.locator('#voice-blend-heart').inputValue(), '49');
  await page.locator('#voice-blend-heart').fill('35');
  assert.equal(await page.locator('#voice-blend-george').inputValue(), '65');
  await page.locator('#voice-library-preview').click();
  await page.waitForFunction(() => window.__voiceEvents.some(e => e.type === 'ended' && e.time > 0), null, { timeout: 60000 });
  assert.deepEqual((await getLibrary()).library, initial.library);
  assert(await page.locator('#voice-library-export').isDisabled());
  report.media = await page.evaluate(() => window.__voiceEvents);
  pass('keyboard blend editing and unsaved real audio preview work without saving');

  const id = await page.locator('#voice-library-picker').inputValue();
  const base = initial.library.voices[0];
  const design = withWeights({ ...base, id, name: 'Evening blend', voice: 'blend', blend: initial.providers[0].design.recipe }, 35);
  const heart = await preview(base), pureHeart = await preview(withWeights(design, 100));
  assert.deepEqual(pureHeart, heart);
  const george = await preview({ ...base, voice: 'bm_george' });
  assert.deepEqual(await preview(withWeights(design, 0, 'en-gb')), george);
  const mixed = await preview(design), other = await preview(withWeights(design, 80));
  assert.notEqual(digest(mixed), digest(heart)); assert.notEqual(digest(mixed), digest(george)); assert.notEqual(digest(mixed), digest(other));
  const reordered = structuredClone(design); reordered.blend.sources.reverse(); assert.deepEqual(await preview(reordered), mixed);
  report.audio = { heart: digest(heart), george: digest(george), mixed: digest(mixed), other: digest(other), mixed_seconds: (mixed.length - 44) / 48000 };
  await writeFile(join(root, 'blend-35-65.wav'), mixed); await writeFile(join(root, 'blend-80-20.wav'), other);
  pass('pure endpoints reproduce presets; distinct weights change audio and source order does not');
  const invalid = structuredClone(design); invalid.blend.sources[0].weight = 99;
  const denied = await fetch(origin + '/api/v1/settings/voices/preview', { method: 'POST', headers: headers(), body: JSON.stringify({ voice: invalid, text: sample }) }); assert.equal(denied.status, 422);

  await page.getByText('Default voice and backups', { exact: true }).click();
  await page.locator('#voice-library-console').selectOption(id); await page.locator('#voice-library-save').click();
  await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.', { exact: true }).waitFor();
  const saved = await getLibrary(); assert.deepEqual(saved.library.voices.find(v => v.id === id).blend, design.blend);
  const spoken = await fetch(origin + '/api/v1/speech', { method: 'POST', headers: headers(), body: JSON.stringify({ text: sample }) });
  assert.equal(spoken.status, 200); assert.deepEqual(Buffer.from(await spoken.arrayBuffer()), mixed);
  await page.getByText('Manage this voice', { exact: true }).click();
  const downloadEvent = page.waitForEvent('download'); await page.locator('#voice-library-export').click(); const download = await downloadEvent;
  await download.saveAs(join(root, 'export.voice.json'));
  const exported = JSON.parse(await readFile(join(root, 'export.voice.json'), 'utf8'));
  assert.equal(exported.format, 'jarvis-voice-design'); assert.equal(exported.name, design.name); assert.deepEqual(exported.blend, design.blend);
  assert.equal(exported.provider, undefined); assert.equal(exported.id, undefined);
  await page.locator('#voice-blend-heart').scrollIntoViewIfNeeded(); await page.screenshot({ path: join(root, 'desktop.png'), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 }); await page.locator('#voice-blend-heart').scrollIntoViewIfNeeded();
  await page.screenshot({ path: join(root, 'mobile.png'), fullPage: true }); assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
  pass('saved design drives console speech and exports the exact recipe in the browser');

  await stop(); await start(); assert.deepEqual((await getLibrary()).library, saved.library);
  assert.deepEqual(await preview((await getLibrary()).library.voices.find(v => v.id === id)), mixed);
  pass('real daemon restart preserves the recipe and reproduces the audio');
  await page.setViewportSize({ width: 1440, height: 1100 });
  await page.getByText('Default voice and backups', { exact: true }).click();
  await page.locator('#voice-library-console').selectOption(''); await page.locator('#voice-library-save').click();
  await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.', { exact: true }).waitFor();
  await page.getByText('Manage this voice', { exact: true }).click();
  await page.getByRole('button', { name: 'Delete voice', exact: true }).click(); await page.locator('#voice-library-save').click();
  await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.', { exact: true }).waitFor();
  assert(!(await getLibrary()).library.voices.some(v => v.id === id));
  assert.equal((await fetch(origin + '/api/v1/settings/voices/' + id + '/export', { headers: headers() })).status, 404);
  await stop(); await start(); assert(!(await getLibrary()).library.voices.some(v => v.id === id));
  assert(!(await readFile(join(root, 'config/voice-library.json'), 'utf8')).includes(id));
  pass('deletion removes the saved recipe and remains deleted after restart');
  assert.deepEqual(errors, []); report.passed = true;
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    try { await stop(); report.stopped = true; } catch { child.kill(); report.passed = false; report.stopped = false; }
  }
  await log?.close();
  await writeFile(join(root, 'verification.json'), JSON.stringify(report, null, 2)); console.log('Evidence: ' + root);
}
assert(report.passed, 'Voice design acceptance failed');
