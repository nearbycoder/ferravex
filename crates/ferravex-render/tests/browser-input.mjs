// Original host contract tests using mocked platform interfaces, no browser automation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {bindInput} from '../../../web/input.js';
import {framebufferSize} from '../../../web/details.js';
import {bindGraphicsLifecycle} from '../../../web/lifecycle.js';
import {configureVisuals} from '../../../web/visuals.js';
const source=fs.readFileSync(new URL('../../../web/runtime.js',import.meta.url),'utf8').replace(/^import .*;$/gm,'');
class Element {
  handlers=new Map(); dataset={game:'emberline'}; width=0; height=0;
  addEventListener(name,fn){this.handlers.set(name,fn);}
  focus(){} setPointerCapture(){} setAttribute(){}
  getBoundingClientRect(){return {left:20,top:40,width:640,height:360};}
}
const canvas=new Element(),status=new Element(),document=new Element(),window=new Element();
const gpuCalls=[];
const gl=new Proxy({getShaderParameter:()=>true,getProgramParameter:()=>true}, {get(target,name){return target[name]??((...args)=>{gpuCalls.push([name,...args]);return {};});}});
canvas.getContext=()=>gl;
document.querySelector=selector=>selector==='canvas'?canvas:status;
const calls=[];let frame;
// Graphics resources and presentation widgets have separate tests. Keep this
// harness focused on the real input bridge, runtime scheduling and logical DPI.
class RetainedRenderer {
 constructor(){this.environment={quality:2};this.meshes=new Map();}
 begin(){} renderShadow(){} draw(){}
}
class SpriteRenderer {begin(){}draw(){}}
class GpuTimer {begin(){}end(){}}
const host={window,document,navigator:{},storage:()=>({getItem:()=>null}),atob,btoa};
const engine={memory:{buffer:new ArrayBuffer(1024)},ferravex_init(){},ferravex_autoplay(v){calls.push(['autoplay',v]);},ferravex_frame(...args){calls.push(['frame',...args]);return 1;},ferravex_clear:()=>0,ferravex_vertices:()=>0,ferravex_vertex_count:()=>0,ferravex_batches:()=>0,ferravex_batch_count:()=>0,ferravex_key:(...a)=>calls.push(['key',...a]),ferravex_button:(...a)=>calls.push(['button',...a]),ferravex_pointer:(...a)=>calls.push(['pointer',...a]),ferravex_touch:(...a)=>calls.push(['touch',...a]),ferravex_touch_mouse:()=>1,ferravex_touch_pointer:(...a)=>calls.push(['pointer',...a,0,0]),ferravex_touch_button:(...a)=>calls.push(['button',...a]),ferravex_focus(){},ferravex_resolution:()=>0,ferravex_visual_look:()=>0,ferravex_command_count:()=>0,ferravex_commands:()=>0,ferravex_culled_count:()=>0,ferravex_visible_count:()=>0,ferravex_grab:()=>0,ferravex_blur:()=>calls.push(['blur']),ferravex_wheel:(v)=>calls.push(['wheel',v])};
engine.ferravex_details_get=()=>0;
await vm.runInNewContext(`(async()=>{${source}\n})()`,{document,window,RetainedRenderer,SpriteRenderer,GpuTimer,framebufferSize,configureVisuals,
 bindInput:(canvas,game,getExports,isPlaying)=>bindInput(canvas,game,getExports,isPlaying,host),
 bindDetails:()=>({ready(){},update(){}}),bindAudio:()=>({sync(){},suspend(){},resume(){}}),
 bindGraphicsLifecycle:(canvas,options)=>bindGraphicsLifecycle(canvas,{...options,request:fn=>{frame=fn;return 1;},cancel(){}}),
 performance:{now:()=>0},devicePixelRatio:2,location:{search:'?autoplay'},URLSearchParams,Float32Array,DataView,console,fetch:async()=>({ok:true,arrayBuffer:async()=>new ArrayBuffer(0)}),WebAssembly:{instantiate:async(_bytes,imports)=>{engine.setPlaying=imports.env.ferravex_set_playing;return {instance:{exports:engine}};}},requestAnimationFrame:fn=>frame=fn});
assert.deepEqual(calls[0],['autoplay',1]);
const key=code=>({code,preventDefault(){}});
window.handlers.get('keydown')(key('KeyA'));window.handlers.get('keyup')(key('KeyA'));
assert.deepEqual(calls.slice(-2),[['key',97,1],['key',97,0]],'both edges reach Rust before a frame');
frame(16);assert.equal(canvas.width,1280);assert.equal(canvas.height,720);
assert.deepEqual(calls.at(-1),['frame',1/60,640,360],'DPI scales only GPU pixels, never logical input');
const pointer={clientX:120,clientY:100,pointerId:7,pointerType:'touch',button:0,preventDefault(){}};
canvas.handlers.get('pointerdown')(pointer);
assert.deepEqual(calls.slice(-3),[['pointer',100,60,0,0],['touch',7,0,100,60],['button',1,1]],'touch and emulated pointer share logical coordinates');
canvas.handlers.get('pointercancel')(pointer);
assert.deepEqual(calls.slice(-2),[['button',1,0],['touch',7,3,100,60]],'cancel releases pointer and touch');
window.handlers.get('blur')();assert.deepEqual(calls.at(-1),['blur']);
document.hidden=true;document.handlers.get('visibilitychange')();assert.deepEqual(calls.at(-1),['blur']);
engine.ferravex_touch_mouse=()=>0;calls.length=0;canvas.handlers.get('pointerdown')(pointer);
assert.deepEqual(calls,[['pointer',100,60,0,0],['touch',7,0,100,60]],'FPS touch handling avoids synthetic firing');
canvas.handlers.get('wheel')({deltaY:90,preventDefault(){}});assert.deepEqual(calls.at(-1),['wheel',-1]);
console.log('Original browser host: key edges, DPI, touch cancellation, focus reset, wheel, FPS touch separation passed.');

// Pointer unlock clears stale movement before queuing the Escape edge.
calls.length=0;document.pointerLockElement=null;
// The game uses this import to report whether its player is active.
engine.setPlaying(1);document.handlers.get('pointerlockchange')();
assert.deepEqual(calls.slice(-3),[['blur'],['key',0xff1b,1],['key',0xff1b,0]],'focus reset must not erase menu toggle');
