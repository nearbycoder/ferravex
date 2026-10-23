// Executes the production Orbit platform against DOM/Wasm doubles. Not browser play.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
const sourceRoot=process.argv[2]?pathToFileURL(resolve(process.argv[2])+'/'):new URL('../../',import.meta.url);

const source=readFileSync(new URL('web/owned-orbit/platform.js',sourceRoot),'utf8').replace(/^import .*;$/gm,'');
const html=readFileSync(new URL('web/owned-orbit/index.html',sourceRoot),'utf8');
async function fixture(width=320){
 const nodes=new Map();
 for(const selector of ['#game','#notice','#notice-title','#notice-message','#resume','#pause','#restart','#game-state','#orbit-readable','[data-controls]','[data-audio]']){
  nodes.set(selector,{hidden:false,textContent:'',dataset:{},attributes:new Map(),addEventListener(){},focus(){},setAttribute(k,v){this.attributes.set(k,v);},getContext(){return {putImageData(){}};},getBoundingClientRect(){return {width,height:width*9/8};},width:640,height:720});
 }
 let next,ptr=32,len=0,frames=0;
 const engine={memory:{buffer:new ArrayBuffer(2_000_000)},init(){},pixels:()=>0,width:()=>640,height:()=>720,status:()=>4,score:()=>750,lives:()=>0,sector:()=>1,orbit_controls_frame(){frames++;},orbit_paused:()=>0,orbit_status_text_v1:()=>ptr,orbit_status_text_v1_len:()=>len};
 const context={document:{querySelector:s=>nodes.get(s),addEventListener(){}},window:{addEventListener(){},removeEventListener(){}},performance,location:{search:''},ImageData:class{},Uint8ClampedArray,Uint8Array,TextDecoder,
  bindInput:()=>({imports:{},poll(){},syncPanel:()=>false}),bindAudio:()=>({sync(){}}),bindDetails:()=>({ready(){},update(){}}),bindVisualPreferences:()=>({refresh(){},dispose(){}}),configureVisuals(){},drawInput2d(){},
  loadWasm:async()=>({instance:{exports:engine}}),showLoadError:error=>{throw error;},requestAnimationFrame:fn=>{next=fn;}};
 await vm.runInNewContext(`(async()=>{${source}\n})()`,context);
 return {nodes,engine,get frames(){return frames;},render:()=>next(100),
  set(text){const bytes=new TextEncoder().encode(text);ptr=32;len=bytes.length;new Uint8Array(engine.memory.buffer,ptr,len).set(bytes);},
  bounds(p,l){ptr=p;len=l;}};
}
const copy='Run ended. Sector 1 of 2. Score 750. 0 lives remaining. Retry: J. Release, then press J again to launch.\nPanels: 28 remaining; 7 cleared. Precision streak: 0. Best precision: 0. Ball saves: 3.\nMove: J / →. Restart: R.';
for(const width of [320,390,640]){
 const f=await fixture(width);f.set(copy);f.render();
 assert.equal(f.nodes.get('#game-state').textContent,copy.split('\n')[0]);
 assert.equal(f.nodes.get('#orbit-readable').textContent,copy.slice(copy.indexOf('\n')+1));
 assert.equal(f.nodes.get('#orbit-readable').hidden,false,'full details stay available in the on-demand disclosure at every width');
 assert.equal(f.frames,1,'DOM presentation does not add simulation frames');
 assert.equal(f.nodes.get('#game').dataset.orbitStatusTextError,'');
 f.set('Ready. Sector 1 of 2. Score 0. 3 lives remaining. Launch: K.\nPanels: 35 remaining; 0 cleared.');f.render();
 assert.match(f.nodes.get('#game-state').textContent,/Launch: K/);
 assert.doesNotMatch(f.nodes.get('#game-state').textContent,/Retry: J|Space|Enter/);
 // Memory growth must not retain a stale TypedArray or old binding string.
 f.engine.memory.buffer=new ArrayBuffer(2_100_000);f.set('Ready. Launch: <J>.\nBest precision: 4. Ball saves: 2.');f.render();
 assert.equal(f.nodes.get('#game-state').textContent,'Ready. Launch: <J>.');
 assert.equal(f.nodes.get('#orbit-readable').textContent,'Best precision: 4. Ball saves: 2.');
 f.bounds(2_100_000,2048);f.render();
 assert.match(f.nodes.get('#game').dataset.orbitStatusTextError,/bounds/);
 assert.equal(f.nodes.get('#orbit-readable').hidden,true);
 assert.equal(f.nodes.get('#orbit-readable').textContent,'');
 assert.doesNotMatch(f.nodes.get('#game-state').textContent,/Retry: J|Launch: K|Space|Enter/);
 f.bounds(32,2049);f.render();assert.match(f.nodes.get('#game').dataset.orbitStatusTextError,/bounds/);
 f.bounds(32,1);new Uint8Array(f.engine.memory.buffer)[32]=255;f.render();
 assert.ok(f.nodes.get('#game').dataset.orbitStatusTextError,'invalid UTF-8 is observable');
 delete f.engine.orbit_status_text_v1;f.render();
 assert.match(f.nodes.get('#game-state').textContent,/Controls to retry, then launch/);
 assert.doesNotMatch(f.nodes.get('#game-state').textContent,/Space|Enter/);
}
assert.match(html,/#orbit-readable\{font:16px\/1\.5[^}]*white-space:pre-line[^}]*overflow-wrap:anywhere/);
assert.match(html,/<p id="orbit-readable" aria-live="off" hidden>/);
assert.doesNotMatch(source,/innerHTML/,'all status and binding bytes are text');
console.log('Production status adapter: 320/390/desktop, current hints, UTF-8, memory growth, bounds, legacy fallback and no extra frames passed. DOM/Wasm doubles only; browser geometry remains a fresh-artifact check.');
