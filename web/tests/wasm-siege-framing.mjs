// Actual-Wasm retained geometry check. This supplies fixed elapsed time and
// reads render buffers; it is not a browser, playtest, or performance result.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const path=process.argv[2] ?? new URL('../../dist/web/siege-foundry/siege_foundry.wasm',import.meta.url);
const bytes=readFileSync(path);
console.log('Wasm SHA256',createHash('sha256').update(bytes).digest('hex'));
const module=new WebAssembly.Module(bytes);
const multiply=(m,v)=>[0,1,2,3].map(row=>v.reduce((n,x,col)=>n+m[col*4+row]*x,0));
for(const [w,h] of [[320,448],[390,652],[1280,608]]) {
  const e=new WebAssembly.Instance(module,{env:{
    ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,
    ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,
    ferravex_set_playing:()=>{},
  }}).exports;
  e.ferravex_init();e.ferravex_autoplay(0);e.ferravex_frame(1/60,w,h);
  const view=new DataView(e.memory.buffer), instances=e.ferravex_retained_instances();
  let targets=0,cannon=0,vertices=0;const bounds=[];
  for(let i=0;i<e.ferravex_retained_batch_count();i++) {
    const at=e.ferravex_retained_batches()+i*224;
    const mesh=view.getUint32(at,true),first=view.getUint32(at+4,true),count=view.getUint32(at+8,true);
    const camera=new Float32Array(e.memory.buffer,at+16,16);
    const geometry=new Float32Array(e.memory.buffer,e.ferravex_mesh_vertices(mesh),e.ferravex_mesh_vertex_count(mesh)*10);
    for(let j=first;j<first+count;j++) {
      const model=new Float32Array(e.memory.buffer,instances+j*112,16),pos=[model[12],model[13],model[14]];
      const isTarget=Math.abs(pos[2]+4)<.001 && pos[1]>2.5 && geometry.length===360;
      const isCannon=pos[2]>=13 && pos[2]<=16.4 && Math.abs(pos[0])<1.4;
      if(!isTarget&&!isCannon)continue;
      if(isTarget)targets++;if(isCannon)cannon++;
      let min=[Infinity,Infinity],max=[-Infinity,-Infinity];
      for(let k=0;k<geometry.length;k+=10) {
        const clip=multiply(camera,multiply(model,[...geometry.slice(k,k+3),1]));
        const x=(clip[0]/clip[3]+1)*w/2,y=(1-clip[1]/clip[3])*h/2;
        const bottom=h-(w<700?196:148)-24;
        assert.ok(x>=15.99&&x<=w-15.99&&y>=95.99&&y<=bottom+.01,
          `${w}x${h}: ${isTarget?'target':'cannon'} at ${pos} vertex projects to ${x},${y} outside scene`);
        min=[Math.min(min[0],x),Math.min(min[1],y)];max=[Math.max(max[0],x),Math.max(max[1],y)];vertices++;
      }
      bounds.push({kind:isTarget?'target':'cannon',position:pos,width:max[0]-min[0],height:max[1]-min[1]});
    }
  }
  assert.equal(targets,6,'all six targets actually submitted, not culled');
  assert.equal(cannon,5,'chassis, two wheels, barrel and muzzle actually submitted');
  console.log(JSON.stringify({w,h,targets,cannon,vertices,bounds}));
}
