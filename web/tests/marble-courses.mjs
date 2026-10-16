// Actual release Wasm, using the shipped UI and ordinary autoplay controller.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const module=new WebAssembly.Module(fs.readFileSync(new URL('../../dist/web/marble-run/marble_run.wasm',import.meta.url)));
function runtime(auto=false,w=1280,h=720){
 const e=new WebAssembly.Instance(module,{env:{ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}}}).exports;
 e.ferravex_init();e.ferravex_autoplay(+auto);
 const frame=()=>{e.ferravex_frame(1/60,w,h);const view=new DataView(e.memory.buffer);for(let i=0;i<e.ferravex_retained_batch_count();i++)assert.ok(e.ferravex_mesh_vertex_count(view.getUint32(e.ferravex_retained_batches()+i*224,true))>0,'drawn meshes stay alive through course-switch submission');assert.ok(new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8).every(Number.isFinite));};
 const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);frame();};
 const modal=()=>{const v=new Float32Array(e.memory.buffer,e.ferravex_vertices(),e.ferravex_vertex_count()*8);for(let i=0;i<v.length;i+=8)if(Math.abs(v[i+4]-.07)<1e-5&&Math.abs(v[i+5]-.13)<1e-5&&Math.abs(v[i+7]-.7)<1e-5)return true;return false;};
 const click=(x,y)=>{e.ferravex_pointer(x,y,0,0);e.ferravex_button(1,1);frame();e.ferravex_button(1,0);frame();frame();};
 const choose=index=>{if(!modal())key(99);const pw=Math.min(w-24,560),ph=Math.min(h-16,268),cw=(pw-36)/2,ch=(ph-48)/2;click((w-pw)/2+12+(index%2)*(cw+12)+cw/2,(h-ph)/2+36+Math.floor(index/2)*(ch+8)+ch/2);assert.ok(!modal(),'course selection closes picker');};
 const courseEnd=(x,y,z)=>{e.ferravex_key(0xff09,1);frame();const v=new Float32Array(e.memory.buffer,e.ferravex_retained_instances(),e.ferravex_retained_instance_count()*28);let found=false;for(let i=0;i<v.length;i+=28)found ||= Math.abs(v[i+12]-x)<1e-4&&Math.abs(v[i+13]-y)<1e-4&&Math.abs(v[i+14]-z)<1e-4;e.ferravex_key(0xff09,0);frame();assert.ok(found,`selected scene contains authored finish platform ${x},${y},${z}`);};
 const source=()=>new TextDecoder().decode(new Uint8Array(e.memory.buffer,e.ferravex_content_source(),e.ferravex_content_source_len()));
 frame();return {e,frame,key,modal,click,choose,courseEnd,source};
}
const files=['aqueduct','needlewalk','clockwork','frostline'].map(id=>fs.readFileSync(new URL(`../../examples/marble-run/content/${id}.fxscene`,import.meta.url),'utf8'));
const ends=[[-3,0,-39],[-5,0,-40],[0,0,-55],[-2,-3,-40]];
for(const [w,h] of [[1280,720],[390,212],[320,152]]){
 const r=runtime(false,w,h);assert.ok(r.modal());
 for(let i=0;i<4;i++){r.choose(i);assert.equal(r.source(),files[i]);r.courseEnd(...ends[i]);}
 r.key(99);assert.ok(r.modal());r.e.ferravex_details_open(1);r.frame();r.click(w/2,h/2);assert.ok(r.modal(),'Details blocks the underlying course picker');r.e.ferravex_details_open(0);r.frame();
}

{
 const r=runtime();r.choose(2);const original=r.source(),documentId=r.e.ferravex_content_document();
 const edit=text=>{const bytes=new TextEncoder().encode(text),p=r.e.ferravex_content_edit_buffer(bytes.length);new Uint8Array(r.e.memory.buffer,p,bytes.length).set(bytes);r.e.ferravex_content_submit(bytes.length);r.frame();};
 edit(original.replace('speed number 0.75;','speed number 9;'));assert.equal(r.source(),original,'invalid sweeper edits preserve the accepted course');
 const edited=original.replace('speed number 0.75;','speed number 0.8;');edit(edited);assert.equal(r.source(),edited);
 r.key(114);r.frame();assert.equal(r.source(),edited,'retry retains live edits');assert.equal(r.e.ferravex_content_document(),documentId,'edits/retry retain the document identity');
 r.choose(1);r.choose(2);assert.equal(r.source(),original,'switching away and back reloads the embedded course');assert.ok(r.e.ferravex_content_document()>documentId,'course changes signal the editor to reload the selected source');
}
const r=runtime(true);
for(let i=0;i<4;i++){
 assert.equal(r.source(),files[i]);r.courseEnd(...ends[i]);
 let frames=0;while(!r.modal()&&frames++<60*90)r.frame();
 assert.ok(r.modal(),`course ${i} finishes through actual Wasm physics and inputs`);
 console.log(`Marble course ${i+1}: completion panel after ${frames} frames`);
 r.click(640,416);
 assert.equal(r.modal(),i===3,'Next progresses; final course returns to selection');
}
console.log('Marble desktop/phone course switching, Details blocking, all-course autoplay and campaign progression passed');
