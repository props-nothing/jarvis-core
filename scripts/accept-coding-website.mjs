// Explicit account-backed acceptance; never part of ordinary offline CI.
// See docs/quality/coding-website-acceptance.md for invocation and limits.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, mkdtemp, open, readFile, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { disposableProfile } from './coding-website-profile.mjs';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const expectedPlaywright = '1.64.0-alpha-1790635538000';
const args = process.argv.slice(2);
assert(args.includes('--run-live'), 'Pass --run-live explicitly: this uses the signed-in coding account.');
const modulePath = args[args.indexOf('--playwright') + 1];
assert(args.includes('--playwright') && modulePath, 'Pass --playwright /absolute/path/to/playwright/index.mjs.');
const browserModule = resolve(modulePath);
const installed = JSON.parse(await readFile(join(dirname(browserModule), 'package.json'), 'utf8'));
assert.equal(installed.version, expectedPlaywright, 'Use the researched, pinned browser verifier.');
const { chromium } = await import(pathToFileURL(browserModule).href);
// Refuse missing browser/runtime prerequisites before consuming model usage.
const browserPreflight = await chromium.launch({headless: true});
await browserPreflight.close();
const binary = join(repo, 'target', 'debug', `jarvisd${process.platform === 'win32' ? '.exe' : ''}`);
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const binaryHash = hash(await readFile(binary));
const evidence = join(repo, 'artifacts', 'reviews', 'coding-website');
await mkdir(evidence, { recursive: true });
const continuing = args.includes('--continue-from');
const previousPath = args[args.indexOf('--continue-from') + 1];
assert(!continuing || previousPath, '--continue-from requires a disposable run directory.');
const root = await disposableProfile(evidence,
  continuing ? resolve(previousPath) : await mkdtemp(join(evidence, 'run-')));
const previous = continuing ? JSON.parse(await readFile(join(root, 'verification.json'), 'utf8')) : null;
if (previous) {
  assert.equal(previous.passed, false, 'a passing acceptance run needs no continuation');
  assert.equal(previous.adapter, 'codex-acp 2.2.2');
  assert.match(previous.task_id, /^[a-zA-Z0-9-]{36}$/);
}
const out = continuing ? await mkdtemp(join(root, 'continuation-')) : root;
const work = join(root, 'work');
if (!continuing) {
  await mkdir(work);
  await mkdir(join(root, 'config'));
}
const report = { platform: process.platform, started_at: new Date().toISOString(),
  adapter: 'codex-acp 2.2.2', playwright: installed.version, binary_sha256: binaryHash,
  harness_sha256: hash(await readFile(fileURLToPath(import.meta.url))),
  passed: false, checks: [], browser: [] };
const save = () => writeFile(join(out, 'verification.json'), JSON.stringify(report, null, 2));
const passed = name => { report.checks.push(name); console.log(`PASS ${name}`); };
const freePort = async () => {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((ok, fail) => server.close(error => error ? fail(error) : ok()));
  return port;
};
const poll = async (label, predicate, milliseconds = 30_000) => {
  const deadline = Date.now() + milliseconds;
  do {
    const value = await predicate();
    if (value) return value;
    await delay(250);
  } while (Date.now() < deadline);
  throw new Error(`${label} exceeded ${milliseconds}ms`);
};
const sentinel = 'Unrelated owner work must remain byte-for-byte unchanged.\n';
const verifier = `import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {addTask,toggleTask,filterTasks} from './board.mjs';
const initial=[{id:'one',title:'First',done:false},{id:'two',title:'Second',done:true}];
const original=JSON.stringify(initial);
const added=addTask(initial,'  Third task  ','three');
assert.equal(added.length,3); assert.equal(added[2].title,'Third task');
assert.equal(added[2].id,'three'); assert.equal(added[2].done,false);
assert.equal(JSON.stringify(initial),original);
assert.deepEqual(filterTasks(added,'active').map(t=>t.id),['one','three']);
assert.deepEqual(filterTasks(added,'completed').map(t=>t.id),['two']);
assert.equal(filterTasks(added,'all').length,3);
const toggled=toggleTask(added,'one'); assert.equal(toggled[0].done,true);
assert.equal(added[0].done,false);
assert.deepEqual(toggleTask(added,'missing'),added);
assert.deepEqual(addTask(initial,'   ','empty'),initial);
assert.match(await readFile('index.html','utf8'),/name=["']viewport["']/);
console.log('independent-board-check-passed');
`;
if (!continuing) {
  await writeFile(join(work, 'owner.txt'), sentinel);
  await writeFile(join(work, 'verify.mjs'), verifier);
}
const preserved = { 'owner.txt': hash(sentinel), 'verify.mjs': hash(verifier) };
report.preserved_sha256 = preserved;
// The verifier-owned static server has a fixed file allowlist and binds only loopback.
const preview = `import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';
const types={'/':'text/html','/index.html':'text/html','/styles.css':'text/css','/app.mjs':'text/javascript','/board.mjs':'text/javascript'};
createServer(async(req,res)=>{ const path=new URL(req.url,'http://localhost').pathname;
if(path==='/favicon.ico'){res.writeHead(204).end();return;}
if(!Object.hasOwn(types,path)){res.writeHead(404).end();return;}
try { const body=await readFile(path==='/'?'index.html':path.slice(1));
res.writeHead(200,{'Content-Type':types[path]+'; charset=utf-8'}).end(body);
}catch{res.writeHead(404).end();}}).listen(Number(process.argv[2]),'127.0.0.1');
`;
await writeFile(join(root, 'preview.mjs'), preview);
const brief = `Build a complete polished responsive task planning website called Harbor in this folder.
Use only index.html, styles.css, app.mjs and board.mjs; vanilla JS/CSS, no dependencies or external assets.
Preserve owner.txt and verify.mjs exactly. Do not initialize Git or run commits. Do not create or start a server.
Give it a distinctive coherent visual design, accessible labels, keyboard controls and a mobile layout without horizontal scrolling.
Seed exactly three tasks: Plan the week (active), Review the draft (active), Water the plants (completed).
Allow adding tasks, toggling completion, filtering All/Active/Completed, and persist changes in localStorage across reloads.
Render user text as text, not HTML. Empty/whitespace submissions must not add a task.
For independent acceptance expose data-testid attributes: task-input, add-task, task-item (each task row),
filter-all, filter-active, filter-completed. Each task row contains a real checkbox for completion.
board.mjs exports immutable pure functions addTask(tasks,title,id), toggleTask(tasks,id), filterTasks(tasks,filter).
Tasks have {id,title,done}. addTask trims titles and ignores whitespace-only input. toggleTask ignores unknown IDs.
filterTasks accepts all, active, completed. The UI must use these functions.
Run node verify.mjs and fix failures. Finish only after implementing the full site. Do not modify the supplied verifier.
No network, external services, browser automation, notifications, or unrelated work is requested.`;
const port = await freePort();
const base = `http://127.0.0.1:${port}/api/v1`;
await writeFile(join(root, 'config', 'config.toml'),
  `schema_version = 1\n[profile]\nname = "website-acceptance"\n[logging]\nlevel = "info"\n[daemon]\nhttp_enabled = true\nhttp_port = ${port}\nshutdown_timeout_seconds = 15\ntool_workspace_roots = [${JSON.stringify(work)}]\ncode_agent = ["npx", "-y", "@agentclientprotocol/codex-acp@2.2.2"]\n`);
const stdout = await open(join(out, 'daemon.stdout.log'), 'w');
const stderr = await open(join(out, 'daemon.stderr.log'), 'w');
let daemon, credential, browser, previewId, taskId;
const request = async (path, body, expected = 200) => {
  const response = await fetch(`${base}${path}`, { method: body === undefined ? 'GET' : 'POST',
    headers: { Authorization: `Bearer ${credential}`, 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(30_000) });
  const result = await response.json();
  assert.equal(response.status, expected, `${path}: ${JSON.stringify(result)}`);
  return result;
};
const tasks = async () => (await request('/code-agents/tasks')).tasks;
const tool = async (name, arguments_, decision = null) => {
  const run = await request('/runs', { objective: 'Disposable website acceptance' }, 201);
  const call = await request(`/tools/${name}/calls`, {run_id: run.run_id, arguments: arguments_}, decision ? 202 : 200);
  if (!decision) return call;
  assert.equal(call.state, 'awaiting_approval');
  await request(`/approvals/${call.approval_id}/decision`, {decision});
  if (decision === 'deny') {
    const refused = await request(`/calls/${call.call_id}/resume`, {arguments: arguments_}, 409);
    assert.equal(refused.code, 'conflict');
    assert.equal(refused.retryable, false);
    assert.equal(refused.message, 'the approval does not authorize at this instant');
    return call;
  }
  const resumed = await request(`/calls/${call.call_id}/resume`, {arguments: arguments_});
  assert.equal(resumed.state, 'confirmed');
  return resumed;
};
const output = reply => {
  assert.equal(reply.state, 'confirmed', JSON.stringify(reply));
  assert.equal(reply.truncated, false, 'truncated tool output is not complete acceptance evidence');
  return JSON.parse(reply.output);
};
const preservedFiles = async () => {
  for (const [name, digest] of Object.entries(preserved)) {
    assert.equal(hash(await readFile(join(work, name))), digest, `${name} was changed`);
  }
};
try {
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.toUpperCase().startsWith('JARVIS_')));
  daemon = spawn(binary, ['--root', root], { env: {...env, JARVIS_NO_BUILTIN_ABILITIES: '1'},
    windowsHide: true, stdio: ['ignore', stdout.fd, stderr.fd] });
  let spawnError;
  daemon.on('error', error => { spawnError = error; });
  await poll('daemon readiness', async () => {
    if (spawnError) throw spawnError;
    assert.equal(daemon.exitCode, null, 'daemon exited before ready');
    try {
      credential = (await readFile(join(root, 'config', 'client.credential'), 'utf8')).trim();
      await tasks();
      return true;
    } catch { return false; }
  });
  console.log(`Evidence: ${out}`);
  const before = await tasks();
  if (!continuing) assert.equal(before.length, 0);
  else {
    const prior = before.find(item => item.task_id === previous.task_id);
    assert(prior && ['failed','cancelled','interrupted'].includes(prior.status));
    assert.equal(resolve(prior.repository), work, 'continuation must retain this disposable workspace');
    await preservedFiles();
    report.continued_from = previous.task_id;
  }
  const taskArgs = {task: brief, allow_commands: true, allow_network: false, minutes: 10,
    checks: [{program: 'node', arguments: ['verify.mjs']}]};
  await tool('jarvis.code.delegate_start', taskArgs, 'deny');
  assert.deepEqual((await tasks()).map(task => task.task_id), before.map(task => task.task_id), 'denied task launched a job');
  await preservedFiles();
  passed('denied coding request creates no job');
  const started = continuing
    ? await tool('jarvis.code.delegate_resume', {task_id:previous.task_id}, 'approve')
    : await tool('jarvis.code.delegate_start', taskArgs, 'approve');
  taskId = output(started).task_id;
  if (continuing) assert.notEqual(taskId, previous.task_id, 'continuation must have a fresh identity');
  report.task_id = taskId;
  let lastStatus;
  const completed = await poll('real coding task', async () => {
    const task = (await tasks()).find(item => item.task_id === taskId);
    if (task.status !== lastStatus) { console.log(`Coding: ${task.status}`); lastStatus = task.status; }
    return ['queued', 'running'].includes(task.status) ? null : task;
  }, 660_000);
  // Keep local provider output out of the public report. The disposable DB retains diagnostics.
  report.task_status = completed.status;
  report.agent_outcome = completed.result?.outcome;
  report.verification = completed.result?.verification?.status;
  assert.equal(completed.status, 'completed', 'inspect disposable task history for provider failure');
  assert.equal(completed.result?.delivery_status, 'checks_passed');
  assert.equal(completed.result?.verification?.status, 'passed');
  await preservedFiles();
  passed('real coding job and independent checks completed; original files preserved');
  const previewPort = await freePort();
  const url = `http://127.0.0.1:${previewPort}`;
  const processReply = await tool('jarvis.process.start', {program: 'node',
    arguments: [join(root, 'preview.mjs'), String(previewPort)], port: previewPort}, 'approve');
  previewId = output(processReply).process_id;
  report.preview_id = previewId;
  await poll('preview ready', async () => {
    try { return (await fetch(url, {signal: AbortSignal.timeout(1000)})).ok; } catch { return false; }
  });
  passed('approved managed loopback preview is reachable');
  browser = await chromium.launch({headless: true});
  report.browser_version = browser.version();
  for (const [name, viewport, mobile] of [['desktop',{width:1440,height:1000},false], ['mobile',{width:390,height:844},true]]) {
    const context = await browser.newContext({viewport, isMobile: mobile, hasTouch: mobile});
    const errors = [];
    await context.route('**/*', route => new URL(route.request().url()).origin === url ? route.continue() : route.abort());
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    await page.goto(url);
    await page.getByTestId('task-item').first().waitFor();
    assert.equal(await page.getByTestId('task-item').count(), 3);
    await page.getByTestId('filter-active').click();
    assert.equal(await page.getByTestId('task-item').count(), 2);
    await page.getByTestId('filter-completed').click();
    assert.equal(await page.getByTestId('task-item').count(), 1);
    await page.getByTestId('filter-all').click();
    await page.getByTestId('task-input').fill('   ');
    await page.getByTestId('task-input').press('Enter');
    assert.equal(await page.getByTestId('task-item').count(), 3);
    const title = `${name} acceptance <b>literal</b>`;
    await page.getByTestId('task-input').fill(title);
    await page.getByTestId('add-task').click();
    assert.equal(await page.getByTestId('task-item').count(), 4);
    const added = page.getByTestId('task-item').filter({hasText: title});
    assert.equal(await added.count(), 1);
    assert.equal(await added.locator('b').count(), 0, 'user input became HTML');
    await added.getByRole('checkbox').check();
    await page.reload();
    await page.getByTestId('filter-completed').click();
    assert.equal(await page.getByTestId('task-item').count(), 2);
    assert.equal(await page.getByTestId('task-item').filter({hasText:title}).getByRole('checkbox').isChecked(), true);
    await page.getByTestId('filter-all').click();
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'horizontal overflow');
    // A scrolled mobile visual viewport can paint an offscreen fixed skip link
    // inside a full-page capture. Capture from the page's actual initial viewport.
    await page.evaluate(() => scrollTo(0, 0));
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.screenshot({path: join(out, `${name}.png`), fullPage: true});
    assert.deepEqual(errors, [], 'browser errors');
    report.browser.push({name, viewport, emulated_mobile: mobile, interactions: 'passed', errors: 0});
    await context.close();
    passed(`${name} add/toggle/filter/reload/escaping/layout`);
  }
  await browser.close(); browser = null;
  await tool('jarvis.process.stop', {process_id:previewId});
  await poll('managed preview stopped', async () => {
    const state = output(await tool('jarvis.process.status', {process_id:previewId}));
    return state.process.status === 'stopped' && !state.port_reachable;
  });
  await assert.rejects(fetch(url, {signal: AbortSignal.timeout(2000)}), 'preview still accepts connections after Stop');
  await preservedFiles();
  passed('preview stopped, port closed and unrelated files preserved');
  report.passed = true;
} catch (error) {
  report.error = String(error);
  process.exitCode = 1;
  console.error(String(error));
} finally {
  if (browser) await browser.close().catch(() => {});
  if (daemon && daemon.exitCode === null) {
    try {
      await request('/shutdown?force=true', {}, 202);
      await poll('daemon shutdown', () => daemon.exitCode !== null);
      assert.equal(daemon.exitCode, 0);
      report.daemon_cleanup = 'passed';
    } catch (error) {
      report.passed = false; report.cleanup_error = String(error); process.exitCode = 1;
      daemon.kill();
      await poll('daemon termination', () => daemon.exitCode !== null).catch(() => {});
    }
  }
  await stdout.close(); await stderr.close();
  report.finished_at = new Date().toISOString();
  await save();
  console.log(`Result: ${join(out, 'verification.json')}`);
}
