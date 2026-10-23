// Browser API contract mocks; this does not automate a live browser.
import assert from 'node:assert/strict';
import {bindInput,MAX_CONTROLS_BYTES} from '../input.js';
class Target {constructor(){this.events=new Map();}addEventListener(t,fn){this.events.set(t,fn);}emit(t,e={}){this.events.get(t)?.({preventDefault(){},...e});}focus(){}setPointerCapture(){}getBoundingClientRect(){return {left:10,top:20};}}
const win=new Target(),doc=new Target(),canvas=new Target(),calls=[];let gamepads=[],time=0;const data=new Map();
const exports={memory:{buffer:new ArrayBuffer(MAX_CONTROLS_BYTES+20)},ferravex_touch_mouse:()=>1,ferravex_grab:()=>0};
for(const key of ['key','blur','focus','touch_pointer','touch_button','touch','pointer','button','wheel','wheel_axis','pad_connected','pad_axis','pad_button','input_settings'])exports['ferravex_'+key]=(...args)=>calls.push([key,...args]);
const storage={getItem:k=>data.get(k)??null,setItem:(k,v)=>data.set(k,v)};
const host={window:win,document:doc,navigator:{getGamepads:()=>gamepads},storage:()=>storage,atob,btoa,now:()=>time};
const input=bindInput(canvas,'test-game',()=>exports,()=>true,host);
win.emit('keydown',{code:'Space'});win.emit('keyup',{code:'Space',target:{closest:()=>true}});assert.deepEqual(calls.slice(-2),[['key',32,1],['key',32,0]]);
win.emit('keydown',{code:'KeyW',target:{closest:()=>true}});assert.notEqual(calls.at(-1)[2],119);
canvas.emit('pointerdown',{pointerType:'touch',pointerId:7,clientX:50,clientY:70,button:0});assert.deepEqual(calls.slice(-3),[['touch_pointer',40,50],['touch',7,0,40,50],['touch_button',1,1]]);assert.ok(!calls.some(c=>c[0]==='button'),'touch mouse emulation must not enter real mouse actions');
canvas.emit('pointermove',{pointerType:'touch',pointerId:7,clientX:60,clientY:80,movementX:10,movementY:10,buttons:1});assert.deepEqual(calls.slice(-2),[['touch_pointer',50,60],['touch',7,1,50,60]]);
canvas.emit('pointercancel',{pointerType:'touch',pointerId:7,clientX:60,clientY:80,button:0});assert.equal(calls.at(-1)[2],3);
canvas.emit('pointerdown',{pointerType:'mouse',pointerId:1,clientX:50,clientY:70,button:4});assert.deepEqual(calls.at(-1),['button',5,1]);
win.emit('blur');win.emit('focus');assert.deepEqual(calls.slice(-2),[['blur'],['focus',1]]);
doc.pointerLockElement={};doc.emit('pointerlockchange');assert.deepEqual(calls.slice(-2),[['blur'],['focus',1]]);doc.pointerLockElement=null;doc.emit('pointerlockchange');assert.deepEqual(calls.slice(-2),[['key',0xff1b,1],['key',0xff1b,0]]);
gamepads=[{connected:true,axes:[0.75,NaN],buttons:[{value:1}]}];input.poll();assert.ok(calls.some(c=>JSON.stringify(c)===JSON.stringify(['pad_axis',0,0,0.75])));const count=calls.length;input.poll();assert.equal(calls.length,count,'unchanged pad values must not cross Wasm again');gamepads=[];input.poll();assert.deepEqual(calls.at(-1),['pad_connected',0,0]);
let panel=true;exports.ferravex_controls_open=()=>panel;
doc.pointerLockElement={};doc.exitPointerLock=()=>{doc.pointerLockElement=null;doc.emit('pointerlockchange');};
const beforePanel=calls.length;assert.equal(input.syncPanel(),true);
assert.ok(!calls.slice(beforePanel).some(c=>c[0]==='key'),'opening settings must not synthesize Escape and close itself');
panel=false;assert.equal(input.syncPanel(),false);
time+=300;doc.pointerLockElement={};doc.emit('pointerlockchange');doc.pointerLockElement=null;doc.emit('pointerlockchange');
assert.deepEqual(calls.slice(-2),[['key',0xff1b,1],['key',0xff1b,0]],'genuine pointer-lock loss still opens the game menu');
const bytes=new Uint8Array(exports.memory.buffer,0,4);bytes.set([0,1,255,42]);assert.equal(input.imports.ferravex_controls_store(0,4),0);assert.equal(input.imports.ferravex_controls_len(),4);assert.equal(input.imports.ferravex_controls_load(10,4),0);assert.deepEqual([...new Uint8Array(exports.memory.buffer,10,4)],[0,1,255,42]);assert.equal(input.imports.ferravex_controls_store(0,MAX_CONTROLS_BYTES+1),-1);assert.equal(input.imports.ferravex_controls_load(-1,4),-1);
const broken=bindInput(new Target(),'bad',()=>exports,()=>false,{...host,storage:()=>({getItem(){throw Error('disabled');},setItem(){throw Error('quota');}})});assert.equal(broken.imports.ferravex_controls_len(),-1);assert.equal(broken.imports.ferravex_controls_store(0,4),-1);
console.log('Shared browser keyboard/mouse/touch/gamepad/storage contracts passed.');

// WebIDL Base64 helpers accept the global receiver, not an arbitrary host object.
const originalAtob=globalThis.atob,originalBtoa=globalThis.btoa;
for(const [name,value] of Object.entries({window:win,document:doc,localStorage:storage}))Object.defineProperty(globalThis,name,{value,configurable:true});
globalThis.atob=function(value){assert.ok(this===undefined||this===globalThis);return originalAtob(value);};
globalThis.btoa=function(value){assert.ok(this===undefined||this===globalThis);return originalBtoa(value);};
const defaults=bindInput(new Target(),'default-host',()=>exports);
assert.equal(defaults.imports.ferravex_controls_store(0,4),0,'default browser helpers must preserve their invocation context');
assert.equal(defaults.imports.ferravex_controls_len(),4);
globalThis.atob=originalAtob;globalThis.btoa=originalBtoa;

time+=300;win.emit('keydown',{code:'Escape'});win.emit('keyup',{code:'Escape'});const afterEscape=calls.length;
doc.pointerLockElement=null;doc.emit('pointerlockchange');
assert.equal(calls.slice(afterEscape).filter(c=>c[0]==='key').length,0,'one physical Escape must not also inject a second menu toggle');
const beforeRepeat=calls.length;win.emit('keydown',{code:'KeyW',repeat:true});assert.equal(calls.length,beforeRepeat,'autorepeat cannot reactivate a held key after a focus reset');

win.emit('keydown',{code:'Semicolon'});win.emit('keyup',{code:'Semicolon'});assert.deepEqual(calls.slice(-2),[['key',59,1],['key',59,0]]);

// Rotation/resizing and OS capture loss must never strand held fingers or keys.
{
 const w=new Target(),d=new Target(),c=new Target(),events=[];
 const e={ferravex_touch_mouse:()=>0,ferravex_grab:()=>0,ferravex_touch_pointer(){},ferravex_touch:(...v)=>events.push(['touch',...v]),ferravex_key:(...v)=>events.push(['key',...v]),ferravex_blur:()=>events.push(['blur']),ferravex_focus:(...v)=>events.push(['focus',...v])};
 bindInput(c,'resize-test',()=>e,()=>false,{...host,window:w,document:d});
 c.emit('pointerdown',{pointerType:'touch',pointerId:9,clientX:30,clientY:50,button:0});
 c.emit('lostpointercapture',{pointerId:9});assert.deepEqual(events.at(-1),['touch',9,3,20,30]);
 w.emit('keydown',{code:'KeyW'});c.emit('pointerdown',{pointerType:'touch',pointerId:10,clientX:30,clientY:50,button:0});
 w.emit('resize');assert.deepEqual(events.slice(-2),[['blur'],['focus',1]]);
 const count=events.length;c.emit('lostpointercapture',{pointerId:10});assert.equal(events.length,count,'capture loss after resize cannot deliver a stale release');
 let prevented=false;c.emit('selectstart',{preventDefault(){prevented=true;}});assert.ok(prevented,'canvas selection is blocked');
 prevented=false;c.emit('touchmove',{preventDefault(){prevented=true;}});assert.ok(prevented,'canvas touch moves cannot scroll or select the surrounding page');
}

// Browser shortcuts retain their defaults and never move the player.
for(const modifier of ['ctrlKey','metaKey','altKey']){const count=calls.length;win.emit('keydown',{code:'KeyW',[modifier]:true,preventDefault(){assert.fail('browser shortcut consumed');}});assert.equal(calls.length,count);}

for(const event of [{isComposing:true},{keyCode:229}]){const count=calls.length;win.emit('keydown',{code:'Space',...event});assert.equal(calls.length,count,'IME composition must not launch or jump');}

// A steady stick must be resent after Rust clears its input on focus loss.
{
 const w=new Target(),d=new Target(),c=new Target(),events=[];
 const e={ferravex_blur(){},ferravex_focus(){},ferravex_pad_connected(){},ferravex_pad_axis:(...args)=>events.push(['axis',...args]),ferravex_pad_button:(...args)=>events.push(['button',...args])};
 const bridge=bindInput(c,'pad-refocus',()=>e,()=>false,{...host,window:w,document:d,navigator:{getGamepads:()=>[{connected:true,axes:[.5],buttons:[{value:1}]}]}});
 bridge.poll();w.emit('blur');const asleep=events.length;bridge.poll();assert.equal(events.length,asleep);w.emit('focus');bridge.poll();assert.ok(events.slice(asleep).some(c=>c[0]==='axis'&&c[3]===.5));assert.ok(events.slice(asleep).some(c=>c[0]==='button'&&c[3]===1));
}
win.emit('keydown',{code:'KeyW'});win.emit('pagehide');assert.deepEqual(calls.at(-1),['blur']);doc.hidden=false;win.emit('pageshow');assert.deepEqual(calls.at(-1),['focus',1]);

canvas.emit('pointerdown',{pointerType:'mouse',pointerId:77,clientX:20,clientY:30,button:0});canvas.emit('pointerup',{pointerType:'mouse',pointerId:77,clientX:20,clientY:30,button:0,buttons:2});const chord=calls.length;canvas.emit('lostpointercapture',{pointerId:77});assert.ok(calls.slice(chord).some(c=>c[0]==='button'&&c[1]===3&&c[2]===0),'capture loss clears remaining mouse chord buttons');

// Independent fixtures model synchronous capture-loss callbacks and window
// releases without needing a browser, timers, or a running Wasm module.
function pointerFixture({captureFails=false,releaseFails=false,hidden=false,hasFocus=true}={}) {
 const w=new Target(),d=new Target(),c=new Target(),events=[],captured=new Set(),released=[];
 w.visualViewport=new Target();d.hidden=hidden;d.hasFocus=()=>hasFocus;
 const e={ferravex_touch_mouse:()=>1,ferravex_grab:()=>0};
 for(const name of ['key','blur','focus','touch_pointer','touch_button','touch','pointer','button','wheel','wheel_axis','pad_connected','pad_axis','pad_button'])e['ferravex_'+name]=(...args)=>events.push([name,...args]);
 c.setPointerCapture=id=>{if(captureFails)throw Error('capture unavailable');captured.add(id);};
 c.releasePointerCapture=id=>{
  released.push(id);
  if(releaseFails)throw Error('capture already gone');
  if(captured.delete(id))c.emit('lostpointercapture',{pointerId:id});
 };
 const bridge=bindInput(c,'pointer-recovery',()=>e,()=>true,{...host,window:w,document:d,navigator:{getGamepads:()=>[{connected:true,axes:[],buttons:[]}]}});
 const pointer=(type,id,extra={})=>c.emit(type,{pointerType:'touch',pointerId:id,clientX:40,clientY:60,button:0,buttons:1,movementX:0,movementY:0,...extra});
 return {w,d,c,e,events,captured,released,bridge,pointer};
}
const buttonEvents=events=>events.filter(e=>e[0]==='button'||e[0]==='touch_button');

// Cancellation often has button=-1 and may omit the contact's final position.
for(const terminal of ['pointercancel','lostpointercapture']) {
 const f=pointerFixture();
 f.pointer('pointerdown',0);
 f.pointer('pointermove',0,{clientX:70,clientY:90});
 f.events.length=0;
 f.c.emit(terminal,{pointerId:0,pointerType:'touch',button:-1,buttons:0});
 assert.deepEqual(f.events,[['touch_button',1,0],['touch',0,3,60,70]],terminal+' uses the tracked button and latest coordinates');
 assert.equal(f.captured.size,0);
 f.events.length=0;
 f.pointer('pointerup',0,{buttons:0});
 f.pointer('pointermove',0);
 f.c.emit('lostpointercapture',{pointerId:0});
 assert.deepEqual(f.events,[],'terminal and stale touch events are idempotent');
 f.pointer('pointerdown',0);
 assert.deepEqual(f.events.at(-1),['touch_button',1,1],'a fresh contact can reuse the cancelled ID');
}

// Every finger owns its touch stream; emulated buttons remain held until the
// last participating finger ends, including when cancellation order varies.
for(const terminal of ['pointerup','pointercancel','lostpointercapture']) {
 for(const first of [11,12]) {
  const f=pointerFixture(),second=first===11?12:11;
  f.pointer('pointerdown',11);f.pointer('pointerdown',12);
  assert.deepEqual(buttonEvents(f.events),[['touch_button',1,1]],'two fingers produce one emulated press');
  f.events.length=0;
  f.c.emit(terminal,{pointerId:first,pointerType:'touch',clientX:40,clientY:60,button:-1,buttons:0});
  assert.deepEqual(buttonEvents(f.events),[],'ending one finger preserves the other hold');
  assert.deepEqual(f.events.at(-1),['touch',first,terminal==='pointerup'?2:3,30,40]);
  f.pointer('pointermove',second,{clientX:60,clientY:80});
  assert.deepEqual(f.events.at(-1),['touch',second,1,50,60]);
  f.pointer('pointerup',second,{buttons:0});
  assert.deepEqual(buttonEvents(f.events),[['touch_button',1,0]],'the final finger releases exactly once');
 }
}

// A settings change during a contact must not strand or invent a mouse hold.
{
 const f=pointerFixture();
 f.pointer('pointerdown',1);
 f.e.ferravex_touch_mouse=()=>0;
 f.pointer('pointerdown',2);
 f.pointer('pointerup',1,{buttons:0});
 assert.deepEqual(buttonEvents(f.events),[['touch_button',1,1],['touch_button',1,0]]);
 f.events.length=0;
 f.e.ferravex_touch_mouse=()=>1;
 f.pointer('pointerup',2,{buttons:0});
 assert.deepEqual(buttonEvents(f.events),[],'disabled-at-start touch does not invent an unmatched mouse release');
}

// Real Pointer Events use pointermove for all intermediate chord changes.
{
 const f=pointerFixture(),mouse={pointerType:'mouse'};
 f.pointer('pointerdown',1,mouse);
 f.pointer('pointermove',1,{...mouse,button:2,buttons:3});
 f.pointer('pointermove',1,{...mouse,button:0,buttons:2});
 f.pointer('pointerup',1,{...mouse,button:2,buttons:0});
 assert.deepEqual(buttonEvents(f.events),[['button',1,1],['button',3,1],['button',1,0],['button',3,0]]);
 f.events.length=0;
 f.pointer('pointerdown',1,{...mouse,buttons:31});
 f.c.emit('lostpointercapture',{pointerId:1});
 assert.deepEqual(buttonEvents(f.events),[1,2,3,4,5].map(b=>['button',b,1]).concat([1,2,3,4,5].map(b=>['button',b,0])),'all five button-mask positions are mapped correctly');
 f.events.length=0;
 f.pointer('pointermove',1,{...mouse,buttons:31});
 assert.deepEqual(buttonEvents(f.events),[],'an orphaned drag cannot re-press cancelled buttons');
 f.pointer('pointerdown',1,mouse);
 f.pointer('pointermove',1,{...mouse,buttons:0});
 f.pointer('pointerup',1,{...mouse,buttons:0});
 assert.deepEqual(buttonEvents(f.events),[['button',1,1],['button',1,0]],'zero-button motion recovers a missed final release');
}

// Cancelling a pen must not release an independent real mouse or touch hold.
{
 const f=pointerFixture();
 f.pointer('pointerdown',1,{pointerType:'mouse'});
 f.pointer('pointerdown',2,{pointerType:'pen',buttons:3});
 f.pointer('pointerdown',3);
 f.events.length=0;
 f.c.emit('pointercancel',{pointerId:2,button:-1,buttons:0});
 assert.deepEqual(buttonEvents(f.events),[['button',3,0]]);
 f.pointer('pointerup',1,{pointerType:'mouse',buttons:0});
 f.pointer('pointerup',3,{buttons:0});
 assert.deepEqual(buttonEvents(f.events),[['button',3,0],['button',1,0],['touch_button',1,0]]);
}

// If setPointerCapture fails, a window-level terminal event still releases the
// canvas contact; duplicate bubbled terminals and unrelated pointers are inert.
for(const pointerType of ['touch','mouse']) {
 for(const terminal of ['pointerup','pointercancel']) {
  const f=pointerFixture({captureFails:true});
  f.pointer('pointerdown',5,{pointerType});
  f.events.length=0;
  const event={pointerId:5,pointerType,clientX:40,clientY:60,button:terminal==='pointercancel'?-1:0,buttons:0};
  f.w.emit(terminal,event);
  assert.deepEqual(buttonEvents(f.events),[[pointerType==='touch'?'touch_button':'button',1,0]]);
  const count=f.events.length;
  f.c.emit(terminal,event);f.w.emit(terminal,event);
  f.w.emit(terminal,{...event,pointerId:999});
  assert.equal(f.events.length,count);
 }
}

// Capacity is bounded by the renderer's 64-contact limit; an ignored contact
// cannot create a move/release, and finishing one makes room for a fresh down.
{
 const f=pointerFixture();
 for(let id=0;id<65;id++)f.pointer('pointerdown',id);
 assert.equal(f.events.filter(e=>e[0]==='touch'&&e[2]===0).length,64);
 assert.equal(f.captured.size,64);
 f.events.length=0;
 f.pointer('pointerdown',0);f.pointer('pointermove',64);f.pointer('pointerup',64,{buttons:0});
 assert.deepEqual(f.events,[],'duplicate and over-capacity contacts do not enter the host stream');
 f.pointer('pointerup',0,{buttons:0});f.pointer('pointerdown',64);
 assert.deepEqual(f.events.at(-1),['touch',64,0,30,40]);
 f.w.emit('resize');
 assert.equal(f.captured.size,0);
 f.events.length=0;f.pointer('pointerdown',65);
 assert.deepEqual(f.events.at(-1),['touch_button',1,1]);
}

// Every reset boundary discards old contacts before releasing capture. The
// synchronous lostpointercapture mock catches accidental reentrant releases.
const resetCases={
 resize:f=>f.w.emit('resize'),
 orientation:f=>f.w.emit('orientationchange'),
 viewport:f=>f.w.visualViewport.emit('resize'),
 blur:f=>{f.w.emit('blur');f.w.emit('focus');},
 pagehide:f=>{f.w.emit('pagehide');f.w.emit('pageshow');},
 hidden:f=>{f.d.hidden=true;f.d.emit('visibilitychange');f.d.hidden=false;f.d.emit('visibilitychange');},
 pointerlock:f=>{f.d.pointerLockElement=f.c;f.d.emit('pointerlockchange');},
 suspend:f=>{f.bridge.suspend();f.bridge.resume();},
};
for(const [name,resetInput] of Object.entries(resetCases)) {
 const f=pointerFixture();
 f.w.emit('keydown',{code:'KeyW'});
 f.pointer('pointerdown',1);f.pointer('pointerdown',2,{pointerType:'mouse'});
 f.events.length=0;
 resetInput(f);
 assert.deepEqual(f.events,[['blur'],['focus',1]],name+' resets without queuing stale terminal edges');
 assert.deepEqual(f.released,[1,2]);
 assert.equal(f.captured.size,0);
 f.events.length=0;
 f.pointer('pointermove',1);f.pointer('pointerup',1,{buttons:0});
 f.pointer('pointermove',2,{pointerType:'mouse'});
 f.pointer('pointerup',2,{pointerType:'mouse',buttons:0});
 f.c.emit('lostpointercapture',{pointerId:2});
 f.w.emit('keydown',{code:'KeyW',repeat:true});
 assert.deepEqual(f.events.filter(e=>e[0]!=='pointer'),[],name+' requires a fresh touch/mouse press and ignores key repeat');
 f.pointer('pointerdown',1);f.pointer('pointerdown',2,{pointerType:'mouse'});
 assert.deepEqual(buttonEvents(f.events),[['touch_button',1,1],['button',1,1]]);
}
{
 const f=pointerFixture({releaseFails:true});
 f.pointer('pointerdown',1);f.w.emit('resize');
 f.events.length=0;f.c.emit('lostpointercapture',{pointerId:1});
 assert.deepEqual(f.events,[],'capture cleanup errors cannot preserve stale host ownership');
}

// Visibility, window focus, and page suspension are independent. One resume
// signal cannot reopen input while another reason still suppresses it.
function assertInactive(f) {
 f.events.length=0;
 f.pointer('pointerdown',6);f.pointer('pointermove',6);
 f.pointer('pointerdown',7,{pointerType:'mouse'});
 f.pointer('pointermove',7,{pointerType:'mouse'});
 f.w.emit('keydown',{code:'Space'});
 f.c.emit('wheel',{deltaY:1,deltaX:1});f.bridge.poll();
 assert.deepEqual(f.events,[],'suspended input and pad polling remain silent');
 assert.equal(f.captured.size,0);
 f.d.pointerLockElement=null;f.d.emit('pointerlockchange');
 assert.deepEqual(f.events,[['blur'],['focus',0]],'background pointer unlock must not queue an Escape');
}
{
 const f=pointerFixture();
 f.w.emit('blur');f.d.hidden=true;f.d.emit('visibilitychange');
 f.d.hidden=false;f.d.emit('visibilitychange');
 assert.deepEqual(f.events.at(-1),['focus',0],'visibility alone cannot override window blur');
 assertInactive(f);
 f.d.hidden=true;f.d.emit('visibilitychange');f.w.emit('focus');
 assert.deepEqual(f.events.at(-1),['focus',0],'window focus cannot override document hiding');
 assertInactive(f);
 f.w.emit('pagehide');f.d.hidden=false;f.d.emit('visibilitychange');f.w.emit('focus');
 assert.deepEqual(f.events.at(-1),['focus',0],'focus and visibility cannot override pagehide');
 assertInactive(f);
 f.w.emit('pageshow');assert.deepEqual(f.events.at(-1),['focus',1]);
 f.pointer('pointerdown',6);assert.deepEqual(f.events.at(-1),['touch_button',1,1]);
}
for(const initial of [{hidden:true},{hasFocus:false}]) {
 const f=pointerFixture(initial);
 assertInactive(f);
 f.d.hidden=false;f.d.emit('visibilitychange');f.w.emit('focus');
 f.pointer('pointerdown',6);assert.deepEqual(f.events.at(-1),['touch_button',1,1]);
}

// Executable use of bindInput's additive graphics-host hooks. Suspending does
// not schedule or stop simulation; it discards host input and requires new downs.
{
 const f=pointerFixture(),input=f.bridge;
 f.pointer('pointerdown',1);f.events.length=0;
 assert.equal(input.suspend(),undefined);
 assert.equal(input.suspend(),undefined);
 assert.deepEqual(f.events,[['blur']],'suspend is idempotent');
 f.w.emit('blur');f.w.emit('focus');f.w.emit('resize');f.w.emit('pageshow');
 assertInactive(f);
 f.events.length=0;
 assert.equal(input.resume(),undefined);
 assert.equal(input.resume(),undefined);
 assert.deepEqual(f.events,[['focus',1]],'resume is idempotent');
 f.events.length=0;f.pointer('pointermove',1);f.pointer('pointerup',1,{buttons:0});
 assert.deepEqual(f.events,[],'resume cannot resurrect the held contact');
 f.pointer('pointerdown',1);assert.deepEqual(f.events.at(-1),['touch_button',1,1]);
}
for(const interrupt of [f=>f.w.emit('blur'),f=>f.w.emit('pagehide'),f=>{f.d.hidden=true;f.d.emit('visibilitychange');}]) {
 const f=pointerFixture();
 f.bridge.suspend();interrupt(f);f.bridge.resume();
 assert.deepEqual(f.events.at(-1),['focus',0],'graphics resume cannot override another suspension reason');
 assertInactive(f);
}

console.log('Pointer cancellation, chord/multi-touch ownership, capture recovery, bounds, and suspend release gates passed.');
