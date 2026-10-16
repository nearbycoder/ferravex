// Actual Wasm material ABI: CPU descriptors survive device caches being dropped.
import assert from 'node:assert/strict';import fs from 'node:fs';import {bindInput} from '../input.js';
class Target{addEventListener(){}}
let e;const input=bindInput(new Target(),'crystal-cavern',()=>e,()=>false,{window:new Target(),document:new Target(),navigator:{},storage:()=>({getItem(){return null;},setItem(){}}),atob,btoa});
const module=new WebAssembly.Module(fs.readFileSync(new URL('../../dist/web/crystal-cavern/crystal_cavern.wasm',import.meta.url)));
e=new WebAssembly.Instance(module,{env:{...input.imports,ferravex_set_playing(){}}}).exports;e.ferravex_init();e.ferravex_frame(1/60,1280,720);
assert.equal(e.ferravex_material_count(),1);const id=e.ferravex_material_id(0),decode=new TextDecoder(),encode=new TextEncoder();
const source=stage=>decode.decode(new Uint8Array(e.memory.buffer,e.ferravex_material_source(id,stage),e.ferravex_material_source_len(id,stage)));
const vertex=source(0),fragment=source(1);assert.ok(vertex&&fragment);assert.equal(source(2),'');e.ferravex_material_feedback(id,1,1,0);assert.equal(source(2),vertex);assert.equal(source(3),fragment);
const v=encode.encode(vertex),f=encode.encode('this is invalid GLSL'),ptr=e.ferravex_material_edit_buffer();new Uint8Array(e.memory.buffer,ptr,v.length+f.length).set(v);new Uint8Array(e.memory.buffer,ptr+v.length,f.length).set(f);assert.equal(e.ferravex_material_reload(id,v.length,f.length),2);
const message=encode.encode('expected compile failure');new Uint8Array(e.memory.buffer,e.ferravex_material_feedback_buffer(),message.length).set(message);e.ferravex_material_feedback(id,2,0,message.length);
assert.equal(e.ferravex_material_status(id),3);assert.equal(source(1),'this is invalid GLSL');assert.equal(source(2),vertex);assert.equal(source(3),fragment);
// Feedback from an old device/revision cannot replace the stored good hooks.
e.ferravex_material_feedback(id,1,1,0);assert.equal(e.ferravex_material_status(id),3);assert.equal(source(3),fragment);assert.equal(e.ferravex_material_source_len(id,4),0);assert.equal(e.ferravex_material_source_len(id,0xffffffff),0);
console.log('Actual-Wasm material descriptors retain last-good hooks after failed reload and reject stale feedback/invalid source stages');
