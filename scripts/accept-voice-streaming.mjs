// Opt-in, fixed synthetic audio in a disposable profile. Never changes the owner's library.
import assert from 'node:assert/strict';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {createServer} from 'node:net';
import {mkdir,mkdtemp,open,readFile,writeFile} from 'node:fs/promises';
import {join,resolve,isAbsolute} from 'node:path';
import {pathToFileURL} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
const args=process.argv.slice(2),option=name=>args[args.indexOf(name)+1];
assert(args.includes('--run-live'),'Pass --run-live for real local synthesis.');
const runtime=option('--runtime'),playwright=option('--playwright');
assert(isAbsolute(runtime)&&isAbsolute(playwright),'Pass absolute --runtime and --playwright paths.');
const {chromium}=await import(pathToFileURL(playwright).href);
const daemon=args.includes('--daemon')?option('--daemon'):resolve('target/debug/jarvisd.exe');
await mkdir('target/voice-stream-acceptance',{recursive:true});
const root=await mkdtemp(resolve('target/voice-stream-acceptance/profile-'));
await mkdir(join(root,'config'));
const report={passed:false,checks:[],measurements:[]},pass=label=>{report.checks.push(label);console.log('PASS '+label);};
const reserve=createServer();await new Promise(done=>reserve.listen(0,'127.0.0.1',done));const port=reserve.address().port;await new Promise(done=>reserve.close(done));
const origin=`http://127.0.0.1:${port}`;
let child,log,browser,credential,page,masked=false;
const headers=()=>({authorization:'Bearer '+credential,'content-type':'application/json'});
const request=(path,body,method='POST')=>fetch(origin+'/api/v1/'+path,{method,headers:headers(),...(body?{body:JSON.stringify(body)}:{})});
async function poll(label,fn,ms=180000){const start=Date.now();while(Date.now()-start<ms){if(await fn())return;await delay(200);}throw Error('Timed out: '+label);}
async function library(){return (await request('settings/voices',null,'GET')).json();}
async function execution(){return (await (await request('settings/voices/runtime',null,'GET')).json()).providers[0].execution;}
async function start(){
 log=await open(join(root,'daemon.log'),'a');
 child=spawn(daemon,['--root',root],{windowsHide:true,stdio:['ignore',log.fd,log.fd],env:masked?{...process.env,HIP_VISIBLE_DEVICES:'-1',CUDA_VISIBLE_DEVICES:'-1'}:process.env});
 await poll('daemon',async()=>{if(child.exitCode!==null)throw Error('Daemon exited');try{credential=(await readFile(join(root,'config/client.credential'),'utf8')).trim();return (await request('settings/voices',null,'GET')).ok;}catch{return false;}});
}
async function stop(){assert((await request('shutdown')).ok);await poll('shutdown',async()=>child.exitCode!==null,40000);await log.close();log=null;}
const description='A calm adult male narrator with a clear British accent.';
const text='Hello. I am here to help you with whatever comes next.';
async function killOwnedWorker(){
 assert.equal(process.platform,'win32');
 const script=`$all=@(Get-CimInstance Win32_Process); $ids=@(${child.pid}); for($i=0;$i -lt 5;$i++){ $ids+=@($all | Where-Object {$ids -contains $_.ParentProcessId} | ForEach-Object {$_.ProcessId}) }; $workers=@($all | Where-Object {$ids -contains $_.ProcessId -and $_.Name -eq 'python.exe'}); foreach($p in $workers){Stop-Process -Id $p.ProcessId -Force}; $workers.Count`;
 const {stdout}=await promisify(execFile)('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{windowsHide:true});assert(Number(stdout.trim())>0);
}
try{
 await writeFile(join(root,'config/config.toml'),`schema_version = 1
[profile]
name = "voice-stream-acceptance"
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
runtime_dir = '${runtime.replaceAll('\\','/')}'
`);
 await start();
 assert.equal((await fetch(origin+'/api/v1/settings/voices/prepare',{method:'POST'})).status,401);
 assert.equal((await request('settings/voices/design',{provider:'designer',description,text,seed:42,quality:'unbounded'})).status,422);
 pass('warm-up requires authentication and unknown quality is rejected');
 browser=await chromium.launch({headless:true,channel:'msedge',args:['--autoplay-policy=no-user-gesture-required']});
 page=await browser.newPage({viewport:{width:1440,height:1100}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
   window.__transfers=[];const fetch0=window.fetch;window.fetch=async(...args)=>{const response=await fetch0(...args);const path=String(args[0]);if(path.endsWith('/preview')||path.endsWith('/design'))response.clone().arrayBuffer().then(data=>window.__transfers.push({path,at:Date.now(),bytes:data.byteLength,text:path.endsWith('/design')?new TextDecoder().decode(data):null})).catch(()=>{});return response;};
   window.__pcm=[];const original=AudioBufferSourceNode.prototype.start;
   AudioBufferSourceNode.prototype.start=function(...args){const row={at:Date.now(),duration:this.buffer?.duration,state:this.context.state};window.__pcm.push(row);this.addEventListener("ended",()=>row.ended=Date.now());return original.apply(this,args);};
 });
 await page.goto(origin+'/#token='+encodeURIComponent(credential));await page.locator('#pill-link').getByText('linked',{exact:true}).waitFor();
 await page.locator('#open-settings').click();await page.locator('#settings-tab-voice').click();await page.locator('#voice-library-create').click();
 await page.getByLabel('Name',{exact:true}).fill('Streaming narrator');
 await page.locator('#voice-design-description').fill(description);await page.locator('#voice-design-sample').fill(text);
 let warmDone=false;const phases=new Set();const phasePoll=(async()=>{while(!warmDone){phases.add((await execution()).phase);await delay(250);}})();
 const warmStart=Date.now(),warmResponse=page.waitForResponse(r=>r.url().endsWith('/settings/voices/prepare'),{timeout:180000});
 await page.locator('#voice-library-prepare').click();
 const prepared=await warmResponse;assert.equal(prepared.status(),200);await prepared.finished();warmDone=true;await phasePoll;
 await page.locator('#voice-prepare-status').getByText(/Model ready/).waitFor();
 const ready=await execution();assert.equal(ready.phase,'ready');assert.equal(ready.backend,'amd');assert.match(ready.device,/RX 9070 XT/);
 assert(phases.has('loading')&&phases.has('warming'));report.warm_seconds=(Date.now()-warmStart)/1000;report.phases=[...phases];report.execution=ready;
 pass('visible warm-up completes real design and reference inference on RX 9070 XT');
 await page.locator('#voice-quality').scrollIntoViewIfNeeded();await page.screenshot({path:join(root,'controls-desktop.png')});await page.setViewportSize({width:390,height:844});await page.locator('#voice-quality').scrollIntoViewIfNeeded();await page.screenshot({path:join(root,'controls-mobile.png')});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));await page.setViewportSize({width:1440,height:1100});
 for(const quality of ['fast','balanced','quality']){
  await page.locator('#voice-quality').selectOption(quality);await page.evaluate(()=>{window.__pcm=[];window.__transfers=[];});
  const began=Date.now(),waiting=page.waitForResponse(r=>r.url().endsWith('/settings/voices/design'),{timeout:180000});
  await page.locator('#voice-library-generate').click();const response=await waiting;assert.equal(response.status(),200);assert.match(response.headers()['content-type'],/ndjson/);
  await poll('browser received design EOF',async()=>(await page.evaluate(()=>window.__transfers.some(t=>t.path.endsWith('/design')))));const transfer=await page.evaluate(()=>window.__transfers.find(t=>t.path.endsWith('/design')));const finished=transfer.at;const events=transfer.text.trim().split('\n').map(JSON.parse);const complete=events.at(-1);
  assert.equal(complete.event,'complete');assert.equal(complete.design.quality,quality);assert.equal(complete.execution.backend,'amd');assert(events.filter(e=>e.event==='audio').length>1);
  const starts=await page.evaluate(()=>window.__pcm);assert(starts.length>1);assert(starts[0].at<finished);assert.equal(starts[0].state,'running');
  console.log('PASS streamed quality '+quality);
  report.measurements.push({quality,first_buffer_scheduled_ms:starts[0].at-began,generation_ms:finished-began,audio_seconds:starts.reduce((n,c)=>n+c.duration,0)});
  await poll('playback and complete reference',()=>page.locator('#voice-library-generate').isEnabled());assert(await page.locator('#voice-library-listen').isEnabled());
 }
 pass('all three quality modes play PCM before HTTP completion and retain complete references');
 await page.getByRole('button',{name:'Save and use in console',exact:true}).click();
 await page.getByText('Voices saved. New speech uses these settings immediately; they survive restart.',{exact:true}).waitFor();
 const saved=(await library()).library,reference=saved.voices[0].design.reference_audio;
 await page.locator('#voice-quality').selectOption('balanced');await page.locator('#voice-library-save').click();
 await poll('quality saved',async()=>(await library()).library.voices[0].settings.quality==='balanced');
 const savedBalanced=(await library()).library;assert.equal(savedBalanced.voices[0].design.reference_audio,reference);assert.equal(savedBalanced.voices[0].design.quality,'quality');
 pass('saved quality hot-applies without changing generated identity');
 await page.getByText('Try this voice with different words',{exact:true}).click();
 await page.locator('#voice-library-sample').fill('This is a longer sentence for streaming playback. You should hear the beginning while the rest is still being generated.');
 await page.evaluate(()=>{window.__pcm=[];window.__transfers=[];});const previewResponse=page.waitForResponse(r=>r.url().endsWith('/settings/voices/preview'));
 const previewStart=Date.now();await page.locator('#voice-library-preview').click();const preview=await previewResponse;assert.equal(preview.status(),200);assert.match(preview.headers()['content-type'],/audio\/pcm/);
 await poll('browser received preview EOF',async()=>(await page.evaluate(()=>window.__transfers.length))>0);const previewEnd=await page.evaluate(()=>window.__transfers.at(-1).at);const previewStarts=await page.evaluate(()=>window.__pcm);assert(previewStarts[0].at<previewEnd);
 console.log('INFO preview HTTP complete '+JSON.stringify({chunks:previewStarts.length,seconds:previewStarts.reduce((n,c)=>n+c.duration,0)}));
 report.preview={first_buffer_scheduled_ms:previewStarts[0].at-previewStart,generation_ms:previewEnd-previewStart};
 await poll('preview played',()=>page.locator('#voice-library-preview').isEnabled());
 await page.screenshot({path:join(root,'desktop.png')});await page.setViewportSize({width:390,height:844});await page.screenshot({path:join(root,'mobile.png')});await page.setViewportSize({width:1440,height:1100});
 const legacy=await request('speech',{text:'The saved voice is ready.'});assert.equal(legacy.status,200);assert.equal(Buffer.from(await legacy.arrayBuffer()).toString('ascii',0,4),'RIFF');
 const consoleStream=await request('speech',{text,stream:true});assert.equal(consoleStream.status,200);assert.match(consoleStream.headers.get('content-type'),/audio\/pcm/);assert((await consoleStream.arrayBuffer()).byteLength>0);
 pass('new-text preview streams in browser; Console route streams PCM and legacy clients receive WAV');
 const consolePage=await browser.newPage();consolePage.on('pageerror',e=>errors.push(e.message));
 await consolePage.addInitScript(()=>{
  window.__pcm=[];window.__speechEnd=null;const start=AudioBufferSourceNode.prototype.start;
  AudioBufferSourceNode.prototype.start=function(...args){const row={at:Date.now(),duration:this.buffer?.duration,state:this.context.state};window.__pcm.push(row);this.addEventListener('ended',()=>row.ended=Date.now());return start.apply(this,args);};
  const original=window.fetch;window.fetch=async(...args)=>{const response=await original(...args);if(String(args[0]).endsWith('/speech')&&args[1]?.method==='POST')response.clone().arrayBuffer().then(()=>window.__speechEnd=Date.now()).catch(()=>{});return response;};
 });
 await consolePage.goto(origin+'/#token='+encodeURIComponent(credential));await consolePage.locator('#pill-link').getByText('linked',{exact:true}).waitFor();
 await consolePage.locator('#voice-options-toggle').click();await consolePage.locator('#speak').check();
 await consolePage.getByRole('textbox',{name:'Message',exact:true}).fill('Check the streaming local voice.');
 const consoleResponse=consolePage.waitForResponse(r=>r.url().endsWith('/api/v1/speech')&&r.request().method()==='POST',{timeout:180000});
 await consolePage.getByRole('button',{name:'Send message',exact:true}).click();const actualSpeech=await consoleResponse;
 assert.equal(actualSpeech.status(),200);assert.match(actualSpeech.headers()['content-type'],/audio\/pcm/);assert.equal(actualSpeech.request().postDataJSON().stream,true);
 await poll('Console PCM playback completed',()=>consolePage.evaluate(()=>window.__speechEnd&&window.__pcm.length>0&&window.__pcm.every(p=>p.ended)));
 report.console=await consolePage.evaluate(()=>({chunks:window.__pcm.length,first_buffer_at:window.__pcm[0].at,http_complete_at:window.__speechEnd,all_running:window.__pcm.every(p=>p.state==='running')}));
 assert(report.console.first_buffer_at<report.console.http_complete_at);assert(report.console.all_running);await consolePage.close();
 pass('Console speaks a real reply through Web Audio before generation finishes and drains every buffer');
 // Stop a live generation after PCM has begun; the partial reference must not replace the saved voice.
 await page.evaluate(()=>window.__pcm=[]);await page.locator('#voice-design-sample').fill('Hello. This is a longer sample to test stopping voice generation before it finishes and before it can be saved.');
 await page.locator('#voice-library-generate').click();await poll('partial audio',async()=>(await page.evaluate(()=>window.__pcm.length))>0);
 await page.locator('#voice-library-cancel-generation').click();await poll('worker released',async()=>(await execution()).phase==='cold');
 assert.equal((await library()).library.voices[0].design.reference_audio,reference);
 pass('Stop after first audio releases the worker and leaves saved identity unchanged');
 await stop();await start();assert.deepEqual((await library()).library,savedBalanced);
 const warmId='cancel-warm';const warming=request('settings/voices/prepare',{provider:'designer',request_id:warmId});await poll('preparation started',async()=>['checking','loading','warming'].includes((await execution()).phase));assert.equal((await request('settings/voices/cancel',{request_id:warmId})).status,204);assert(!(await warming).ok);await poll('preparation stopped',async()=>(await execution()).phase==='cold');pass('preparation cancellation releases the worker before readiness');
 const restartWarm=await request('settings/voices/prepare',{provider:'designer'});assert.equal(restartWarm.status,200);assert.equal((await execution()).backend,'amd');
 pass('quality and reference survive restart and warm-up recovers after cancellation');
 if(process.platform==='win32'){
  // Kill only Python descendants of this disposable daemon, after first audio.
  const broken=await request('speech',{text:'This deliberately long sentence checks a worker failure during streaming. '.repeat(15),stream:true});assert.equal(broken.status,200);
  const reader=broken.body.getReader();assert(!(await reader.read()).done);await killOwnedWorker();
  let failed=false;try{while(!(await reader.read()).done){}}catch{failed=true;}assert(failed,'partial failure must end abnormally');
  await poll('partial failure release',async()=>(await execution()).phase==='cold');assert.equal((await execution()).backend,'amd','no CPU replay after first audio');
  pass('worker failure after first audio ends the response without CPU replay');
 }
 await stop();masked=true;await start();
 const cpuWarm=await request('settings/voices/prepare',{provider:'designer'});assert.equal(cpuWarm.status,200);const cpu=await execution();assert.equal(cpu.phase,'ready');assert.equal(cpu.backend,'cpu');assert(cpu.fallback_reason);
 const cpuSpeech=await request('speech',{text:'The same voice works on the CPU.',stream:true});assert.equal(cpuSpeech.status,200);assert((await cpuSpeech.arrayBuffer()).byteLength>0);assert.equal((await library()).library.voices[0].design.reference_audio,reference);
 report.cpu=cpu;pass('masked GPU warms and streams through CPU fallback with unchanged saved voice');
 assert.deepEqual(errors,[]);report.passed=true;
}catch(error){
 if(page){await page.screenshot({path:join(root,'failure.png')}).catch(()=>{});report.failure=String(error);report.browser_state=await page.evaluate(()=>({pcm:window.__pcm,notes:[...document.querySelectorAll('.voice-library .help')].map(n=>n.textContent)})).catch(()=>null);}
 throw error;
}finally{
 if(browser)await browser.close();
 if(child&&child.exitCode===null){try{await stop();report.stopped=true;}catch{child.kill();report.passed=false;report.stopped=false;}}
 if(log)await log.close();await writeFile(join(root,'verification.json'),JSON.stringify(report,null,2));console.log('Evidence: '+root);
}
