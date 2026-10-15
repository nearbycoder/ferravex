// Offline ring/worklet/browser contracts; no live browser is automated.
import assert from 'node:assert/strict';
import {PcmRing} from '../audio-ring.js';
import {bindAudio} from '../audio.js';
const ring=new PcmRing(128),left=new Float32Array(128),right=new Float32Array(128);
assert.equal(ring.push(new Float32Array([.25,-.5,NaN,2])),2);ring.readStereo(left,right);assert.deepEqual([...left.slice(0,2)],[.25,0]);assert.deepEqual([...right.slice(0,2)],[-.5,1]);assert.equal(ring.underruns,126);
ring.push(new Float32Array(256).fill(.3));ring.push(new Float32Array([.7,-.7]));assert.equal(ring.overflows,1);assert.equal(ring.count,128);ring.clear();assert.equal(ring.count,0);assert.equal(ring.push(new Float32Array(3)),0);
const posted=[];globalThis.AudioWorkletProcessor=class{constructor(){this.port={postMessage:m=>posted.push(m)};}};globalThis.sampleRate=48000;globalThis.registerProcessor=()=>{};
const {FerravexPcmProcessor}=await import('../audio-worklet.js');const processor=new FerravexPcmProcessor(),out=[new Float32Array(128),new Float32Array(128)];
processor.port.onmessage({data:{type:'reset',epoch:1,active:true}});processor.process([], [out]);assert.equal(posted.at(-1).frames,1536);
processor.port.onmessage({data:{type:'pcm',epoch:0,data:new Float32Array(3072).fill(.5)}});assert.equal(processor.ring.count,0,'late audio cannot cross reset epochs');
processor.port.onmessage({data:{type:'pcm',epoch:1,data:new Float32Array(3072).fill(.25)}});processor.process([], [out]);assert.ok(out[0].every(v=>v===.25));
processor.port.onmessage({data:{type:'reset',epoch:2,active:false}});processor.process([], [out]);assert.ok(out[0].every(v=>v===0));
class Target{constructor(){this.events=new Map();this.dataset={};}addEventListener(t,fn){if(!this.events.has(t))this.events.set(t,[]);this.events.get(t).push(fn);}emit(t,e={}){for(const fn of this.events.get(t)??[])fn({type:t,...e});}setAttribute(){}focus(){} }
class Context extends Target{constructor(){super();this.state='suspended';this.sampleRate=48000;this.audioWorklet={addModule:async()=>{}};}resume(){this.state='running';return Promise.resolve();}close(){this.state='closed';return Promise.resolve();}}
const win=new Target(),doc=new Target(),canvas=new Target(),button=new Target(),messages=[];let context,node,muted=0,settings=0,active=0;
const wasm={memory:{buffer:new ArrayBuffer(32768)},ferravex_audio_render:(n)=>{new Float32Array(wasm.memory.buffer,0,n*2).fill(.2);return n;},ferravex_audio_pcm:()=>0,ferravex_audio_state:v=>active=v,ferravex_audio_mute:v=>muted=v,ferravex_audio_muted:()=>muted,ferravex_controls_open:()=>settings};
const audio=bindAudio(canvas,()=>wasm,{button,host:{window:win,document:doc,createContext:()=>context=new Context(),createNode:()=>node={port:{postMessage:m=>messages.push(m)},connect(){},disconnect(){},addEventListener(){}}}});
assert.equal(audio.state(),'locked');assert.equal(await audio.enable(),true);assert.equal(active,1);const epoch=messages.at(-1).epoch;
node.port.onmessage({data:{type:'need',epoch,frames:512,rate:48000}});assert.equal(messages.at(-1).data.length,1024);
settings=1;audio.sync();assert.equal(active,0);assert.equal(messages.at(-1).active,false);
const before=messages.length;node.port.onmessage({data:{type:'need',epoch,frames:512,rate:48000}});assert.equal(messages.length,before,'stale worklet demand must not advance Rust');
settings=0;audio.sync();win.emit('blur');assert.equal(active,0);audio.mute();assert.equal(audio.state(),'muted');assert.equal(muted,1);audio.dispose();assert.equal(context.state,'closed');
console.log('Audio ring, epoch, PCM pull, mute, focus and host contracts passed.');

const large=new FerravexPcmProcessor(),largeOutput=[new Float32Array(2048),new Float32Array(2048)];
large.port.onmessage({data:{type:'reset',epoch:9,active:true}});large.process([],[largeOutput]);assert.equal(posted.at(-1).frames,4096,'queue demand adapts to larger output quanta');
large.port.onmessage({data:{type:'pcm',epoch:9,data:new Float32Array(8192).fill(.2)}});large.process([],[largeOutput]);assert.equal(large.ring.underruns,0);assert.ok(largeOutput[0].every(v=>Math.abs(v-.2)<1e-6));
