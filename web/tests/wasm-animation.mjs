// Offline ABI integration against Strike's actual release Wasm, not a mock game.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const module=new WebAssembly.Module(fs.readFileSync(new URL('../../dist/web/strike/strike.wasm',import.meta.url)));
const env={ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}};
const e=new WebAssembly.Instance(module,{env}).exports;
e.ferravex_init();
const key=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);};
const poses=new Set();let operator=false,weapon=false,recoil=false,reload=false;
for(let frame=0;frame<200;frame++){
 if(frame===1){key(0xff0d);}
 if(frame===3){e.ferravex_key(119,1);e.ferravex_button(1,1);}
 if(frame===20)e.ferravex_button(1,0);
 if(frame===50)key(114);
 e.ferravex_frame(1/60,1280,720);
 const joints=e.ferravex_skin_joint_count();assert.ok(joints<=32768);
 const view=new DataView(e.memory.buffer),matrices=new Float32Array(e.memory.buffer,e.ferravex_skin_matrices(),joints*16),normals=new Float32Array(e.memory.buffer,e.ferravex_skin_normals(),joints*9);
 assert.ok(matrices.every(Number.isFinite)&&normals.every(Number.isFinite));
 for(let i=0;i<e.ferravex_retained_batch_count();i++){
  const at=e.ferravex_retained_batches()+i*224,mesh=view.getUint32(at,true),first=view.getUint32(at+216,true),count=view.getUint32(at+220,true);
  if(!count)continue;
  assert.ok(count<=32&&first+count<=joints);assert.ok(e.ferravex_mesh_skin(mesh)>0,'skinned draw has immutable GPU influences');
  const points=e.ferravex_mesh_vertex_count(mesh),weights=new Float32Array(e.memory.buffer,e.ferravex_mesh_skin(mesh),points*8);
  for(let v=0;v<points;v++){let sum=0;for(let j=0;j<4;j++){assert.ok(weights[v*8+j]>=0&&weights[v*8+j]<count);sum+=weights[v*8+4+j];}assert.ok(Math.abs(sum-1)<1e-5);}
  if(count===8){operator=true;poses.add(Array.from(matrices.subarray(first*16,(first+count)*16)).map(x=>x.toFixed(4)).join(','));}
  if(count===4){weapon=true;recoil ||= matrices[first*16+14]>.01;reload ||= matrices[(first+1)*16+13]<-.2;}
 }
}
assert.ok(operator&&weapon,'actual game records both operator and first-person weapon skins');
assert.ok(poses.size>8,'operators evaluate changing animation poses');assert.ok(recoil,'accepted shots drive recoil');assert.ok(reload,'reload state detaches the magazine');
console.log(JSON.stringify({operator,weapon,recoil,reload,operatorPoses:poses.size}));
