// Real release-Wasm authoring ABI: validation, adoption and resource retirement.
import assert from 'node:assert/strict';import fs from 'node:fs';
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
const encoder=new TextEncoder(),decoder=new TextDecoder('utf-8',{fatal:true});
for(const game of catalog){
 const module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${game.id.replaceAll('-','_')}.wasm`,import.meta.url)));
 const names=new Set(WebAssembly.Module.exports(module).map(x=>x.name));
 assert.equal(names.has('ferravex_content_source'),!!game.editable_scene,game.id+' optional editor ABI must be isolated');
 if(!game.editable_scene)continue;
 const env={ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}};
 const e=new WebAssembly.Instance(module,{env}).exports;e.ferravex_init();e.ferravex_autoplay(1);
 const frame=()=>{e.ferravex_frame(1/60,1280,720);assert.ok(e.ferravex_vertex_count()>0);assert.ok(new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8).every(Number.isFinite));};frame();
 const source=()=>decoder.decode(new Uint8Array(e.memory.buffer,e.ferravex_content_source(),e.ferravex_content_source_len()));
 const submit=bytes=>{const ptr=e.ferravex_content_edit_buffer(bytes.length);assert.ok(ptr);new Uint8Array(e.memory.buffer,ptr,bytes.length).set(bytes);return e.ferravex_content_submit(bytes.length);};
 const initial=source();assert.ok(initial.startsWith('ferravex_scene 1;'));assert.equal(e.ferravex_content_generation(),0);
 assert.equal(submit(encoder.encode('ferravex_scene 1;node invalid {component unknown {}}')),1);frame();assert.equal(e.ferravex_content_status(),3);assert.equal(e.ferravex_content_generation(),0);assert.equal(source(),initial);
 assert.equal(submit(new Uint8Array([255])),0);assert.equal(e.ferravex_content_status(),3);assert.equal(source(),initial);
 assert.equal(e.ferravex_content_edit_buffer(2*1024*1024+1),0);
 const oldMeshes=new Set();
 const meshes=()=>{const result=new Set(),view=new DataView(e.memory.buffer);for(let i=0;i<e.ferravex_retained_batch_count();i++)result.add(view.getUint32(e.ferravex_retained_batches()+i*224,true));return result;};
 for(const id of meshes())oldMeshes.add(id);
 for(let generation=1;generation<=12;generation++){
  const changed=game.id==='marble-run'?initial.replace('radius number 0.55;',`radius number ${generation%2?.7:.6};`):initial.replace('money int 200;',`money int ${320+generation};`);
  assert.notEqual(changed,initial);assert.equal(submit(encoder.encode(changed)),1);frame();assert.equal(e.ferravex_content_generation(),generation);assert.equal(e.ferravex_content_status(),1);assert.equal(source(),changed);
  const view=new DataView(e.memory.buffer),count=e.ferravex_retained_instance_count();assert.ok(count<2048);assert.ok(new Float32Array(e.memory.buffer,e.ferravex_retained_instances(),count*28).every(Number.isFinite));
  if(game.id==='marble-run'){assert.ok([...meshes()].every(id=>!oldMeshes.has(id)),'new scene records fresh geometry');for(const id of oldMeshes)assert.equal(e.ferravex_mesh_vertex_count(id),0,'retired generation must release immutable mesh ownership');}
 }
 for(let i=0;i<300;i++)frame();console.log(JSON.stringify({game:game.id,generation:e.ferravex_content_generation(),invalidPreserved:true,finite:true}));
}
console.log('Optional editor ABI isolated; both release games reject invalid edits and adopt twelve valid generations.');
