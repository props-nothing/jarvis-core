import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source=readFileSync(new URL('../apps/jarvisd/src/hud/hud.js',import.meta.url),'utf8');
const helpers=source.slice(source.indexOf('  // ---- bounded PCM playback'),source.indexOf('  // ---- voice requests'));
function harness() {
  const sources=[],contexts=[];
  class Context {
    constructor(){this.currentTime=0;this.destination={};contexts.push(this);}
    createAnalyser(){return {connect(){},fftSize:256};}
    createBuffer(channels,count,rate){assert.equal(channels,1);assert.equal(rate,48000);const values=new Float32Array(count);return {duration:count/rate,getChannelData(){return values;},values};}
    createBufferSource(){const item={connect(){},disconnect(){},start(at){this.at=at;},stop(){this.stopped=true;}};sources.push(item);return item;}
    async resume(){}
    async close(){this.closed=true;}
  }
  const env={window:{AudioContext:Context},Uint8Array,Float32Array,DataView,ArrayBuffer,Set,Blob,TextDecoder,DOMException,atob};
  vm.runInNewContext(helpers,env);return {...env,sources,contexts};
}
function response(chunks){let cancelled=false;return {body:{getReader(){let index=0;return {async read(){return index<chunks.length?{value:chunks[index++],done:false}:{done:true};},async cancel(){cancelled=true;}};}},get cancelled(){return cancelled;}};}
test('PCM starts before EOF, preserves split samples, and cancellation stops scheduled buffers',async()=>{
 const h=harness(),cancel=new AbortController(),player=h.pcmPlayer(cancel.signal);await player.play();
 player.push(new Uint8Array([0]));assert.equal(h.sources.length,0);
 player.push(new Uint8Array([64,0,192]));await player.first;
 assert.equal(h.sources.length,1);assert.equal(h.sources[0].buffer.values[0],.5);assert.equal(h.sources[0].buffer.values[1],-.5);
 assert(h.sources[0].at>0);cancel.abort();await assert.rejects(player.finished);assert(h.sources[0].stopped);assert(h.contexts[0].closed);
});
test('PCM lookahead does not play until requested and ends after scheduled audio drains',async()=>{
 const h=harness(),player=h.pcmPlayer();player.push(new Uint8Array([1,0]));player.end();assert.equal(h.sources.length,0);
 await player.play();assert.equal(h.sources.length,1);h.sources[0].onended();await player.finished;assert(h.contexts[0].closed);
});
test('odd, empty and oversized PCM fail closed',async()=>{
 for(const bytes of [new Uint8Array(),new Uint8Array([1]),new Uint8Array(96000*60+2)]){
  const h=harness(),player=h.pcmPlayer();const res=response([bytes]);await assert.rejects(h.readPcm(res,player,true));assert(res.cancelled);
 }
});
test('design reader handles network splits and requires exactly one final complete event',async()=>{
 const h=harness(),events=[];const lines=JSON.stringify({event:'audio',pcm:'AQA='})+'\n'+JSON.stringify({event:'complete',design:{reference_audio:'UklGRg=='}})+'\n';
 const bytes=new TextEncoder().encode(lines);const parsed=await h.readDesign(response([bytes.slice(0,9),bytes.slice(9,32),bytes.slice(32)]),b=>events.push(b));
 assert.equal(parsed.event,'complete');assert.equal(events.length,1);
 for(const broken of [lines.split('\n')[0]+'\n',lines+lines,lines.slice(0,-1),JSON.stringify({event:'audio',pcm:'AQ=='})+'\n',JSON.stringify({event:'complete',design:{reference_audio:'UklGRg=='}})+'\n']){
  await assert.rejects(h.readDesign(response([new TextEncoder().encode(broken)]),()=>{}));
 }
});
