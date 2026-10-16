// Mock the host's browser interfaces. Rust action buffering has separate unit
// and offline Wasm integration tests; this checks the custom loader's routing.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {bindInput} from '../../../web/input.js';
const source=fs.readFileSync(new URL('../../../web/owned-orbit/platform.js',import.meta.url),'utf8').replace(/^import .*;$/gm,'');
class Element {
 constructor(){this.hidden=false;this.textContent='';this.handlers=new Map();this.dataset={};this.width=640;this.height=720;}
 addEventListener(type,fn){if(!this.handlers.has(type))this.handlers.set(type,[]);this.handlers.get(type).push(fn);}
 emit(type,event={}){for(const fn of this.handlers.get(type)??[])fn({preventDefault(){},...event});}
 focus(){}setAttribute(){}setPointerCapture(){}getContext(){return {putImageData(){}};}
 getBoundingClientRect(){return {left:0,top:0,width:640,height:720};}
}
const nodes=new Map(['#game','#notice','#notice-title','#notice-message','#resume','#pause','#game-state','#restart','[data-controls]'].map(s=>[s,new Element()]));
const window=new Element(),document=new Element();document.querySelector=s=>nodes.get(s);
const events=[],frames=[];let paused=false,pendingFrame;
const engine={memory:{buffer:new ArrayBuffer(640*720*4)},init(){},width:()=>640,height:()=>720,pixels:()=>0,status:()=>0,score:()=>0,lives:()=>3,
 ferravex_grab:()=>0,ferravex_controls_open:()=>0,orbit_controls_frame(...args){frames.push(args);},orbit_paused:()=>paused,orbit_pause(v){paused=!!v;},orbit_restart(){},
 ferravex_key:(...args)=>events.push(args),ferravex_blur:()=>events.push(['blur']),ferravex_focus(){},ferravex_input_settings(){events.push(['settings']);}};
const host={window,document,navigator:{},storage:()=>({getItem:()=>null}),atob,btoa};
const sandbox={document,window,performance,ImageData:class{},Uint8ClampedArray,Math,Number,Error,
 location:{search:""},configureVisuals(){},bindDetails:()=>({ready(){},update(){}}),
 bindInput:(canvas,game,getExports,isPlaying)=>bindInput(canvas,game,getExports,isPlaying,host),drawInput2d(){},bindAudio:()=>({sync(){}}),
 requestAnimationFrame:fn=>pendingFrame=fn,fetch:async()=>({ok:true,clone(){return this;}}),WebAssembly:{instantiateStreaming:async()=>({instance:{exports:engine}})}};
await vm.runInNewContext(`(async()=>{${source}\n})()`,sandbox);
function tap(code){window.emit('keydown',{code});window.emit('keyup',{code});}
tap('Space');assert.deepEqual(events.slice(-2),[[32,1],[32,0]],'quick taps must cross the Rust boundary in order before a render frame');
pendingFrame(16);assert.equal(frames.length,1);assert.deepEqual(frames[0],[0,640,720]);
tap('KeyR');assert.deepEqual(events.slice(-2),[[114,1],[114,0]]);
window.emit('keydown',{code:'KeyD'});assert.deepEqual(events.at(-1),[100,1]);
window.emit('blur');assert.equal(paused,true,'focus loss pauses the custom host');assert.deepEqual(events.at(-1),['blur']);
nodes.get('[data-controls]').emit('click');assert.deepEqual(events.at(-1),['settings']);
console.log('Orbit shared browser input routing passed.');
