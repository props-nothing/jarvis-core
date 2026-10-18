// Real AMD speech, synthetic model text, disposable profile. No cloud or owner library changes.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createServer} from 'node:net';
import {mkdir,mkdtemp,open,readFile,writeFile} from 'node:fs/promises';
import {join,resolve,isAbsolute} from 'node:path';
import {pathToFileURL} from 'node:url';
import {setTimeout as delay} from 'node:timers/promises';
const args=process.argv.slice(2),option=n=>args[args.indexOf(n)+1];
assert(args.includes('--run-live'));const runtime=option('--runtime'),playwright=option('--playwright'),daemon=option('--daemon');
assert([runtime,playwright,daemon].every(isAbsolute));const {chromium}=await import(pathToFileURL(playwright).href);
await mkdir('target/voice-queue-acceptance',{recursive:true});const root=await mkdtemp(resolve('target/voice-queue-acceptance/profile-'));await mkdir(join(root,'config'));
const socket=createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));
const origin=`http://127.0.0.1:${port}`,report={passed:false,checks:[],scenarios:[]};let child,log,browser,page,token;
const request=(path,body,method='POST')=>fetch(origin+'/api/v1/'+path,{method,headers:{authorization:'Bearer '+token,'content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});
async function poll(label,check,timeout=180000){const end=Date.now()+timeout;while(Date.now()<end){if(await check())return;await delay(100);}throw Error('Timeout: '+label);}
const pass=label=>{report.checks.push(label);console.log('PASS '+label);};
try{
 await writeFile(join(root,'config/config.toml'),`schema_version=1\n[profile]\nname="voice-queue-acceptance"\n[logging]\nlevel="warn"\n[daemon]\nshutdown_timeout_seconds=30\nhttp_enabled=true\nhttp_port=${port}\nexecutor_model="scripted"\n[[daemon.speech.providers]]\nid="designer"\nkind="local-voxcpm2"\nruntime_dir='${runtime.replaceAll('\\','/')}'\n`);
 log=await open(join(root,'daemon.log'),'a');child=spawn(daemon,['--root',root],{windowsHide:true,stdio:['ignore',log.fd,log.fd]});
 await poll('ready',async()=>{if(child.exitCode!==null)throw Error('Daemon exited');try{token=(await readFile(join(root,'config/client.credential'),'utf8')).trim();return (await request('settings/voices',null,'GET')).ok;}catch{return false;}});
 const warming=request('settings/voices/prepare',{provider:'designer'});
 await poll('warm-up running',async()=>{const r=await (await request('settings/voices/runtime',null,'GET')).json();return ['checking','loading','warming'].includes(r.providers[0].execution.phase);});
 const busy=await request('settings/voices/prepare',{provider:'designer'});assert.equal(busy.status,503);assert.equal(busy.headers.get('retry-after'),'1');
 const prepared=await warming;assert.equal(prepared.status,200);const execution=(await prepared.json()).execution;assert.equal(execution.backend,'amd');assert.match(execution.device,/RX 9070 XT/);report.execution=execution;
 pass('real local admission returns retryable Busy, then AMD preparation completes');
 const generated=await request('settings/voices/design',{provider:'designer',description:'A calm adult male narrator with a clear British accent.',text:'Hello. I will speak each sentence in the correct order.',seed:42,quality:'fast'});
 assert.equal(generated.status,200);const {design}=await generated.json();const {library}=await (await request('settings/voices',null,'GET')).json();
 library.voices=[{id:'queue-test',name:'Queue test',provider:'designer',model:'voxcpm2',voice:'designed',settings:{quality:'fast'},design}];library.default_voice='queue-test';library.channels={console:'queue-test'};library.backups=[];
 assert.equal((await request('settings/voices',library,'PUT')).status,200);
 browser=await chromium.launch({headless:true,channel:'msedge',args:['--autoplay-policy=no-user-gesture-required']});page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 // Model SSE and failure responses are controlled; successful speech calls reach the real daemon/GPU.
 await page.addInitScript(()=>{
  const nativeFetch=window.fetch;let run=0;
  window.__trace={requests:[],audio:[],browser:[],stops:0,inflight:0,maxInflight:0};
  window.__scenario={lines:[],interval:120,statuses:[]};
  const original=AudioBufferSourceNode.prototype.start,stop=AudioBufferSourceNode.prototype.stop,contexts=new Map();
  AudioBufferSourceNode.prototype.start=function(...args){if(!contexts.has(this.context))contexts.set(this.context,contexts.size);
   const row={context:contexts.get(this.context),at:Date.now(),duration:this.buffer?.duration};window.__trace.audio.push(row);this.addEventListener('ended',()=>row.ended=Date.now());return original.apply(this,args);};
  AudioBufferSourceNode.prototype.stop=function(...args){window.__trace.stops++;return stop.apply(this,args);};
  let browserGeneration=0;
  Object.defineProperty(window,'SpeechSynthesisUtterance',{value:class {constructor(text){this.text=text;}}});
  Object.defineProperty(window,'speechSynthesis',{value:{getVoices:()=>[],get speaking(){return false;},get pending(){return false;},cancel(){browserGeneration++;},speak(utterance){
   const epoch=browserGeneration,row={text:utterance.text};window.__trace.browser.push(row);setTimeout(()=>{if(epoch!==browserGeneration)return;utterance.onstart?.();setTimeout(()=>{if(epoch!==browserGeneration)return;row.ended=Date.now();utterance.onend?.();},40);},0);
  }}});
  window.fetch=async(...args)=>{
   const path=String(args[0]),options=args[1]||{},scenario=window.__scenario,trace=window.__trace;
   if(path==='/api/v1/runs'&&options.method==='POST')return Response.json({run_id:'queue-run-'+(++run),session_id:'queue-session'});
   if(/\/api\/v1\/runs\/queue-run-\d+\/stream$/.test(path)){
    let next=0;const lines=[...scenario.lines],encoder=new TextEncoder();
    return new Response(new ReadableStream({start(controller){function send(){let event,payload;
     if(next<lines.length){event='output_delta';payload={text:lines[next]};}else if(next===lines.length){event='output_completed';payload={text:lines.join('')};}else{event='run_completed';payload={};}
     controller.enqueue(encoder.encode(`id: ${++next}\nevent: ${event}\ndata: ${JSON.stringify({payload})}\n\n`));
     if(event==='run_completed'){controller.close();trace.modelDone=Date.now();}else setTimeout(send,scenario.interval);
    }send();} }),{headers:{'content-type':'text/event-stream'}});
   }
   if(path==='/api/v1/speech'&&options.method==='POST'){
    const row={text:JSON.parse(options.body).text,at:Date.now()},status=scenario.statuses.shift();trace.requests.push(row);
    if(status){row.status=status;return new Response('{}',{status,headers:{'content-type':'application/json',...(status===503?{'retry-after':'1'}:{})}});}
    trace.inflight++;trace.maxInflight=Math.max(trace.maxInflight,trace.inflight);
    try{const response=await nativeFetch(...args);row.status=response.status;row.bytes=0;const reader=response.body.getReader();let finished=false;
     function finish(failed){if(finished)return;finished=true;row.eof=Date.now();row.failed=failed;trace.inflight--;}
     const counted=new ReadableStream({async pull(controller){try{const chunk=await reader.read();if(chunk.done){finish(false);controller.close();}else{row.bytes+=chunk.value.byteLength;controller.enqueue(chunk.value);}}catch(error){finish(true);controller.error(error);}},cancel(){finish(true);return reader.cancel();}});
     return new Response(counted,{status:response.status,headers:response.headers});}
    catch(error){trace.inflight--;throw error;}
   }
   return nativeFetch(...args);
  };
 });
 await page.goto(origin+'/#token='+encodeURIComponent(token));await page.locator('#pill-link').getByText('linked',{exact:true}).waitFor();await page.locator('#voice-options-toggle').click();await page.locator('#speak').check();
 async function scenario(name,lines,statuses=[],interval=120,browserFallback=false){
  await page.evaluate(({lines,statuses,interval})=>{window.__scenario={lines,statuses,interval};window.__trace={requests:[],audio:[],browser:[],stops:0,inflight:0,maxInflight:0};},{lines,statuses,interval});
  await page.locator('#input').fill('Read the synthetic queue test.');await page.locator('#send').click();
  await poll(name+' completed',()=>page.evaluate(({browserFallback})=>{const t=window.__trace;return t.modelDone&&Date.now()-t.modelDone>500&&!t.inflight&&(browserFallback?t.browser.length&&t.browser.every(r=>r.ended):t.audio.length&&t.audio.every(r=>r.ended));},{browserFallback}));
  // Allow item-completion callbacks to schedule a following queued item before inspecting the terminal UI state.
  await poll(name+' queue drained',()=>page.locator('#quiet').isHidden());
  const trace=await page.evaluate(()=>window.__trace);const normalized=s=>s.replace(/\s+/g,' ').trim();
  if(browserFallback){assert.equal(normalized(trace.browser.map(r=>r.text).join(' ')),normalized(lines.join('')));}
  else{
   assert.equal(normalized(trace.requests.filter(r=>r.status===200).map(r=>r.text).join(' ')),normalized(lines.join('')));
   assert.equal(trace.browser.length,0);assert.equal(trace.stops,0);assert(trace.requests.filter(r=>r.status===200).every(r=>r.bytes>0));assert.equal(trace.maxInflight,1);
   const contexts=[...new Set(trace.audio.map(r=>r.context))];assert.equal(contexts.length,trace.requests.filter(r=>r.status===200).length,'every synthesized item plays');
   assert(Math.abs(trace.audio.reduce((n,r)=>n+r.duration*96000,0)-trace.requests.filter(r=>r.status===200).reduce((n,r)=>n+r.bytes,0))<1,'every received PCM sample is scheduled');
   for(let i=1;i<contexts.length;i++){
    const previous=trace.audio.filter(r=>r.context===contexts[i-1]),next=trace.audio.filter(r=>r.context===contexts[i]);assert(next[0].at>=Math.max(...previous.map(r=>r.ended))-30,'next sentence waits for previous playback');
   }
  }
  report.scenarios.push({name,...trace});pass(name);
 }
 await scenario('AMD multi-line speech finishes each queued item before the next',Array.from({length:9},(_,i)=>`Line ${i+1}. Please finish speaking these words before moving to the following sentence.\n`));
 await scenario('busy responses retry the same AMD voice without dropping text',['Temporary waiting must not change the selected voice.\n','The next line must also finish speaking.\n'],[503,503]);
 await scenario('transient 429 retries without browser fallback',['A temporary rate limit should not lose this sentence.\n'],[429]);
 await scenario('browser fallback retains later model deltas beyond 1800 characters',['The first line uses the browser fallback.\n',('Later text remains in order without being cut short. ').repeat(42)+'The final marker is spoken.\n'],[502],1000,true);
 await page.evaluate(()=>{window.__scenario={lines:['This waiting sentence should stop when requested.\n'],statuses:[503,503,503],interval:120};window.__trace={requests:[],audio:[],browser:[],stops:0,inflight:0,maxInflight:0};});
 await page.locator('#input').fill('Stop the waiting speech.');await page.locator('#send').click();await poll('retry began',()=>page.evaluate(()=>window.__trace.requests.length>0));await page.locator('#quiet').click();
 const count=await page.evaluate(()=>window.__trace.requests.length);await delay(1300);assert.equal(await page.evaluate(()=>window.__trace.requests.length),count);assert.equal(await page.evaluate(()=>window.__trace.browser.length),0);pass('Stop cancels pending backoff without starting another voice');
 assert.deepEqual(errors,[]);report.passed=true;await page.screenshot({path:join(root,'console.png')});
}catch(error){report.failure=String(error);if(page)await page.screenshot({path:join(root,'failure.png')}).catch(()=>{});throw error;}
finally{
 await browser?.close();if(child&&child.exitCode===null){const stopped=await request('shutdown');if(stopped.ok)await poll('shutdown',()=>child.exitCode!==null,45000);else child.kill();}
 await log?.close();await writeFile(join(root,'verification.json'),JSON.stringify(report,null,2));console.log('Evidence: '+root);
}
