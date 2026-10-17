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
