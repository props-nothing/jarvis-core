// Opt-in real description generation, reference reuse, cancellation and browser acceptance.
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { dirname, join, resolve, isAbsolute } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live to synthesize and play fixed sample text.');
const option = name => args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const runtime = option('--runtime'), modulePath = option('--playwright');
assert(runtime && isAbsolute(runtime), 'Pass --runtime with the local designer directory.');
assert(modulePath && isAbsolute(modulePath), 'Pass --playwright with the absolute index.mjs path.');
assert.equal(JSON.parse(await readFile(join(dirname(modulePath), 'package.json'), 'utf8')).version, '1.62.1');
const { chromium } = await import(pathToFileURL(modulePath).href);
const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const daemon = option('--daemon') || join(repo, 'target/debug', process.platform === 'win32' ? 'jarvisd.exe' : 'jarvisd');
const evidence = join(repo, 'target/voice-description-acceptance'); await mkdir(evidence, { recursive: true });
const root = await mkdtemp(join(evidence, 'profile-'));
await mkdir(join(root, 'config'));
const report = { checks: [], passed: false }, pass = text => { report.checks.push(text); console.log('PASS ' + text); };
let child, browser, log, credential, origin, forceCpu = false;
const expectedBackend = option('--expect-backend');
async function poll(label, check, timeout = 90000) {
  const start = Date.now(); while (Date.now() - start < timeout) { if (await check()) return; await delay(250); } throw Error('Timed out: ' + label);
}
const headers = () => ({ authorization: 'Bearer ' + credential, 'content-type': 'application/json' });
async function start() {
  log = await open(join(root, 'daemon.log'), 'a');
  child = spawn(daemon, ['--root', root], { windowsHide: true, stdio: ['ignore', log.fd, log.fd], env: forceCpu ? {...process.env, HIP_VISIBLE_DEVICES:'-1', CUDA_VISIBLE_DEVICES:'-1'} : process.env });
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
const request = (path, body, method = 'POST') => fetch(origin + '/api/v1/' + path, { method, headers: headers(), body: JSON.stringify(body) });
const description = 'A warm adult male voice with a British accent, calm and clear.';
const sample = 'Hello. I am here to help you today.';
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function audio(bytes) {
  assert.equal(bytes.subarray(0, 4).toString(), 'RIFF'); assert.equal(bytes.readUInt32LE(24), 48000);
  assert(bytes.subarray(44).some(byte => byte)); return (bytes.length - 44) / 96000;
}
try {
  const tampered = join(root, 'tampered-model'); await mkdir(tampered);
  await writeFile(join(tampered, 'model.safetensors'), 'tampered');
  const worker = await readFile(join(repo, 'crates/jarvis-voice/src/description_worker.py'), 'utf8');
  const manifest = await readFile(join(repo, 'crates/jarvis-voice/src/description_assets.json'), 'utf8');
  const invalid = spawn(join(runtime, process.platform === 'win32' ? 'venv/Scripts/python.exe' : 'venv/bin/python'), ['-I', '-u', '-c', worker, tampered, manifest], { windowsHide: true });
  const invalidOutput = []; invalid.stdout.on('data', b => invalidOutput.push(b)); invalid.stderr.on('data', b => invalidOutput.push(b));
  const invalidExit = await new Promise((done, reject) => { invalid.once('error', reject); invalid.once('exit', done); });
  assert.notEqual(invalidExit, 0); assert.deepEqual(Buffer.concat(invalidOutput), Buffer.concat([Buffer.from('JVD3P'), Buffer.from([8,0,0,0]), Buffer.from('checking')]));
  pass('tampered model fails before worker readiness or inference without raw SDK output');
  const reserve = createServer(); const port = await new Promise(done => reserve.listen(0, '127.0.0.1', () => done(reserve.address().port)));
  await new Promise(done => reserve.close(done)); origin = `http://127.0.0.1:${port}`;
  await writeFile(join(root, 'config/config.toml'), `schema_version = 1
[profile]
name = "voice-description-acceptance"
[logging]
level = "warn"
[daemon]
shutdown_timeout_seconds = 30
http_enabled = true
http_port = ${port}
executor_model = "scripted"
[[daemon.speech.providers]]
id = "designer"
kind = "local-voxcpm2"
runtime_dir = '${runtime.replaceAll('\\', '/')}'
`);
  await start();
  const initial = await getLibrary(); assert.equal(initial.library.voices.length, 0); assert(initial.providers[0].description_design);
  assert.equal(initial.providers[0].design_unavailable_reason, null);
  for (const path of ['design', 'cancel', 'preview']) assert.equal((await fetch(origin + '/api/v1/settings/voices/' + path, { method: 'POST' })).status, 401);
  assert.equal((await fetch(origin + '/api/v1/settings/voices/runtime')).status, 401);
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
  await page.locator('#voice-library-create').click();
  await page.getByLabel('Name', { exact: true }).fill('Warm narrator');
  await page.locator('#voice-design-description').fill(description); await page.locator('#voice-design-sample').fill(sample);
  await page.route('**/settings/voices/design', route => route.fulfill({status:502,contentType:'application/json',body:JSON.stringify({message:'Local runtime unavailable. Check setup and retry.'})}));
  await page.locator('#voice-library-generate').click();
  await page.locator('#voice-design-status').getByText('Local runtime unavailable. Check setup and retry.', { exact:true }).waitFor();
  assert(await page.locator('#voice-library-generate').isEnabled());
  await page.screenshot({path:join(root,'inline-error.png')});
  await page.unroute('**/settings/voices/design');
  // This older suite exercises the compatible complete-response mode; accept-voice-streaming covers incremental playback.
  await page.route('**/settings/voices/design', route => route.continue({postData:JSON.stringify({...route.request().postDataJSON(),stream:false})}));
  pass('injected setup failure is visible beside Generate and leaves Retry available');
  const began = Date.now();
  const generatedResponse = page.waitForResponse(r => r.url().endsWith('/settings/voices/design'), { timeout: 180000 });
  await page.locator('#voice-library-generate').click();
  await page.locator('#voice-design-status').getByText(/Generating your voice locally/).waitFor();
  assert(await page.locator('#voice-library-cancel-generation').isVisible());
  await page.screenshot({path:join(root,'generation-progress.png')});
  const generated = await generatedResponse;
  assert.equal(generated.status(), 200, await generated.text()); const generatedBody = generated.headers()['content-type'].includes('ndjson') ? JSON.parse((await generated.text()).trim().split('\n').at(-1)) : await generated.json(), design = generatedBody.design;
  report.execution = generatedBody.execution;
  if (expectedBackend) assert.equal(report.execution.backend, expectedBackend);
  if (['amd','nvidia'].includes(expectedBackend)) assert(report.execution.allocated_bytes > 1000000000);
  report.first_generation_seconds = (Date.now() - began) / 1000;
  const reference = Buffer.from(design.reference_audio, 'base64'); report.reference_seconds = audio(reference);
  await writeFile(join(root, 'warm-narrator.wav'), reference);
  await poll('generated sample ready', () => page.locator('#voice-library-generate').isEnabled());
  await page.locator('#voice-library-listen').click();
  await page.waitForFunction(() => window.__voiceEvents.some(e => e.type === 'ended' && e.time > 0), null, { timeout: 30000 });
  assert.deepEqual((await getLibrary()).library, initial.library);
  report.media = await page.evaluate(() => window.__voiceEvents);
  if (['amd','nvidia'].includes(expectedBackend)) assert((await page.locator('#voice-design-device').textContent()).includes(report.execution.device));
  pass('description and sample text generate real local audio and play to completion without saving');
  const id = await page.locator('#voice-library-picker').inputValue();
  await page.getByRole('button', { name: 'Save and use in console', exact: true }).click();
  await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.', { exact: true }).waitFor();
  const saved = await getLibrary(); assert.deepEqual(saved.library.voices[0].design, design);
  await page.getByText('Manage this voice', { exact: true }).click();
  const downloadEvent = page.waitForEvent('download'); await page.locator('#voice-library-export').click(); const download = await downloadEvent;
  await download.saveAs(join(root, 'export.voice.json'));
  const exported = JSON.parse(await readFile(join(root, 'export.voice.json'), 'utf8'));
  assert.deepEqual(exported.design, design); assert.equal(exported.provider, undefined);
  await page.locator('#voice-design-description').scrollIntoViewIfNeeded(); await page.screenshot({ path: join(root, 'desktop.png') });
  await page.setViewportSize({ width: 390, height: 844 }); await page.locator('#voice-design-description').scrollIntoViewIfNeeded();
  await page.screenshot({ path: join(root, 'mobile.png') }); assert(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
  const speak = async name => {
    const started = Date.now(); const response = await request('speech', { text: 'Your saved voice is ready for another conversation.' });
    assert.equal(response.status, 200, await (response.ok ? Promise.resolve('') : response.text()));
    const bytes = Buffer.from(await response.arrayBuffer()); audio(bytes); await writeFile(join(root, name + '.wav'), bytes);
    report[name] = { seconds: (Date.now() - started) / 1000, sha256: digest(bytes) };
  };
  await speak('saved-speech'); pass('save, Console selection, export and new speech reuse the generated identity');
  const other = await request('settings/voices/design', { provider: 'designer', description: 'A bright young adult female voice with an American accent and cheerful delivery.', text: sample, seed: 42 });
  assert.equal(other.status, 200); const second = (await other.json()).design;
  const secondAudio = Buffer.from(second.reference_audio, 'base64'); audio(secondAudio); assert.notEqual(digest(reference), digest(secondAudio));
  await writeFile(join(root, 'bright-voice.wav'), secondAudio); pass('a second description generates a different real sample');
  const injected = structuredClone(saved.library); injected.voices[0].design.seed++;
  assert.equal((await request('settings/voices', injected, 'PUT')).status, 422);
  assert.equal((await request('settings/voices/preview', { voice: injected.voices[0], text: sample })).status, 422);
  assert.equal((await request('settings/voices/design', { provider: 'designer', description, text: sample, seed: 1, reference: 'untrusted' })).status, 422);
  pass('arbitrary reference injection is refused by save, preview and design routes');
  await stop(); await start(); assert.deepEqual((await getLibrary()).library, saved.library); await speak('restarted-speech');
  report.restart_waveform_identical = report['restarted-speech'].sha256 === report['saved-speech'].sha256;
  // GPU kernels may produce different waveforms. Identity is the persisted reference,
  // verified above; exact PCM reproduction is an additional CPU regression assertion.
  if (!['amd', 'nvidia'].includes(report.execution?.backend)) assert(report.restart_waveform_identical);
  pass('restart preserves the exact reference and synthesizes from the saved voice');
  const pending = request('settings/voices/design', { provider: 'designer', description, text: sample, seed: 12, request_id: 'cancel-live' });
  await delay(1000); assert.equal((await request('settings/voices/cancel', { request_id: 'cancel-live' })).status, 204);
  const cancelled = await pending; assert(!cancelled.ok); report.cancelled_status = cancelled.status;
  await speak('after-cancel-speech'); pass('cancelling generation releases the worker and the next synthesis recovers');
  if (args.includes('--test-cpu-fallback')) {
    if (process.platform === 'win32') {
      // Kill only Python descendants of this disposable daemon, never another profile.
      const script = `$rows=@(Get-CimInstance Win32_Process); $owned=@(${child.pid}); do { $next=@($rows | Where-Object { $owned -contains $_.ParentProcessId -and $owned -notcontains $_.ProcessId } | Select-Object -ExpandProperty ProcessId); $owned += $next } while($next.Count); $workers=@($rows | Where-Object { $owned -contains $_.ProcessId -and $_.Name -eq 'python.exe' }); if(-not $workers.Count){throw 'No owned voice worker'}; $workers | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }; Write-Output $workers.Count`;
      await new Promise((done, reject) => execFile('powershell.exe', ['-NoProfile','-NonInteractive','-Command',script], {windowsHide:true}, (error, stdout) => error ? reject(error) : Number(stdout.trim()) > 0 ? done() : reject(Error('No worker killed'))));
      await speak('failed-worker-cpu-speech');
      report.failed_worker_fallback = (await getLibrary()).providers[0].execution;
      assert.equal(report.failed_worker_fallback.backend, 'cpu');
      assert.match(report.failed_worker_fallback.fallback_reason, /GPU unavailable/);
      assert.deepEqual((await getLibrary()).library, saved.library);
      pass('terminated GPU worker retries the same saved-reference request on CPU');
    }
    await stop(); forceCpu = true; await start();
    assert.deepEqual((await getLibrary()).library, saved.library);
    await speak('cpu-fallback-speech');
    const status = await fetch(origin + '/api/v1/settings/voices/runtime', { headers:headers() });
    const snapshot = await status.json(); report.cpu_fallback = snapshot.providers[0].execution;
    assert.equal(snapshot.library, undefined);
    assert.equal(report.cpu_fallback.backend, 'cpu'); assert.equal(report.cpu_fallback.allocated_bytes, 0);
    assert.match(report.cpu_fallback.fallback_reason, /GPU unavailable/);
    assert.deepEqual((await getLibrary()).library, saved.library);
    await speak('cpu-sticky-speech');
    assert.equal(report['cpu-sticky-speech'].sha256, report['cpu-fallback-speech'].sha256);
    pass('masked GPU retries on CPU, preserves the GPU-generated reference and stays on CPU');
  }
  await request('settings/voices/cancel', { request_id: 'early-cancel' });
  const early = await request('settings/voices/design', { provider: 'designer', description, text: sample, seed: 9, request_id: 'early-cancel' }); assert(!early.ok);
  const removed = structuredClone(saved.library); removed.voices = []; removed.default_voice = null; removed.channels.console = null;
  assert.equal((await request('settings/voices', removed, 'PUT')).status, 200); await stop(); await start();
  assert.equal((await getLibrary()).library.voices.length, 0);
  assert.equal((await fetch(origin + '/api/v1/settings/voices/' + id + '/export', { headers: headers() })).status, 404);
  assert(!(await readFile(join(root, 'config/voice-library.json'), 'utf8')).includes(design.reference_audio));
  pass('early cancellation is retained and deletion removes the reference across restart');
  assert.deepEqual(errors, []); report.passed = true;
} finally {
  await browser?.close();
  if (child && child.exitCode === null) {
    try { await stop(); report.stopped = true; } catch { child.kill(); report.passed = false; report.stopped = false; }
  }
  await log?.close();
  await writeFile(join(root, 'verification.json'), JSON.stringify(report, null, 2)); console.log('Evidence: ' + root);
}
assert(report.passed, 'Voice description acceptance failed');
