import {bindVisualPreferences} from '../../../web/visuals.js';
// Mock the host's browser interfaces. Rust action buffering has separate unit
// and offline Wasm integration tests; this checks the custom loader's routing.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {bindInput} from '../../../web/input.js';
const source=fs.readFileSync(new URL('../../../web/owned-orbit/platform.js',import.meta.url),'utf8').replace(/^import .*;$/gm,'');
class Element {
 constructor(){this.hidden=false;this.textContent='';this.handlers=new Map();this.attributes=new Map();this.dataset={};this.width=640;this.height=720;}
 addEventListener(type,fn){if(!this.handlers.has(type))this.handlers.set(type,[]);this.handlers.get(type).push(fn);}
 removeEventListener(type,fn){this.handlers.set(type,(this.handlers.get(type)??[]).filter(handler=>handler!==fn));}
 emit(type,event={}){for(const fn of this.handlers.get(type)??[])fn({preventDefault(){},...event});}
 focus(){}setAttribute(name,value){this.attributes.set(name,value);}setPointerCapture(){}getContext(){return {putImageData(){}};}
 getBoundingClientRect(){return {left:0,top:0,width:640,height:720};}
}
const nodes=new Map(['#game','#notice','#notice-title','#notice-message','#resume','#pause','#game-state','#restart','[data-controls]'].map(s=>[s,new Element()]));
const window=new Element(),document=new Element();document.querySelector=s=>nodes.get(s);
const events=[],frames=[];let paused=false,pendingFrame,phase=0,lifeCount=3,sector=1,restarts=0;
const engine={memory:{buffer:new ArrayBuffer(640*720*4)},init(){},width:()=>640,height:()=>720,pixels:()=>0,status:()=>phase,score:()=>0,lives:()=>lifeCount,sector:()=>sector,
 ferravex_grab:()=>0,ferravex_controls_open:()=>0,orbit_controls_frame(...args){frames.push(args);},orbit_paused:()=>paused,orbit_pause(v){paused=!!v;},orbit_restart(){restarts++;phase=0;lifeCount=3;sector=1;},
 ferravex_key:(...args)=>events.push(args),ferravex_blur:()=>events.push(['blur']),ferravex_focus(){},ferravex_input_settings(){events.push(['settings']);}};
const host={window,document,navigator:{},storage:()=>({getItem:()=>null}),atob,btoa};
const sandbox={document,window,loadWasm:async()=>({instance:{exports:engine}}),performance,ImageData:class{},Uint8ClampedArray,Math,Number,Error,
 location:{search:""},configureVisuals(){},bindVisualPreferences,bindDetails:()=>({ready(){},update(){}}),
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
pendingFrame(5000);
assert.equal(nodes.get('#notice').hidden,false,'pause feedback is visible after focus loss');
assert.equal(nodes.get('#notice-title').textContent,'GAME PAUSED');
assert.equal(nodes.get('#pause').attributes.get('aria-pressed'),'true');
assert.equal(frames.at(-1)[0],0,'focus transitions discard host elapsed time');
window.emit('focus');assert.equal(paused,true,'focus return alone must not resume');
nodes.get('#resume').emit('click');pendingFrame(10000);
assert.equal(paused,false);assert.equal(nodes.get('#notice').hidden,true);
assert.equal(frames.at(-1)[0],0,'resume cannot catch up the paused wall clock');
pendingFrame(10050);assert.equal(frames.at(-1)[0],.05);
tap('Enter');assert.deepEqual(events.slice(-2),[[0xff0d,1],[0xff0d,0]],'Enter retry uses the shared key mapping');
phase=2;pendingFrame(10100);
assert.match(nodes.get('#game-state').textContent,/Sector 1 secured.*reinforced panels/);
phase=0;sector=2;lifeCount=1;pendingFrame(10150);
assert.match(nodes.get('#game-state').textContent,/Sector 2 of 2.*1 life remaining/);
phase=4;pendingFrame(10200);
assert.match(nodes.get('#game-state').textContent,/retry, then launch/);
nodes.get('#pause').emit('click');assert.equal(paused,true);
nodes.get('#restart').emit('click');pendingFrame(20000);
assert.equal(restarts,1);assert.equal(paused,false);
assert.equal(nodes.get('#notice').hidden,true);
assert.equal(frames.at(-1)[0],0,'toolbar restart discards prior host elapsed time');
document.hidden=true;document.emit('visibilitychange');assert.equal(paused,true);
document.hidden=false;document.emit('visibilitychange');assert.equal(paused,true,'returning to the tab requires explicit resume');
console.log('Orbit shared browser input routing passed.');
