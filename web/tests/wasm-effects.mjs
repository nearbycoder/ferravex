// Actual release-Wasm effects/geometry ABI integration. No browser or GPU mock.
import assert from 'node:assert/strict';
import fs from 'node:fs';
const catalog=JSON.parse(fs.readFileSync(new URL('../../examples/catalog.json',import.meta.url)));
for(const id of ['strike','neon-swarm','dune-skimmer','pocket-rally','sky-courier']){
 const game=catalog.find(g=>g.id===id),module=new WebAssembly.Module(fs.readFileSync(new URL(`../../dist/web/${game.host}/${id.replaceAll('-','_')}.wasm`,import.meta.url)));
 const env={ferravex_save_len:()=>0,ferravex_save_load:()=>-1,ferravex_save_store:()=>-1,ferravex_controls_len:()=>0,ferravex_controls_load:()=>-1,ferravex_controls_store:()=>-1,ferravex_set_playing(){}};
 const e=new WebAssembly.Instance(module,{env}).exports;e.ferravex_init();e.ferravex_autoplay(id==='sky-courier'?0:1);
 const tap=code=>{e.ferravex_key(code,1);e.ferravex_key(code,0);};let sprites=false,ribbons=false,firstSprite=null,firstRibbon=null;
 for(let frame=0;frame<900;frame++){
  if(frame===1)tap(0xff0d);
  if(frame===3){if(id==='strike')e.ferravex_button(1,1);if(id==='sky-courier')e.ferravex_key(32,1);}
  if(frame===20&&id==='strike')e.ferravex_button(1,0);
  if(frame===50&&id==='strike')tap(114);
  e.ferravex_frame(1/60,1280,720);
  const count=e.ferravex_sprite_instance_count(),view=new DataView(e.memory.buffer);assert.ok(count<=65536);
  if(count){sprites=true;firstSprite??=frame;assert.ok(new Float32Array(e.memory.buffer,e.ferravex_sprite_instances(),count*16).every(Number.isFinite));}
  for(let i=0;i<e.ferravex_sprite_batch_count();i++){const at=e.ferravex_sprite_batches()+i*16,first=view.getUint32(at,true),n=view.getUint32(at+4,true),flags=view.getUint32(at+8,true),triangles=view.getUint32(at+12,true);assert.ok(first+n<=count&&flags<8&&triangles>=1&&triangles<=2);}
  for(let i=0;i<e.ferravex_effect_batch_count();i++){const at=e.ferravex_effect_batches()+i*12;assert.ok(view.getUint32(at,true)+view.getUint32(at+4,true)<=e.ferravex_vertex_count());assert.ok(view.getUint32(at+8,true)<8);ribbons=true;firstRibbon??=frame;}
  if(id==='pocket-rally'&&e.ferravex_effect_vertex_count()>0){ribbons=true;firstRibbon??=frame;}
  const wantRibbons=['strike','pocket-rally','sky-courier'].includes(id),wantSprites=id!=='pocket-rally';
  if((!wantRibbons||ribbons)&&(!wantSprites||sprites))break;
 }
 if(id!=='pocket-rally')assert.ok(sprites,id+' emits actual instanced particles');
 if(['strike','pocket-rally','sky-courier'].includes(id))assert.ok(ribbons,id+' emits actual ribbon/beam geometry');
 console.log(JSON.stringify({game:id,sprites,ribbons,firstSprite,firstRibbon}));
}
