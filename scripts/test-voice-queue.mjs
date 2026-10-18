import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source=readFileSync(new URL('../apps/jarvisd/src/hud/hud.js',import.meta.url),'utf8');
const section=(start,end)=>source.slice(source.indexOf(start),source.indexOf(end,source.indexOf(start)));
const tick=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};}
function harness(){
 const notes=[],utterances=[],timers=[],played=[],requests=[];
 const h={window:{speechSynthesis:{getVoices:()=>[],speak:u=>utterances.push(u),cancel(){}}},S:{},token:'fixture',AbortController,DOMException,
  SpeechSynthesisUtterance:function(text){this.text=text;},URL:{revokeObjectURL(){}},say:s=>notes.push(s),resumeWake(){},stopListening(){},
  setTimeout(fn,ms){const timer={fn,ms};timers.push(timer);return timer;},clearTimeout(t){if(t)t.cancelled=true;}};
 vm.runInNewContext(section('  // ---- voice: speaking','  // How loud the voice is right now'),h);
 vm.runInNewContext(section('  function interruptSpeech()', '  // Whether the daemon has a speech provider'),h);
 h.attachAnalyser=()=>{};
 h.openSpeech=(session,item)=>{const ready=deferred(),received=deferred();const index=requests.length;
  const element={play(){played.push(index);this.onplaying?.();return Promise.resolve();},pause(){this.stopped=true;}};
  requests.push({ready,received,element,text:item.text,handle:{element,url:null,received:received.promise}});return ready.promise;};
 return Object.assign(h,{notes,utterances,timers,played,requests});
}
test('new lines keep playback ordered and synthesis admits one request until EOF',async()=>{
 const h=harness(),s=h.neuralSession();
 const lines=Array.from({length:7},(_,i)=>`Sentence number ${i} must finish before the next line starts.`);
 lines.forEach(text=>h.enqueue(s,text));s.ended=true;
 assert.equal(h.requests.length,1);
 h.requests[0].ready.resolve(h.requests[0].handle);await tick();
 assert.deepEqual(h.played,[0]);h.drive(s);assert.equal(h.requests.length,1,'first PCM is not synthesis EOF');
 for(let i=0;i<lines.length;i++){
  const r=h.requests[i];assert(r);r.ready.resolve(r.handle);await tick();
  assert.equal(h.played.at(-1),i);
  r.received.resolve();await tick();
  if(i+1<lines.length){h.requests[i+1].ready.resolve(h.requests[i+1].handle);await tick();assert.equal(h.played.at(-1),i,'prefetched audio must wait');}
  const ended=r.element.onended;ended();ended();await tick();
 }
 assert.deepEqual(h.played,[0,1,2,3,4,5,6]);assert.deepEqual(h.requests.map(r=>r.text),lines);assert.equal(h.S.voiceBusy,false);
});
test('browser fallback retains late deltas, finishes all text beyond 1800 characters, and Stop fences callbacks',async()=>{
 const h=harness(),s=h.neuralSession();const early='This first sentence should be spoken by the fallback voice.\n';
 h.feed(s,early);h.requests[0].ready.reject(Object.assign(new Error('refused'),{status:502}));await tick();
 assert(s.browser);assert.equal(h.playing.session,s);assert.equal(h.S.voiceBusy,true);
 const late=('The later words must also be spoken without truncation. ').repeat(45)+'The final marker survives.';
 h.feed(s,late);h.endFeed(s,early+late);
 for(let i=0;i<h.utterances.length;i++)h.utterances[i].onend?.();
 assert.equal(h.utterances.map(u=>u.text).join(' ').replace(/\s+/g,' ').trim(),(early+late).replace(/\s+/g,' ').trim());
 assert.equal(h.S.voiceBusy,false);assert.equal(h.requests.length,1);
 const stopped=h.neuralSession();h.feed(stopped,early);h.requests[1].ready.reject(new Error('offline'));await tick();
 h.feed(stopped,late);h.endFeed(stopped,null);const before=h.utterances.length;const stale=h.utterances.at(-1).onend;
 h.interruptSpeech();stale?.();assert.equal(h.utterances.length,before);
});
test('speech chunks bound long sentences and preserve every word',()=>{
 const h=harness(),text=('unpunctuated words keep their original order ').repeat(90)+'last 🐈';
 const chunks=h.groups(text);assert(chunks.every(c=>Array.from(c).length<=400));assert.equal(chunks.join(' '),text);
});
test('busy and 429 retries retain the sentence and Stop cancels backoff',async()=>{
 const h=harness();vm.runInNewContext(section('  function speechDelay(', '  function alive('),h);
 const statuses=[503,429,200];h.fetch=async()=>({status:statuses.shift(),ok:statuses.length===0,headers:{get:k=>k==='retry-after'&&statuses.length===2?'1':null},body:{async cancel(){}}});
 const controller=new AbortController(),s={controller,turn:h.playing.token};const result=h.speechResponse(s,{text:'Keep these words.'});
 await tick();assert.equal(h.timers[0].ms,1000);h.timers[0].fn();await tick();h.timers[1].fn();await tick();assert.equal((await result).status,200);
 h.fetch=async()=>({status:503,headers:{get:()=> '1'},body:{async cancel(){}}});const stopped=h.speechResponse(s,{text:'Stop this wait.'});await tick();
 controller.abort();await assert.rejects(stopped,{name:'AbortError'});assert(h.timers.at(-1).cancelled);
});
test('delayed wake listening cannot cancel preparation or queued speech',()=>{
 const h=harness();let listens=0;h.recognizer=null;h.startListening=()=>listens++;
 vm.runInNewContext(section('  function resumeWake()', '  $("mic").addEventListener'),h);
 h.S.wake=true;h.S.speaking=false;h.resumeWake();h.S.voiceBusy=true;h.timers.at(-1).fn();assert.equal(listens,0);
 h.S.voiceBusy=false;h.resumeWake();h.timers.at(-1).fn();assert.equal(listens,1);
});
test('permanent busy is bounded and never retries after its deadline',async()=>{
 const h=harness();let now=0,calls=0;h.Date={now:()=>now,parse:Date.parse};
 h.setTimeout=(fn,ms)=>{now+=ms;queueMicrotask(fn);return 1;};h.clearTimeout=()=>{};
 h.fetch=async()=>{calls++;return {status:503,headers:{get:()=> '30'},body:{async cancel(){}}};};
 const result=await h.speechResponse({controller:new AbortController(),turn:0},{text:'Bounded wait.'});
 assert.equal(result.status,503);assert.equal(now,90000);assert.equal(calls,4);
});
test('MP3 ready-to-play keeps synthesis occupied until the network stream ends',async()=>{
 const h=harness(),reads=[],sources=[];let update;
 class MediaSource {
  static isTypeSupported(){return true;}
  constructor(){this.readyState='open';sources.push(this);}
  addEventListener(name,fn){this[name]=fn;}
  addSourceBuffer(){return {updating:false,addEventListener(name,fn){update=fn;},appendBuffer(){queueMicrotask(update);}};}
  endOfStream(){this.readyState='ended';}
 }
 h.window.MediaSource=MediaSource;h.MediaSource=MediaSource;h.Audio=function(){};h.URL.createObjectURL=()=> 'blob:mpeg';
 h.fetch=async()=>({ok:true,status:200,headers:{get:key=>key==='content-type'?'audio/mpeg':null},body:{getReader(){return {read(){const r=deferred();reads.push(r);return r.promise;}};}}});
 vm.runInNewContext(section('  // ---- the neural voice, streamed','  function alive('),h);
 const opened=h.openSpeech({controller:new AbortController()},{text:'Cloud streaming voice.'});await tick();sources[0].sourceopen();
 reads[0].resolve({value:new Uint8Array([1,2,3]),done:false});const handle=await opened;let eof=false;handle.received.then(()=>eof=true);
 await tick();assert.equal(eof,false);reads[1].resolve({done:true});await handle.received;assert.equal(eof,true);
});
